//! Extraction cache. Files extracted from the original games live in the
//! user's cache folder, never in the repository.

use std::path::PathBuf;

use crate::config::Game;

/// Root of Chaos FC's cache: `%LOCALAPPDATA%\chaos-fc` on Windows,
/// `~/Library/Caches/chaos-fc` on macOS, `$XDG_CACHE_HOME/chaos-fc` (or
/// `~/.cache/chaos-fc`) elsewhere. `None` if the platform folder is unknown.
pub fn cache_root() -> Option<PathBuf> {
    platform_cache_dir().map(|dir| dir.join("chaos-fc"))
}

/// Default folder for files extracted from `game`.
pub fn extraction_dir(game: Game) -> Option<PathBuf> {
    cache_root().map(|root| root.join("extracted").join(game.config_key()))
}

fn platform_cache_dir() -> Option<PathBuf> {
    let var = |name: &str| {
        std::env::var_os(name)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
    };
    if cfg!(windows) {
        var("LOCALAPPDATA")
    } else if cfg!(target_os = "macos") {
        var("HOME").map(|home| home.join("Library").join("Caches"))
    } else {
        var("XDG_CACHE_HOME").or_else(|| var("HOME").map(|home| home.join(".cache")))
    }
}
