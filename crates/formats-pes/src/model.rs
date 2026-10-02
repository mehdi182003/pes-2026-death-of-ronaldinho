//! PES 6 models (signature `20 05 04 20`): vertex parts, one triangle
//! strip of part-local indices, and a small draw program that cuts the
//! strip into draws, each with its part, bone and texture.
//!
//! Layout checked on the 9647 models of the full PES 6 PC game (9546
//! read), see
//! `docs/formats/pes-model.md`.

use crate::content::MODEL_MAGIC;

/// Offset of the draw program (the u32 at offset 24 of every model).
const PROGRAM_START: usize = 0x48;

/// How the vertices of a part are laid out: position, then the optional
/// fields, then the texture coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VertexFormat {
    /// Bytes per vertex.
    pub stride: u8,
    /// Bones per vertex: 0, 1 (bone numbers only) or 2 to 4 (weights and
    /// bone numbers).
    pub bones: u8,
    pub has_normal: bool,
    pub has_color: bool,
}

impl VertexFormat {
    /// From the stride and flags of a part header. The low four bits of the
    /// flags give the bones per vertex; what is left of the stride once the
    /// position (12 bytes), the bones and the texture coordinates (8 bytes)
    /// are counted says whether there is a normal (12 bytes), a colour
    /// (4 bytes) or both.
    // HYPOTHÈSE: bit 0x20 of the flags (stadium parts) has no effect on the
    // layout; the 12 formats of the game all follow this rule.
    pub fn new(stride: u8, flags: u8) -> Option<Self> {
        let bones = flags & 0x0f;
        let bone_bytes = match bones {
            0 => 0,
            1 => 4,
            2..=4 => 8,
            _ => return None,
        };
        let (has_normal, has_color) = match usize::from(stride).checked_sub(20 + bone_bytes)? {
            0 => (false, false),
            4 => (false, true),
            12 => (true, false),
            16 => (true, true),
            _ => return None,
        };
        Some(Self {
            stride,
            bones,
            has_normal,
            has_color,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vertex {
    pub position: [f32; 3],
    /// Not of unit length in every model (length 8 in faces).
    pub normal: Option<[f32; 3]>,
    /// R, G, B, A.
    pub color: Option<[u8; 4]>,
    pub uv: [f32; 2],
    /// Bone numbers, indices into the bone table of the draws that use the
    /// vertex (see [`PesModel::skeleton_bone`]). Unused slots are 0.
    pub joints: [u8; 4],
    /// Out of 255; `[255, 0, 0, 0]` when a vertex follows a single bone.
    pub weights: [u8; 4],
}

#[derive(Debug, Clone, PartialEq)]
pub struct VertexPart {
    pub format: VertexFormat,
    pub vertices: Vec<Vertex>,
}

/// A run of the triangle strip, drawn with one part and one texture.
#[derive(Debug, Clone, PartialEq)]
pub struct Draw {
    /// Index into [`PesModel::parts`].
    pub part: usize,
    /// Texture number given by the program (opcode `02`).
    // HYPOTHÈSE: a texture slot of the model, filled by the game with the
    // kit, skin, boots... It goes from 0 to 10 on a player.
    pub texture: u16,
    /// Argument of the last opcode `0a` before the draw.
    // HYPOTHÈSE: not a bone of the skeleton (it goes up to 49 on bodies
    // with 19 bones); perhaps a batch of the PlayStation 2 renderer.
    pub group: u16,
    /// Index into [`PesModel::bone_tables`] of the last table (opcode `03`)
    /// before the draw, if any.
    pub bone_table: Option<usize>,
    /// Triangles, counter-clockwise, as indices into the part's vertices.
    pub triangles: Vec<[u16; 3]>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PesModel {
    pub parts: Vec<VertexPart>,
    pub draws: Vec<Draw>,
    /// Empty for models that do not move with bones (faces, balls...).
    pub bones: Vec<Bone>,
    /// Tables of the draw program (opcode `03`): entry `j` is the skeleton
    /// bone of the vertices whose bone number is `j`. Checked on the 573
    /// bodies with 19 bones: one table each, a permutation of the 19 bones,
    /// and 99.9 % of the vertices that follow one bone lie by it.
    pub bone_tables: Vec<Vec<u8>>,
}

impl PesModel {
    /// The skeleton bone (index into [`PesModel::bones`]) that bone number
    /// `joint` of a vertex drawn by `draw` stands for.
    pub fn skeleton_bone(&self, draw: &Draw, joint: u8) -> Option<usize> {
        let table = self.bone_tables.get(draw.bone_table?)?;
        let bone = usize::from(*table.get(usize::from(joint))?);
        (bone < self.bones.len()).then_some(bone)
    }
}

/// A bone of the skeleton, in the bind pose (T-pose).
///
/// The record gives the transform from model space to the bone's space:
/// a point `p` of the model is at `rotation · p + translation` for the
/// bone. The joint is at `-rotationᵀ · translation` (the head of a
/// player's body: (0, 670.6, -0.7)).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bone {
    /// Euler angles, radians, around X, Y and Z.
    pub angles: [f32; 3],
    pub translation: [f32; 3],
    /// Index of the parent bone; `None` for the root.
    pub parent: Option<usize>,
}

impl Bone {
    /// The rotation part of the record, as a row-major 3 × 3 matrix:
    /// `Rz · Ry · Rx` (X first).
    // HYPOTHÈSE: of the twelve possible orders, several put the joints
    // where the mesh has them, but only half of them turn an attached head
    // the way the body faces (nose towards +Z, like the toes); Ry · Rz · Rx,
    // tried first, turned it backwards. With this order every joint of a
    // body falls in place once the vertices' bone numbers go through the
    // bone table (hips, knees, ankles, spine, shoulders, elbows, wrists,
    // neck, head).
    pub fn rotation(&self) -> [[f32; 3]; 3] {
        let [x, y, z] = self.angles;
        let rx = [
            [1.0, 0.0, 0.0],
            [0.0, x.cos(), -x.sin()],
            [0.0, x.sin(), x.cos()],
        ];
        let ry = [
            [y.cos(), 0.0, y.sin()],
            [0.0, 1.0, 0.0],
            [-y.sin(), 0.0, y.cos()],
        ];
        let rz = [
            [z.cos(), -z.sin(), 0.0],
            [z.sin(), z.cos(), 0.0],
            [0.0, 0.0, 1.0],
        ];
        multiply(&multiply(&rz, &ry), &rx)
    }

    /// Where the joint is in the model, in the bind pose.
    pub fn joint(&self) -> [f32; 3] {
        let r = self.rotation();
        let t = self.translation;
        std::array::from_fn(|i| -(r[0][i] * t[0] + r[1][i] * t[1] + r[2][i] * t[2]))
    }

    /// The record itself, from the model to the bone's space, as a
    /// column-major 4 × 4 matrix: the inverse bind matrix of skinning.
    pub fn inverse_bind_matrix(&self) -> [f32; 16] {
        let r = self.rotation();
        let t = self.translation;
        [
            r[0][0], r[1][0], r[2][0], 0.0, //
            r[0][1], r[1][1], r[2][1], 0.0, //
            r[0][2], r[1][2], r[2][2], 0.0, //
            t[0], t[1], t[2], 1.0,
        ]
    }

    /// Transform from the bone's space to the model, as a column-major 4 × 4
    /// matrix (the inverse of the record).
    pub fn bind_matrix(&self) -> [f32; 16] {
        let r = self.rotation();
        let joint = self.joint();
        // Columns of the inverse rotation are the rows of `r`.
        [
            r[0][0], r[0][1], r[0][2], 0.0, //
            r[1][0], r[1][1], r[1][2], 0.0, //
            r[2][0], r[2][1], r[2][2], 0.0, //
            joint[0], joint[1], joint[2], 1.0,
        ]
    }
}

fn multiply(a: &[[f32; 3]; 3], b: &[[f32; 3]; 3]) -> [[f32; 3]; 3] {
    std::array::from_fn(|i| std::array::from_fn(|j| (0..3).map(|k| a[i][k] * b[k][j]).sum()))
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ModelError {
    #[error("pas un modèle PES (signature {0:02x?})")]
    BadMagic([u8; 4]),

    #[error("modèle tronqué : {0}")]
    Truncated(&'static str),

    #[error("partie {part} : format de sommet inconnu (taille {stride}, drapeaux {flags:#04x})")]
    UnknownVertexFormat { part: usize, stride: u8, flags: u8 },

    #[error("programme de dessin : instruction inconnue {opcode:#04x} à l'octet {at}")]
    UnknownOpcode { opcode: u8, at: usize },

    #[error("programme de dessin : {0}")]
    BadProgram(String),
}

/// A little-endian reader over the file that fails instead of panicking.
struct Bytes<'a>(&'a [u8]);

impl Bytes<'_> {
    fn get(&self, at: usize, len: usize, what: &'static str) -> Result<&[u8], ModelError> {
        self.0
            .get(at..at.checked_add(len).ok_or(ModelError::Truncated(what))?)
            .ok_or(ModelError::Truncated(what))
    }

    fn u16(&self, at: usize, what: &'static str) -> Result<u16, ModelError> {
        Ok(u16::from_le_bytes(
            self.get(at, 2, what)?.try_into().unwrap(),
        ))
    }

    fn u32(&self, at: usize, what: &'static str) -> Result<usize, ModelError> {
        Ok(u32::from_le_bytes(self.get(at, 4, what)?.try_into().unwrap()) as usize)
    }

    fn f32s<const N: usize>(&self, at: usize, what: &'static str) -> Result<[f32; N], ModelError> {
        let bytes = self.get(at, 4 * N, what)?;
        Ok(std::array::from_fn(|i| {
            f32::from_le_bytes(bytes[4 * i..4 * i + 4].try_into().unwrap())
        }))
    }
}

pub fn parse(bytes: &[u8]) -> Result<PesModel, ModelError> {
    let file = Bytes(bytes);
    let magic: [u8; 4] = file.get(0, 4, "signature")?.try_into().unwrap();
    if magic != MODEL_MAGIC {
        return Err(ModelError::BadMagic(magic));
    }
    // Offsets of the header: end of the program, vertex parts, index strip
    // (start and end). The other u32 of the header are not used yet.
    let program_end = file.u32(4, "en-tête")?;
    let vertices_at = file.u32(16, "en-tête")?;
    let strip_at = file.u32(20, "en-tête")?;
    let strip_end = file.u32(28, "en-tête")?;

    let parts = parse_parts(&file, vertices_at)?;
    let strip = parse_strip(&file, strip_at, strip_end)?;
    let program = file.get(
        PROGRAM_START,
        program_end
            .checked_sub(PROGRAM_START)
            .ok_or(ModelError::Truncated("programme"))?,
        "programme",
    )?;
    let (draws, bone_tables) = run_program(program, &parts, &strip)?;
    let bones = parse_bones(&file, program_end)?;
    Ok(PesModel {
        parts,
        draws,
        bones,
        bone_tables,
    })
}

/// The skeleton, right after the draw program: a count (u32), then per bone
/// three Euler angles and a translation (6 × f32), then the parents (i16
/// per bone, -1 for the root). Checked on the models of the full game: most
/// have no bone; the 594 bodies of players and referees have 19.
fn parse_bones(file: &Bytes, at: usize) -> Result<Vec<Bone>, ModelError> {
    let count = file.u32(at, "squelette")?;
    let parents_at = at + 4 + 24 * count;
    (0..count)
        .map(|index| {
            let record = file.f32s::<6>(at + 4 + 24 * index, "squelette")?;
            let parent = file.u16(parents_at + 2 * index, "squelette")? as i16;
            let parent = match usize::try_from(parent) {
                Ok(parent) if parent < index => Some(parent),
                Err(_) if parent == -1 => None,
                _ => {
                    return Err(ModelError::BadProgram(format!(
                        "os {index} : parent {parent} invalide"
                    )));
                }
            };
            Ok(Bone {
                angles: [record[0], record[1], record[2]],
                translation: [record[3], record[4], record[5]],
                parent,
            })
        })
        .collect()
}

/// Vertex parts: a count, one offset per part (from the start of the
/// section), and at each offset the number of vertices (u16), the stride
/// (u8) and the flags (u8), 4 unused bytes, then the vertices.
fn parse_parts(file: &Bytes, at: usize) -> Result<Vec<VertexPart>, ModelError> {
    let count = file.u32(at, "sommets")?;
    let mut parts = Vec::with_capacity(count.min(1024));
    for part in 0..count {
        let start = at + file.u32(at + 4 + 4 * part, "sommets")?;
        let vertex_count = usize::from(file.u16(start, "sommets")?);
        let [stride, flags] = file.get(start + 2, 2, "sommets")?.try_into().unwrap();
        let format = VertexFormat::new(stride, flags).ok_or(ModelError::UnknownVertexFormat {
            part,
            stride,
            flags,
        })?;
        let first = start + 8;
        let vertices = (0..vertex_count)
            .map(|index| read_vertex(file, first + index * usize::from(stride), format))
            .collect::<Result<_, _>>()?;
        parts.push(VertexPart { format, vertices });
    }
    Ok(parts)
}

fn read_vertex(file: &Bytes, at: usize, format: VertexFormat) -> Result<Vertex, ModelError> {
    let position = file.f32s::<3>(at, "sommet")?;
    let mut field = at + 12;
    let mut weights = [255, 0, 0, 0];
    let mut joints = [0; 4];
    if format.bones >= 2 {
        weights = file.get(field, 4, "sommet")?.try_into().unwrap();
        field += 4;
    }
    if format.bones >= 1 {
        joints = file.get(field, 4, "sommet")?.try_into().unwrap();
        field += 4;
    }
    let normal = if format.has_normal {
        field += 12;
        Some(file.f32s::<3>(field - 12, "sommet")?)
    } else {
        None
    };
    let color = if format.has_color {
        field += 4;
        Some(file.get(field - 4, 4, "sommet")?.try_into().unwrap())
    } else {
        None
    };
    let uv = file.f32s::<2>(field, "sommet")?;
    Ok(Vertex {
        position,
        normal,
        color,
        uv,
        joints,
        weights,
    })
}

/// The index strip: runs of a count (u16) and that many indices, until a
/// count of zero or the end of the section. Draws use them in order.
fn parse_strip(file: &Bytes, mut at: usize, end: usize) -> Result<Vec<u16>, ModelError> {
    let mut strip = Vec::new();
    while at + 2 <= end {
        let count = usize::from(file.u16(at, "indices")?);
        if count == 0 {
            break;
        }
        let indices = file.get(at + 2, 2 * count, "indices")?;
        strip.extend(
            indices
                .as_chunks::<2>()
                .0
                .iter()
                .map(|pair| u16::from_le_bytes(*pair)),
        );
        at += 2 + 2 * count;
    }
    Ok(strip)
}

/// Runs the draw program. Instructions are 2-byte aligned; the first byte
/// is the opcode:
///
/// - `00`: nothing (padding, 2 bytes);
/// - `01`, `0d`: unknown, one u16 argument (4 bytes);
/// - `02`: texture of the next draws (4 bytes);
/// - `03`: a bone table of `n` bytes (`03 00 n`, then the bytes, then
///   padding), see [`PesModel::bone_tables`];
/// - `04`: vertex part of the next draws, with its flags (`04 flags u16`);
/// - `07`: draw (`07 mode`, then u16 index count, first vertex, vertex count
///   and triangle count): the next `index count` indices of the strip,
///   which stay among the given vertices;
/// - `0a`: unknown, one u16 argument kept as [`Draw::group`] (4 bytes).
fn run_program(
    program: &[u8],
    parts: &[VertexPart],
    strip: &[u16],
) -> Result<(Vec<Draw>, Vec<Vec<u8>>), ModelError> {
    let code = Bytes(program);
    let mut draws = Vec::new();
    let mut tables = Vec::new();
    let (mut at, mut used) = (0, 0);
    let (mut part, mut texture, mut group) = (None, 0, 0);
    while at < program.len() {
        let opcode = program[at];
        match opcode {
            0x00 => at += 2,
            0x01 | 0x0d => at += 4,
            0x02 => {
                texture = code.u16(at + 2, "programme")?;
                at += 4;
            }
            0x03 => {
                let len = usize::from(code.get(at + 2, 1, "programme")?[0]);
                tables.push(code.get(at + 3, len, "programme")?.to_vec());
                at += 3 + len;
                at += at % 2;
            }
            0x04 => {
                let index = usize::from(code.u16(at + 2, "programme")?);
                if index >= parts.len() {
                    return Err(ModelError::BadProgram(format!(
                        "partie {index} inexistante"
                    )));
                }
                part = Some(index);
                at += 4;
            }
            0x07 => {
                let field = |n: usize| code.u16(at + 2 + 2 * n, "programme").map(usize::from);
                let (count, first, vertex_count, triangle_count) =
                    (field(0)?, field(1)?, field(2)?, field(3)?);
                let part = part.ok_or_else(|| {
                    ModelError::BadProgram("dessin avant le choix d'une partie".into())
                })?;
                let run = strip
                    .get(used..used + count)
                    .ok_or_else(|| ModelError::BadProgram("bande d'indices trop courte".into()))?;
                used += count;
                let available = parts[part].vertices.len();
                if first + vertex_count > available
                    || run
                        .iter()
                        .any(|&i| !(first..first + vertex_count).contains(&usize::from(i)))
                {
                    return Err(ModelError::BadProgram(format!(
                        "indices hors des sommets {first}..+{vertex_count} de la partie {part}"
                    )));
                }
                let triangles = strip_triangles(run);
                if triangles.len() != triangle_count {
                    return Err(ModelError::BadProgram(format!(
                        "{} triangles au lieu des {triangle_count} annoncés",
                        triangles.len()
                    )));
                }
                draws.push(Draw {
                    part,
                    texture,
                    group,
                    bone_table: tables.len().checked_sub(1),
                    triangles,
                });
                at += 10;
            }
            0x0a => {
                group = code.u16(at + 2, "programme")?;
                at += 4;
            }
            _ => return Err(ModelError::UnknownOpcode { opcode, at }),
        }
    }
    if used != strip.len() {
        return Err(ModelError::BadProgram(format!(
            "{} indices sur {} utilisés",
            used,
            strip.len()
        )));
    }
    Ok((draws, tables))
}

/// Triangles of a strip, skipping the degenerate ones that join runs.
/// Every other triangle is flipped to keep one winding.
// HYPOTHÈSE: the first triangle is counter-clockwise; to be checked by eye
// (back faces culled).
fn strip_triangles(strip: &[u16]) -> Vec<[u16; 3]> {
    strip
        .windows(3)
        .enumerate()
        .filter(|(_, w)| w[0] != w[1] && w[1] != w[2] && w[0] != w[2])
        .map(|(i, w)| {
            if i % 2 == 0 {
                [w[0], w[1], w[2]]
            } else {
                [w[1], w[0], w[2]]
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vertex_formats_follow_the_stride_and_flags() {
        let format = |stride, flags| VertexFormat::new(stride, flags).unwrap();
        assert_eq!(
            format(32, 0),
            VertexFormat {
                stride: 32,
                bones: 0,
                has_normal: true,
                has_color: false
            }
        );
        assert!(!format(24, 0).has_normal && format(24, 0).has_color);
        assert_eq!(format(36, 1).bones, 1);
        assert!(format(36, 1).has_normal);
        assert!(format(40, 3).has_normal && !format(40, 3).has_color);
        assert!(format(28, 1).has_color);
        assert!(format(36, 0x20).has_normal && format(36, 0x20).has_color);
        assert_eq!(VertexFormat::new(30, 0), None);
        assert_eq!(VertexFormat::new(40, 7), None);
    }

    #[test]
    fn strips_skip_degenerate_triangles_and_keep_one_winding() {
        assert_eq!(
            strip_triangles(&[0, 1, 2, 3, 3, 4, 4, 5, 6]),
            [[0, 1, 2], [2, 1, 3], [4, 5, 6]]
        );
    }

    /// A model with one part of three 32-byte vertices and one draw.
    fn tiny_model() -> Vec<u8> {
        let mut program = Vec::new();
        program.extend([0x02, 0, 5, 0]); // texture 5
        program.extend([0x03, 0, 2, 7, 8, 0]); // table of 2 bytes, padded
        program.extend([0x04, 0, 0, 0]); // part 0
        program.extend([0x0a, 0, 2, 0]); // bone 2
        program.extend([0x07, 0x05, 3, 0, 0, 0, 3, 0, 1, 0]); // draw
        program.extend([0, 0]);
        let program_end = PROGRAM_START + program.len();
        // An empty skeleton right after the program.
        let vertices_at = (program_end + 4).next_multiple_of(4);
        let mut part = vec![1, 0, 0, 0, 8, 0, 0, 0, 3, 0, 32, 0, 0, 0, 0, 0];
        for i in 0..3 {
            for value in [i as f32, 0.0, 1.0, 0.0, 1.0, 0.0, 0.5, 0.25] {
                part.extend(value.to_le_bytes());
            }
        }
        let strip_at = vertices_at + part.len();
        let strip: Vec<u8> = [3u16, 0, 1, 2]
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect();
        let strip_end = strip_at + strip.len();

        let mut bytes = vec![0u8; PROGRAM_START];
        bytes[..4].copy_from_slice(&MODEL_MAGIC);
        for (at, value) in [
            (4, program_end),
            (16, vertices_at),
            (20, strip_at),
            (24, PROGRAM_START),
            (28, strip_end),
        ] {
            bytes[at..at + 4].copy_from_slice(&(value as u32).to_le_bytes());
        }
        bytes.extend(program);
        bytes.resize(vertices_at, 0);
        bytes.extend(part);
        bytes.extend(strip);
        bytes
    }

    #[test]
    fn reads_parts_strip_and_draws() {
        let model = parse(&tiny_model()).unwrap();
        assert_eq!(model.parts.len(), 1);
        let vertex = model.parts[0].vertices[2];
        assert_eq!(vertex.position, [2.0, 0.0, 1.0]);
        assert_eq!(vertex.normal, Some([0.0, 1.0, 0.0]));
        assert_eq!(vertex.uv, [0.5, 0.25]);
        assert_eq!(vertex.weights, [255, 0, 0, 0]);
        assert_eq!(
            model.draws,
            [Draw {
                part: 0,
                texture: 5,
                group: 2,
                bone_table: Some(0),
                triangles: vec![[0, 1, 2]],
            }]
        );
        assert_eq!(model.bone_tables, [vec![7, 8]]);
        assert!(model.bones.is_empty());
    }

    #[test]
    fn vertex_bone_numbers_go_through_the_bone_table() {
        let mut model = parse(&tiny_model()).unwrap();
        let bone = Bone {
            angles: [0.0; 3],
            translation: [0.0; 3],
            parent: None,
        };
        model.bones = vec![bone; 9];
        let draw = model.draws[0].clone();
        assert_eq!(model.skeleton_bone(&draw, 0), Some(7));
        assert_eq!(model.skeleton_bone(&draw, 1), Some(8));
        // Past the end of the table, or a table entry past the skeleton.
        assert_eq!(model.skeleton_bone(&draw, 2), None);
        model.bones.truncate(8);
        assert_eq!(model.skeleton_bone(&draw, 1), None);
        let untabled = Draw {
            bone_table: None,
            ..draw
        };
        assert_eq!(model.skeleton_bone(&untabled, 0), None);
    }

    #[test]
    fn bone_records_give_their_joint() {
        use std::f32::consts::FRAC_PI_2;
        // The head bone of a player's body: model space to bone space.
        let head = Bone {
            angles: [0.0, -FRAC_PI_2, -std::f32::consts::PI],
            translation: [0.713, 670.576, -0.002],
            parent: Some(11),
        };
        let joint = head.joint();
        assert!((joint[1] - 670.576).abs() < 0.01, "{joint:?}");
        assert!(
            joint[0].abs() < 0.01 && (joint[2] + 0.713).abs() < 0.01,
            "{joint:?}"
        );
        // The bind matrix takes the bone's origin to the joint, and the
        // bone's +X (where the nose of a head points) to the model's +Z,
        // the way the body faces.
        let m = head.bind_matrix();
        assert_eq!([m[12], m[13], m[14]], joint);
        assert!((m[2] - 1.0).abs() < 1e-5, "{m:?}");

        // The record and the bind matrix undo each other.
        let apply = |m: &[f32; 16], p: [f32; 3]| -> [f32; 3] {
            std::array::from_fn(|i| m[i] * p[0] + m[4 + i] * p[1] + m[8 + i] * p[2] + m[12 + i])
        };
        let point = [1.0, 2.0, 3.0];
        let back = apply(&head.inverse_bind_matrix(), apply(&m, point));
        assert!(
            back.iter().zip(point).all(|(a, b)| (a - b).abs() < 1e-3),
            "{back:?}"
        );
    }

    #[test]
    fn rejects_inconsistent_programs() {
        let mut bytes = tiny_model();
        // Announce two triangles instead of one.
        let draw = PROGRAM_START + 4 + 6 + 4 + 4;
        bytes[draw + 8] = 2;
        assert!(matches!(parse(&bytes), Err(ModelError::BadProgram(_))));

        let mut bytes = tiny_model();
        bytes[PROGRAM_START] = 0x11;
        assert_eq!(
            parse(&bytes),
            Err(ModelError::UnknownOpcode {
                opcode: 0x11,
                at: 0
            })
        );
    }
}
