//! Copies the proxy next to PES6.exe and removes it, without ever touching a
//! `dinput8.dll` that is not ours.

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use crate::MARKER;

/// Name the proxy must have in the game folder.
pub const DLL_NAME: &str = "dinput8.dll";

/// Where `cargo build -p pes6-mod --release --target i686-pc-windows-msvc`
/// puts the proxy, relative to the workspace root.
pub const BUILT_DLL: &str = "target/i686-pc-windows-msvc/release/dinput8.dll";

/// Configuration written next to the proxy: where the mod finds Vice City.
/// Same `[paths]` section as `config.toml`, read with `asset_bridge::config`.
pub const CONFIG_NAME: &str = "chaos-fc-mod.toml";

/// Path of the mod's configuration for a game executable.
pub fn config_path(game_exe: &Path) -> PathBuf {
    game_exe
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(CONFIG_NAME)
}

/// TOML basic string for `text`.
fn toml_string(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Writes the mod's configuration in `game_dir`.
pub fn write_config(game_dir: &Path, vice_city: &Path) -> Result<PathBuf, InstallError> {
    let path = game_dir.join(CONFIG_NAME);
    let text = format!(
        "# Écrit par `asset-tools mod install` : où le mod Chaos FC trouve Vice City.\n[paths]\nvice_city = {}\n",
        toml_string(&vice_city.to_string_lossy())
    );
    std::fs::write(&path, text).map_err(|e| InstallError::Io(path.clone(), e))?;
    Ok(path)
}

#[derive(Debug)]
pub enum InstallError {
    /// The built DLL does not exist or is not the proxy.
    NotBuilt(PathBuf),
    /// The game folder already holds a `dinput8.dll` from someone else.
    Foreign(PathBuf),
    Io(PathBuf, io::Error),
}

impl fmt::Display for InstallError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotBuilt(path) => write!(
                f,
                "{} n'est pas le mod Chaos FC compilé (lancer `cargo build -p pes6-mod --release --target i686-pc-windows-msvc`)",
                path.display()
            ),
            Self::Foreign(path) => write!(
                f,
                "{} existe déjà et ne vient pas de Chaos FC : il n'est pas modifié",
                path.display()
            ),
            Self::Io(path, err) => write!(f, "{} : {err}", path.display()),
        }
    }
}

impl std::error::Error for InstallError {}

/// What `uninstall` found.
#[derive(Debug, PartialEq, Eq)]
pub enum Removed {
    Removed(PathBuf),
    NothingInstalled,
}

/// True when the file is a build of this crate.
pub fn is_ours(path: &Path) -> io::Result<bool> {
    let bytes = std::fs::read(path)?;
    Ok(bytes
        .windows(MARKER.len())
        .any(|window| window == MARKER.as_bytes()))
}

/// Copies `dll` into `game_dir` as `dinput8.dll`, replacing an older build of
/// the mod but nothing else. Returns the installed path.
pub fn install(dll: &Path, game_dir: &Path) -> Result<PathBuf, InstallError> {
    if !is_ours(dll).unwrap_or(false) {
        return Err(InstallError::NotBuilt(dll.to_path_buf()));
    }
    let target = game_dir.join(DLL_NAME);
    if target.exists() && !is_ours(&target).map_err(|e| InstallError::Io(target.clone(), e))? {
        return Err(InstallError::Foreign(target));
    }
    std::fs::copy(dll, &target).map_err(|e| InstallError::Io(target.clone(), e))?;
    Ok(target)
}

/// Removes the proxy from `game_dir` if it is ours.
pub fn uninstall(game_dir: &Path) -> Result<Removed, InstallError> {
    let target = game_dir.join(DLL_NAME);
    if !target.exists() {
        return Ok(Removed::NothingInstalled);
    }
    if !is_ours(&target).map_err(|e| InstallError::Io(target.clone(), e))? {
        return Err(InstallError::Foreign(target));
    }
    std::fs::remove_file(&target).map_err(|e| InstallError::Io(target.clone(), e))?;
    let config = game_dir.join(CONFIG_NAME);
    if config.exists() {
        std::fs::remove_file(&config).map_err(|e| InstallError::Io(config, e))?;
    }
    Ok(Removed::Removed(target))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_build(dir: &Path) -> PathBuf {
        let path = dir.join("built.dll");
        std::fs::write(&path, format!("MZ....{MARKER}....")).unwrap();
        path
    }

    #[test]
    fn installs_then_uninstalls() {
        let tmp = tempfile::tempdir().unwrap();
        let built = fake_build(tmp.path());
        let game = tmp.path().join("game");
        std::fs::create_dir(&game).unwrap();

        let installed = install(&built, &game).unwrap();
        assert_eq!(installed, game.join(DLL_NAME));
        // Reinstalling over our own build is allowed.
        install(&built, &game).unwrap();

        assert_eq!(uninstall(&game).unwrap(), Removed::Removed(installed));
        assert_eq!(uninstall(&game).unwrap(), Removed::NothingInstalled);
    }

    #[test]
    fn the_config_gives_vice_city_back() {
        let tmp = tempfile::tempdir().unwrap();
        let vice_city = Path::new(r#"C:\Jeux\Rockstar "VC"\ViceCity"#);
        let written = write_config(tmp.path(), vice_city).unwrap();
        assert_eq!(written, config_path(&tmp.path().join("PES6.exe")));
        let paths = asset_bridge::config::read_paths(&written).unwrap();
        assert_eq!(paths.vice_city.as_deref(), Some(vice_city));
    }

    #[test]
    fn uninstall_removes_the_config() {
        let tmp = tempfile::tempdir().unwrap();
        let built = fake_build(tmp.path());
        let game = tmp.path().join("game");
        std::fs::create_dir(&game).unwrap();
        install(&built, &game).unwrap();
        write_config(&game, Path::new(r"C:\VC")).unwrap();
        uninstall(&game).unwrap();
        assert!(!game.join(CONFIG_NAME).exists());
    }

    #[test]
    fn never_touches_a_foreign_dll() {
        let tmp = tempfile::tempdir().unwrap();
        let built = fake_build(tmp.path());
        let foreign = tmp.path().join(DLL_NAME);
        std::fs::write(&foreign, b"MZ other mod").unwrap();

        assert!(matches!(
            install(&built, tmp.path()),
            Err(InstallError::Foreign(_))
        ));
        assert!(matches!(
            uninstall(tmp.path()),
            Err(InstallError::Foreign(_))
        ));
        assert_eq!(std::fs::read(&foreign).unwrap(), b"MZ other mod");
    }

    #[test]
    fn refuses_a_file_that_is_not_the_mod() {
        let tmp = tempfile::tempdir().unwrap();
        let not_built = tmp.path().join("random.dll");
        std::fs::write(&not_built, b"MZ").unwrap();
        assert!(matches!(
            install(&not_built, tmp.path()),
            Err(InstallError::NotBuilt(_))
        ));
    }
}
