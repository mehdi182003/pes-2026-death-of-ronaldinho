//! Tommy from the player's own Vice City install; skipped when Vice City is
//! not configured.

use asset_bridge::config::Game;
use asset_bridge::vice_city::ViceCity;
use dinput8::{pes, tommy};

#[test]
fn tommy_stands_on_the_pitch_at_his_real_height() {
    let Some(dir) = asset_bridge::testing::game_dir(Game::ViceCity) else {
        return;
    };
    let mut vc = ViceCity::open(&dir).unwrap();
    let model = vc.load_model("player").unwrap();
    let textures = vc.load_textures("player").unwrap();
    let batches = tommy::batches(&model, [0.0; 3], pes::LOGIC_UNITS_PER_METRE);
    assert!(!batches.is_empty());

    let ys: Vec<f32> = batches
        .iter()
        .flat_map(|b| &b.vertices)
        .map(|v| v.y)
        .collect();
    let feet = ys.iter().copied().fold(f32::MIN, f32::max);
    let head = ys.iter().copied().fold(f32::MAX, f32::min);
    let height = (feet - head) / pes::LOGIC_UNITS_PER_METRE;
    println!(
        "{} lots, {} triangles, Tommy mesure {height:.2} m",
        batches.len(),
        model.triangle_count()
    );
    assert!(feet.abs() < 1e-3);
    assert!((1.6..2.0).contains(&height), "{height}");

    for texture in batches.iter().filter_map(|b| b.texture.as_deref()) {
        assert!(
            textures
                .iter()
                .any(|t| t.name.eq_ignore_ascii_case(texture)),
            "texture {texture} absente de player.txd"
        );
    }
}
