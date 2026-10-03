//! Chaos FC mod for the real PES 6.
//!
//! PES6.exe imports `DINPUT8.dll`. This crate builds a `dinput8.dll` that sits
//! next to the executable: Windows loads it instead of the system one, the mod
//! starts, and `DirectInput8Create` is forwarded to the real system DLL so the
//! game keeps its controls.
//!
//! Build: `cargo build -p pes6-mod --release --target i686-pc-windows-msvc`
//! (PES 6 is a 32-bit game). On any other target the crate is empty apart
//! from the portable helpers below.

use std::path::{Path, PathBuf};

/// Marker written into the DLL so the installer can tell our proxy from
/// another `dinput8.dll` (another mod, or a copy of the system one).
pub const MARKER: &str = "chaos-fc-pes6-mod";

/// Log file written next to PES6.exe.
pub const LOG_FILE_NAME: &str = "chaos-fc-mod.log";

/// Path of the real DirectInput 8 DLL inside the system directory.
pub fn system_dinput8(system_dir: &Path) -> PathBuf {
    system_dir.join("dinput8.dll")
}

/// Path of the log file for a game executable.
pub fn log_path(game_exe: &Path) -> PathBuf {
    game_exe
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(LOG_FILE_NAME)
}

pub mod bmp;
pub mod install;
pub mod marker;
pub mod overlay;
pub mod pe;
pub mod pes;
pub mod record;
pub mod scene;
pub mod tommy;
pub mod trace;

#[cfg(all(windows, target_arch = "x86"))]
mod assets;
#[cfg(all(windows, target_arch = "x86"))]
mod game;
#[cfg(all(windows, target_arch = "x86"))]
mod memory;
#[cfg(all(windows, target_arch = "x86"))]
mod proxy;
#[cfg(all(windows, target_arch = "x86"))]
mod render;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_dll_is_in_the_system_directory() {
        let path = system_dinput8(Path::new(r"C:\Windows\SysWOW64"));
        assert_eq!(path, Path::new(r"C:\Windows\SysWOW64\dinput8.dll"));
    }

    #[test]
    fn log_sits_next_to_the_game() {
        let path = log_path(Path::new(r"C:\Games\KONAMI\PES6.exe"));
        assert_eq!(path, Path::new(r"C:\Games\KONAMI\chaos-fc-mod.log"));
    }
}
