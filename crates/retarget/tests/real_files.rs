//! Retargeting between the real files of both games. Skipped when either
//! game is not configured (see `asset_bridge::testing::game_dir`).

use asset_bridge::config::Game;
use asset_bridge::model::{Animation, Model};
use asset_bridge::pes6::Pes6;
use asset_bridge::testing::game_dir;
use asset_bridge::vice_city::ViceCity;
use glam::{Mat4, Quat, Vec3};
use retarget::{PES6_FROM_VICE_CITY, VICE_CITY_ANIMATION_TO_COMMON, pose, retarget};

/// Tommy, `run_player` and the PES field player's body n° 1010.
fn models() -> Option<(Model, Animation, Model)> {
    let vice_city = game_dir(Game::ViceCity)?;
    let pes6 = game_dir(Game::Pes6)?;
    let mut game = ViceCity::open(&vice_city).unwrap();
    let tommy = game.load_model("player").unwrap();
    let run = game
        .load_animations("ped")
        .unwrap()
        .into_iter()
        .find(|animation| animation.name == "run_player")
        .unwrap();
    let body = Pes6::open(&pes6)
        .unwrap()
        .load_model(&"0_text:1010".parse().unwrap(), None)
        .unwrap();
    Some((tommy, run, body))
}

fn joint(model: &Model, worlds: &[Mat4], name: &str) -> Vec3 {
    let node = model.nodes.iter().position(|n| n.name == name).unwrap();
    worlds[node].w_axis.truncate()
}

#[test]
fn run_player_stands_in_the_common_frame() {
    let Some((tommy, run, _)) = models() else {
        return;
    };
    let common = Quat::from_array(VICE_CITY_ANIMATION_TO_COMMON);
    let worlds = pose(&tommy, &run, 0.0);
    let at = |name| common * joint(&tommy, &worlds, name);
    // Up is +Y, the left side +X, as in the bind pose.
    assert!(at("Head").y > at("Pelvis").y + 0.5, "{:?}", at("Head"));
    assert!(at("L Thigh").x > at("R Thigh").x, "{:?}", at("L Thigh"));
    // It runs forwards, towards +Z.
    let end = pose(&tommy, &run, run.duration);
    let travel = common * joint(&tommy, &end, "Pelvis") - at("Pelvis");
    assert!(travel.z > 3.0, "{travel:?}");
}

#[test]
fn run_player_plays_on_a_pes_body() {
    let Some((tommy, run, body)) = models() else {
        return;
    };
    let moved = retarget(
        &run,
        &tommy,
        &body,
        PES6_FROM_VICE_CITY,
        VICE_CITY_ANIMATION_TO_COMMON,
        true,
    )
    .unwrap();
    assert_eq!(moved.tracks.len(), PES6_FROM_VICE_CITY.len());
    let (mut lowest, mut highest_foot) = (f32::MAX, f32::MIN);
    let mut left_ahead = Vec::new();
    for step in 0..=20 {
        let time = run.duration * step as f32 / 20.0;
        let worlds = pose(&body, &moved, time);
        let at = |name| joint(&body, &worlds, name);
        // Upright: the head over the pelvis, the pelvis over the feet (a
        // foot kicked up behind still stays 0.4 m below it, as Tommy's).
        assert!(
            at("head").y > at("pelvis").y + 150.0,
            "{time} : {:?}",
            at("head")
        );
        for foot in ["left foot", "right foot"] {
            assert!(at("pelvis").y > at(foot).y + 150.0, "{time} : {foot}");
            lowest = lowest.min(at(foot).y);
            highest_foot = highest_foot.max(at(foot).y);
        }
        // The legs stay on their side.
        assert!(at("left thigh").x > at("right thigh").x, "{time}");
        // In place: the pelvis does not travel.
        assert!(at("pelvis").z.abs() < 120.0, "{time} : {:?}", at("pelvis"));
        left_ahead.push(at("left foot").z > at("right foot").z);
    }
    // The ankles come down to the ground (41 units in the bind pose) and
    // lift off it while running.
    assert!((-20.0..120.0).contains(&lowest), "{lowest}");
    assert!(highest_foot > lowest + 100.0, "{lowest} .. {highest_foot}");
    // The feet take turns in front.
    assert!(left_ahead.contains(&true) && left_ahead.contains(&false));
}

#[test]
fn pes_body_mesh_follows_its_bones_while_running() {
    let Some((tommy, run, body)) = models() else {
        return;
    };
    let moved = retarget(
        &run,
        &tommy,
        &body,
        PES6_FROM_VICE_CITY,
        VICE_CITY_ANIMATION_TO_COMMON,
        true,
    )
    .unwrap();
    let skeleton = body.skeleton.as_ref().unwrap();
    // No edge of the skinned mesh stretches by more than 25 cm: every
    // vertex follows bones next to it (a vertex left on a wrong bone, such
    // as a hand on the pelvis, stretches by nearly a metre).
    let mut worst = 0.0f32;
    for step in 0..8 {
        let worlds = pose(&body, &moved, run.duration * step as f32 / 8.0);
        for mesh in &body.meshes {
            let skin = mesh.skin.as_ref().unwrap();
            let skinned: Vec<Vec3> = mesh
                .positions
                .iter()
                .zip(&skin.joints)
                .zip(&skin.weights)
                .map(|((position, joints), weights)| {
                    joints
                        .iter()
                        .zip(weights)
                        .map(|(&joint, &weight)| {
                            let bone = &skeleton.bones[usize::from(joint)];
                            let m = worlds[bone.node] * Mat4::from_cols_array(&bone.inverse_bind);
                            weight * m.transform_point3(Vec3::from_array(*position))
                        })
                        .sum()
                })
                .collect();
            for triangle in mesh.primitives.iter().flat_map(|p| p.indices.chunks(3)) {
                for (a, b) in [(0, 1), (1, 2), (2, 0)] {
                    let (a, b) = (triangle[a] as usize, triangle[b] as usize);
                    let bind = Vec3::from_array(mesh.positions[a])
                        .distance(Vec3::from_array(mesh.positions[b]));
                    worst = worst.max(skinned[a].distance(skinned[b]) - bind);
                }
            }
        }
    }
    assert!(worst < 0.25 * 420.0, "étirement de {worst} unités");
}
