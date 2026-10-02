//! Skeleton retargeting between Vice City and PES 6 rigs: bone mapping,
//! scale and axis conventions.
//!
//! An animation of one skeleton plays on another by giving each target bone
//! the rotation its source bone has *relative to its bind pose*, both
//! expressed in a common frame: Y up, the character facing +Z, its left
//! towards +X. Tommy's bind pose and PES bodies are both in that frame (T
//! poses, arms along X); Vice City animations are not (see
//! [`VICE_CITY_ANIMATION_TO_COMMON`]).

use std::f32::consts::FRAC_1_SQRT_2;

use asset_bridge::model::{Animation, Key, Model, Track};
use glam::{Mat3, Mat4, Quat, Vec3};

/// Rotation from Vice City's world to a Y-up world such as Bevy's, as a
/// quaternion (x, y, z, w): -90° around X.
///
/// Vice City is Z-up, and characters move towards +Y (the root of
/// `run_player` advances along +Y). After this rotation, up is +Y and their
/// forward is -Z, Bevy's forward. Both worlds use metres.
pub const VICE_CITY_TO_Y_UP: [f32; 4] = [-FRAC_1_SQRT_2, 0.0, 0.0, FRAC_1_SQRT_2];

/// Rotation from the space of Vice City animations to the common frame of
/// retargeting, as a quaternion (x, y, z, w): 180° around (0, 1, 1)/√2.
///
/// Animations stand the character along +Z and move it towards +Y, its
/// left towards -X (the right thigh of `run_player` is at +X). This turns
/// +Z into +Y, +Y into +Z and +X into -X.
pub const VICE_CITY_ANIMATION_TO_COMMON: [f32; 4] = [0.0, FRAC_1_SQRT_2, FRAC_1_SQRT_2, 0.0];

/// Units of PES 6 models per metre. PES 6 models are already Y-up, feet at
/// Y = 0 (a player's body goes from -18 to 673.8 units, its head up to about
/// 750).
// HYPOTHÈSE: chosen so that a player is about 1.80 m tall (1.83 m with
// these figures); to be checked by eye next to Tommy. A face (about 90
// units) then measures about 21 cm.
pub const PES6_UNITS_PER_METRE: f32 = 420.0;

/// Bones of a PES 6 body (names of `asset_bridge::pes6::BODY_BONES`) and the
/// bones of Tommy (`player.dff`) they follow, the root first. The PES "hips"
/// bone, between the pelvis and the thighs, has no counterpart: it keeps
/// its bind pose relative to the pelvis.
// HYPOTHÈSE: "spine" (the upper trunk, from 0.14 m above the pelvis up to
// the clavicles) follows Spine1, which covers the same part of Tommy; Spine
// starts at the pelvis.
pub const PES6_FROM_VICE_CITY: &[(&str, &str)] = &[
    ("pelvis", "Pelvis"),
    ("spine", "Spine1"),
    ("neck", "Neck"),
    ("head", "Head"),
    ("left clavicle", "Bip01 L Clavicle"),
    ("right clavicle", "Bip01 R Clavicle"),
    ("left upper arm", "L UpperArm"),
    ("right upper arm", "R UpperArm"),
    ("left forearm", "L Forearm"),
    ("right forearm", "R Forearm"),
    ("left hand", "L Hand"),
    ("right hand", "R Hand"),
    ("left thigh", "L Thigh"),
    ("right thigh", "R Thigh"),
    ("left calf", "L Calf"),
    ("right calf", "R Calf"),
    ("left foot", "L Foot"),
    ("right foot", "R Foot"),
];

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RetargetError {
    #[error("os « {0} » absent du modèle")]
    MissingBone(String),
}

/// Plays `animation`, made for `source`, on `target`.
///
/// `map` pairs target node names with source node names, the root first;
/// `source_to_common` turns the space of the animation into the common
/// frame (both models' bind poses must already be in it). Each target bone
/// of the map gets, at every key of the animation, the rotation its source
/// bone has relative to its bind pose; the root also gets the move of the
/// source root, scaled by the ratio of the heights of the two roots above
/// their lowest mapped bone (the legs). With `in_place`, the horizontal
/// travel of the root over the whole animation is taken out, so that the
/// animation loops on the spot. Target bones outside the map keep their
/// bind pose relative to their parent.
pub fn retarget(
    animation: &Animation,
    source: &Model,
    target: &Model,
    map: &[(&str, &str)],
    source_to_common: [f32; 4],
    in_place: bool,
) -> Result<Animation, RetargetError> {
    let node = |model: &Model, name: &str| {
        model
            .nodes
            .iter()
            .position(|node| node.name.eq_ignore_ascii_case(name))
            .ok_or_else(|| RetargetError::MissingBone(name.to_owned()))
    };
    let pairs: Vec<(usize, usize)> = map
        .iter()
        .map(|(to, from)| Ok((node(target, to)?, node(source, from)?)))
        .collect::<Result<_, RetargetError>>()?;
    let Some(&(target_root, source_root)) = pairs.first() else {
        return Ok(Animation {
            tracks: Vec::new(),
            ..animation.clone()
        });
    };

    let source_bind = bind_pose(source);
    let target_bind = bind_pose(target);
    let to_common = Quat::from_array(source_to_common);
    let lowest = |bind: &[Mat4], nodes: &mut dyn Iterator<Item = usize>| {
        nodes
            .map(|node| bind[node].w_axis.y)
            .fold(f32::MAX, f32::min)
    };
    let height = |bind: &[Mat4], root: usize, low: f32| (bind[root].w_axis.y - low).max(1e-6);
    let ratio = height(
        &target_bind.worlds,
        target_root,
        lowest(&target_bind.worlds, &mut pairs.iter().map(|p| p.0)),
    ) / height(
        &source_bind.worlds,
        source_root,
        lowest(&source_bind.worlds, &mut pairs.iter().map(|p| p.1)),
    );

    let times = key_times(animation);
    let source_root_at = |time: f32| {
        to_common
            * animated_worlds(source, &source_bind, animation, time)[source_root]
                .w_axis
                .truncate()
    };
    let drift = match (times.first(), times.last()) {
        (Some(&first), Some(&last)) if in_place && last > first => {
            let travel = source_root_at(last) - source_root_at(first);
            Vec3::new(travel.x, 0.0, travel.z) / (last - first)
        }
        _ => Vec3::ZERO,
    };

    let mut tracks: Vec<Track> = pairs
        .iter()
        .map(|&(node, _)| Track {
            bone_name: target.nodes[node].name.clone(),
            bone_id: None,
            keys: Vec::with_capacity(times.len()),
        })
        .collect();
    let source_bind_root = source_bind.worlds[source_root].w_axis.truncate();
    for &time in &times {
        let worlds = animated_worlds(source, &source_bind, animation, time);
        // Rotation of each target node in the common frame; a parent comes
        // before its children.
        let mut rotations: Vec<Quat> = Vec::with_capacity(target.nodes.len());
        for node in 0..target.nodes.len() {
            let bind = rotation_of(&target_bind.worlds[node]);
            let delta = match pairs.iter().find(|pair| pair.0 == node) {
                Some(&(_, from)) => {
                    to_common
                        * rotation_of(&worlds[from])
                        * rotation_of(&source_bind.worlds[from]).inverse()
                }
                // Outside the map: turn with the parent.
                None => match target.nodes[node].parent {
                    Some(parent) => {
                        rotations[parent] * rotation_of(&target_bind.worlds[parent]).inverse()
                    }
                    None => Quat::IDENTITY,
                },
            };
            rotations.push((delta * bind).normalize());
        }
        for (track, &(node, _)) in tracks.iter_mut().zip(&pairs) {
            let parent = target.nodes[node].parent;
            let parent_rotation = parent.map_or(Quat::IDENTITY, |p| rotations[p]);
            let local = parent_rotation.inverse() * rotations[node];
            let translation = (node == target_root).then(|| {
                // The bind pose is already in the common frame.
                let moved =
                    (source_root_at(time) - source_bind_root - drift * (time - times[0])) * ratio;
                let world = target_bind.worlds[node].w_axis.truncate() + moved;
                let parent_world = parent.map_or(Mat4::IDENTITY, |p| target_bind.worlds[p]);
                parent_world.inverse().transform_point3(world).to_array()
            });
            track.keys.push(Key {
                time,
                rotation: local.to_array(),
                translation,
                scale: None,
            });
        }
    }
    Ok(Animation {
        name: animation.name.clone(),
        duration: animation.duration,
        tracks,
    })
}

/// The bind pose of a model: a bone sits where the inverse of its inverse
/// bind matrix puts it, other nodes keep their own transform (as in
/// `bevy_bridge::BindPose`).
struct BindPose {
    locals: Vec<Mat4>,
    worlds: Vec<Mat4>,
}

fn bind_pose(model: &Model) -> BindPose {
    let mut worlds: Vec<Mat4> = Vec::with_capacity(model.nodes.len());
    let mut locals = Vec::with_capacity(model.nodes.len());
    for (index, node) in model.nodes.iter().enumerate() {
        let parent_world = node.parent.map_or(Mat4::IDENTITY, |p| worlds[p]);
        let bone = model
            .skeleton
            .as_ref()
            .and_then(|skeleton| skeleton.bones.iter().find(|bone| bone.node == index));
        let world = match bone {
            Some(bone) => Mat4::from_cols_array(&bone.inverse_bind).inverse(),
            None => parent_world * Mat4::from_cols_array(&node.local),
        };
        locals.push(parent_world.inverse() * world);
        worlds.push(world);
    }
    BindPose { locals, worlds }
}

/// World transform of every node of `model` at `time` of `animation`, in
/// the model's space; nodes without a track keep their bind pose.
pub fn pose(model: &Model, animation: &Animation, time: f32) -> Vec<Mat4> {
    animated_worlds(model, &bind_pose(model), animation, time)
}

fn animated_worlds(model: &Model, bind: &BindPose, animation: &Animation, time: f32) -> Vec<Mat4> {
    let mut locals = bind.locals.clone();
    for track in &animation.tracks {
        let Some(node) = track.node_in(model) else {
            continue;
        };
        let (_, _, bind_translation) = locals[node].to_scale_rotation_translation();
        let (rotation, translation) = sample(track, time);
        let translation = translation.unwrap_or(bind_translation);
        locals[node] = Mat4::from_rotation_translation(rotation, translation);
    }
    let mut worlds: Vec<Mat4> = Vec::with_capacity(locals.len());
    for (node, local) in model.nodes.iter().zip(locals) {
        worlds.push(node.parent.map_or(Mat4::IDENTITY, |p| worlds[p]) * local);
    }
    worlds
}

/// Rotation and translation of `track` at `time`, interpolated between the
/// two surrounding keys.
fn sample(track: &Track, time: f32) -> (Quat, Option<Vec3>) {
    let keys = &track.keys;
    let next = keys
        .iter()
        .position(|key| key.time > time)
        .unwrap_or(keys.len());
    let (a, b, s) = match next {
        0 => (0, 0, 0.0),
        n if n >= keys.len() => (keys.len() - 1, keys.len() - 1, 0.0),
        n => {
            let span = keys[n].time - keys[n - 1].time;
            let s = if span > 0.0 {
                (time - keys[n - 1].time) / span
            } else {
                0.0
            };
            (n - 1, n, s)
        }
    };
    let rotation = |i: usize| Quat::from_array(keys[i].rotation).normalize();
    let translation = match (keys[a].translation, keys[b].translation) {
        (Some(from), Some(to)) => Some(Vec3::from_array(from).lerp(Vec3::from_array(to), s)),
        _ => None,
    };
    (rotation(a).slerp(rotation(b), s), translation)
}

/// Every key time of the animation, in order, without duplicates.
fn key_times(animation: &Animation) -> Vec<f32> {
    let mut times: Vec<f32> = animation
        .tracks
        .iter()
        .flat_map(|track| track.keys.iter().map(|key| key.time))
        .collect();
    times.sort_by(f32::total_cmp);
    times.dedup_by(|a, b| (*a - *b).abs() < 1e-4);
    times
}

fn rotation_of(matrix: &Mat4) -> Quat {
    Quat::from_mat3(&Mat3::from_mat4(*matrix).orthonormalized()).normalize()
}

/// Gram-Schmidt on the columns: removes scale and shear before taking the
/// rotation of a matrix.
trait Orthonormalized {
    fn orthonormalized(self) -> Self;
}

impl Orthonormalized for Mat3 {
    fn orthonormalized(self) -> Self {
        let x = self.x_axis.normalize_or_zero();
        let y = (self.y_axis - x * x.dot(self.y_axis)).normalize_or_zero();
        let z = x.cross(y);
        Mat3::from_cols(x, y, z)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use asset_bridge::model::{Bone, Node, Skeleton};

    /// Rotates `v` by the unit quaternion `q`.
    fn rotate(q: [f32; 4], v: [f32; 3]) -> [f32; 3] {
        Quat::from_array(q).mul_vec3(Vec3::from_array(v)).to_array()
    }

    fn assert_close(a: [f32; 3], b: [f32; 3]) {
        assert!(
            a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-4),
            "{a:?} != {b:?}"
        );
    }

    #[test]
    fn vice_city_up_becomes_y_and_forward_becomes_minus_z() {
        assert_close(rotate(VICE_CITY_TO_Y_UP, [0.0, 0.0, 1.0]), [0.0, 1.0, 0.0]);
        assert_close(rotate(VICE_CITY_TO_Y_UP, [0.0, 1.0, 0.0]), [0.0, 0.0, -1.0]);
        assert_close(rotate(VICE_CITY_TO_Y_UP, [1.0, 0.0, 0.0]), [1.0, 0.0, 0.0]);
    }

    #[test]
    fn vice_city_animations_turn_into_the_common_frame() {
        let q = VICE_CITY_ANIMATION_TO_COMMON;
        assert_close(rotate(q, [0.0, 0.0, 1.0]), [0.0, 1.0, 0.0]);
        assert_close(rotate(q, [0.0, 1.0, 0.0]), [0.0, 0.0, 1.0]);
        assert_close(rotate(q, [1.0, 0.0, 0.0]), [-1.0, 0.0, 0.0]);
    }

    /// A leg: a root node and two bones, `pelvis` at `height` and `foot` on
    /// the ground, the foot's bind pose turned by `foot_turn`.
    fn leg(names: [&str; 2], height: f32, foot_turn: Quat) -> Model {
        let pelvis = Mat4::from_translation(Vec3::Y * height);
        let foot = Mat4::from_quat(foot_turn);
        let node = |name: &str, parent, local: Mat4| Node {
            name: name.into(),
            parent,
            local: local.to_cols_array(),
            bone_id: None,
        };
        Model {
            name: "jambe".into(),
            nodes: vec![
                node("root", None, Mat4::IDENTITY),
                node(names[0], Some(0), pelvis),
                node(names[1], Some(1), pelvis.inverse() * foot),
            ],
            meshes: Vec::new(),
            skeleton: Some(Skeleton {
                bones: [(names[0], pelvis), (names[1], foot)]
                    .into_iter()
                    .enumerate()
                    .map(|(index, (name, world))| Bone {
                        name: name.into(),
                        node: index + 1,
                        inverse_bind: world.inverse().to_cols_array(),
                    })
                    .collect(),
            }),
        }
    }

    fn key(time: f32, rotation: Quat, translation: Option<Vec3>) -> Key {
        Key {
            time,
            rotation: rotation.to_array(),
            translation: translation.map(|t| t.to_array()),
            scale: None,
        }
    }

    #[test]
    fn rotations_carry_over_relative_to_the_bind_pose() {
        // The source foot has a different bind orientation from the target
        // foot; bending the source foot by 30° around X bends the target
        // foot by the same 30°, in the common frame.
        let source = leg(["Pelvis", "Foot"], 1.0, Quat::from_rotation_y(1.0));
        let target = leg(["pelvis", "foot"], 400.0, Quat::IDENTITY);
        let bend = Quat::from_rotation_x(0.5);
        let source_foot_bind = Quat::from_rotation_y(1.0);
        let animation = Animation {
            name: "test".into(),
            duration: 1.0,
            tracks: vec![
                Track {
                    bone_name: "Pelvis".into(),
                    bone_id: None,
                    keys: vec![
                        key(0.0, Quat::IDENTITY, Some(Vec3::Y)),
                        key(1.0, Quat::IDENTITY, Some(Vec3::new(0.0, 0.9, 2.0))),
                    ],
                },
                Track {
                    bone_name: "Foot".into(),
                    bone_id: None,
                    // Local to the pelvis, which has no rotation.
                    keys: vec![
                        key(0.0, source_foot_bind, None),
                        key(1.0, bend * source_foot_bind, None),
                    ],
                },
            ],
        };
        let map = [("pelvis", "Pelvis"), ("foot", "Foot")];
        let moved = retarget(
            &animation,
            &source,
            &target,
            &map,
            Quat::IDENTITY.to_array(),
            false,
        )
        .unwrap();
        assert_eq!(moved.tracks.len(), 2);
        let foot = &moved.tracks[1];
        assert_eq!(foot.bone_name, "foot");
        let last = Quat::from_array(foot.keys.last().unwrap().rotation);
        assert!(last.angle_between(bend) < 1e-4, "{last:?}");
        let first = Quat::from_array(foot.keys[0].rotation);
        assert!(first.angle_between(Quat::IDENTITY) < 1e-4, "{first:?}");

        // The pelvis moves 0.1 down and 2 forward in the source, a leg of
        // height 1; the target's leg is 400 high.
        let pelvis = &moved.tracks[0].keys;
        assert_close(pelvis[0].translation.unwrap(), [0.0, 400.0, 0.0]);
        assert_close(pelvis[1].translation.unwrap(), [0.0, 360.0, 800.0]);

        // In place, the forward travel is taken out, the drop stays.
        let in_place = retarget(
            &animation,
            &source,
            &target,
            &map,
            Quat::IDENTITY.to_array(),
            true,
        )
        .unwrap();
        assert_close(
            in_place.tracks[0].keys[1].translation.unwrap(),
            [0.0, 360.0, 0.0],
        );
    }

    #[test]
    fn the_source_frame_is_turned_into_the_common_one() {
        // A Z-up source: the same animation turned by the inverse of the
        // source frame gives the same result.
        let source = leg(["Pelvis", "Foot"], 1.0, Quat::IDENTITY);
        let target = leg(["pelvis", "foot"], 1.0, Quat::IDENTITY);
        let frame = Quat::from_array(VICE_CITY_ANIMATION_TO_COMMON);
        let bend = Quat::from_rotation_x(0.5);
        let animation = Animation {
            name: "test".into(),
            duration: 1.0,
            tracks: vec![
                Track {
                    bone_name: "Pelvis".into(),
                    bone_id: None,
                    keys: vec![key(0.0, frame.inverse(), Some(frame.inverse() * Vec3::Y))],
                },
                Track {
                    bone_name: "Foot".into(),
                    bone_id: None,
                    keys: vec![key(0.0, bend, None)],
                },
            ],
        };
        let moved = retarget(
            &animation,
            &source,
            &target,
            &[("pelvis", "Pelvis"), ("foot", "Foot")],
            VICE_CITY_ANIMATION_TO_COMMON,
            false,
        )
        .unwrap();
        let pelvis = Quat::from_array(moved.tracks[0].keys[0].rotation);
        let foot = Quat::from_array(moved.tracks[1].keys[0].rotation);
        assert!(pelvis.angle_between(Quat::IDENTITY) < 1e-4, "{pelvis:?}");
        assert!(foot.angle_between(bend) < 1e-4, "{foot:?}");
        assert_close(
            moved.tracks[0].keys[0].translation.unwrap(),
            [0.0, 1.0, 0.0],
        );
    }

    #[test]
    fn missing_bones_are_reported() {
        let model = leg(["pelvis", "foot"], 1.0, Quat::IDENTITY);
        let animation = Animation {
            name: "test".into(),
            duration: 1.0,
            tracks: Vec::new(),
        };
        assert_eq!(
            retarget(
                &animation,
                &model,
                &model,
                &[("pelvis", "Pelvis2")],
                Quat::IDENTITY.to_array(),
                false
            ),
            Err(RetargetError::MissingBone("Pelvis2".into()))
        );
    }
}
