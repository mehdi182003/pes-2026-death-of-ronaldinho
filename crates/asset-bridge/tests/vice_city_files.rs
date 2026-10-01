//! Tests on the real files of the player's Vice City install.
//!
//! Skipped when Vice City is not configured (e.g. in CI): see
//! `asset_bridge::testing::game_dir`.

use asset_bridge::config::Game;
use asset_bridge::testing::game_dir;
use asset_bridge::vice_city::{ViceCity, ViceCityError};
use std::path::Path;

use formats_rw::dff::{self, Clump, Matrix};
use formats_rw::ifp;
use formats_rw::img::{DIR_ENTRY_SIZE, DirEntry, ImgArchive};
use formats_rw::rw::{self, Version};
use formats_rw::txd::{self, raster_format};

fn open_gta3(vice_city: &Path) -> ImgArchive {
    ImgArchive::open_pair(&vice_city.join("models").join("gta3")).unwrap()
}

fn dff_entries(archive: &ImgArchive) -> Vec<DirEntry> {
    archive
        .entries()
        .iter()
        .filter(|entry| entry.name.to_lowercase().ends_with(".dff"))
        .cloned()
        .collect()
}

#[test]
fn gta3_img_directory_is_consistent() {
    let Some(vice_city) = game_dir(Game::ViceCity) else {
        return;
    };
    let models = vice_city.join("models");
    // Opening checks that every entry lies inside gta3.img.
    let mut archive = open_gta3(&vice_city);

    let dir_len = std::fs::metadata(models.join("gta3.dir")).unwrap().len();
    assert_eq!(
        archive.entries().len() as u64,
        dir_len / DIR_ENTRY_SIZE as u64
    );
    assert!(archive.entries().iter().all(|entry| !entry.name.is_empty()));

    let player = archive
        .find("player.dff")
        .expect("Tommy's model is in gta3.img")
        .clone();
    let data = archive.read(&player).unwrap();
    assert_eq!(data.len() as u64, player.byte_size());
}

/// Every DFF is a well-formed chunk tree: a Clump root whose chunks all fit
/// in their parents, followed by zero padding only.
#[test]
fn every_dff_is_a_well_formed_renderware_stream() {
    let Some(vice_city) = game_dir(Game::ViceCity) else {
        return;
    };
    let mut archive = open_gta3(&vice_city);
    let entries = dff_entries(&archive);
    assert!(!entries.is_empty());

    let mut failures = Vec::new();
    for entry in &entries {
        let data = archive.read(entry).unwrap();
        match rw::walk(&data) {
            Ok(tree) => {
                let root = tree[0].chunk;
                if root.kind() != rw::id::CLUMP {
                    failures.push(format!("{} : racine {:#x}", entry.name, root.kind()));
                }
                let end = root.data_offset() + root.data.len();
                if data[end..].iter().any(|&byte| byte != 0) {
                    failures.push(format!("{} : octets non nuls après la racine", entry.name));
                }
            }
            Err(err) => failures.push(format!("{} : {err}", entry.name)),
        }
    }
    assert!(
        failures.is_empty(),
        "{} échec(s) sur {} DFF :
{}",
        failures.len(),
        entries.len(),
        failures.join(
            "
"
        )
    );
}

#[test]
fn every_dff_parses() {
    let Some(vice_city) = game_dir(Game::ViceCity) else {
        return;
    };
    let mut archive = open_gta3(&vice_city);
    let entries = dff_entries(&archive);
    let mut failures = Vec::new();
    for entry in &entries {
        let data = archive.read(entry).unwrap();
        if let Err(err) = dff::parse_dff(&data) {
            failures.push(format!("{} : {err}", entry.name));
        }
    }
    assert!(
        failures.is_empty(),
        "{} échec(s) sur {} DFF :
{}",
        failures.len(),
        entries.len(),
        failures.join(
            "
"
        )
    );
}

fn player(vice_city: &Path) -> Clump {
    let mut archive = open_gta3(vice_city);
    let entry = archive.find("player.dff").unwrap().clone();
    dff::parse_dff(&archive.read(&entry).unwrap()).unwrap()
}

#[test]
fn player_dff_has_the_expected_structure() {
    let Some(vice_city) = game_dir(Game::ViceCity) else {
        return;
    };
    let clump = player(&vice_city);
    assert_eq!(clump.version, Version(0x3_3002));
    assert_eq!(clump.frames.len(), 25);
    let names: Vec<&str> = clump
        .frames
        .iter()
        .filter_map(|frame| frame.name.as_deref())
        .collect();
    for expected in ["Root", "Pelvis", "Spine", "Head", "L Hand", "R Foot"] {
        assert!(names.contains(&expected), "{expected} absent de {names:?}");
    }

    let [geometry] = clump.geometries.as_slice() else {
        panic!("une seule géométrie attendue");
    };
    assert_eq!(geometry.vertex_count, 1153);
    assert_eq!(geometry.triangles.len(), 1355);
    let [material] = geometry.materials.as_slice() else {
        panic!("un seul matériau attendu");
    };
    assert_eq!(material.texture.as_ref().unwrap().name, "player");
    assert_eq!(geometry.skin.as_ref().unwrap().bone_count, 24);
    assert_eq!(clump.atomics.len(), 1);
}

/// Composing each inverse bind matrix of the skin with the world matrix of
/// its bone's frame gives the identity. This validates at once the frames,
/// their composition order, the HAnim bone mapping and the skin matrices.
#[test]
fn player_bind_pose_matches_its_frames() {
    let Some(vice_city) = game_dir(Game::ViceCity) else {
        return;
    };
    let clump = player(&vice_city);
    let mut world: Vec<Matrix> = Vec::with_capacity(clump.frames.len());
    for frame in &clump.frames {
        let matrix = match frame.parent {
            Some(parent) => world[parent].mul(&frame.transform),
            None => frame.transform,
        };
        world.push(matrix);
    }
    let hierarchy = clump
        .frames
        .iter()
        .find_map(|frame| frame.hanim.as_ref()?.hierarchy.as_ref())
        .unwrap();
    let skin = clump.geometries[0].skin.as_ref().unwrap();
    assert_eq!(hierarchy.nodes.len(), skin.inverse_bind_matrices.len());

    for (node, inverse_bind) in hierarchy.nodes.iter().zip(&skin.inverse_bind_matrices) {
        let frame = clump
            .frames
            .iter()
            .position(|frame| frame.hanim.as_ref().is_some_and(|h| h.node_id == node.id))
            .unwrap();
        let product = inverse_bind.mul(&world[frame]).to_cols_array();
        let identity = Matrix::IDENTITY.to_cols_array();
        let error = product
            .iter()
            .zip(identity)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0, f32::max);
        assert!(error < 1e-4, "os {} : écart {error}", node.id);
    }
}

/// In (vertex 1, vertex 2, vertex 3) order, nearly all triangles turn
/// counter-clockwise around their vertex normals.
#[test]
fn player_triangles_face_their_normals() {
    let Some(vice_city) = game_dir(Game::ViceCity) else {
        return;
    };
    let clump = player(&vice_city);
    let geometry = &clump.geometries[0];
    let target = &geometry.morph_targets[0];
    let (vertices, normals) = (
        target.vertices.as_ref().unwrap(),
        target.normals.as_ref().unwrap(),
    );
    let sub = |a: [f32; 3], b: [f32; 3]| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    let agreeing = geometry
        .triangles
        .iter()
        .filter(|triangle| {
            let [a, b, c] = triangle.vertices.map(usize::from);
            let (u, v) = (sub(vertices[b], vertices[a]), sub(vertices[c], vertices[a]));
            let face = [
                u[1] * v[2] - u[2] * v[1],
                u[2] * v[0] - u[0] * v[2],
                u[0] * v[1] - u[1] * v[0],
            ];
            let normal: [f32; 3] =
                std::array::from_fn(|i| normals[a][i] + normals[b][i] + normals[c][i]);
            face.iter().zip(normal).map(|(f, n)| f * n).sum::<f32>() > 0.0
        })
        .count();
    // Measured: 1341 of 1355.
    assert!(
        agreeing * 100 >= geometry.triangles.len() * 95,
        "{agreeing} triangles sur {} suivent leurs normales",
        geometry.triangles.len()
    );
}

#[test]
fn player_model_converts_to_the_neutral_model() {
    let Some(vice_city) = game_dir(Game::ViceCity) else {
        return;
    };
    let mut game = ViceCity::open(&vice_city).unwrap();
    // Any case, with or without the extension.
    let model = game.load_model("PLAYER").unwrap();
    assert_eq!(model.name, "player.dff");
    assert_eq!(model.nodes.len(), 25);
    assert_eq!((model.vertex_count(), model.triangle_count()), (1153, 1355));

    let skeleton = model.skeleton.as_ref().unwrap();
    assert_eq!(skeleton.bones.len(), 24);
    assert_eq!(skeleton.bones[1].name, "Pelvis");

    // Bind pose: Tommy stands along +Y, about 1.8 m tall, arms spread
    // along X.
    let positions = &model.meshes[0].positions;
    let extent = |axis: usize| {
        let values = positions.iter().map(|p| p[axis]);
        values.clone().fold(f32::MIN, f32::max) - values.fold(f32::MAX, f32::min)
    };
    let (width, height, depth) = (extent(0), extent(1), extent(2));
    assert!((1.7..2.0).contains(&height), "hauteur {height}");
    assert!(
        width > depth && height > depth,
        "{width} x {height} x {depth}"
    );

    assert!(matches!(
        game.load_model("pas-un-modele"),
        Err(ViceCityError::NotFound(_))
    ));
}

/// Every TXD of gta3.img, and the loose ones of models/ and txd/, parses and
/// decodes. INTRO.TXD is left out: an older RenderWare 3.1 file whose raster
/// layout differs and which Chaos FC does not need.
#[test]
fn every_txd_parses_and_decodes() {
    let Some(vice_city) = game_dir(Game::ViceCity) else {
        return;
    };
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    let mut archive = open_gta3(&vice_city);
    let entries: Vec<DirEntry> = archive
        .entries()
        .iter()
        .filter(|entry| entry.name.to_lowercase().ends_with(".txd"))
        .cloned()
        .collect();
    for entry in &entries {
        files.push((entry.name.clone(), archive.read(entry).unwrap()));
    }
    for folder in ["models", "txd"] {
        for file in std::fs::read_dir(vice_city.join(folder)).unwrap() {
            let path = file.unwrap().path();
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            if name.to_lowercase().ends_with(".txd") && !name.eq_ignore_ascii_case("intro.txd") {
                files.push((name, std::fs::read(&path).unwrap()));
            }
        }
    }
    assert!(files.len() > entries.len(), "TXD isolés introuvables");

    let mut failures = Vec::new();
    let mut texture_count = 0;
    for (name, bytes) in &files {
        match txd::parse_txd(bytes) {
            Ok(dictionary) => {
                for texture in &dictionary.textures {
                    texture_count += 1;
                    if let Err(err) = texture.decode_rgba8() {
                        failures.push(format!("{name} : {err}"));
                    }
                }
            }
            Err(err) => failures.push(format!("{name} : {err}")),
        }
    }
    assert!(
        failures.is_empty(),
        "{} échec(s) sur {texture_count} textures :
{}",
        failures.len(),
        failures.join(
            "
"
        )
    );
}

#[test]
fn player_txd_holds_tommy_texture() {
    let Some(vice_city) = game_dir(Game::ViceCity) else {
        return;
    };
    let mut archive = open_gta3(&vice_city);
    let entry = archive.find("player.txd").unwrap().clone();
    let dictionary = txd::parse_txd(&archive.read(&entry).unwrap()).unwrap();
    let [texture] = dictionary.textures.as_slice() else {
        panic!("une seule texture attendue");
    };
    assert_eq!(texture.name, "player");
    assert_eq!((texture.width, texture.height), (256, 256));
    assert_eq!(
        texture.raster_format,
        raster_format::FORMAT_565 | raster_format::MIPMAP
    );
    assert_eq!((texture.compression, texture.levels.len()), (1, 9));

    let image = texture.decode_rgba8().unwrap();
    assert_eq!(image.pixels.len(), 256 * 256 * 4);
    assert!(image.pixels.chunks(4).all(|pixel| pixel[3] == 255));
    let distinct: std::collections::HashSet<&[u8]> = image.pixels.chunks(4).collect();
    assert!(
        distinct.len() > 1000,
        "{} couleurs seulement",
        distinct.len()
    );
}

#[test]
fn player_textures_load_by_name() {
    let Some(vice_city) = game_dir(Game::ViceCity) else {
        return;
    };
    let mut game = ViceCity::open(&vice_city).unwrap();
    let textures = game.load_textures("player").unwrap();
    let [texture] = textures.as_slice() else {
        panic!("une seule texture attendue");
    };
    assert_eq!(
        (texture.name.as_str(), texture.width, texture.height),
        ("player", 256, 256)
    );
    assert!(texture.is_opaque());

    // The model's material names this texture.
    let model = game.load_model("player").unwrap();
    let material = &model.meshes[0].primitives[0].material;
    assert_eq!(material.texture.as_deref(), Some("player"));
}

#[test]
fn every_ifp_parses() {
    let Some(vice_city) = game_dir(Game::ViceCity) else {
        return;
    };
    let ped =
        ifp::parse_ifp(&std::fs::read(vice_city.join("anim").join("ped.ifp")).unwrap()).unwrap();
    assert_eq!(ped.name, "ped");
    assert_eq!(ped.animations.len(), 234);

    let mut archive = open_gta3(&vice_city);
    let entries: Vec<DirEntry> = archive
        .entries()
        .iter()
        .filter(|entry| entry.name.to_lowercase().ends_with(".ifp"))
        .cloned()
        .collect();
    assert_eq!(entries.len(), 28);
    let mut packages = vec![ped];
    for entry in &entries {
        let bytes = archive.read(entry).unwrap();
        match ifp::parse_ifp(&bytes) {
            Ok(package) => packages.push(package),
            Err(err) => panic!("{} : {err}", entry.name),
        }
    }

    // Unit quaternions, times in increasing order.
    for package in &packages {
        for animation in &package.animations {
            for object in &animation.objects {
                let keyframes = &object.keyframes;
                assert!(keyframes[0].time >= 0.0);
                assert!(keyframes.windows(2).all(|k| k[0].time <= k[1].time));
                for keyframe in keyframes {
                    let norm: f32 = keyframe.rotation.iter().map(|c| c * c).sum::<f32>().sqrt();
                    assert!(
                        (norm - 1.0).abs() < 1e-3,
                        "{} / {}",
                        animation.name,
                        object.name
                    );
                }
            }
        }
    }
}

/// Rotation matrix (columns right, up, at) of a unit quaternion (x, y, z, w).
fn quaternion_matrix([x, y, z, w]: [f32; 4]) -> Matrix {
    Matrix {
        right: [
            1.0 - 2.0 * (y * y + z * z),
            2.0 * (x * y + z * w),
            2.0 * (x * z - y * w),
        ],
        up: [
            2.0 * (x * y - z * w),
            1.0 - 2.0 * (x * x + z * z),
            2.0 * (y * z + x * w),
        ],
        at: [
            2.0 * (x * z + y * w),
            2.0 * (y * z - x * w),
            1.0 - 2.0 * (x * x + y * y),
        ],
        position: [0.0; 3],
    }
}

/// Angle, in degrees, between two rotation matrices.
fn angle_between(a: &Matrix, b: &Matrix) -> f32 {
    let trace: f32 = [(a.right, b.right), (a.up, b.up), (a.at, b.at)]
        .iter()
        .map(|(u, v)| u[0] * v[0] + u[1] * v[1] + u[2] * v[2])
        .sum();
    ((trace - 1.0) / 2.0).clamp(-1.0, 1.0).acos().to_degrees()
}

/// run_player drives the bones of player.dff by their HAnim IDs, and its
/// stored quaternions are the inverse of the bone rotations: their conjugate
/// is close to the bind pose for the trunk and the head.
#[test]
fn run_player_drives_tommy_skeleton() {
    let Some(vice_city) = game_dir(Game::ViceCity) else {
        return;
    };
    let ped =
        ifp::parse_ifp(&std::fs::read(vice_city.join("anim").join("ped.ifp")).unwrap()).unwrap();
    let run = ped.find("run_player").unwrap();
    assert_eq!(run.objects.len(), 22);
    assert!((run.duration() - 0.6667).abs() < 1e-3, "{}", run.duration());

    let clump = player(&vice_city);
    for object in &run.objects {
        let id = object.bone_id.unwrap();
        let frame = clump
            .frames
            .iter()
            .find(|frame| frame.hanim.as_ref().is_some_and(|h| h.node_id == id))
            .unwrap_or_else(|| panic!("os {id} absent de player.dff"));
        assert_eq!(frame.name.as_deref(), Some(object.name.as_str()));

        if ["Pelvis", "Neck", "Head"].contains(&object.name.as_str()) {
            let keyframe = object.keyframes[0];
            let stored = angle_between(&frame.transform, &quaternion_matrix(keyframe.rotation));
            let local = angle_between(
                &frame.transform,
                &quaternion_matrix(keyframe.local_rotation()),
            );
            assert!(
                local < 10.0 && local < stored,
                "{} : {local} / {stored}",
                object.name
            );
        }
    }
}
