//! Tests on the real files of the player's Vice City install.
//!
//! Skipped when Vice City is not configured (e.g. in CI): see
//! `asset_bridge::testing::game_dir`.

use asset_bridge::config::Game;
use asset_bridge::testing::game_dir;
use std::path::Path;

use formats_rw::img::{DIR_ENTRY_SIZE, DirEntry, ImgArchive};
use formats_rw::rw;

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
