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
    /// Bone ID used by animations (Vice City: HAnim ID), if this node is a
    /// bone.
    pub bone_id: Option<i32>,
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
    /// How the vertices follow the bones of [`Model::skeleton`].
    pub skin: Option<MeshSkin>,
}

/// Per vertex: up to four bones (indices into [`Skeleton::bones`]) and
/// their weights, which add up to 1. Unused slots have a zero weight and
/// index 0.
#[derive(Debug, Clone, PartialEq)]
pub struct MeshSkin {
    pub joints: Vec<[u16; 4]>,
    pub weights: Vec<[f32; 4]>,
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

/// A decoded texture: 4 bytes per pixel (R, G, B, A, sRGB), rows from the
/// top. Materials refer to it by name, ignoring case like the game.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Texture {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub rgba8: Vec<u8>,
}

impl Texture {
    /// Whether every pixel is fully opaque.
    pub fn is_opaque(&self) -> bool {
        self.rgba8
            .as_chunks::<4>()
            .0
            .iter()
            .all(|pixel| pixel[3] == 255)
    }
}

/// A skeletal animation, independent of any model.
#[derive(Debug, Clone, PartialEq)]
pub struct Animation {
    pub name: String,
    /// Seconds.
    pub duration: f32,
    pub tracks: Vec<Track>,
}

/// Key frames of one bone.
#[derive(Debug, Clone, PartialEq)]
pub struct Track {
    pub bone_name: String,
    /// When the file gives one; otherwise the bone is found by name.
    pub bone_id: Option<i32>,
    pub keys: Vec<Key>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Key {
    /// Seconds from the start.
    pub time: f32,
    /// Rotation relative to the parent bone, quaternion (x, y, z, w).
    pub rotation: [f32; 4],
    /// Position relative to the parent bone; when absent, the bone keeps
    /// the position of its node.
    pub translation: Option<[f32; 3]>,
    pub scale: Option<[f32; 3]>,
}

impl Track {
    /// Node of `model` driven by this track: by bone ID, else by name
    /// (ignoring case).
    pub fn node_in(&self, model: &Model) -> Option<usize> {
        self.bone_id
            .and_then(|id| model.nodes.iter().position(|node| node.bone_id == Some(id)))
            .or_else(|| {
                model
                    .nodes
                    .iter()
                    .position(|node| node.name.eq_ignore_ascii_case(&self.bone_name))
            })
    }
}

/// A sound: signed 16-bit mono samples.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sound {
    pub sample_rate: u32,
    pub samples: Vec<i16>,
}

impl Sound {
    /// Length in seconds.
    pub fn duration(&self) -> f32 {
        self.samples.len() as f32 / self.sample_rate.max(1) as f32
    }

    /// The sound as a WAV file, for players that decode files.
    pub fn to_wav(&self) -> Vec<u8> {
        formats_rw::sfx::wav_bytes(self.sample_rate, &self.samples)
    }
}

/// A firearm, with everything needed to hold it and shoot.
#[derive(Debug, Clone, PartialEq)]
pub struct Weapon {
    pub name: String,
    pub model: Model,
    pub textures: Vec<Texture>,
    /// Arm animation of the shot, played over the body animation: it only
    /// moves the bones it has tracks for.
    pub fire_animation: Animation,
    /// Loop start, loop end and firing instant in `fire_animation`, in
    /// seconds: holding the trigger loops between the first two, a round is
    /// fired at the third.
    pub fire_loop: [f32; 3],
    /// Where the shot leaves the weapon, in the weapon's space.
    pub muzzle: [f32; 3],
    /// Range of a shot, in metres.
    pub range: f32,
    pub fire_sound: Sound,
}
