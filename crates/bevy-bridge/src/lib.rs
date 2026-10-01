//! Brings the engine-neutral assets of `asset-bridge` into Bevy: textured and
//! skinned models, animations played on their bones, sounds.
//!
//! The asset viewer and the game share this code; neither of them touches
//! the format crates.

use std::collections::HashMap;
use std::sync::Arc;

use asset_bridge::model::{Animation, Model, Sound, Texture, Track};
use bevy::asset::RenderAssetUsages;
use bevy::audio::AudioSource;
use bevy::camera::visibility::NoFrustumCulling;
use bevy::ecs::system::SystemParam;
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::mesh::skinning::{SkinnedMesh, SkinnedMeshInverseBindposes};
use bevy::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::transform::TransformSystems;

/// Plays the [`AnimationLayers`] of spawned models.
pub struct BevyBridgePlugin;

impl Plugin for BevyBridgePlugin {
    fn build(&self, app: &mut App) {
        // After game logic (which starts and stops layers), before the
        // transforms are propagated to the skinned meshes.
        app.add_systems(
            PostUpdate,
            play_animations.before(TransformSystems::Propagate),
        );
    }
}

/// On the root entity of a spawned model.
#[derive(Component, Debug, Clone)]
pub struct SpawnedModel {
    /// Entity of each node of the model, in the model's order.
    pub nodes: Vec<Entity>,
    /// Name of each node.
    pub node_names: Vec<String>,
    /// Bind-pose transform of each node relative to its parent.
    pub bind_locals: Vec<Transform>,
}

impl SpawnedModel {
    /// Entity of the node with this name, ignoring case (e.g. `R Hand`).
    pub fn node(&self, name: &str) -> Option<Entity> {
        self.node_names
            .iter()
            .position(|node| node.eq_ignore_ascii_case(name))
            .map(|index| self.nodes[index])
    }
}

/// Bind pose of a model: transforms of the nodes relative to their parent
/// (`locals`) and to the model (`worlds`).
///
/// A bone sits where the inverse of its inverse bind matrix puts it: this is
/// the pose the skinned mesh was made for, while the frames of a few Vice
/// City models are not in that pose. Other nodes keep their own transform.
pub struct BindPose {
    pub locals: Vec<Mat4>,
    pub worlds: Vec<Mat4>,
}

impl BindPose {
    pub fn of(model: &Model) -> Self {
        let mut worlds: Vec<Mat4> = Vec::with_capacity(model.nodes.len());
        let mut locals = Vec::with_capacity(model.nodes.len());
        for (index, node) in model.nodes.iter().enumerate() {
            let parent_world = node.parent.map_or(Mat4::IDENTITY, |p| worlds[p]);
            let world = match bone_of_node(model, index) {
                Some(bone) => {
                    let skeleton = model.skeleton.as_ref().expect("a bone implies a skeleton");
                    Mat4::from_cols_array(&skeleton.bones[bone].inverse_bind).inverse()
                }
                None => parent_world * Mat4::from_cols_array(&node.local),
            };
            locals.push(parent_world.inverse() * world);
            worlds.push(world);
        }
        Self { locals, worlds }
    }
}

/// Index of the bone that moves `node`, if any.
pub fn bone_of_node(model: &Model, node: usize) -> Option<usize> {
    model
        .skeleton
        .as_ref()?
        .bones
        .iter()
        .position(|bone| bone.node == node)
}

/// For each bone: its node, and its parent bone (the closest ancestor node
/// that is also a bone). Used to draw skeletons.
pub fn bone_links(model: &Model) -> Vec<(usize, Option<usize>)> {
    let Some(skeleton) = &model.skeleton else {
        return Vec::new();
    };
    skeleton
        .bones
        .iter()
        .map(|bone| {
            let mut parent = model.nodes[bone.node].parent;
            let parent_bone = loop {
                match parent {
                    Some(node) => match bone_of_node(model, node) {
                        Some(index) => break Some(index),
                        None => parent = model.nodes[node].parent,
                    },
                    None => break None,
                }
            };
            (bone.node, parent_bone)
        })
        .collect()
}

/// How to draw a spawned model.
#[derive(Debug, Clone, Copy, Default)]
pub struct SpawnOptions {
    /// No lighting (texture boards).
    pub unlit: bool,
    /// Both faces of the triangles are drawn.
    pub double_sided: bool,
}

/// Spawns models into the world.
#[derive(SystemParam)]
pub struct ModelSpawner<'w, 's> {
    commands: Commands<'w, 's>,
    meshes: ResMut<'w, Assets<Mesh>>,
    materials: ResMut<'w, Assets<StandardMaterial>>,
    images: ResMut<'w, Assets<Image>>,
    bindposes: ResMut<'w, Assets<SkinnedMeshInverseBindposes>>,
}

impl<'w, 's> ModelSpawner<'w, 's> {
    pub fn commands(&mut self) -> &mut Commands<'w, 's> {
        &mut self.commands
    }

    /// Spawns `model` in its bind pose under a new root entity, which holds
    /// a [`SpawnedModel`] (also returned, to reach nodes right away).
    /// Materials find their texture among `textures` by name, ignoring case
    /// like the game.
    pub fn spawn(
        &mut self,
        model: &Model,
        textures: &[Texture],
        options: SpawnOptions,
    ) -> (Entity, SpawnedModel) {
        let images: HashMap<String, (Handle<Image>, bool)> = textures
            .iter()
            .map(|texture| {
                let handle = self.images.add(texture_image(texture));
                (texture.name.to_lowercase(), (handle, texture.is_opaque()))
            })
            .collect();

        let bind = BindPose::of(model);
        let bind_locals: Vec<Transform> = bind
            .locals
            .iter()
            .map(|&local| Transform::from_matrix(local))
            .collect();
        let root = self
            .commands
            .spawn((Transform::IDENTITY, Visibility::default()))
            .id();

        // One entity per node, in the bind pose; animations move them.
        let mut nodes: Vec<Entity> = Vec::with_capacity(model.nodes.len());
        for (node, local) in model.nodes.iter().zip(&bind_locals) {
            let parent = node.parent.map_or(root, |p| nodes[p]);
            nodes.push(
                self.commands
                    .spawn((*local, Visibility::default(), ChildOf(parent)))
                    .id(),
            );
        }

        let skin = model.skeleton.as_ref().map(|skeleton| SkinnedMesh {
            inverse_bindposes: self.bindposes.add(SkinnedMeshInverseBindposes::from(
                skeleton
                    .bones
                    .iter()
                    .map(|bone| Mat4::from_cols_array(&bone.inverse_bind))
                    .collect::<Vec<_>>(),
            )),
            joints: skeleton.bones.iter().map(|bone| nodes[bone.node]).collect(),
        });

        for mesh in &model.meshes {
            for primitive in &mesh.primitives {
                let mut bevy_mesh = Mesh::new(
                    PrimitiveTopology::TriangleList,
                    RenderAssetUsages::default(),
                )
                .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, mesh.positions.clone())
                .with_inserted_indices(Indices::U32(primitive.indices.clone()));
                match &mesh.normals {
                    Some(normals) => {
                        bevy_mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals.clone());
                    }
                    None => bevy_mesh.compute_smooth_normals(),
                }
                if let Some(uvs) = &mesh.uvs {
                    bevy_mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs.clone());
                }
                if let Some(colors) = &mesh.colors {
                    // Bevy expects linear vertex colours.
                    let linear: Vec<[f32; 4]> = colors
                        .iter()
                        .map(|&[r, g, b, a]| Color::srgba(r, g, b, a).to_linear().to_f32_array())
                        .collect();
                    bevy_mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, linear);
                }
                let skinned = mesh.skin.as_ref().zip(skin.as_ref());
                if let Some((mesh_skin, _)) = skinned {
                    bevy_mesh.insert_attribute(
                        Mesh::ATTRIBUTE_JOINT_INDEX,
                        VertexAttributeValues::Uint16x4(mesh_skin.joints.clone()),
                    );
                    bevy_mesh
                        .insert_attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT, mesh_skin.weights.clone());
                }

                let [r, g, b, a] = primitive.material.base_color;
                let texture = primitive
                    .material
                    .texture
                    .as_ref()
                    .and_then(|name| images.get(&name.to_lowercase()));
                let material = StandardMaterial {
                    base_color: Color::srgba(r, g, b, a),
                    base_color_texture: texture.map(|(handle, _)| handle.clone()),
                    alpha_mode: match texture {
                        Some((_, false)) => AlphaMode::Mask(0.5),
                        _ => AlphaMode::Opaque,
                    },
                    perceptual_roughness: 0.8,
                    unlit: options.unlit,
                    cull_mode: if options.double_sided {
                        None
                    } else {
                        StandardMaterial::default().cull_mode
                    },
                    ..default()
                };
                let mut entity = self.commands.spawn((
                    Mesh3d(self.meshes.add(bevy_mesh)),
                    MeshMaterial3d(self.materials.add(material)),
                ));
                match skinned {
                    // Skinned vertices follow the joints; bounds computed
                    // from the bind pose would wrongly cull the moving mesh.
                    Some((_, skin)) => {
                        entity.insert((skin.clone(), NoFrustumCulling, ChildOf(root)))
                    }
                    None => entity.insert((Transform::IDENTITY, ChildOf(nodes[mesh.node]))),
                };
            }
        }

        let spawned = SpawnedModel {
            nodes,
            node_names: model.nodes.iter().map(|node| node.name.clone()).collect(),
            bind_locals,
        };
        self.commands.entity(root).insert(spawned.clone());
        (root, spawned)
    }
}

/// Bevy image of a decoded texture.
pub fn texture_image(texture: &Texture) -> Image {
    let mut image = Image::new(
        Extent3d {
            width: texture.width,
            height: texture.height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        texture.rgba8.clone(),
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    // HYPOTHÈSE: Vice City textures repeat (sampler 0x1106 of the DFF and
    // TXD files, read as wrap/wrap after the GTAMods wiki).
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        ..default()
    });
    image
}

/// Bevy audio source playing `sound`.
pub fn audio_source(sound: &Sound) -> AudioSource {
    AudioSource {
        bytes: sound.to_wav().into(),
    }
}

/// Animations playing on a spawned model, in order: a layer overrides the
/// bones it has tracks for (e.g. a firing animation of the arm over a body
/// animation). Bones without any track stay in the bind pose.
#[derive(Component, Default)]
pub struct AnimationLayers(pub Vec<AnimationLayer>);

pub struct AnimationLayer {
    animation: Arc<Animation>,
    /// Node driven by each track.
    track_nodes: Vec<Option<usize>>,
    /// Track of the root bone and its move over the whole animation,
    /// removed to play in place.
    root_drift: Option<(usize, Vec3)>,
    /// Seconds into the animation.
    pub time: f32,
    pub paused: bool,
    /// While set, the time loops between these two instants; otherwise the
    /// layer plays to the end and is then removed.
    pub loop_range: Option<(f32, f32)>,
}

impl AnimationLayer {
    /// A layer playing `animation` on `model` once, from the start.
    /// `in_place` removes the move of the root bone.
    pub fn new(animation: Arc<Animation>, model: &Model, in_place: bool) -> Self {
        let track_nodes: Vec<Option<usize>> = animation
            .tracks
            .iter()
            .map(|track| track.node_in(model))
            .collect();
        let root_drift = in_place
            .then(|| {
                let root = model.skeleton.as_ref()?.bones.first()?.node;
                let track = track_nodes.iter().position(|&node| node == Some(root))?;
                let keys = &animation.tracks[track].keys;
                let first = Vec3::from_array(keys.first()?.translation?);
                let last = Vec3::from_array(keys.last()?.translation?);
                Some((track, last - first))
            })
            .flatten();
        Self {
            animation,
            track_nodes,
            root_drift,
            time: 0.0,
            paused: false,
            loop_range: None,
        }
    }

    /// Loops over the whole animation.
    pub fn looping(mut self) -> Self {
        self.loop_range = Some((0.0, self.animation.duration));
        self
    }

    pub fn animation(&self) -> &Animation {
        &self.animation
    }

    pub fn finished(&self) -> bool {
        self.loop_range.is_none() && self.time >= self.animation.duration
    }

    fn advance(&mut self, delta: f32) {
        if self.paused {
            return;
        }
        self.time += delta;
        if let Some((start, end)) = self.loop_range
            && self.time >= end
        {
            let length = (end - start).max(1e-3);
            self.time = start + (self.time - start) % length;
        }
    }

    /// Writes the pose at `time` into the node transforms it drives.
    pub fn apply_at(&self, time: f32, locals: &mut [Transform]) {
        let duration = self.animation.duration.max(1e-3);
        for (index, (track, node)) in self
            .animation
            .tracks
            .iter()
            .zip(&self.track_nodes)
            .enumerate()
        {
            let Some(node) = node else {
                continue;
            };
            let drift = self
                .root_drift
                .filter(|&(root, _)| root == index)
                .map(|(_, drift)| drift * (time.min(duration) / duration));
            sample(&mut locals[*node], track, time, drift);
        }
    }
}

/// Sets the transform of a bone from its track at time `t`, interpolated
/// between the two surrounding keys. `drift` is subtracted from the
/// translation.
fn sample(transform: &mut Transform, track: &Track, t: f32, drift: Option<Vec3>) {
    let keys = &track.keys;
    let Some(last) = keys.len().checked_sub(1) else {
        return;
    };
    let next = keys
        .iter()
        .position(|key| key.time > t)
        .unwrap_or(keys.len());
    let (a, b, s) = match next {
        0 => (0, 0, 0.0),
        n if n > last => (last, last, 0.0),
        n => {
            let span = keys[n].time - keys[n - 1].time;
            let s = if span > 0.0 {
                (t - keys[n - 1].time) / span
            } else {
                0.0
            };
            (n - 1, n, s)
        }
    };
    let rotation = |i: usize| Quat::from_array(keys[i].rotation).normalize();
    transform.rotation = rotation(a).slerp(rotation(b), s);
    if let (Some(from), Some(to)) = (keys[a].translation, keys[b].translation) {
        let translation = Vec3::from_array(from).lerp(Vec3::from_array(to), s);
        transform.translation = translation - drift.unwrap_or(Vec3::ZERO);
    }
    if let (Some(from), Some(to)) = (keys[a].scale, keys[b].scale) {
        transform.scale = Vec3::from_array(from).lerp(Vec3::from_array(to), s);
    }
}

fn play_animations(
    time: Res<Time>,
    mut models: Query<(&SpawnedModel, &mut AnimationLayers)>,
    mut transforms: Query<&mut Transform>,
) {
    for (model, mut layers) in &mut models {
        let mut locals = model.bind_locals.clone();
        for layer in &mut layers.0 {
            layer.advance(time.delta_secs());
            layer.apply_at(layer.time, &mut locals);
        }
        layers.0.retain(|layer| !layer.finished());
        for (&entity, local) in model.nodes.iter().zip(locals) {
            if let Ok(mut transform) = transforms.get_mut(entity) {
                transform.set_if_neq(local);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use asset_bridge::model::{Key, Node};

    fn node(name: &str, parent: Option<usize>, y: f32) -> Node {
        Node {
            name: name.into(),
            parent,
            local: Mat4::from_translation(Vec3::new(0.0, y, 0.0)).to_cols_array(),
            bone_id: None,
        }
    }

    fn model() -> Model {
        Model {
            name: "test".into(),
            nodes: vec![node("Root", None, 0.0), node("Arm", Some(0), 1.0)],
            meshes: vec![],
            skeleton: None,
        }
    }

    fn animation() -> Arc<Animation> {
        let key = |time, angle: f32| Key {
            time,
            rotation: Quat::from_rotation_z(angle).to_array(),
            translation: None,
            scale: None,
        };
        Arc::new(Animation {
            name: "wave".into(),
            duration: 1.0,
            tracks: vec![asset_bridge::model::Track {
                bone_name: "arm".into(),
                bone_id: None,
                keys: vec![key(0.0, 0.0), key(1.0, 1.0)],
            }],
        })
    }

    #[test]
    fn bind_pose_composes_node_transforms() {
        let bind = BindPose::of(&model());
        assert_eq!(bind.worlds[1].w_axis.truncate(), Vec3::new(0.0, 1.0, 0.0));
        assert_eq!(bind.locals[1], bind.worlds[1]);
    }

    #[test]
    fn layers_interpolate_and_keep_untracked_translation() {
        let layer = AnimationLayer::new(animation(), &model(), false);
        let mut locals = vec![Transform::IDENTITY, Transform::from_xyz(0.0, 1.0, 0.0)];
        layer.apply_at(0.5, &mut locals);
        let expected = Quat::from_rotation_z(0.5);
        assert!(locals[1].rotation.angle_between(expected) < 1e-4);
        // No translation keys: the node keeps its bind translation.
        assert_eq!(locals[1].translation, Vec3::new(0.0, 1.0, 0.0));
        assert_eq!(locals[0], Transform::IDENTITY);
    }

    #[test]
    fn loops_stay_in_their_range_and_single_plays_finish() {
        let mut layer = AnimationLayer::new(animation(), &model(), false);
        layer.loop_range = Some((0.25, 0.75));
        layer.time = 0.5;
        layer.advance(0.4);
        assert!((layer.time - 0.4).abs() < 1e-5, "{}", layer.time);
        assert!(!layer.finished());

        let mut once = AnimationLayer::new(animation(), &model(), false);
        once.advance(1.5);
        assert!(once.finished());
    }
}
