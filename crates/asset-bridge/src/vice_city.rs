//! Vice City assets, read from the player's install.

use std::io;
use std::path::{Path, PathBuf};

use formats_rw::dff::{self, Clump};
use formats_rw::ifp::{self, IfpError};
use formats_rw::img::{ImgArchive, ImgError};
use formats_rw::rw::RwError;
use formats_rw::sfx::{SfxError, SoundBank};
use formats_rw::txd;
use formats_rw::weapon_dat::{self, WeaponDatError};

use crate::model::{
    Animation, Bone, Key, Material, Mesh, MeshSkin, Model, Node, Primitive, Skeleton, Sound,
    Texture, Track, Weapon,
};

#[derive(Debug, thiserror::Error)]
pub enum ViceCityError {
    #[error(transparent)]
    Img(#[from] ImgError),

    #[error("{0} est absent de models/gta3.img")]
    NotFound(String),

    #[error("{name} : {source}")]
    Format {
        name: String,
        #[source]
        source: RwError,
    },

    #[error("{name} : {message}")]
    Unsupported { name: String, message: String },

    #[error("{name} : {source}")]
    Animation {
        name: String,
        #[source]
        source: IfpError,
    },

    #[error("impossible de lire {} : {source}", .path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error(transparent)]
    Sound(#[from] SfxError),

    #[error(transparent)]
    WeaponDat(#[from] WeaponDatError),

    #[error("{0}")]
    Missing(String),
}

/// What Chaos FC needs to know about a weapon besides `weapon.dat`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WeaponSpec {
    /// Name in `weapon.dat`.
    pub name: &'static str,
    /// Model of `gta3.img` (its dictionary has the same name).
    pub model: &'static str,
    /// Number of the firing sound in the SFX bank.
    pub fire_sound: usize,
}

// Sound numbers from the GTAMods list of Vice City sounds: 50 (Colt 45)
// confirmed by ear. HYPOTHÈSE for the others, and for the model names until
// the IDE files are read.
pub const COLT45: WeaponSpec = WeaponSpec {
    name: "Colt45",
    model: "colt45",
    fire_sound: 50,
};
pub const UZI: WeaponSpec = WeaponSpec {
    name: "Uzi",
    model: "uzi",
    fire_sound: 54,
};
pub const M4: WeaponSpec = WeaponSpec {
    name: "m4",
    model: "m4",
    fire_sound: 74,
};
/// Vice City's other assault rifle (the AK47 of GTA III is not in the game).
pub const RUGER: WeaponSpec = WeaponSpec {
    name: "Ruger",
    model: "ruger",
    fire_sound: 74,
};

/// Frames per second of the animation instants of `weapon.dat` (see
/// docs/formats/weapon-dat.md).
const WEAPON_DAT_FPS: f32 = 30.0;

/// The player's Vice City install.
pub struct ViceCity {
    root: PathBuf,
    archive: ImgArchive,
}

impl ViceCity {
    /// Opens `models/gta3.img` in the install folder `root`.
    pub fn open(root: &Path) -> Result<Self, ViceCityError> {
        Ok(Self {
            root: root.to_path_buf(),
            archive: ImgArchive::open_pair(&root.join("models").join("gta3"))?,
        })
    }

    /// Reads a sound of the SFX bank (`audio/sfx.SDT` and `sfx.RAW`).
    pub fn load_sound(&mut self, index: usize) -> Result<Sound, ViceCityError> {
        let (sdt, raw) = sound_bank_paths(&self.root).ok_or_else(|| {
            ViceCityError::Missing("audio/sfx.SDT ou audio/sfx.RAW introuvable".into())
        })?;
        let mut bank = SoundBank::open(&sdt, &raw)?;
        let samples = bank.read_samples(index)?;
        Ok(Sound {
            sample_rate: bank.entries()[index].sample_rate,
            samples,
        })
    }

    /// Loads a weapon: its line of `data/weapon.dat`, its model and
    /// textures, the firing animation of its animation group and its sound.
    pub fn load_weapon(&mut self, spec: &WeaponSpec) -> Result<Weapon, ViceCityError> {
        let path = find_ignoring_case(&self.root, "data")
            .and_then(|data| find_ignoring_case(&data, "weapon.dat"))
            .ok_or_else(|| ViceCityError::Missing("data/weapon.dat introuvable".into()))?;
        let bytes = std::fs::read(&path).map_err(|source| ViceCityError::Io { path, source })?;
        let info = weapon_dat::parse_weapon_dat(&String::from_utf8_lossy(&bytes))?
            .into_iter()
            .find(|info| info.name.eq_ignore_ascii_case(spec.name))
            .ok_or_else(|| ViceCityError::Missing(format!("{} absent de weapon.dat", spec.name)))?;

        // HYPOTHÈSE: the firing animation of a group is "<group>_fire"
        // (colt45_fire, UZI_fire, RIFLE_fire in gta3.img).
        let fire_name = format!("{}_fire", info.anim_group);
        let fire_animation = self
            .load_animations(&info.anim_group)?
            .into_iter()
            .find(|animation| animation.name.eq_ignore_ascii_case(&fire_name))
            .ok_or_else(|| {
                ViceCityError::Missing(format!("{fire_name} absente de {}.ifp", info.anim_group))
            })?;

        Ok(Weapon {
            name: info.name,
            model: self.load_model(spec.model)?,
            textures: self.load_textures(spec.model)?,
            fire_animation,
            fire_loop: info.anim_loop.map(|frame| frame / WEAPON_DAT_FPS),
            muzzle: info.fire_offset,
            range: info.range,
            fire_sound: self.load_sound(spec.fire_sound)?,
        })
    }

    /// Loads an animation package: `ped` (the characters, in `anim/ped.ifp`)
    /// or one of the IFP files of `gta3.img` (`colt45`, `python`...).
    pub fn load_animations(&mut self, name: &str) -> Result<Vec<Animation>, ViceCityError> {
        let (file, bytes) = match self.read(name, "ifp") {
            Ok(found) => found,
            Err(ViceCityError::NotFound(file)) => {
                let path = self.root.join("anim").join(&file);
                if !path.is_file() {
                    return Err(ViceCityError::NotFound(file));
                }
                let bytes =
                    std::fs::read(&path).map_err(|source| ViceCityError::Io { path, source })?;
                (file, bytes)
            }
            Err(err) => return Err(err),
        };
        let package = ifp::parse_ifp(&bytes)
            .map_err(|source| ViceCityError::Animation { name: file, source })?;
        Ok(package.animations.iter().map(animation_from_ifp).collect())
    }

    /// Loads a model of `gta3.img`: `player`, `player.dff`, any case.
    pub fn load_model(&mut self, name: &str) -> Result<Model, ViceCityError> {
        let (file, bytes) = self.read(name, "dff")?;
        let clump = dff::parse_dff(&bytes).map_err(|source| ViceCityError::Format {
            name: file.clone(),
            source,
        })?;
        model_from_clump(&file, &clump).map_err(|message| ViceCityError::Unsupported {
            name: file,
            message,
        })
    }

    /// Decodes the textures of a dictionary of `gta3.img`: `player` or
    /// `player.txd`, any case. A model usually has a dictionary of the
    /// same name.
    pub fn load_textures(&mut self, name: &str) -> Result<Vec<Texture>, ViceCityError> {
        let (file, bytes) = self.read(name, "txd")?;
        decode_txd(&file, &bytes)
    }

    /// Reads an entry of `gta3.img`, adding `extension` to `name` if needed.
    /// Returns the name as stored in the archive, and the content.
    fn read(&mut self, name: &str, extension: &str) -> Result<(String, Vec<u8>), ViceCityError> {
        let file = if name.to_lowercase().ends_with(&format!(".{extension}")) {
            name.to_owned()
        } else {
            format!("{name}.{extension}")
        };
        let entry = self
            .archive
            .find(&file)
            .ok_or(ViceCityError::NotFound(file))?
            .clone();
        let bytes = self.archive.read(&entry)?;
        Ok((entry.name, bytes))
    }
}

/// Paths of the sound bank `audio/sfx.SDT` and `audio/sfx.RAW` of the install
/// `root`. Names are matched ignoring case (the install has `Audio/sfx.RAW`).
pub fn sound_bank_paths(root: &Path) -> Option<(PathBuf, PathBuf)> {
    let audio = find_ignoring_case(root, "audio")?;
    Some((
        find_ignoring_case(&audio, "sfx.sdt")?,
        find_ignoring_case(&audio, "sfx.raw")?,
    ))
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

/// Decodes every texture of a TXD file. `name` is used in error messages.
pub fn decode_txd(name: &str, bytes: &[u8]) -> Result<Vec<Texture>, ViceCityError> {
    let dictionary = txd::parse_txd(bytes).map_err(|source| ViceCityError::Format {
        name: name.to_owned(),
        source,
    })?;
    dictionary
        .textures
        .iter()
        .map(|texture| {
            let image = texture
                .decode_rgba8()
                .map_err(|err| ViceCityError::Unsupported {
                    name: name.to_owned(),
                    message: err.to_string(),
                })?;
            Ok(Texture {
                name: texture.name.clone(),
                width: image.width,
                height: image.height,
                rgba8: image.pixels,
            })
        })
        .collect()
}

/// Converts a parsed DFF into a neutral model. Each atomic becomes a mesh
/// placed by its frame; triangles are grouped by material.
pub fn model_from_clump(name: &str, clump: &Clump) -> Result<Model, String> {
    let nodes = clump
        .frames
        .iter()
        .enumerate()
        .map(|(index, frame)| Node {
            name: frame
                .name
                .clone()
                .unwrap_or_else(|| format!("frame {index}")),
            parent: frame.parent,
            local: frame.transform.to_cols_array(),
            bone_id: frame
                .hanim
                .as_ref()
                .map(|hanim| hanim.node_id)
                .filter(|&id| id >= 0),
        })
        .collect();

    let mut meshes = Vec::with_capacity(clump.atomics.len());
    let mut skeleton = None;
    for atomic in &clump.atomics {
        let geometry = &clump.geometries[atomic.geometry];
        // Vice City never uses morphing: one target holds the vertices.
        let target = geometry
            .morph_targets
            .first()
            .ok_or("géométrie sans morph target")?;
        let positions = target
            .vertices
            .clone()
            .ok_or("géométrie sans positions de sommets")?;

        let mut indices_by_material = vec![Vec::new(); geometry.materials.len()];
        for triangle in &geometry.triangles {
            indices_by_material[usize::from(triangle.material)]
                .extend(triangle.vertices.map(u32::from));
        }
        let primitives = geometry
            .materials
            .iter()
            .zip(indices_by_material)
            .filter(|(_, indices)| !indices.is_empty())
            .map(|(material, indices)| Primitive {
                material: Material {
                    base_color: material.color.map(|c| f32::from(c) / 255.0),
                    texture: material.texture.as_ref().map(|t| t.name.clone()),
                },
                indices,
            })
            .collect();

        meshes.push(Mesh {
            node: atomic.frame,
            positions,
            normals: target.normals.clone(),
            uvs: geometry.tex_coord_sets.first().cloned(),
            colors: geometry.prelit_colors.as_ref().map(|colors| {
                colors
                    .iter()
                    .map(|color| color.map(|c| f32::from(c) / 255.0))
                    .collect()
            }),
            primitives,
            skin: geometry.skin.as_ref().map(mesh_skin).transpose()?,
        });

        if let (Some(skin), None) = (&geometry.skin, &skeleton) {
            skeleton = Some(skeleton_from_skin(clump, skin)?);
        }
    }

    Ok(Model {
        name: name.to_owned(),
        nodes,
        meshes,
        skeleton,
    })
}

/// Joint indices and weights of the vertices. A slot without weight points
/// to bone 0, so that every index is valid for the renderer.
fn mesh_skin(skin: &dff::Skin) -> Result<MeshSkin, String> {
    let mut joints = Vec::with_capacity(skin.bone_indices.len());
    for (indices, weights) in skin.bone_indices.iter().zip(&skin.weights) {
        let mut vertex = [0u16; 4];
        for slot in 0..4 {
            if weights[slot] > 0.0 {
                if indices[slot] >= skin.bone_count {
                    return Err(format!(
                        "os {} pondéré alors que le skin en a {}",
                        indices[slot], skin.bone_count
                    ));
                }
                vertex[slot] = u16::from(indices[slot]);
            }
        }
        joints.push(vertex);
    }
    Ok(MeshSkin {
        joints,
        weights: skin.weights.clone(),
    })
}

/// Converts an IFP animation. The stored quaternions are the inverse of the
/// bone rotations (see docs/formats/ifp.md): keys hold the local rotation.
pub fn animation_from_ifp(animation: &ifp::Animation) -> Animation {
    Animation {
        name: animation.name.clone(),
        duration: animation.duration(),
        tracks: animation
            .objects
            .iter()
            .map(|object| Track {
                bone_name: object.name.clone(),
                bone_id: object.bone_id,
                keys: object
                    .keyframes
                    .iter()
                    .map(|keyframe| Key {
                        time: keyframe.time,
                        rotation: keyframe.local_rotation(),
                        translation: keyframe.translation,
                        scale: keyframe.scale,
                    })
                    .collect(),
            })
            .collect(),
    }
}

/// Bone `i` of the skin is node `i` of the HAnim hierarchy, found among the
/// frames by its bone ID.
fn skeleton_from_skin(clump: &Clump, skin: &dff::Skin) -> Result<Skeleton, String> {
    let hierarchy = clump
        .frames
        .iter()
        .find_map(|frame| frame.hanim.as_ref()?.hierarchy.as_ref())
        .ok_or("skin sans hiérarchie HAnim")?;
    if hierarchy.nodes.len() != skin.inverse_bind_matrices.len() {
        return Err(format!(
            "{} os dans la hiérarchie HAnim, {} dans le skin",
            hierarchy.nodes.len(),
            skin.inverse_bind_matrices.len()
        ));
    }
    let bones = hierarchy
        .nodes
        .iter()
        .zip(&skin.inverse_bind_matrices)
        .map(|(node, inverse_bind)| {
            let frame = clump
                .frames
                .iter()
                .position(|frame| frame.hanim.as_ref().is_some_and(|h| h.node_id == node.id))
                .ok_or_else(|| format!("aucune frame pour l'os {}", node.id))?;
            Ok(Bone {
                name: clump.frames[frame].name.clone().unwrap_or_default(),
                node: frame,
                inverse_bind: inverse_bind.to_cols_array(),
            })
        })
        .collect::<Result<_, String>>()?;
    Ok(Skeleton { bones })
}

#[cfg(test)]
mod tests {
    use super::*;
    use formats_rw::dff::{
        Atomic, Frame, Geometry, HAnim, HAnimHierarchy, HAnimNode, Matrix, MorphTarget, Skin,
        Texture, Triangle,
    };
    use formats_rw::rw::Version;

    fn frame(name: &str, parent: Option<usize>, bone: i32, hierarchy: Option<Vec<i32>>) -> Frame {
        Frame {
            transform: Matrix {
                position: [0.0, 1.0, 0.0],
                ..Matrix::IDENTITY
            },
            parent,
            matrix_flags: 0,
            name: Some(name.into()),
            hanim: Some(HAnim {
                version: 0x100,
                node_id: bone,
                hierarchy: hierarchy.map(|ids| HAnimHierarchy {
                    flags: 0,
                    key_frame_size: 36,
                    nodes: ids
                        .into_iter()
                        .enumerate()
                        .map(|(index, id)| HAnimNode {
                            id,
                            index: index as i32,
                            flags: 0,
                        })
                        .collect(),
                }),
            }),
        }
    }

    fn material(color: [u8; 4], texture: Option<&str>) -> dff::Material {
        dff::Material {
            color,
            surface: None,
            texture: texture.map(|name| Texture {
                sampler: 0,
                name: name.into(),
                mask_name: String::new(),
            }),
        }
    }

    /// Root frame, then two bones listed in the opposite order of the
    /// hierarchy (as in player.dff, where the orders differ).
    fn clump() -> Clump {
        let inverse_bind = |y: f32| Matrix {
            position: [0.0, -y, 0.0],
            ..Matrix::IDENTITY
        };
        Clump {
            version: Version(0x3_3002),
            frames: vec![
                frame("Root", None, -1, None),
                frame("Spine", Some(0), 2, None),
                frame("Pelvis", Some(0), 1, Some(vec![1, 2])),
            ],
            geometries: vec![Geometry {
                format: 0,
                vertex_count: 4,
                surface: None,
                prelit_colors: Some(vec![[255, 0, 51, 255]; 4]),
                tex_coord_sets: vec![vec![[0.0, 0.0]; 4]],
                triangles: vec![
                    Triangle {
                        vertices: [0, 1, 2],
                        material: 1,
                    },
                    Triangle {
                        vertices: [0, 2, 3],
                        material: 1,
                    },
                ],
                morph_targets: vec![MorphTarget {
                    bounding_sphere: [0.0; 4],
                    vertices: Some(vec![[0.0; 3]; 4]),
                    normals: None,
                }],
                // The first material is not used by any triangle.
                materials: vec![
                    material([0, 0, 0, 255], None),
                    material([255, 255, 255, 255], Some("player")),
                ],
                skin: Some(Skin {
                    bone_count: 2,
                    used_bones: vec![],
                    max_weights_per_vertex: 0,
                    bone_indices: vec![[0; 4]; 4],
                    weights: vec![[1.0, 0.0, 0.0, 0.0]; 4],
                    inverse_bind_matrices: vec![inverse_bind(1.0), inverse_bind(2.0)],
                }),
            }],
            atomics: vec![Atomic {
                frame: 0,
                geometry: 0,
                flags: 5,
            }],
            light_count: 0,
        }
    }

    #[test]
    fn converts_frames_meshes_and_materials() {
        let model = model_from_clump("test.dff", &clump()).unwrap();
        assert_eq!(model.name, "test.dff");
        assert_eq!(model.nodes.len(), 3);
        assert_eq!(model.nodes[1].name, "Spine");
        assert_eq!(model.nodes[1].parent, Some(0));
        assert_eq!(model.nodes[1].local[13], 1.0, "translation Y, column-major");

        let [mesh] = model.meshes.as_slice() else {
            panic!("one mesh expected");
        };
        assert_eq!(mesh.node, 0);
        assert_eq!(mesh.positions.len(), 4);
        assert_eq!(mesh.normals, None);
        assert_eq!(mesh.colors.as_ref().unwrap()[0], [1.0, 0.0, 0.2, 1.0]);
        // Unused materials are dropped, triangles are grouped by material.
        let [primitive] = mesh.primitives.as_slice() else {
            panic!("one primitive expected");
        };
        assert_eq!(primitive.indices, [0, 1, 2, 0, 2, 3]);
        assert_eq!(primitive.material.texture.as_deref(), Some("player"));
        assert_eq!(primitive.material.base_color, [1.0; 4]);
        assert_eq!((model.vertex_count(), model.triangle_count()), (4, 2));

        // HAnim IDs; -1 (not a bone) gives none.
        let ids: Vec<_> = model.nodes.iter().map(|node| node.bone_id).collect();
        assert_eq!(ids, [None, Some(2), Some(1)]);
        let skin = mesh.skin.as_ref().unwrap();
        assert_eq!(skin.joints[0], [0; 4]);
        assert_eq!(skin.weights[0], [1.0, 0.0, 0.0, 0.0]);
    }

    #[test]
    fn unweighted_skin_slots_point_to_bone_zero() {
        let mut clump = clump();
        let skin = clump.geometries[0].skin.as_mut().unwrap();
        skin.bone_indices[0] = [1, 200, 0, 0];
        skin.weights[0] = [1.0, 0.0, 0.0, 0.0];
        let model = model_from_clump("test.dff", &clump).unwrap();
        assert_eq!(
            model.meshes[0].skin.as_ref().unwrap().joints[0],
            [1, 0, 0, 0]
        );

        // A weighted slot past the bones is an error.
        let skin = clump.geometries[0].skin.as_mut().unwrap();
        skin.weights[0] = [0.5, 0.5, 0.0, 0.0];
        assert!(model_from_clump("test.dff", &clump).is_err());
    }

    #[test]
    fn animations_hold_local_rotations_and_find_their_bones() {
        use formats_rw::ifp::{self, AnimationObject, Keyframe, KeyframeKind};
        let object = |name: &str, bone_id| AnimationObject {
            name: name.into(),
            bone_id,
            links: None,
            kind: KeyframeKind::Rotation,
            keyframes: vec![Keyframe {
                rotation: [0.5, 0.5, 0.5, 0.5],
                translation: None,
                scale: None,
                time: 0.25,
            }],
        };
        let animation = animation_from_ifp(&ifp::Animation {
            name: "run".into(),
            objects: vec![object("whatever", Some(2)), object("PELVIS", None)],
        });
        assert_eq!(animation.duration, 0.25);
        assert_eq!(
            animation.tracks[0].keys[0].rotation,
            [-0.5, -0.5, -0.5, 0.5]
        );

        let model = model_from_clump("test.dff", &clump()).unwrap();
        // By bone ID first, then by name.
        assert_eq!(animation.tracks[0].node_in(&model), Some(1));
        assert_eq!(animation.tracks[1].node_in(&model), Some(2));
    }

    #[test]
    fn bones_follow_the_hierarchy_order() {
        let model = model_from_clump("test.dff", &clump()).unwrap();
        let bones = &model.skeleton.unwrap().bones;
        // Hierarchy: bone ID 1 (frame 2, "Pelvis"), then ID 2 (frame 1).
        assert_eq!(
            bones
                .iter()
                .map(|b| (b.name.as_str(), b.node))
                .collect::<Vec<_>>(),
            [("Pelvis", 2), ("Spine", 1)]
        );
        assert_eq!(bones[1].inverse_bind[13], -2.0);
    }

    #[test]
    fn rejects_a_skin_that_does_not_match_the_hierarchy() {
        let mut clump = clump();
        clump.frames[2].hanim.as_mut().unwrap().hierarchy = None;
        clump.frames[1].hanim.as_mut().unwrap().hierarchy = Some(HAnimHierarchy {
            flags: 0,
            key_frame_size: 36,
            nodes: vec![],
        });
        assert!(model_from_clump("test.dff", &clump).is_err());
    }
}
