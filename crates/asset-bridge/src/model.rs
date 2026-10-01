//! Engine-neutral 3D models, built from the games' files.
//!
//! Coordinates stay in the space and units of the source game (Vice City:
//! metres; characters stand along +Y in their bind pose). Changing axes is
//! up to the consumer, or to the `retarget` crate.

/// Column-major 4x4 matrix, the convention of glam and Bevy.
pub type Mat4 = [f32; 16];

#[derive(Debug, Clone, PartialEq)]
pub struct Model {
    pub name: String,
    /// Transform hierarchy. A parent always comes before its children.
    pub nodes: Vec<Node>,
    pub meshes: Vec<Mesh>,
    /// Present for skinned models (characters).
    pub skeleton: Option<Skeleton>,
}

impl Model {
    pub fn vertex_count(&self) -> usize {
        self.meshes.iter().map(|mesh| mesh.positions.len()).sum()
    }

    pub fn triangle_count(&self) -> usize {
        self.meshes
            .iter()
            .flat_map(|mesh| &mesh.primitives)
            .map(|primitive| primitive.indices.len() / 3)
            .sum()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub name: String,
    pub parent: Option<usize>,
    /// Transform relative to the parent.
    pub local: Mat4,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Mesh {
    /// Node whose transform places the mesh.
    pub node: usize,
    pub positions: Vec<[f32; 3]>,
    /// Missing in some prelit geometries (city objects): to be computed.
    pub normals: Option<Vec<[f32; 3]>>,
    pub uvs: Option<Vec<[f32; 2]>>,
    /// Baked vertex colours, sRGB, from 0 to 1.
    pub colors: Option<Vec<[f32; 4]>>,
    /// One triangle list per material.
    pub primitives: Vec<Primitive>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Primitive {
    pub material: Material,
    /// Triangle list, counter-clockwise front faces.
    pub indices: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Material {
    /// sRGB colour and alpha, from 0 to 1.
    pub base_color: [f32; 4],
    /// Name of the texture in the game's texture dictionary.
    pub texture: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Skeleton {
    /// Bones in skinning order.
    pub bones: Vec<Bone>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Bone {
    pub name: String,
    /// Node moved by this bone.
    pub node: usize,
    /// From model space to bone space, in the bind pose. Its inverse places
    /// the bone where the mesh expects it.
    pub inverse_bind: Mat4,
}
