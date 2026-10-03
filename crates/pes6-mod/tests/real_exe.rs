//! Checks against the player's own PES6.exe; skipped when PES 6 is not
//! configured.

use asset_bridge::config::Game;
use dinput8::pe;

#[test]
fn finds_the_direct3d8_import_of_pes6() {
    let Some(dir) = asset_bridge::testing::game_dir(Game::Pes6) else {
        return;
    };
    let exe = std::fs::read(dir.join("PES6.exe")).unwrap();
    // The import tables sit in .rdata, stored at the same offset on disk as in
    // memory, so the file can be read like the mapped image.
    let d3d = pe::import_slot(exe.as_slice(), "d3d8.dll", "Direct3DCreate8");
    let input = pe::import_slot(exe.as_slice(), "DINPUT8.dll", "DirectInput8Create");
    assert_eq!(d3d, Some(0x77d3a8));
    assert_eq!(input, Some(0x77d01c));
}
