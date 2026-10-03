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
