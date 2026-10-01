//! Tests on the real files of the player's Vice City install.
//!
//! Skipped when Vice City is not configured (e.g. in CI): see
//! `asset_bridge::testing::game_dir`.

use asset_bridge::config::Game;
use asset_bridge::testing::game_dir;
use formats_rw::img::{DIR_ENTRY_SIZE, ImgArchive};

#[test]
fn gta3_img_directory_is_consistent() {
    let Some(vice_city) = game_dir(Game::ViceCity) else {
        return;
    };
    let models = vice_city.join("models");
    // Opening checks that every entry lies inside gta3.img.
    let mut archive = ImgArchive::open_pair(&models.join("gta3")).unwrap();

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
