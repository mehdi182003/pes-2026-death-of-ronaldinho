//! Tests on the real files of the player's PES 6 install.
//!
//! Skipped when PES 6 is not configured (e.g. in CI): see
//! `asset_bridge::testing::game_dir`. Written on the full PC game; they
//! check properties rather than exact counts.

use asset_bridge::config::Game;
use asset_bridge::model::transform_point;
use asset_bridge::pes6::{self, Pes6, PlayerParts, PlayerSlot};
use asset_bridge::testing::game_dir;
use formats_pes::content::{self, Kind, Packing, Report};
use formats_pes::model::{self, ModelError};
use formats_pes::texture::{self, TextureError};

/// Every non-empty file of `archive`, with its index and report.
fn inspect_all(pes: &Pes6, archive: &str) -> Vec<(usize, Report)> {
    let mut afs = pes.open_archive(archive).unwrap();
    let entries: Vec<_> = afs
        .entries()
        .iter()
        .filter(|e| !e.is_empty())
        .copied()
        .collect();
    entries
        .iter()
        .map(|entry| (entry.index, content::inspect(&afs.read(entry).unwrap())))
        .collect()
}

#[test]
fn every_archive_of_dat_opens() {
    let Some(dir) = game_dir(Game::Pes6) else {
        return;
    };
    let pes = Pes6::open(&dir).unwrap();
    let names = pes.archive_names().unwrap();
    assert!(
        names
            .iter()
            .any(|name| name.eq_ignore_ascii_case("0_text.afs")),
        "{names:?}"
    );
    for name in &names {
        let archive = pes.open_archive(name).unwrap();
        assert!(!archive.entries().is_empty(), "{name}");
    }
}

#[test]
fn sound_archive_holds_adx_sounds_only() {
    let Some(dir) = game_dir(Game::Pes6) else {
        return;
    };
    let pes = Pes6::open(&dir).unwrap();
    let files = inspect_all(&pes, "0_sound.afs");
    assert!(!files.is_empty());
    for (index, report) in &files {
        assert_eq!(report.packing, Packing::Plain, "n° {index}");
        assert_eq!(report.kind, Kind::Adx, "n° {index}");
    }
}

#[test]
fn text_archive_contents_match_the_community_map() {
    let Some(dir) = game_dir(Game::Pes6) else {
        return;
    };
    let pes = Pes6::open(&dir).unwrap();
    let files = inspect_all(&pes, "0_text.afs");
    // What does not unpack is where the game keeps its encrypted files: one
    // file of every kit (the big texture), and the block 1193 to 1890 of
    // the full game. A single other file is let through (n° 7051 of the
    // full game, an incomplete zlib stream).
    let unreadable: Vec<usize> = files
        .iter()
        .filter(|(_, report)| matches!(report.packing, Packing::Unreadable(_)))
        .map(|(index, _)| *index)
        .collect();
    let elsewhere: Vec<usize> = unreadable
        .iter()
        .copied()
        .filter(|&index| {
            pes6::section("0_text.afs", index) != Some("maillots")
                && !(1193..=1890).contains(&index)
        })
        .collect();
    assert!(
        elsewhere.len() <= 1,
        "illisibles hors des zones chiffrées : {elsewhere:?}"
    );
    assert!(
        unreadable.len() * 5 < files.len(),
        "{} / {}",
        unreadable.len(),
        files.len()
    );

    let kinds_in = |section: &str| -> Vec<Kind> {
        files
            .iter()
            .filter(|(index, _)| pes6::section("0_text.afs", *index) == Some(section))
            .flat_map(|(_, report)| report.walk().into_iter().map(|r| r.kind.clone()))
            .collect()
    };
    // Faces and hairstyles: containers of models and their textures, and
    // nothing else.
    for section in ["visages", "coiffures (éditeur)"] {
        let kinds = kinds_in(section);
        assert!(kinds.contains(&Kind::Model), "{section}");
        assert!(
            kinds
                .iter()
                .all(|kind| matches!(kind, Kind::Container | Kind::Model | Kind::Texture)),
            "{section} : {kinds:?}"
        );
    }
    // Numbers and palettes: textures.
    for section in ["numéros et polices", "palettes"] {
        let kinds = kinds_in(section);
        assert!(kinds.contains(&Kind::Texture), "{section} : {kinds:?}");
        assert!(!kinds.contains(&Kind::Model), "{section} : {kinds:?}");
    }
    assert!(kinds_in("sons").contains(&Kind::Adx));
}

#[test]
fn texture_headers_give_sizes_and_their_logarithms() {
    let Some(dir) = game_dir(Game::Pes6) else {
        return;
    };
    let pes = Pes6::open(&dir).unwrap();
    let mut afs = pes.open_archive("0_text.afs").unwrap();
    let entries: Vec<_> = afs
        .entries()
        .iter()
        .filter(|e| !e.is_empty())
        .copied()
        .collect();
    let mut checked = 0;
    for entry in &entries {
        for file in content::extract(&afs.read(entry).unwrap()) {
            if file.kind != Kind::Texture {
                continue;
            }
            // Width and height (u16 at 20 and 22), then their base-2
            // logarithms rounded up (bytes 26 and 27): 64 × 48 gives 6 and
            // 6. Both zero for a palette alone.
            let data = &file.data;
            let fits = |size: u16, log: u8| {
                let storage = 1u32 << log;
                u32::from(size) <= storage && 2 * u32::from(size) > storage
            };
            let width = u16::from_le_bytes([data[20], data[21]]);
            let height = u16::from_le_bytes([data[22], data[23]]);
            let palette_only = width == 0 && height == 0;
            assert!(
                palette_only || (fits(width, data[26]) && fits(height, data[27])),
                "n° {} {:?} : {width} × {height}",
                entry.index,
                file.path
            );
            checked += 1;
        }
    }
    assert!(checked > 0);
}

#[test]
fn every_texture_decodes_or_is_reported() {
    let Some(dir) = game_dir(Game::Pes6) else {
        return;
    };
    let pes = Pes6::open(&dir).unwrap();
    let mut afs = pes.open_archive("0_text.afs").unwrap();
    let entries: Vec<_> = afs
        .entries()
        .iter()
        .filter(|e| !e.is_empty())
        .copied()
        .collect();
    let (mut decoded, mut set_aside) = (0, 0);
    for entry in &entries {
        for file in content::extract(&afs.read(entry).unwrap()) {
            if file.kind != Kind::Texture {
                continue;
            }
            match texture::decode(&file.data) {
                Ok(image) => {
                    assert_eq!(image.rgba8.len(), 4 * (image.width * image.height) as usize);
                    decoded += 1;
                }
                Err(
                    TextureError::PaletteOnly
                    | TextureError::ExternalPalette
                    | TextureError::Swizzled { .. },
                ) => set_aside += 1,
                Err(err) => panic!("n° {} {:?} : {err}", entry.index, file.path),
            }
        }
    }
    // Palettes alone (dimensions of zero, or colour variants that stop
    // before their pixels), textures whose palette is elsewhere and
    // swizzled textures are set aside.
    // Full game: 5165 decoded, 3173 set aside (mostly colour variants).
    eprintln!("textures : {decoded} décodées, {set_aside} mises de côté");
    assert!(decoded > set_aside, "{decoded} / {set_aside}");
}

#[test]
fn models_parse_with_consistent_draws() {
    let Some(dir) = game_dir(Game::Pes6) else {
        return;
    };
    let pes = Pes6::open(&dir).unwrap();
    let mut afs = pes.open_archive("0_text.afs").unwrap();
    let entries: Vec<_> = afs
        .entries()
        .iter()
        .filter(|e| !e.is_empty())
        .copied()
        .collect();
    let (mut read, mut unknown_opcodes) = (0, 0);
    for entry in &entries {
        for file in content::extract(&afs.read(entry).unwrap()) {
            if file.kind != Kind::Model {
                continue;
            }
            // parse() checks that every draw stays within its vertices and
            // gives the announced number of triangles.
            match model::parse(&file.data) {
                Ok(parsed) => {
                    assert!(
                        !parsed.draws.is_empty(),
                        "n° {} {:?}",
                        entry.index,
                        file.path
                    );
                    read += 1;
                }
                Err(ModelError::UnknownOpcode { .. }) => unknown_opcodes += 1,
                Err(err) => panic!("n° {} {:?} : {err}", entry.index, file.path),
            }
        }
    }
    // Full game: 9546 read, 101 with an opcode not understood yet.
    eprintln!("modèles : {read} lus, {unknown_opcodes} avec une instruction inconnue");
    assert!(unknown_opcodes * 50 < read, "{read} / {unknown_opcodes}");
}

#[test]
fn body_vertices_lie_by_the_bones_of_their_bone_table() {
    let Some(dir) = game_dir(Game::Pes6) else {
        return;
    };
    let pes = Pes6::open(&dir).unwrap();
    let mut afs = pes.open_archive("0_text.afs").unwrap();
    let entries: Vec<_> = afs.entries()[990..1130].to_vec();
    let (mut bodies, mut near, mut far) = (0, 0, 0);
    for entry in &entries {
        for file in content::extract(&afs.read(entry).unwrap()) {
            let Some(body) = (file.kind == Kind::Model)
                .then(|| model::parse(&file.data).ok())
                .flatten()
                .filter(|body| body.bones.len() == 19)
            else {
                continue;
            };
            bodies += 1;
            assert_eq!(body.bone_tables.len(), 1, "n° {}", entry.index);
            let joints: Vec<[f32; 3]> = body.bones.iter().map(|bone| bone.joint()).collect();
            for draw in &body.draws {
                let part = &body.parts[draw.part];
                let first = draw.triangles.iter().flatten().min().copied().unwrap_or(0);
                let last = draw.triangles.iter().flatten().max().copied().unwrap_or(0);
                for vertex in &part.vertices[usize::from(first)..=usize::from(last)] {
                    if part.format.bones == 0 || vertex.weights[0] < 250 {
                        continue;
                    }
                    let bone = body.skeleton_bone(draw, vertex.joints[0]).unwrap();
                    // The vertex follows a bone that starts or ends next to
                    // it: its own joint is among the four nearest.
                    let distance = |joint: &[f32; 3]| {
                        (0..3)
                            .map(|i| (joint[i] - vertex.position[i]).powi(2))
                            .sum::<f32>()
                    };
                    let own = distance(&joints[bone]);
                    let closer = joints.iter().filter(|j| distance(j) < own).count();
                    if closer < 4 { near += 1 } else { far += 1 }
                }
            }
        }
    }
    // Files 990 to 1129 of the full game: 393 bodies, 165 146 vertices by
    // their bone, 285 not (the whole game has 573 bodies of 19 bones).
    eprintln!("corps : {bodies}, sommets près de leur os : {near}, loin : {far}");
    assert!(bodies > 300, "{bodies}");
    assert!(far * 100 < near, "{near} / {far}");
}

#[test]
fn player_body_loads_textured_at_a_plausible_size() {
    let Some(dir) = game_dir(Game::Pes6) else {
        return;
    };
    let pes = Pes6::open(&dir).unwrap();
    // A kit: shirt, shorts and socks on one texture.
    let texture = pes.load_texture(&"0_text:419".parse().unwrap()).unwrap();
    assert_eq!((texture.width, texture.height), (512, 256));
    let model = pes
        .load_model(&"0_text:1064".parse().unwrap(), Some(&texture.name))
        .unwrap();
    assert!(model.triangle_count() > 3000, "{}", model.triangle_count());
    let heights = model
        .meshes
        .iter()
        .flat_map(|m| m.positions.iter().map(|p| p[1]));
    let (low, high) = heights.fold((f32::MAX, f32::MIN), |(lo, hi), y| (lo.min(y), hi.max(y)));
    // Soles a little below 0, neck at 673.8 units; with a head (up to about
    // 750), 1.83 m at retarget::PES6_UNITS_PER_METRE.
    assert!(
        (-25.0..0.0).contains(&low) && (high - 673.8).abs() < 2.0,
        "{low} .. {high}"
    );
    // Texture coordinates: within the texture, apart from a few that make
    // it repeat.
    let uvs: Vec<[f32; 2]> = model
        .meshes
        .iter()
        .flat_map(|m| m.uvs.iter().flatten().copied())
        .collect();
    let outside = uvs
        .iter()
        .filter(|uv| !uv.iter().all(|c| (-0.01..=1.01).contains(c)))
        .count();
    assert!(outside * 10 < uvs.len(), "{outside} / {}", uvs.len());
    assert!(uvs.iter().flatten().all(|c| c.abs() < 4.0));
}

#[test]
fn player_body_gets_its_head_on_the_shoulders() {
    let Some(dir) = game_dir(Game::Pes6) else {
        return;
    };
    let pes = Pes6::open(&dir).unwrap();
    let file = |text: &str| text.parse().unwrap();
    let (model, textures) = pes
        .load_player(&PlayerParts {
            body: file("0_text:1010"),
            kit: Some(file("0_text:419")),
            boots: Some(file("0_text:5322/0/0")),
            head: Some(file("0_text:1943")),
            hair: Some(file("0_text:4570")),
        })
        .unwrap();
    assert_eq!(textures.len(), 4, "tenue, chaussures, visage et cheveux");
    // Every body triangle that is drawn uses the kit, the boots or the
    // skin colour; the markings (numbers...) are left out.
    let body_materials: Vec<_> = model
        .meshes
        .iter()
        .filter(|mesh| mesh.node == 0)
        .flat_map(|mesh| &mesh.primitives)
        .map(|primitive| primitive.material.texture.clone())
        .collect();
    assert!(body_materials.contains(&Some("0_text:419".into())));
    assert!(body_materials.contains(&Some("0_text:5322/0/0".into())));
    assert!(body_materials.contains(&None), "peau");
    let head = model.nodes.iter().position(|n| n.name == "head").unwrap();
    let m = model.node_world(head);
    let heights: Vec<f32> = model
        .meshes
        .iter()
        .filter(|mesh| mesh.node == head)
        .flat_map(|mesh| &mesh.positions)
        .map(|p| m[1] * p[0] + m[5] * p[1] + m[9] * p[2] + m[13])
        .collect();
    assert!(!heights.is_empty());
    let (low, high) = heights
        .iter()
        .fold((f32::MAX, f32::MIN), |(lo, hi), &y| (lo.min(y), hi.max(y)));
    // The body stops at the neck (673.8 units).
    assert!(
        low > 580.0 && high < 800.0 && high > 700.0,
        "{low} .. {high}"
    );

    // The face looks the way the toes point (+Z): the head vertex furthest
    // along the head bone's +X (the nose) ends up in front.
    let heads: Vec<_> = model
        .meshes
        .iter()
        .filter(|mesh| mesh.node == head)
        .collect();
    // One level of detail of the head, and the hair.
    assert_eq!(heads.len(), 2, "tête et cheveux");
    let nose = heads[0]
        .positions
        .iter()
        .max_by(|a, b| a[0].total_cmp(&b[0]))
        .unwrap();
    let nose_z = m[2] * nose[0] + m[6] * nose[1] + m[10] * nose[2] + m[14];
    assert!(nose_z > 30.0, "nez en z = {nose_z}");
}

#[test]
fn player_body_is_skinned_and_keeps_its_shape_in_the_bind_pose() {
    let Some(dir) = game_dir(Game::Pes6) else {
        return;
    };
    let pes = Pes6::open(&dir).unwrap();
    let model = pes
        .load_model(&"0_text:1010".parse().unwrap(), None)
        .unwrap();
    let skeleton = model.skeleton.as_ref().expect("squelette");
    assert_eq!(skeleton.bones.len(), 19);
    assert_eq!(model.nodes.len(), 20, "racine et un nœud par os");
    // The joints are where the nodes put them: the left hip at +X.
    let joint = |name: &str| {
        let node = model.nodes.iter().position(|n| n.name == name).unwrap();
        transform_point(&model.node_world(node), [0.0; 3])
    };
    let hip = joint("left thigh");
    assert!(
        (hip[0] - 37.6).abs() < 0.5 && (hip[1] - 380.8).abs() < 0.5,
        "{hip:?}"
    );
    let wrist = joint("right hand");
    assert!((wrist[0] + 297.3).abs() < 0.5, "{wrist:?}");

    // Skinning in the bind pose gives back every vertex: the nodes and the
    // inverse bind matrices agree.
    let mut worst = 0.0f32;
    for mesh in &model.meshes {
        let skin = mesh.skin.as_ref().expect("maillage skinné");
        for ((position, joints), weights) in
            mesh.positions.iter().zip(&skin.joints).zip(&skin.weights)
        {
            let mut skinned = [0.0; 3];
            for (&bone, &weight) in joints.iter().zip(weights) {
                let bone = &skeleton.bones[usize::from(bone)];
                let to_bone = transform_point(&bone.inverse_bind, *position);
                let back = transform_point(&model.node_world(bone.node), to_bone);
                for i in 0..3 {
                    skinned[i] += weight * back[i];
                }
            }
            let error = (0..3)
                .map(|i| (skinned[i] - position[i]).abs())
                .fold(0.0, f32::max);
            worst = worst.max(error);
        }
    }
    assert!(worst < 0.05, "écart {worst}");
}

#[test]
fn body_slots_get_their_role_from_the_geometry() {
    let Some(dir) = game_dir(Game::Pes6) else {
        return;
    };
    let pes = Pes6::open(&dir).unwrap();
    let roles = |body: &str| {
        let data = pes
            .extract(&body.parse().unwrap())
            .unwrap()
            .into_iter()
            .find(|file| file.kind == Kind::Model)
            .unwrap()
            .data;
        let mut roles = PlayerSlot::classify(&model::parse(&data).unwrap());
        roles.sort_by_key(|&(slot, _)| slot);
        roles
    };
    use PlayerSlot::{Boots, Kit, Marking, Skin};
    // The footballer's body: arms, neck and legs 0, kit 1, marking 2,
    // feet 3, collar strip 4, hands 5.
    assert_eq!(
        roles("0_text:995"),
        [
            (0, Skin),
            (1, Kit),
            (2, Marking),
            (3, Boots),
            (4, Kit),
            (5, Skin)
        ]
    );
    // A field player's body: the numbers (slot 8, 64 vertices on the chest
    // and the back) are a marking, not skin.
    let field = roles("0_text:1010");
    assert!(
        field.contains(&(1, Kit)) && field.contains(&(4, Boots)),
        "{field:?}"
    );
    assert!(field.contains(&(8, Marking)), "{field:?}");
    assert!(
        field.contains(&(0, Skin)) && field.contains(&(7, Skin)),
        "{field:?}"
    );
    // A body of another family numbers its slots differently.
    let other = roles("0_text:1064");
    assert!(
        other.contains(&(1, Kit)) && other.contains(&(4, Boots)),
        "{other:?}"
    );
    assert!(
        other.contains(&(0, Skin)) && other.contains(&(7, Skin)),
        "{other:?}"
    );
}
