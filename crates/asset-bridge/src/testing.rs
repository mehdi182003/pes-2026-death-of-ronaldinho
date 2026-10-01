//! Support for tests that read the real game files.
//!
//! Such tests must be skipped automatically when the games are not
//! configured (non-negotiable rule 6), for instance in CI.

use std::path::{Path, PathBuf};

use crate::config::{CONFIG_ENV_VAR, Game, read_paths};

/// Install directory of `game` according to `$CHAOS_FC_CONFIG`, or else the
/// `config.toml` at the root of the workspace.
///
/// When the game is not configured, prints a notice and returns `None`: the
/// calling test then returns early.
///
/// ```ignore
/// let Some(vice_city) = asset_bridge::testing::game_dir(Game::ViceCity) else {
///     return;
/// };
/// ```
pub fn game_dir(game: Game) -> Option<PathBuf> {
    let config_file = std::env::var_os(CONFIG_ENV_VAR)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("..")
                .join("..")
                .join("config.toml")
        });
    match read_paths(&config_file).map(|paths| paths.check(game)) {
        Ok(Ok(dir)) => Some(dir),
        _ => {
            eprintln!(
                "test ignoré : {game} n'est pas configuré ({})",
                config_file.display()
            );
            None
        }
    }
}
