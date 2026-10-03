//! Vice City assets loaded inside PES: Tommy's model and textures, read in
//! the background when the mod starts, from the install named in
//! `chaos-fc-mod.toml` (written by `asset-tools mod install`).

use std::sync::OnceLock;

use asset_bridge::config::{self, Game};
use asset_bridge::model::Texture;
use asset_bridge::vice_city::ViceCity;

use crate::proxy::log;
use crate::tommy::{self, Batch};
use crate::{install, pes};

/// Tommy, ready to draw.
pub struct Tommy {
    pub batches: Vec<Batch>,
    pub textures: Vec<Texture>,
}

static TOMMY: OnceLock<Option<Tommy>> = OnceLock::new();

/// Tommy, once loaded (`None` while loading or if loading failed).
pub fn tommy() -> Option<&'static Tommy> {
    TOMMY.get()?.as_ref()
}

/// Starts loading in a background thread. Safe to call from `DllMain`: the
/// thread only runs once the loader lock is released.
pub fn start_loading() {
    std::thread::spawn(|| {
        let tommy = load()
            .map_err(|err| log(&format!("Tommy non chargé : {err}")))
            .ok();
        let _ = TOMMY.set(tommy);
    });
}

fn load() -> Result<Tommy, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let config_file = install::config_path(&exe);
    let paths = config::read_paths(&config_file).map_err(|e| e.to_string())?;
    let root = paths.check(Game::ViceCity).map_err(|e| e.to_string())?;
    let mut vice_city = ViceCity::open(&root).map_err(|e| e.to_string())?;
    let model = vice_city.load_model("player").map_err(|e| e.to_string())?;
    let textures = vice_city
        .load_textures("player")
        .map_err(|e| e.to_string())?;
    let batches = tommy::batches(&model, tommy::SIDELINE_SPOT, pes::LOGIC_UNITS_PER_METRE);
    log(&format!(
        "Tommy chargé depuis {} : {} triangles, {} lots, {} textures",
        root.display(),
        model.triangle_count(),
        batches.len(),
        textures.len()
    ));
    Ok(Tommy { batches, textures })
}
