//! Tests on the real files of the player's PES 6 install.
//!
//! Skipped when PES 6 is not configured (e.g. in CI): see
//! `asset_bridge::testing::game_dir`. Written on the PC demo; they check
//! properties, not counts, so that they hold on the full game too.

use asset_bridge::config::Game;
use asset_bridge::pes6::{self, Pes6};
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
    let readable = files
        .iter()
        .filter(|(_, report)| !matches!(report.packing, Packing::Unreadable(_)))
        .count();
    // Nearly everything unpacks (803 of the 820 compressed files of the demo).
    assert!(
        readable * 50 >= files.len() * 49,
        "{readable} / {}",
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
    // Demo: 1184 decoded, 706 set aside (mostly colour variants).
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
    // Demo: 751 read, 11 with an opcode not understood yet (08, 11).
    eprintln!("modèles : {read} lus, {unknown_opcodes} avec une instruction inconnue");
    assert!(unknown_opcodes * 50 < read, "{read} / {unknown_opcodes}");
}

#[test]
fn referee_loads_textured_at_a_plausible_size() {
    let Some(dir) = game_dir(Game::Pes6) else {
        return;
    };
    let pes = Pes6::open(&dir).unwrap();
    let texture = pes.load_texture(&"0_text:432".parse().unwrap()).unwrap();
    assert_eq!((texture.width, texture.height), (512, 256));
    let model = pes
        .load_model(&"0_text:431".parse().unwrap(), Some(&texture.name))
        .unwrap();
    assert!(model.triangle_count() > 1000, "{}", model.triangle_count());
    let heights = model
        .meshes
        .iter()
        .flat_map(|m| m.positions.iter().map(|p| p[1]));
    let (low, high) = heights.fold((f32::MAX, f32::MIN), |(lo, hi), y| (lo.min(y), hi.max(y)));
    // Feet at 0, head at 756 units: 1.80 m (retarget::PES6_UNITS_PER_METRE).
    assert!(
        low.abs() < 1.0 && (high - 756.0).abs() < 2.0,
        "{low} .. {high}"
    );
    // Texture coordinates: within the texture, apart from a few that make
    // it repeat (v up to 1.5 on the demo's referee).
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
