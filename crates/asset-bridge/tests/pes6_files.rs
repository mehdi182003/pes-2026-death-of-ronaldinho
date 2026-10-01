//! Tests on the real files of the player's PES 6 install.
//!
//! Skipped when PES 6 is not configured (e.g. in CI): see
//! `asset_bridge::testing::game_dir`. Written on the PC demo; they check
//! properties, not counts, so that they hold on the full game too.

use asset_bridge::config::Game;
use asset_bridge::pes6::{self, Pes6};
use asset_bridge::testing::game_dir;
use formats_pes::content::{self, Kind, Packing, Report};

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
