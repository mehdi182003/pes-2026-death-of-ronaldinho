//! The pitch: its dimensions and its goals. Grass, lines, boards and stands
//! come from the PES stadium (see `football::spawn_stadium`).
//!
//! Dimensions are those of the Laws of the Game (IFAB), which the lines of
//! the PES stadium follow (105 × 68 m). The pitch lies along X (goals at
//! x = ±52.5 m), touchlines at z = ±34 m, Y up, in metres.

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

pub const LENGTH: f32 = 105.0;
pub const WIDTH: f32 = 68.0;
/// Between the inner edges of the posts.
pub const GOAL_WIDTH: f32 = 7.32;
/// From the ground to the lower edge of the crossbar.
pub const GOAL_HEIGHT: f32 = 2.44;
pub const POST_RADIUS: f32 = 0.06;
/// How far the net goes behind the goal line, at the ground.
pub const GOAL_DEPTH: f32 = 2.0;

/// x of the goal line at the end of the pitch `side` (-1 or +1).
pub fn goal_line(side: f32) -> f32 {
    side * LENGTH / 2.0
}

/// Spawns both goals: posts and crossbar on the goal lines, nets behind
/// with `net` as their texture (the net of the PES stadium).
pub fn spawn_goals(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    net: Option<Handle<Image>>,
) {
    let white = materials.add(StandardMaterial {
        base_color: Color::srgb(0.97, 0.97, 0.97),
        perceptual_roughness: 0.4,
        ..default()
    });
    let net = materials.add(StandardMaterial {
        base_color: if net.is_some() {
            Color::WHITE
        } else {
            Color::srgba(0.95, 0.95, 0.95, 0.35)
        },
        base_color_texture: net,
        alpha_mode: AlphaMode::Blend,
        cull_mode: None,
        unlit: true,
        ..default()
    });
    let half = GOAL_WIDTH / 2.0 + POST_RADIUS;
    let height = GOAL_HEIGHT + 2.0 * POST_RADIUS;
    let post = meshes.add(Cylinder::new(POST_RADIUS, height));
    let bar = meshes.add(Cylinder::new(POST_RADIUS, 2.0 * half));
    for side in [-1.0f32, 1.0] {
        // The posts stand on the line: their back edge is on its outer edge.
        let x = goal_line(side) + side * POST_RADIUS;
        for z in [-half, half] {
            commands.spawn((
                Mesh3d(post.clone()),
                MeshMaterial3d(white.clone()),
                Transform::from_xyz(x, height / 2.0, z),
            ));
        }
        commands.spawn((
            Mesh3d(bar.clone()),
            MeshMaterial3d(white.clone()),
            Transform::from_xyz(x, GOAL_HEIGHT + POST_RADIUS, 0.0)
                .with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)),
        ));
        for corners in net_panels(side) {
            commands.spawn((
                Mesh3d(meshes.add(quad(corners))),
                MeshMaterial3d(net.clone()),
            ));
        }
    }
}

/// The four panels of the net at the end `side`, as quads: a roof sloping
/// down from the crossbar, the back, and two sides.
pub fn net_panels(side: f32) -> [[Vec3; 4]; 4] {
    let line = goal_line(side);
    let top_back = line + side * GOAL_DEPTH * 0.5;
    let back = line + side * GOAL_DEPTH;
    let half = GOAL_WIDTH / 2.0 + POST_RADIUS;
    let h = GOAL_HEIGHT;
    let h_back = GOAL_HEIGHT * 0.75;
    [
        [
            Vec3::new(line, h, -half),
            Vec3::new(line, h, half),
            Vec3::new(top_back, h_back, half),
            Vec3::new(top_back, h_back, -half),
        ],
        [
            Vec3::new(top_back, h_back, -half),
            Vec3::new(top_back, h_back, half),
            Vec3::new(back, 0.0, half),
            Vec3::new(back, 0.0, -half),
        ],
        [
            Vec3::new(line, 0.0, -half),
            Vec3::new(line, h, -half),
            Vec3::new(top_back, h_back, -half),
            Vec3::new(back, 0.0, -half),
        ],
        [
            Vec3::new(line, 0.0, half),
            Vec3::new(line, h, half),
            Vec3::new(top_back, h_back, half),
            Vec3::new(back, 0.0, half),
        ],
    ]
}

/// Size of one tile of the net texture, in metres.
// HYPOTHÈSE: the PES net texture (0_text:6949/1/43, 128 × 128) shows about
// three meshes across; real meshes are 12 cm wide.
const NET_TILE: f32 = 0.4;

/// A flat quad through four corners, in order, its texture repeated every
/// [`NET_TILE`] metres.
fn quad(corners: [Vec3; 4]) -> Mesh {
    let normal = (corners[1] - corners[0])
        .cross(corners[3] - corners[0])
        .normalize_or_zero();
    let u = (corners[1] - corners[0]).normalize_or_zero();
    let v = normal.cross(u);
    let uvs: Vec<[f32; 2]> = corners
        .iter()
        .map(|c| {
            let d = *c - corners[0];
            [d.dot(u) / NET_TILE, d.dot(v) / NET_TILE]
        })
        .collect();
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(
        Mesh::ATTRIBUTE_POSITION,
        corners.iter().map(|c| c.to_array()).collect::<Vec<_>>(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![normal.to_array(); 4])
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
    .with_inserted_indices(Indices::U32(vec![0, 1, 2, 0, 2, 3]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_net_hangs_behind_the_goal_line() {
        for side in [-1.0, 1.0] {
            let line = goal_line(side);
            for panel in net_panels(side) {
                assert!(
                    panel
                        .iter()
                        .all(|c| (c.x - line) * side >= 0.0 && c.y <= GOAL_HEIGHT)
                );
            }
        }
    }
}
