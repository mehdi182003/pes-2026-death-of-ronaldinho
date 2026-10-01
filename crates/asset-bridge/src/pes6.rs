//! The player's PES 6 install: the AFS archives of its `dat` folder, and
//! what is known of the files they hold.

use std::fmt;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use formats_pes::afs::{AfsArchive, AfsError};
use formats_pes::content::{self, Extracted, Kind};
use formats_pes::model::{self as pes_model, ModelError, PesModel};
use formats_pes::texture::{self as pes_texture, TextureError};

use crate::model::{Material, Mesh, Model, Node, Primitive, Texture};

#[derive(Debug, thiserror::Error)]
pub enum Pes6Error {
    #[error(transparent)]
    Afs(#[from] AfsError),

    #[error("{file} : {source}")]
    Model {
        file: PesFile,
        #[source]
        source: ModelError,
    },

    #[error("{file} : {source}")]
    Texture {
        file: PesFile,
        #[source]
        source: TextureError,
    },

    #[error("impossible de lire {} : {source}", .path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("{0}")]
    Missing(String),
}

/// The player's PES 6 install.
pub struct Pes6 {
    dat: PathBuf,
}

impl Pes6 {
    /// Finds the `dat` folder of the install folder `root`.
    pub fn open(root: &Path) -> Result<Self, Pes6Error> {
        let dat = find_ignoring_case(root, "dat").ok_or_else(|| {
            Pes6Error::Missing(format!("dossier dat introuvable dans {}", root.display()))
        })?;
        Ok(Self { dat })
    }

    /// File names of the AFS archives, sorted (e.g. `0_sound.afs`).
    pub fn archive_names(&self) -> Result<Vec<String>, Pes6Error> {
        let entries = std::fs::read_dir(&self.dat).map_err(|source| Pes6Error::Io {
            path: self.dat.clone(),
            source,
        })?;
        let mut names: Vec<String> = entries
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| {
                Path::new(name)
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("afs"))
            })
            .collect();
        names.sort_by_key(|name| name.to_lowercase());
        Ok(names)
    }

    /// Opens the archive `name` of `dat`, ignoring case; `.afs` is optional.
    pub fn open_archive(&self, name: &str) -> Result<AfsArchive, Pes6Error> {
        let file = if Path::new(name)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("afs"))
        {
            name.to_owned()
        } else {
            format!("{name}.afs")
        };
        let path = find_ignoring_case(&self.dat, &file)
            .ok_or_else(|| Pes6Error::Missing(format!("{file} absent du dossier dat")))?;
        Ok(AfsArchive::open(&path)?)
    }

    /// Unpacks `file` and every sub-file inside it, the file itself first
    /// (see [`content::extract`]).
    pub fn extract(&self, file: &PesFile) -> Result<Vec<Extracted>, Pes6Error> {
        let mut archive = self.open_archive(&file.archive)?;
        let entry = archive
            .entries()
            .get(file.index)
            .copied()
            .filter(|entry| !entry.is_empty())
            .ok_or_else(|| Pes6Error::Missing(format!("{file} : emplacement vide ou absent")))?;
        let below: Vec<Extracted> = content::extract(&archive.read(&entry)?)
            .into_iter()
            .filter(|extracted| extracted.path.starts_with(&file.path))
            .collect();
        if below.is_empty() {
            return Err(Pes6Error::Missing(format!("{file} : sous-fichier absent")));
        }
        Ok(below)
    }

    /// The first model at or below `file`, converted to the neutral types.
    /// Every material uses `texture` when it is given.
    pub fn load_model(&self, file: &PesFile, texture: Option<&str>) -> Result<Model, Pes6Error> {
        let (found, data) = self.first_of(file, Kind::Model)?;
        let parsed = pes_model::parse(&data).map_err(|source| Pes6Error::Model {
            file: found.clone(),
            source,
        })?;
        Ok(convert_model(&found.to_string(), &parsed, texture))
    }

    /// The first texture at or below `file`, decoded and named after it.
    pub fn load_texture(&self, file: &PesFile) -> Result<Texture, Pes6Error> {
        let (found, data) = self.first_of(file, Kind::Texture)?;
        let decoded = pes_texture::decode(&data).map_err(|source| Pes6Error::Texture {
            file: found.clone(),
            source,
        })?;
        Ok(Texture {
            name: found.to_string(),
            width: decoded.width,
            height: decoded.height,
            rgba8: decoded.rgba8,
        })
    }

    /// A player assembled from a body, its kit, its boots, its head and its
    /// hair. Each texture slot of the body gets its texture (see
    /// [`PlayerSlot::classify`]); the first model of `head` and of `hair`,
    /// each with the first texture of its file, are placed on the body's
    /// head bone. Returns the model and its textures.
    pub fn load_player(&self, parts: &PlayerParts) -> Result<(Model, Vec<Texture>), Pes6Error> {
        let mut textures = Vec::new();
        let mut load = |file: Option<&PesFile>| -> Result<Option<String>, Pes6Error> {
            let Some(file) = file else {
                return Ok(None);
            };
            let texture = self.load_texture(file)?;
            let name = texture.name.clone();
            textures.push(texture);
            Ok(Some(name))
        };
        let kit = load(parts.kit.as_ref())?;
        let boots = load(parts.boots.as_ref())?;
        let face = match &parts.head {
            Some(head) => self.load_texture(head).ok(),
            None => None,
        };
        let face_name = face.as_ref().map(|t| t.name.clone());
        let skin = face.as_ref().map_or(DEFAULT_SKIN, average_color);
        textures.extend(face);

        let (found, data) = self.first_of(&parts.body, Kind::Model)?;
        let parsed = pes_model::parse(&data).map_err(|source| Pes6Error::Model {
            file: found.clone(),
            source,
        })?;
        let textured = |texture: &Option<String>| Material {
            base_color: [1.0; 4],
            texture: texture.clone(),
        };
        let roles = PlayerSlot::classify(&parsed);
        let role = |slot: u16| {
            roles
                .iter()
                .find(|(known, _)| *known == slot)
                .map_or(PlayerSlot::Marking, |&(_, role)| role)
        };
        let mut model = convert_model_with(&found.to_string(), &parsed, |slot| match role(slot) {
            PlayerSlot::Kit => Some(textured(&kit)),
            PlayerSlot::Boots => Some(textured(if boots.is_some() { &boots } else { &kit })),
            PlayerSlot::Skin => Some(Material {
                base_color: skin,
                texture: None,
            }),
            PlayerSlot::Marking => None,
        });
        if parts.head.is_none() && parts.hair.is_none() {
            return Ok((model, textures));
        }

        // HYPOTHÈSE: the head bone is the one whose joint is highest (bone
        // 16 of the 19-bone bodies, at 670.6 units). Head and hair models
        // are in its space (their bounds match: nose towards +X, hair
        // covering the back of the skull towards -X).
        let bone = parsed
            .bones
            .iter()
            .max_by(|a, b| a.joint()[1].total_cmp(&b.joint()[1]))
            .ok_or_else(|| Pes6Error::Missing(format!("{found} : pas de squelette")))?;
        let node = model.nodes.len();
        model.nodes.push(Node {
            name: "tête".into(),
            parent: Some(0),
            local: bone.bind_matrix(),
            bone_id: None,
        });
        let mut name = found.to_string();
        if let Some(head) = &parts.head {
            // HYPOTHÈSE: a head file holds the same head twice (514 and 114
            // vertices with the same bounds for 0_text:1943): two levels of
            // detail, the first being the detailed one.
            self.attach(&mut model, node, head, face_name.as_deref())?;
            name = format!("{name} + {head}");
        }
        if let Some(hair) = &parts.hair {
            // A hairstyle file holds one model and two textures, the second
            // a smaller copy of the first (0_text:4570: 128 × 64, 64 × 32).
            let texture = self.load_texture(hair)?;
            let texture_name = texture.name.clone();
            textures.push(texture);
            self.attach(&mut model, node, hair, Some(&texture_name))?;
            name = format!("{name} + {hair}");
        }
        model.name = name;
        Ok((model, textures))
    }

    /// Adds the first model of `file`, with `texture` on every slot, to
    /// `model` under `node`.
    fn attach(
        &self,
        model: &mut Model,
        node: usize,
        file: &PesFile,
        texture: Option<&str>,
    ) -> Result<(), Pes6Error> {
        let (found, data) = self.first_of(file, Kind::Model)?;
        let parsed = pes_model::parse(&data).map_err(|source| Pes6Error::Model {
            file: found.clone(),
            source,
        })?;
        let converted = convert_model(&found.to_string(), &parsed, texture);
        model.meshes.extend(
            converted
                .meshes
                .into_iter()
                .map(|mesh| Mesh { node, ..mesh }),
        );
        Ok(())
    }

    fn first_of(&self, file: &PesFile, kind: Kind) -> Result<(PesFile, Vec<u8>), Pes6Error> {
        let label = kind.label();
        let found = self
            .extract(file)?
            .into_iter()
            .find(|extracted| extracted.kind == kind)
            .ok_or_else(|| Pes6Error::Missing(format!("{file} : pas de {label}")))?;
        let path = PesFile {
            path: found.path,
            ..file.clone()
        };
        Ok((path, found.data))
    }
}

/// The files a player is assembled from (see [`Pes6::load_player`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerParts {
    /// The body (`0_text:995`).
    pub body: PesFile,
    /// The kit: shirt, shorts and socks on one 512 × 256 texture
    /// (`0_text:419`).
    pub kit: Option<PesFile>,
    /// The boots (`0_text:5322/0/0`).
    pub boots: Option<PesFile>,
    /// The head and its face texture (`0_text:1943`).
    pub head: Option<PesFile>,
    /// The hairstyle and its texture (`0_text:4570`).
    pub hair: Option<PesFile>,
}

/// What a texture slot of a player's body (opcode `02` of its draws)
/// shows.
///
/// The slot numbers differ between families of bodies (at least eight
/// numberings among the 581 bodies with 19 bones), so the role of a slot is
/// found from where its vertices are on the body and which part of the
/// texture they use (see [`PlayerSlot::classify`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayerSlot {
    /// Shirt, sleeves, shorts and socks, on one kit texture. Checked on
    /// bodies 995 and 1064: the texture coordinates fall exactly on the
    /// parts of a 512 × 256 kit, both sleeves on the same colour.
    Kit,
    /// The feet.
    // HYPOTHÈSE: the boots texture; the feet only use its left third.
    Boots,
    /// Arms, hands, neck, legs.
    // HYPOTHÈSE: bare skin, drawn with the average colour of the face
    // texture until a skin texture is found.
    Skin,
    /// Small patches on the chest, the back and the shorts (numbers, name,
    /// badge ?). Not drawn yet: the game builds them from fonts.
    Marking,
}

impl PlayerSlot {
    /// The role of every texture slot of a body, from its vertices:
    ///
    /// - all of them near the ground (below 70 units): the feet;
    /// - all of them at the end of the arms (|x| > 300): the hands;
    /// - fewer than 60: a marking;
    /// - all of them on the right edge of the texture (u > 0.9): a strip of
    ///   the kit (collar, cuffs);
    /// - otherwise a large part: the one with the most vertices in the
    ///   middle of the chest (|x| < 60, 400 < y < 560) is the kit; the
    ///   others are skin if they reach the arms (|x| > 200) or the legs
    ///   (y < 300), markings if they stay on the trunk.
    // HYPOTHÈSE: rules drawn up on bodies 995, 1010 and 1064 and checked on
    // the texture coordinates and by eye (body 1010); to be confirmed on
    // other bodies.
    pub fn classify(model: &PesModel) -> Vec<(u16, PlayerSlot)> {
        let mut slots: Vec<(u16, Vec<SlotPoint>)> = Vec::new();
        for draw in &model.draws {
            let vertices = &model.parts[draw.part].vertices;
            let points = draw.triangles.iter().flatten().map(|&i| {
                (
                    vertices[usize::from(i)].position,
                    vertices[usize::from(i)].uv,
                )
            });
            match slots.iter_mut().find(|(slot, _)| *slot == draw.texture) {
                Some((_, all)) => all.extend(points),
                None => slots.push((draw.texture, points.collect())),
            }
        }
        let chest = |points: &[SlotPoint]| {
            points
                .iter()
                .filter(|(p, _)| p[0].abs() < 60.0 && (400.0..560.0).contains(&p[1]))
                .count()
        };
        let mut roles: Vec<(u16, Option<PlayerSlot>)> = slots
            .iter()
            .map(|(slot, points)| {
                let role = if points.iter().all(|(p, _)| p[1] < 70.0) {
                    Some(PlayerSlot::Boots)
                } else if points.iter().all(|(p, _)| p[0].abs() > 300.0) {
                    Some(PlayerSlot::Skin)
                } else if points.len() / 3 < 20 || unique(points) < 60 {
                    Some(PlayerSlot::Marking)
                } else if points.iter().all(|(_, uv)| uv[0] > 0.9) {
                    Some(PlayerSlot::Kit)
                } else {
                    None
                };
                (*slot, role)
            })
            .collect();
        let kit = slots
            .iter()
            .zip(&roles)
            .filter(|(_, (_, role))| role.is_none())
            .max_by_key(|((_, points), _)| chest(points))
            .map(|((slot, _), _)| *slot);
        // Bare skin reaches the arms or the legs; a large part that stays
        // on the trunk is a marking (the numbers of body 1010: 64 vertices
        // on the chest and the back).
        let reaches_limbs = |points: &[SlotPoint]| {
            points
                .iter()
                .any(|(p, _)| p[0].abs() > 200.0 || p[1] < 300.0)
        };
        for ((slot, points), (_, role)) in slots.iter().zip(&mut roles) {
            if role.is_none() {
                *role = Some(if Some(*slot) == kit {
                    PlayerSlot::Kit
                } else if reaches_limbs(points) {
                    PlayerSlot::Skin
                } else {
                    PlayerSlot::Marking
                });
            }
        }
        roles
            .into_iter()
            .map(|(slot, role)| (slot, role.expect("every slot has a role")))
            .collect()
    }
}

/// A vertex of a texture slot: its position and texture coordinates.
type SlotPoint = ([f32; 3], [f32; 2]);

/// Number of distinct positions among `points`.
fn unique(points: &[SlotPoint]) -> usize {
    let mut keys: Vec<[u32; 3]> = points.iter().map(|(p, _)| p.map(f32::to_bits)).collect();
    keys.sort_unstable();
    keys.dedup();
    keys.len()
}

/// Skin colour when no face texture is given, sRGB.
const DEFAULT_SKIN: [f32; 4] = [0.47, 0.33, 0.24, 1.0];

/// Average colour of a texture, sRGB from 0 to 1.
fn average_color(texture: &Texture) -> [f32; 4] {
    let pixels = texture.rgba8.as_chunks::<4>().0;
    let mut sum = [0u64; 3];
    for pixel in pixels {
        for (total, &channel) in sum.iter_mut().zip(pixel) {
            *total += u64::from(channel);
        }
    }
    let count = pixels.len().max(1) as f32;
    [
        sum[0] as f32 / count / 255.0,
        sum[1] as f32 / count / 255.0,
        sum[2] as f32 / count / 255.0,
        1.0,
    ]
}

/// A file of an archive, or a sub-file inside it: `0_text:431` (file 431
/// of `0_text.afs`), `0_text:1943/2` (third sub-file of file 1943).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PesFile {
    /// Archive name, without `.afs`.
    pub archive: String,
    pub index: usize,
    /// Positions among the sub-files, level by level.
    pub path: Vec<usize>,
}

impl FromStr for PesFile {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let usage = || {
            format!(
                "« {text} » : attendu <archive>:<numéro>[/<sous-fichier>...], par exemple 0_text:431"
            )
        };
        let (archive, rest) = text.split_once(':').ok_or_else(usage)?;
        let mut numbers = rest.split('/').map(str::parse::<usize>);
        let index = numbers.next().ok_or_else(usage)?.map_err(|_| usage())?;
        let path = numbers.collect::<Result<_, _>>().map_err(|_| usage())?;
        let archive = archive.trim_end_matches(".afs").trim_end_matches(".AFS");
        if archive.is_empty() {
            return Err(usage());
        }
        Ok(Self {
            archive: archive.to_owned(),
            index,
            path,
        })
    }
}

impl fmt::Display for PesFile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.archive, self.index)?;
        for position in &self.path {
            write!(f, "/{position}")?;
        }
        Ok(())
    }
}

/// Converts a PES model to the neutral types: one node, one mesh per vertex
/// part, one primitive per texture slot of that part. Coordinates stay in
/// PES units (see `retarget::PES6_UNITS_PER_METRE`); there is no skeleton
/// yet (J6).
// HYPOTHÈSE: one texture for every slot is enough for the referee's model,
// whose single texture covers the whole body; players will need a texture
// per slot (kit, skin, boots...).
pub fn convert_model(name: &str, model: &PesModel, texture: Option<&str>) -> Model {
    convert_model_with(name, model, |_| {
        Some(Material {
            base_color: [1.0; 4],
            texture: texture.map(str::to_owned),
        })
    })
}

/// Like [`convert_model`], with the material of each texture slot given by
/// `material`; a slot whose material is `None` is not drawn.
pub fn convert_model_with(
    name: &str,
    model: &PesModel,
    material: impl Fn(u16) -> Option<Material>,
) -> Model {
    let meshes = model
        .parts
        .iter()
        .enumerate()
        .map(|(index, part)| {
            let vertices = &part.vertices;
            let mut slots: Vec<(u16, Vec<u32>)> = Vec::new();
            for draw in model.draws.iter().filter(|draw| draw.part == index) {
                let triangles = draw.triangles.iter().flatten().map(|&i| u32::from(i));
                match slots.iter_mut().find(|(slot, _)| *slot == draw.texture) {
                    Some((_, indices)) => indices.extend(triangles),
                    None => slots.push((draw.texture, triangles.collect())),
                }
            }
            Mesh {
                node: 0,
                positions: vertices.iter().map(|v| v.position).collect(),
                normals: part.format.has_normal.then(|| {
                    vertices
                        .iter()
                        .map(|v| normalized(v.normal.unwrap_or([0.0, 1.0, 0.0])))
                        .collect()
                }),
                uvs: Some(vertices.iter().map(|v| v.uv).collect()),
                colors: part.format.has_color.then(|| {
                    vertices
                        .iter()
                        .map(|v| v.color.unwrap_or([255; 4]).map(|c| f32::from(c) / 255.0))
                        .collect()
                }),
                primitives: slots
                    .into_iter()
                    .filter_map(|(slot, indices)| {
                        Some(Primitive {
                            material: material(slot)?,
                            indices,
                        })
                    })
                    .collect(),
                skin: None,
            }
        })
        .collect();
    Model {
        name: name.to_owned(),
        nodes: vec![Node {
            name: "root".into(),
            parent: None,
            local: IDENTITY,
            bone_id: None,
        }],
        meshes,
        skeleton: None,
    }
}

const IDENTITY: [f32; 16] = [
    1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
];

fn normalized(v: [f32; 3]) -> [f32; 3] {
    let length = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if length > 0.0 {
        v.map(|c| c / length)
    } else {
        [0.0, 1.0, 0.0]
    }
}

/// Slots of `0_text.afs` (first and last index, both included), from the
/// map published by the PES 6 modding community ("MAP 0_text.afs PES6",
/// obipes6.blogspot.com). Indices start at 0.
// Checked on the full PC game: every slot holds the expected kind of
// content (see docs/formats/afs.md). Approximate around the sounds: the
// "sons" slots also hold WAV sounds and unknown files, and the ADX sounds
// go on into the crowd and advertising slots (up to 6920).
const TEXT_MAP: &[(usize, usize, &str)] = &[
    (0, 48, "ballons"),
    (431, 446, "arbitres"),
    (535, 536, "drapeaux et emblèmes"),
    (1891, 2937, "visages"),
    (2938, 3404, "visages (éditeur)"),
    (4448, 4902, "coiffures (éditeur)"),
    (4922, 5316, "coiffures (éditeur)"),
    (5322, 5338, "chaussures"),
    (5339, 5443, "chaussures (éditeur)"),
    (5444, 5455, "palettes"),
    (5456, 5472, "numéros et polices"),
    (5473, 6831, "maillots"),
    (6872, 6912, "sons"),
    (6913, 6914, "foule"),
    (6915, 6939, "panneaux publicitaires"),
];

/// What the community map says slot `index` of archive `archive` holds.
pub fn section(archive: &str, index: usize) -> Option<&'static str> {
    if archive.eq_ignore_ascii_case("0_text.afs") {
        TEXT_MAP
            .iter()
            .find(|(first, last, _)| (*first..=*last).contains(&index))
            .map(|(_, _, name)| *name)
    } else if archive.eq_ignore_ascii_case("0_sound.afs") {
        // Every file of 0_sound.afs is an ADX sound.
        Some("sons")
    } else {
        None
    }
}

/// Entry of `dir` named `name`, ignoring case.
fn find_ignoring_case(dir: &Path, name: &str) -> Option<PathBuf> {
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .find(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .eq_ignore_ascii_case(name)
        })
        .map(|entry| entry.path())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_sections_are_ordered_and_do_not_overlap() {
        for pair in TEXT_MAP.windows(2) {
            assert!(pair[0].0 <= pair[0].1 && pair[0].1 < pair[1].0, "{pair:?}");
        }
        assert_eq!(section("0_text.afs", 1943), Some("visages"));
        assert_eq!(section("0_TEXT.AFS", 5481), Some("maillots"));
        assert_eq!(section("0_text.afs", 57), None);
        assert_eq!(section("0_sound.afs", 3), Some("sons"));
        assert_eq!(section("e_text.afs", 3), None);
    }

    #[test]
    fn file_references_parse_and_print() {
        let file: PesFile = "0_text:1943/2".parse().unwrap();
        assert_eq!(
            file,
            PesFile {
                archive: "0_text".into(),
                index: 1943,
                path: vec![2],
            }
        );
        assert_eq!(file.to_string(), "0_text:1943/2");
        assert_eq!(
            "0_text.afs:431".parse::<PesFile>().unwrap().to_string(),
            "0_text:431"
        );
        for bad in ["431", "0_text:", "0_text:x", ":3", "0_text:3/a"] {
            assert!(bad.parse::<PesFile>().is_err(), "{bad}");
        }
    }

    #[test]
    fn converted_models_group_triangles_by_texture_slot() {
        use formats_pes::model::{Draw, Vertex, VertexFormat, VertexPart};
        let vertex = |x: f32| Vertex {
            position: [x, 0.0, 0.0],
            normal: Some([0.0, 8.0, 0.0]),
            color: None,
            uv: [x, 0.0],
            joints: [0; 4],
            weights: [255, 0, 0, 0],
        };
        let model = PesModel {
            parts: vec![VertexPart {
                format: VertexFormat::new(32, 0).unwrap(),
                vertices: (0..4).map(|i| vertex(i as f32)).collect(),
            }],
            draws: [(0, [0, 1, 2]), (3, [1, 2, 3]), (0, [0, 2, 3])]
                .into_iter()
                .map(|(texture, triangle)| Draw {
                    part: 0,
                    texture,
                    bone: 0,
                    triangles: vec![triangle],
                })
                .collect(),
            bones: Vec::new(),
        };
        let converted = convert_model("test", &model, Some("tex"));
        let mesh = &converted.meshes[0];
        assert_eq!(mesh.positions.len(), 4);
        assert_eq!(mesh.normals.as_ref().unwrap()[0], [0.0, 1.0, 0.0]);
        assert_eq!(mesh.primitives.len(), 2);
        assert_eq!(mesh.primitives[0].indices, [0, 1, 2, 0, 2, 3]);
        assert_eq!(mesh.primitives[1].indices, [1, 2, 3]);
        assert_eq!(mesh.primitives[1].material.texture.as_deref(), Some("tex"));
        assert_eq!(converted.triangle_count(), 3);
    }

    #[test]
    fn lists_and_opens_archives_ignoring_case() {
        let tmp = tempfile::tempdir().unwrap();
        let dat = tmp.path().join("DAT");
        std::fs::create_dir(&dat).unwrap();
        let mut archive = b"AFS\0".to_vec();
        archive.extend([0; 12]);
        std::fs::write(dat.join("0_Text.afs"), &archive).unwrap();
        std::fs::write(dat.join("readme.txt"), b"").unwrap();

        let pes = Pes6::open(tmp.path()).unwrap();
        assert_eq!(pes.archive_names().unwrap(), ["0_Text.afs"]);
        assert!(pes.open_archive("0_text").unwrap().entries().is_empty());
        assert!(matches!(
            pes.open_archive("e_text.afs"),
            Err(Pes6Error::Missing(_))
        ));
    }
}
