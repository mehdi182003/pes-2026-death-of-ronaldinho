//! Vice City assets, read from the player's install.

use std::path::Path;

use formats_rw::dff::{self, Clump};
use formats_rw::img::{ImgArchive, ImgError};
use formats_rw::rw::RwError;
use formats_rw::txd;

use crate::model::{Bone, Material, Mesh, Model, Node, Primitive, Skeleton, Texture};

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
}

/// The player's Vice City install.
pub struct ViceCity {
    archive: ImgArchive,
}

impl ViceCity {
    /// Opens `models/gta3.img` in the install folder `root`.
    pub fn open(root: &Path) -> Result<Self, ViceCityError> {
        Ok(Self {
            archive: ImgArchive::open_pair(&root.join("models").join("gta3"))?,
        })
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
