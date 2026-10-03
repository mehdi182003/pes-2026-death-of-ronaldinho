//! Tommy's model turned into batches PES's Direct3D 8 device can draw:
//! one triangle list per material, in PES's logic coordinates.
//!
//! The model comes from the player's Vice City install through
//! `asset_bridge` (the same readers as the Rust engine), in its bind pose:
//! metres, standing along +Y, facing +Z (docs/BRIEF.md, J6).

use asset_bridge::model::{self, Model};

/// A vertex for `D3DFVF_XYZ | D3DFVF_DIFFUSE | D3DFVF_TEX1` (24 bytes).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TexturedVertex {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    /// ARGB.
    pub color: u32,
    pub u: f32,
    pub v: f32,
}

/// `D3DFVF_XYZ | D3DFVF_DIFFUSE | D3DFVF_TEX1`.
pub const FVF: u32 = 0x0002 | 0x0040 | 0x0100;

// HYPOTHÈSE: where Tommy waits, in logic coordinates: on the halfway line,
// 2 m behind the touchline on the camera's side. The touchline is taken at
// Z = 8729, the off-pitch position of every player slot before kick-off
// (34 m × 256.5 ≈ 8721); the match camera films from +Z (M3 log).
pub const SIDELINE_SPOT: [f32; 3] = [0.0, 0.0, 8729.0 + 2.0 * 256.5];

/// One draw call: a triangle list with 16-bit indices and one texture.
#[derive(Debug, Clone, PartialEq)]
pub struct Batch {
    pub vertices: Vec<TexturedVertex>,
    pub indices: Vec<u16>,
    /// Name in Tommy's texture dictionary.
    pub texture: Option<String>,
    /// Blended by the texture's alpha; otherwise alpha below one half is cut.
    pub blend: bool,
}

// HYPOTHÈSE: from Vice City's bind pose (Y up, facing +Z) to PES's logic
// coordinates (Y down, M3): Y and Z are negated, a half turn about X, so the
// model is not mirrored and faces −Z, towards the pitch from the near
// touchline. Whether PES's own chain mirrors its models is to be checked in
// game (M4b: the gun in Tommy's right hand).
fn to_logic([x, y, z]: [f32; 3], units_per_metre: f32, at: [f32; 3]) -> [f32; 3] {
    [
        at[0] + x * units_per_metre,
        at[1] - y * units_per_metre,
        at[2] - z * units_per_metre,
    ]
}

fn argb([r, g, b, a]: [f32; 4]) -> u32 {
    let c = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u32;
    (c(a) << 24) | (c(r) << 16) | (c(g) << 8) | c(b)
}

/// Positions of a mesh in model space, bind pose: skinned meshes are already
/// in model space; the others are placed by their node.
fn model_space_positions(model: &Model, mesh: &model::Mesh) -> Vec<[f32; 3]> {
    if mesh.skin.is_some() {
        mesh.positions.clone()
    } else {
        let world = model.node_world(mesh.node);
        mesh.positions
            .iter()
            .map(|&p| model::transform_point(&world, p))
            .collect()
    }
}

/// Lowest point of the model in its bind pose (the soles of the feet).
fn lowest_y(model: &Model) -> f32 {
    model
        .meshes
        .iter()
        .flat_map(|mesh| model_space_positions(model, mesh))
        .map(|[_, y, _]| y)
        .fold(f32::INFINITY, f32::min)
}

/// Batches for `model` standing with its feet at `at` (logic coordinates).
pub fn batches(model: &Model, at: [f32; 3], units_per_metre: f32) -> Vec<Batch> {
    let floor = lowest_y(model);
    let mut out = Vec::new();
    for mesh in &model.meshes {
        let positions = model_space_positions(model, mesh);
        for primitive in &mesh.primitives {
            // Keep only the vertices this material uses, renumbered.
            let mut remap = std::collections::HashMap::new();
            let mut vertices = Vec::new();
            let mut indices = Vec::with_capacity(primitive.indices.len());
            for &index in &primitive.indices {
                let index = index as usize;
                let new = *remap.entry(index).or_insert_with(|| {
                    let [x, y, z] = positions[index];
                    let [lx, ly, lz] = to_logic([x, y - floor, z], units_per_metre, at);
                    let [u, v] = mesh.uvs.as_ref().map_or([0.0, 0.0], |uvs| uvs[index]);
                    let tint = mesh
                        .colors
                        .as_ref()
                        .map_or([1.0; 4], |colors| colors[index]);
                    let base = primitive.material.base_color;
                    vertices.push(TexturedVertex {
                        x: lx,
                        y: ly,
                        z: lz,
                        color: argb(std::array::from_fn(|i| base[i] * tint[i])),
                        u,
                        v,
                    });
                    vertices.len() - 1
                });
                indices.push(new as u16);
            }
            if vertices.len() > usize::from(u16::MAX) {
                continue;
            }
            out.push(Batch {
                vertices,
                indices,
                texture: primitive.material.texture.clone(),
                blend: primitive.material.blend,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use asset_bridge::model::{Material, Mesh, Node, Primitive};

    /// A 2 m tall quad, from y = −1 to 1, made of two materials.
    fn model() -> Model {
        let material = |texture: &str| Material {
            base_color: [1.0, 1.0, 1.0, 1.0],
            texture: Some(texture.into()),
            blend: false,
            layer: 0,
        };
        Model {
            name: "test".into(),
            nodes: vec![Node {
                name: "root".into(),
                parent: None,
                local: model::IDENTITY,
                bone_id: None,
            }],
            meshes: vec![Mesh {
                node: 0,
                positions: vec![
                    [0.0, -1.0, 0.0],
                    [1.0, -1.0, 0.0],
                    [1.0, 1.0, 0.0],
                    [0.0, 1.0, 0.5],
                ],
                normals: None,
                uvs: Some(vec![[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]]),
                colors: None,
                primitives: vec![
                    Primitive {
                        material: material("shirt"),
                        indices: vec![0, 1, 2],
                    },
                    Primitive {
                        material: material("face"),
                        indices: vec![0, 2, 3],
                    },
                ],
                skin: None,
            }],
            skeleton: None,
        }
    }

    #[test]
    fn vertex_matches_the_fvf_stride() {
        assert_eq!(std::mem::size_of::<TexturedVertex>(), 24);
    }

    #[test]
    fn one_batch_per_material() {
        let b = batches(&model(), [0.0; 3], 100.0);
        assert_eq!(b.len(), 2);
        assert_eq!(b[0].texture.as_deref(), Some("shirt"));
        assert_eq!(b[0].indices, vec![0, 1, 2]);
        assert_eq!(b[1].vertices.len(), 3);
        assert_eq!(b[0].vertices[1].u, 1.0);
        assert_eq!(b[0].vertices[0].color, 0xffff_ffff);
    }

    #[test]
    fn stands_on_the_ground_up_the_negative_y() {
        let b = batches(&model(), [10.0, 0.0, 20.0], 100.0);
        let ys: Vec<f32> = b.iter().flat_map(|b| &b.vertices).map(|v| v.y).collect();
        // Feet on the ground (y = 0), head 2 m up, i.e. at −200 in a Y-down world.
        assert_eq!(ys.iter().copied().fold(f32::MIN, f32::max), 0.0);
        assert_eq!(ys.iter().copied().fold(f32::MAX, f32::min), -200.0);
        // Z is turned with Y (half turn about X), and the model is moved to `at`.
        let top = b[1]
            .vertices
            .iter()
            .find(|v| v.y == -200.0 && v.x == 10.0)
            .unwrap();
        assert_eq!(top.z, 20.0 - 50.0);
    }
}
