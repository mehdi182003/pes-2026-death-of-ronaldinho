//! DFF models: a RenderWare Clump with its frame hierarchy, geometries and
//! atomics.
//!
//! Layout checked on every DFF of Vice City's `models/gta3.img`
//! (RenderWare 3.2.0.0, 3.3.0.2 and 3.4.0.3), see `docs/formats/dff.md`.

use binrw::{BinRead, binread};
use tracing::debug;

use crate::rw::{
    Chunk, RwError, Version, check_count, expect_child, id, parse_exact, parse_prefix, root_chunk,
};
use crate::text::latin1_until_nul;

/// A parsed DFF file.
#[derive(Debug, Clone, PartialEq)]
pub struct Clump {
    /// RenderWare version of the root chunk.
    pub version: Version,
    pub frames: Vec<Frame>,
    pub geometries: Vec<Geometry>,
    pub atomics: Vec<Atomic>,
    /// Lights attached to the clump. They are not decoded.
    pub light_count: u32,
}

/// Affine transform as stored by RenderWare: three basis vectors and a
/// position. It applies to column vectors: `world = parent * local`.
#[derive(Debug, Clone, Copy, PartialEq, BinRead)]
#[br(little)]
pub struct Matrix {
    pub right: [f32; 3],
    pub up: [f32; 3],
    pub at: [f32; 3],
    pub position: [f32; 3],
}

impl Matrix {
    pub const IDENTITY: Matrix = Matrix {
        right: [1.0, 0.0, 0.0],
        up: [0.0, 1.0, 0.0],
        at: [0.0, 0.0, 1.0],
        position: [0.0, 0.0, 0.0],
    };

    /// Column-major 4x4 matrix: the columns are right, up, at and position.
    pub fn to_cols_array(&self) -> [f32; 16] {
        let [r, u, a, p] = [self.right, self.up, self.at, self.position];
        [
            r[0], r[1], r[2], 0.0, u[0], u[1], u[2], 0.0, a[0], a[1], a[2], 0.0, p[0], p[1], p[2],
            1.0,
        ]
    }

    /// Applies the linear part (rotation and scale) to a vector.
    pub fn transform_vector(&self, v: [f32; 3]) -> [f32; 3] {
        std::array::from_fn(|i| self.right[i] * v[0] + self.up[i] * v[1] + self.at[i] * v[2])
    }

    pub fn transform_point(&self, p: [f32; 3]) -> [f32; 3] {
        let v = self.transform_vector(p);
        std::array::from_fn(|i| v[i] + self.position[i])
    }

    /// `self * other`: the transform that applies `other`, then `self`.
    pub fn mul(&self, other: &Matrix) -> Matrix {
        Matrix {
            right: self.transform_vector(other.right),
            up: self.transform_vector(other.up),
            at: self.transform_vector(other.at),
            position: self.transform_point(other.position),
        }
    }
}

/// A node of the frame hierarchy (a bone, for characters).
#[derive(Debug, Clone, PartialEq)]
pub struct Frame {
    /// Transform relative to the parent frame.
    pub transform: Matrix,
    /// Always an earlier frame.
    pub parent: Option<usize>,
    /// RwMatrix creation flags, kept as stored. Meaning not needed so far.
    pub matrix_flags: u32,
    /// From Rockstar's Node Name extension.
    pub name: Option<String>,
    pub hanim: Option<HAnim>,
}

/// HAnim PLG extension of a frame: bone ID and, on the root bone, the
/// hierarchy used by skinning and animations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HAnim {
    /// 0x100 in every Vice City file.
    pub version: u32,
    /// Bone ID (`-1` on frames that are not bones).
    pub node_id: i32,
    pub hierarchy: Option<HAnimHierarchy>,
}

#[derive(Debug, Clone, PartialEq, Eq, BinRead)]
#[br(little, import(node_count: u32))]
pub struct HAnimHierarchy {
    /// RpHAnimHierarchyFlag; 0 in Vice City.
    pub flags: u32,
    /// Bytes per animation key frame; 36 in Vice City.
    pub key_frame_size: u32,
    /// Bones in skinning order: bone `i` of a Skin PLG is `nodes[i]`, found
    /// among the frames by its ID. This order differs from the frame order.
    #[br(count = node_count)]
    pub nodes: Vec<HAnimNode>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, BinRead)]
#[br(little)]
pub struct HAnimNode {
    pub id: i32,
    pub index: i32,
    /// RpHAnimNodeFlag: 1 = pop the parent matrix (last child), 2 = push it.
    pub flags: u32,
}

/// Bits of [`Geometry::format`] (RpGeometryFlag).
pub mod geometry_flags {
    pub const TRISTRIP: u32 = 0x01;
    pub const POSITIONS: u32 = 0x02;
    pub const TEXTURED: u32 = 0x04;
    pub const PRELIT: u32 = 0x08;
    pub const NORMALS: u32 = 0x10;
    pub const LIGHT: u32 = 0x20;
    pub const MODULATE_MATERIAL_COLOR: u32 = 0x40;
    pub const TEXTURED2: u32 = 0x80;
    pub const NATIVE: u32 = 0x0100_0000;
}

/// A mesh: vertices, triangles and materials.
#[derive(Debug, Clone, PartialEq)]
pub struct Geometry {
    /// [`geometry_flags`] bits; bits 16-23 hold the number of texture
    /// coordinate sets.
    pub format: u32,
    pub vertex_count: usize,
    /// Only stored before RenderWare 3.4.
    pub surface: Option<SurfaceProperties>,
    /// Baked vertex colours (RGBA), when the geometry is prelit.
    pub prelit_colors: Option<Vec<[u8; 4]>>,
    pub tex_coord_sets: Vec<Vec<[f32; 2]>>,
    pub triangles: Vec<Triangle>,
    /// Always exactly one in Vice City.
    pub morph_targets: Vec<MorphTarget>,
    pub materials: Vec<Material>,
    pub skin: Option<Skin>,
}

/// A triangle, vertices in counter-clockwise order seen from the front.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Triangle {
    pub vertices: [u16; 3],
    /// Index into [`Geometry::materials`].
    pub material: u16,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MorphTarget {
    /// Centre (x, y, z) and radius.
    pub bounding_sphere: [f32; 4],
    pub vertices: Option<Vec<[f32; 3]>>,
    pub normals: Option<Vec<[f32; 3]>>,
}

#[derive(Debug, Clone, Copy, PartialEq, BinRead)]
#[br(little)]
pub struct SurfaceProperties {
    pub ambient: f32,
    pub specular: f32,
    pub diffuse: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Material {
    /// RGBA.
    pub color: [u8; 4],
    /// Present from RenderWare 3.4.0.1 on (all Vice City files).
    pub surface: Option<SurfaceProperties>,
    pub texture: Option<Texture>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Texture {
    /// Filtering and addressing modes, kept as stored.
    // HYPOTHÈSE: filtering in bits 0-7, U addressing in bits 8-11, V
    // addressing in bits 12-15 (GTAMods wiki). Plausible values on
    // player.dff, not checked visually yet: textures arrive in J2.
    pub sampler: u32,
    /// Name of the texture in the TXD dictionary.
    pub name: String,
    /// Name of the alpha mask texture, often empty.
    pub mask_name: String,
}

/// Binds a frame to a geometry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Atomic {
    pub frame: usize,
    pub geometry: usize,
    /// 0x01 collision test, 0x04 render.
    pub flags: u32,
}

/// Skin PLG: how each vertex follows up to four bones.
#[derive(Debug, Clone, PartialEq)]
pub struct Skin {
    pub bone_count: u8,
    /// Bones that actually influence the mesh (empty in old files).
    pub used_bones: Vec<u8>,
    /// 0 in RenderWare 3.2 and 3.3 files, 3 or 4 in 3.4 files.
    pub max_weights_per_vertex: u8,
    /// Per vertex: four indices into the hierarchy of the HAnim PLG.
    pub bone_indices: Vec<[u8; 4]>,
    /// Per vertex: the weights of those bones (sum = 1).
    pub weights: Vec<[f32; 4]>,
    /// Per bone: transform from the model to the bone, in bind pose.
    pub inverse_bind_matrices: Vec<Matrix>,
}

/// Parses a DFF file. Bytes after the root chunk (IMG padding) are ignored.
pub fn parse_dff(bytes: &[u8]) -> Result<Clump, RwError> {
    let root = root_chunk(bytes)?;
    if root.kind() != id::CLUMP {
        return Err(RwError::UnexpectedChunk {
            context: "DFF",
            offset: root.offset,
            expected: id::CLUMP,
            found: root.kind(),
        });
    }
    parse_clump(&root)
}

fn parse_clump(clump: &Chunk<'_>) -> Result<Clump, RwError> {
    const CONTEXT: &str = "Clump";
    let mut children = clump.children();

    // 4 bytes (atomic count) up to RenderWare 3.3.0.0, then 12 (atomic,
    // light and camera counts). In gta3.img: 12 bytes in all 3.3.0.2 and
    // 3.4.0.3 files, 4 bytes in the five 3.2.0.0 ones. The size decides.
    let counts = expect_child(&mut children, clump, id::STRUCT, CONTEXT)?;
    let read_u32 = |at: usize| u32::from_le_bytes(counts.data[at..at + 4].try_into().unwrap());
    let (atomic_count, light_count) = match counts.data.len() {
        4 => (read_u32(0), 0),
        12 => (read_u32(0), read_u32(4)),
        len => {
            return Err(RwError::Invalid {
                context: CONTEXT,
                offset: counts.offset,
                message: format!("struct de {len} octets (4 ou 12 attendus)"),
            });
        }
    };

    let frames = parse_frame_list(&expect_child(
        &mut children,
        clump,
        id::FRAME_LIST,
        CONTEXT,
    )?)?;
    let geometries = parse_geometry_list(&expect_child(
        &mut children,
        clump,
        id::GEOMETRY_LIST,
        CONTEXT,
    )?)?;

    let mut atomics = Vec::new();
    for child in children {
        let child = child?;
        match child.kind() {
            id::ATOMIC => atomics.push(parse_atomic(&child, frames.len(), geometries.len())?),
            // Lights come as (Struct, Light) pairs; the extension is unused.
            id::STRUCT | id::LIGHT | id::EXTENSION => {}
            other => debug!(kind = other, offset = child.offset, "Clump: chunk ignoré"),
        }
    }
    if atomics.len() != atomic_count as usize {
        return Err(RwError::Invalid {
            context: CONTEXT,
            offset: clump.offset,
            message: format!("{} atomics trouvés, {atomic_count} annoncés", atomics.len()),
        });
    }

    Ok(Clump {
        version: clump.version(),
        frames,
        geometries,
        atomics,
        light_count,
    })
}

#[binread]
#[br(little)]
struct RawFrameList {
    #[br(temp)]
    count: u32,
    #[br(count = count)]
    frames: Vec<RawFrame>,
}

/// 56 bytes per frame. The GTAMods wiki says 0x44, but its own field table
/// adds up to 56, and so do the real files.
#[derive(BinRead)]
#[br(little)]
struct RawFrame {
    transform: Matrix,
    parent: i32,
    matrix_flags: u32,
}

#[binread]
#[br(little)]
struct RawHAnim {
    version: u32,
    node_id: i32,
    #[br(temp)]
    node_count: u32,
    #[br(if(node_count > 0), args(node_count))]
    hierarchy: Option<HAnimHierarchy>,
}

fn parse_frame_list(list: &Chunk<'_>) -> Result<Vec<Frame>, RwError> {
    const CONTEXT: &str = "Frame List";
    let mut children = list.children();
    let data = expect_child(&mut children, list, id::STRUCT, CONTEXT)?;
    let count = data
        .data
        .get(..4)
        .map_or(0, |b| u32::from_le_bytes(b.try_into().unwrap()));
    check_count(&data, CONTEXT, count)?;
    let raw: RawFrameList = parse_exact(&data, CONTEXT, ())?;

    let mut frames = Vec::with_capacity(raw.frames.len());
    for (index, raw_frame) in raw.frames.into_iter().enumerate() {
        let parent = match raw_frame.parent {
            -1 => None,
            p if p >= 0 && (p as usize) < index => Some(p as usize),
            p => {
                return Err(RwError::Invalid {
                    context: CONTEXT,
                    offset: data.offset,
                    message: format!("la frame {index} a pour parent {p}"),
                });
            }
        };
        frames.push(Frame {
            transform: raw_frame.transform,
            parent,
            matrix_flags: raw_frame.matrix_flags,
            name: None,
            hanim: None,
        });
    }

    // One Extension per frame, in the same order.
    for (frame, extension) in frames.iter_mut().zip(children) {
        let extension = extension?;
        if extension.kind() != id::EXTENSION {
            return Err(RwError::UnexpectedChunk {
                context: CONTEXT,
                offset: extension.offset,
                expected: id::EXTENSION,
                found: extension.kind(),
            });
        }
        for plugin in extension.children() {
            let plugin = plugin?;
            match plugin.kind() {
                id::NODE_NAME => frame.name = Some(latin1_until_nul(plugin.data)),
                id::HANIM_PLG => {
                    let raw: RawHAnim = parse_exact(&plugin, "HAnim PLG", ())?;
                    frame.hanim = Some(HAnim {
                        version: raw.version,
                        node_id: raw.node_id,
                        hierarchy: raw.hierarchy,
                    });
                }
                other => debug!(
                    kind = other,
                    offset = plugin.offset,
                    "Frame: extension ignorée"
                ),
            }
        }
    }
    Ok(frames)
}

fn parse_geometry_list(list: &Chunk<'_>) -> Result<Vec<Geometry>, RwError> {
    const CONTEXT: &str = "Geometry List";
    let mut children = list.children();
    let count = expect_child(&mut children, list, id::STRUCT, CONTEXT)?;
    let count = parse_exact::<u32>(&count, CONTEXT, ())?;
    (0..count)
        .map(|_| parse_geometry(&expect_child(&mut children, list, id::GEOMETRY, CONTEXT)?))
        .collect()
}

#[binread]
#[br(little, import(version: Version))]
struct RawGeometry {
    format: u32,
    #[br(temp)]
    triangle_count: u32,
    vertex_count: u32,
    #[br(temp)]
    morph_target_count: u32,
    #[br(if(version.0 < 0x3_4000))]
    surface: Option<SurfaceProperties>,
    #[br(if(format & geometry_flags::NATIVE == 0 && format & geometry_flags::PRELIT != 0), count = vertex_count)]
    prelit_colors: Option<Vec<[u8; 4]>>,
    #[br(
        if(format & geometry_flags::NATIVE == 0),
        count = tex_coord_set_count(format),
        args { inner: (vertex_count,) }
    )]
    tex_coord_sets: Vec<TexCoordSet>,
    #[br(if(format & geometry_flags::NATIVE == 0), count = triangle_count)]
    triangles: Vec<RawTriangle>,
    #[br(count = morph_target_count, args { inner: (vertex_count,) })]
    morph_targets: Vec<RawMorphTarget>,
}

/// Number of texture coordinate sets: bits 16-23 of the format, or, when
/// they are zero, deduced from the TEXTURED / TEXTURED2 flags.
fn tex_coord_set_count(format: u32) -> u32 {
    match (format >> 16) & 0xFF {
        0 if format & geometry_flags::TEXTURED2 != 0 => 2,
        0 if format & geometry_flags::TEXTURED != 0 => 1,
        count => count,
    }
}

#[derive(BinRead)]
#[br(little, import(vertex_count: u32))]
struct TexCoordSet(#[br(count = vertex_count)] Vec<[f32; 2]>);

/// Stored as (vertex 2, vertex 1, material, vertex 3), following the wiki's
/// naming. Reordered as (vertex 1, vertex 2, vertex 3), the triangles of
/// player.dff face the same way as their vertex normals (counter-clockwise
/// front faces) for 1341 of 1355 triangles; the rest are thin details.
#[derive(BinRead)]
#[br(little)]
struct RawTriangle {
    vertex2: u16,
    vertex1: u16,
    material: u16,
    vertex3: u16,
}

#[binread]
#[br(little, import(vertex_count: u32))]
struct RawMorphTarget {
    bounding_sphere: [f32; 4],
    #[br(temp)]
    has_vertices: u32,
    #[br(temp)]
    has_normals: u32,
    #[br(if(has_vertices != 0), count = vertex_count)]
    vertices: Option<Vec<[f32; 3]>>,
    #[br(if(has_normals != 0), count = vertex_count)]
    normals: Option<Vec<[f32; 3]>>,
}

fn parse_geometry(geometry: &Chunk<'_>) -> Result<Geometry, RwError> {
    const CONTEXT: &str = "Geometry";
    let mut children = geometry.children();
    let data = expect_child(&mut children, geometry, id::STRUCT, CONTEXT)?;
    let header: [u32; 4] = parse_prefix(&data, CONTEXT, ())?.0;
    let [format, triangle_count, vertex_count, morph_target_count] = header;
    if format & geometry_flags::NATIVE != 0 {
        return Err(RwError::Invalid {
            context: CONTEXT,
            offset: data.offset,
            message: "géométrie native (format console), non gérée".into(),
        });
    }
    for count in [triangle_count, vertex_count, morph_target_count] {
        check_count(&data, CONTEXT, count)?;
    }
    let raw: RawGeometry = parse_exact(&data, CONTEXT, (geometry.version(),))?;

    let materials = parse_material_list(&expect_child(
        &mut children,
        geometry,
        id::MATERIAL_LIST,
        CONTEXT,
    )?)?;

    let vertex_count = raw.vertex_count as usize;
    let mut triangles = Vec::with_capacity(raw.triangles.len());
    for raw_triangle in raw.triangles {
        let triangle = Triangle {
            vertices: [
                raw_triangle.vertex1,
                raw_triangle.vertex2,
                raw_triangle.vertex3,
            ],
            material: raw_triangle.material,
        };
        if triangle
            .vertices
            .iter()
            .any(|&v| usize::from(v) >= vertex_count)
            || usize::from(triangle.material) >= materials.len()
        {
            return Err(RwError::Invalid {
                context: CONTEXT,
                offset: data.offset,
                message: format!("triangle hors limites : {triangle:?}"),
            });
        }
        triangles.push(triangle);
    }

    let mut skin = None;
    if let Some(extension) = children.next() {
        let extension = extension?;
        for plugin in extension.children() {
            let plugin = plugin?;
            match plugin.kind() {
                id::SKIN_PLG => skin = Some(parse_skin(&plugin, raw.vertex_count)?),
                other => {
                    debug!(
                        kind = other,
                        offset = plugin.offset,
                        "Geometry: extension ignorée"
                    )
                }
            }
        }
    }

    Ok(Geometry {
        format: raw.format,
        vertex_count,
        surface: raw.surface,
        prelit_colors: raw.prelit_colors,
        tex_coord_sets: raw.tex_coord_sets.into_iter().map(|set| set.0).collect(),
        triangles,
        morph_targets: raw
            .morph_targets
            .into_iter()
            .map(|target| MorphTarget {
                bounding_sphere: target.bounding_sphere,
                vertices: target.vertices,
                normals: target.normals,
            })
            .collect(),
        materials,
        skin,
    })
}

#[binread]
#[br(little)]
struct RawMaterialList {
    #[br(temp)]
    count: u32,
    /// -1 for a new material, otherwise the index of an earlier material
    /// this one is an instance of.
    #[br(count = count)]
    indices: Vec<i32>,
}

fn parse_material_list(list: &Chunk<'_>) -> Result<Vec<Material>, RwError> {
    const CONTEXT: &str = "Material List";
    let mut children = list.children();
    let data = expect_child(&mut children, list, id::STRUCT, CONTEXT)?;
    let count = data
        .data
        .get(..4)
        .map_or(0, |b| u32::from_le_bytes(b.try_into().unwrap()));
    check_count(&data, CONTEXT, count)?;
    let raw: RawMaterialList = parse_exact(&data, CONTEXT, ())?;

    let mut materials: Vec<Material> = Vec::with_capacity(raw.indices.len());
    for index in raw.indices {
        let material = match index {
            -1 => parse_material(&expect_child(&mut children, list, id::MATERIAL, CONTEXT)?)?,
            i if i >= 0 && (i as usize) < materials.len() => materials[i as usize].clone(),
            i => {
                return Err(RwError::Invalid {
                    context: CONTEXT,
                    offset: data.offset,
                    message: format!("instance du matériau {i}, pas encore défini"),
                });
            }
        };
        materials.push(material);
    }
    Ok(materials)
}

#[derive(BinRead)]
#[br(little, import(version: Version))]
struct RawMaterial {
    _flags: u32,
    color: [u8; 4],
    _unused: u32,
    is_textured: u32,
    #[br(if(version.0 > 0x3_0400))]
    surface: Option<SurfaceProperties>,
}

fn parse_material(material: &Chunk<'_>) -> Result<Material, RwError> {
    const CONTEXT: &str = "Material";
    let mut children = material.children();
    let data = expect_child(&mut children, material, id::STRUCT, CONTEXT)?;
    let raw: RawMaterial = parse_exact(&data, CONTEXT, (material.version(),))?;
    let texture = if raw.is_textured != 0 {
        Some(parse_texture(&expect_child(
            &mut children,
            material,
            id::TEXTURE,
            CONTEXT,
        )?)?)
    } else {
        None
    };
    Ok(Material {
        color: raw.color,
        surface: raw.surface,
        texture,
    })
}

fn parse_texture(texture: &Chunk<'_>) -> Result<Texture, RwError> {
    const CONTEXT: &str = "Texture";
    let mut children = texture.children();
    let sampler = parse_exact::<u32>(
        &expect_child(&mut children, texture, id::STRUCT, CONTEXT)?,
        CONTEXT,
        (),
    )?;
    // Strings are NUL-terminated and padded to 4 bytes.
    let name = expect_child(&mut children, texture, id::STRING, CONTEXT)?;
    let mask_name = expect_child(&mut children, texture, id::STRING, CONTEXT)?;
    Ok(Texture {
        sampler,
        name: latin1_until_nul(name.data),
        mask_name: latin1_until_nul(mask_name.data),
    })
}

#[derive(BinRead)]
#[br(little)]
struct RawAtomic {
    frame: u32,
    geometry: u32,
    flags: u32,
    _unused: u32,
}

fn parse_atomic(
    atomic: &Chunk<'_>,
    frame_count: usize,
    geometry_count: usize,
) -> Result<Atomic, RwError> {
    const CONTEXT: &str = "Atomic";
    let mut children = atomic.children();
    let data = expect_child(&mut children, atomic, id::STRUCT, CONTEXT)?;
    let raw: RawAtomic = parse_exact(&data, CONTEXT, ())?;
    let (frame, geometry) = (raw.frame as usize, raw.geometry as usize);
    if frame >= frame_count || geometry >= geometry_count {
        return Err(RwError::Invalid {
            context: CONTEXT,
            offset: data.offset,
            message: format!("frame {frame} ou géométrie {geometry} inexistante"),
        });
    }
    Ok(Atomic {
        frame,
        geometry,
        flags: raw.flags,
    })
}

#[binread]
#[br(little, import(vertex_count: u32, version: Version))]
struct RawSkin {
    bone_count: u8,
    #[br(temp)]
    used_bone_count: u8,
    max_weights_per_vertex: u8,
    _padding: u8,
    #[br(count = used_bone_count)]
    used_bones: Vec<u8>,
    #[br(count = vertex_count)]
    bone_indices: Vec<[u8; 4]>,
    #[br(count = vertex_count)]
    weights: Vec<[f32; 4]>,
    /// Each matrix is preceded by a marker (0xDEADDEAD) in old files. The
    /// wiki's rule (version < 3.7 and no max weight) matches all 275 skinned
    /// models of gta3.img.
    #[br(
        count = bone_count,
        args { inner: (version.0 < 0x3_7000 && max_weights_per_vertex == 0,) }
    )]
    inverse_bind_matrices: Vec<SkinMatrix>,
}

/// A 4x4 matrix stored as four rows: right, up, at and position, each
/// followed by a fourth float that is padding (it holds leftover values).
/// Checked on 270 of the 275 skinned models of gta3.img: inverting these
/// matrices gives back the bind pose of their bones' frames.
#[derive(BinRead)]
#[br(little, import(has_marker: bool))]
struct SkinMatrix {
    #[br(if(has_marker))]
    _marker: Option<u32>,
    rows: [[f32; 4]; 4],
}

fn parse_skin(skin: &Chunk<'_>, vertex_count: u32) -> Result<Skin, RwError> {
    const CONTEXT: &str = "Skin PLG";
    let (raw, consumed): (RawSkin, usize) =
        parse_prefix(skin, CONTEXT, (vertex_count, skin.version()))?;
    // RenderWare 3.4 files end with three u32 (bone limit, group count and
    // remap count), all zero in Vice City; older files end right here.
    let rest = &skin.data[consumed..];
    if !(rest.is_empty() || rest == [0; 12]) {
        return Err(RwError::Invalid {
            context: CONTEXT,
            offset: skin.offset,
            message: format!("{} octets finaux non reconnus", rest.len()),
        });
    }
    let row = |r: [f32; 4]| [r[0], r[1], r[2]];
    Ok(Skin {
        bone_count: raw.bone_count,
        used_bones: raw.used_bones,
        max_weights_per_vertex: raw.max_weights_per_vertex,
        bone_indices: raw.bone_indices,
        weights: raw.weights,
        inverse_bind_matrices: raw
            .inverse_bind_matrices
            .into_iter()
            .map(|m| Matrix {
                right: row(m.rows[0]),
                up: row(m.rows[1]),
                at: row(m.rows[2]),
                position: row(m.rows[3]),
            })
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    //! Synthetic DFF files built in code: no game data.

    use super::*;
    use crate::rw::test_util::*;

    const IDENTITY_ROWS: [f32; 12] = [1., 0., 0., 0., 1., 0., 0., 0., 1., 0., 0., 0.];
    /// 90° around Z, then 1 unit up: right = +Y, up = -X.
    const BONE_ROWS: [f32; 12] = [0., 1., 0., -1., 0., 0., 0., 0., 1., 0., 0., 1.];
    /// Inverse of BONE_ROWS.
    const BONE_INVERSE_ROWS: [f32; 12] = [0., -1., 0., 1., 0., 0., 0., 0., 1., 0., 0., -1.];

    fn frame_list(lib: u32) -> Vec<u8> {
        let data = Payload::default()
            .u32(2)
            .f32s(&IDENTITY_ROWS)
            .i32(-1)
            .u32(0x20003)
            .f32s(&BONE_ROWS)
            .i32(0)
            .u32(0);
        let root_hanim = Payload::default().u32(0x100).i32(-1).u32(0);
        // The bone carries the hierarchy: one node, ID 7.
        let bone_hanim = Payload::default()
            .u32(0x100)
            .i32(7)
            .u32(1)
            .u32(0)
            .u32(36)
            .i32(7)
            .i32(0)
            .u32(1);
        container(
            id::FRAME_LIST,
            lib,
            &[
                chunk(id::STRUCT, lib, &data.0),
                container(
                    id::EXTENSION,
                    lib,
                    &[
                        chunk(id::NODE_NAME, lib, b"Root"),
                        chunk(id::HANIM_PLG, lib, &root_hanim.0),
                    ],
                ),
                container(
                    id::EXTENSION,
                    lib,
                    &[
                        chunk(id::NODE_NAME, lib, b"Pelvis"),
                        chunk(id::HANIM_PLG, lib, &bone_hanim.0),
                    ],
                ),
            ],
        )
    }

    fn material_list(lib: u32) -> Vec<u8> {
        let material = Payload::default()
            .u32(0)
            .bytes(&[255, 128, 0, 255])
            .u32(0)
            .u32(1)
            .f32s(&[1.0, 0.5, 1.0]);
        let texture = container(
            id::TEXTURE,
            lib,
            &[
                chunk(id::STRUCT, lib, &0x1106u32.to_le_bytes()),
                chunk(id::STRING, lib, b"tex\0"),
                chunk(id::STRING, lib, &[0; 4]),
                container(id::EXTENSION, lib, &[]),
            ],
        );
        // Two entries: a material, then an instance of it.
        let list = Payload::default().u32(2).i32(-1).i32(0);
        container(
            id::MATERIAL_LIST,
            lib,
            &[
                chunk(id::STRUCT, lib, &list.0),
                container(
                    id::MATERIAL,
                    lib,
                    &[
                        chunk(id::STRUCT, lib, &material.0),
                        texture,
                        container(id::EXTENSION, lib, &[]),
                    ],
                ),
            ],
        )
    }

    /// Skin of one bone. Old files: a marker before each matrix. RenderWare
    /// 3.4 files: a list of used bones, no marker, 12 zero bytes at the end.
    fn skin(lib: u32) -> Vec<u8> {
        let old = Version::from_library_id(lib) < Version(0x3_4000);
        let mut skin = if old {
            Payload::default().bytes(&[1, 0, 0, 0])
        } else {
            Payload::default().bytes(&[1, 1, 4, 0, 0])
        };
        for _ in 0..3 {
            skin = skin.bytes(&[0, 0, 0, 0]);
        }
        for _ in 0..3 {
            skin = skin.f32s(&[1.0, 0.0, 0.0, 0.0]);
        }
        if old {
            skin = skin.u32(0xDEAD_DEAD);
        }
        for row in BONE_INVERSE_ROWS.chunks(3) {
            // The fourth float of each row is padding with leftover values.
            skin = skin.f32s(row).f32s(&[1.0e34]);
        }
        if !old {
            skin = skin.u32(0).u32(0).u32(0);
        }
        chunk(id::SKIN_PLG, lib, &skin.0)
    }

    /// One triangle (0, 1, 2) in the XY plane, facing +Z.
    fn geometry(lib: u32, triangle: [u16; 4]) -> Vec<u8> {
        let format = geometry_flags::POSITIONS
            | geometry_flags::TEXTURED
            | geometry_flags::PRELIT
            | geometry_flags::NORMALS
            | geometry_flags::LIGHT
            | (1 << 16);
        let mut data = Payload::default().u32(format).u32(1).u32(3).u32(1);
        if Version::from_library_id(lib) < Version(0x3_4000) {
            data = data.f32s(&[1.0, 1.0, 1.0]);
        }
        data = data
            .bytes(&[10, 20, 30, 255, 10, 20, 30, 255, 10, 20, 30, 255])
            .f32s(&[0.0, 0.0, 1.0, 0.0, 0.0, 1.0]);
        for index in triangle {
            data = data.u16(index);
        }
        data = data
            .f32s(&[0.3, 0.3, 0.0, 1.0])
            .u32(1)
            .u32(1)
            .f32s(&[0., 0., 0., 1., 0., 0., 0., 1., 0.])
            .f32s(&[0., 0., 1., 0., 0., 1., 0., 0., 1.]);
        let extension = container(
            id::EXTENSION,
            lib,
            &[skin(lib), chunk(id::MORPH_PLG, lib, &[0; 4])],
        );
        container(
            id::GEOMETRY,
            lib,
            &[
                chunk(id::STRUCT, lib, &data.0),
                material_list(lib),
                extension,
            ],
        )
    }

    /// Stored triangle: (vertex 2, vertex 1, material, vertex 3).
    const STORED_TRIANGLE: [u16; 4] = [1, 0, 0, 2];

    fn dff(lib: u32, clump_struct: &[u8], triangle: [u16; 4]) -> Vec<u8> {
        let geometry_list = container(
            id::GEOMETRY_LIST,
            lib,
            &[
                chunk(id::STRUCT, lib, &1u32.to_le_bytes()),
                geometry(lib, triangle),
            ],
        );
        let atomic_struct = Payload::default().u32(0).u32(0).u32(5).u32(0);
        let atomic = container(
            id::ATOMIC,
            lib,
            &[
                chunk(id::STRUCT, lib, &atomic_struct.0),
                container(id::EXTENSION, lib, &[]),
            ],
        );
        let mut bytes = container(
            id::CLUMP,
            lib,
            &[
                chunk(id::STRUCT, lib, clump_struct),
                frame_list(lib),
                geometry_list,
                atomic,
                container(id::EXTENSION, lib, &[]),
            ],
        );
        bytes.extend([0; 32]); // IMG sector padding
        bytes
    }

    /// Atomic, light and camera counts.
    const CLUMP_STRUCT_12: [u8; 12] = [1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];

    fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
        [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
    }

    fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    }

    #[test]
    fn parses_a_skinned_clump_of_renderware_3_3() {
        let clump = parse_dff(&dff(LIB_3_3_0_2, &CLUMP_STRUCT_12, STORED_TRIANGLE)).unwrap();
        assert_eq!(clump.version, Version(0x3_3002));
        assert_eq!(clump.light_count, 0);

        let [root, bone] = clump.frames.as_slice() else {
            panic!("two frames expected");
        };
        assert_eq!(root.name.as_deref(), Some("Root"));
        assert_eq!(root.parent, None);
        assert_eq!(root.matrix_flags, 0x20003);
        assert_eq!(bone.name.as_deref(), Some("Pelvis"));
        assert_eq!(bone.parent, Some(0));
        assert_eq!(bone.transform.right, [0.0, 1.0, 0.0]);
        assert_eq!(bone.transform.position, [0.0, 0.0, 1.0]);
        let hanim = bone.hanim.as_ref().unwrap();
        assert_eq!(hanim.node_id, 7);
        let hierarchy = hanim.hierarchy.as_ref().unwrap();
        assert_eq!(hierarchy.key_frame_size, 36);
        assert_eq!(
            hierarchy.nodes,
            [HAnimNode {
                id: 7,
                index: 0,
                flags: 1
            }]
        );
        assert_eq!(root.hanim.as_ref().unwrap().hierarchy, None);

        let geometry = &clump.geometries[0];
        assert_eq!(geometry.vertex_count, 3);
        assert!(geometry.surface.is_some(), "stored before 3.4");
        assert_eq!(
            geometry.prelit_colors.as_ref().unwrap()[2],
            [10, 20, 30, 255]
        );
        assert_eq!(geometry.tex_coord_sets.len(), 1);
        assert_eq!(geometry.tex_coord_sets[0][2], [0.0, 1.0]);
        assert_eq!(
            geometry.triangles,
            [Triangle {
                vertices: [0, 1, 2],
                material: 0
            }]
        );

        let target = &geometry.morph_targets[0];
        let vertices = target.vertices.as_ref().unwrap();
        let normals = target.normals.as_ref().unwrap();
        // Counter-clockwise from the front: the face normal follows the
        // vertex normals.
        let [a, b, c] = geometry.triangles[0]
            .vertices
            .map(|v| vertices[usize::from(v)]);
        assert_eq!(cross(sub(b, a), sub(c, a)), normals[0]);

        assert_eq!(geometry.materials.len(), 2);
        assert_eq!(geometry.materials[0], geometry.materials[1]);
        let material = &geometry.materials[0];
        assert_eq!(material.color, [255, 128, 0, 255]);
        let texture = material.texture.as_ref().unwrap();
        assert_eq!(
            (texture.name.as_str(), texture.mask_name.as_str()),
            ("tex", "")
        );
        assert_eq!(texture.sampler, 0x1106);

        let skin = geometry.skin.as_ref().unwrap();
        assert_eq!((skin.bone_count, skin.max_weights_per_vertex), (1, 0));
        assert!(skin.used_bones.is_empty());
        assert_eq!(skin.weights[1], [1.0, 0.0, 0.0, 0.0]);
        // Inverse bind matrix times the bone's world matrix: identity.
        let world = root.transform.mul(&bone.transform);
        assert_eq!(skin.inverse_bind_matrices[0].mul(&world), Matrix::IDENTITY);

        assert_eq!(
            clump.atomics,
            [Atomic {
                frame: 0,
                geometry: 0,
                flags: 5
            }]
        );
    }

    #[test]
    fn renderware_3_4_layout_differs_in_geometry_clump_and_skin() {
        let clump = parse_dff(&dff(LIB_3_4_0_3, &1u32.to_le_bytes(), STORED_TRIANGLE)).unwrap();
        assert_eq!(clump.version, Version(0x3_4003));
        let geometry = &clump.geometries[0];
        assert!(geometry.surface.is_none(), "not stored from 3.4 on");
        let skin = geometry.skin.as_ref().unwrap();
        assert_eq!(skin.used_bones, [0]);
        assert_eq!(skin.max_weights_per_vertex, 4);
        assert_eq!(skin.inverse_bind_matrices[0].position, [0.0, 0.0, -1.0]);
    }

    #[test]
    fn rejects_a_file_that_is_not_a_clump() {
        let bytes = chunk(id::TEXTURE, LIB_3_4_0_3, &[]);
        assert!(matches!(
            parse_dff(&bytes),
            Err(RwError::UnexpectedChunk {
                found: id::TEXTURE,
                ..
            })
        ));
    }

    #[test]
    fn rejects_a_triangle_pointing_past_the_vertices() {
        let err = parse_dff(&dff(LIB_3_4_0_3, &1u32.to_le_bytes(), [1, 0, 0, 9])).unwrap_err();
        assert!(
            matches!(
                err,
                RwError::Invalid {
                    context: "Geometry",
                    ..
                }
            ),
            "{err}"
        );
    }

    #[test]
    fn rejects_an_unknown_clump_struct_size() {
        // 8 bytes is neither of the two known layouts (4 or 12 bytes).
        let err = parse_dff(&dff(
            LIB_3_4_0_3,
            &[1, 0, 0, 0, 0, 0, 0, 0],
            STORED_TRIANGLE,
        ))
        .unwrap_err();
        assert!(
            matches!(
                err,
                RwError::Invalid {
                    context: "Clump",
                    ..
                }
            ),
            "{err}"
        );
    }
}
