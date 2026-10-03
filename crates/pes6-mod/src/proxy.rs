//! The `dinput8.dll` proxy itself: entry point, log, and forwarding of
//! `DirectInput8Create` to the system DLL.

use std::ffi::c_void;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

type Hmodule = *mut c_void;
type Hresult = i32;
type DirectInput8CreateFn = unsafe extern "system" fn(
    hinst: *mut c_void,
    version: u32,
    riid: *const c_void,
    out: *mut *mut c_void,
    outer: *mut c_void,
) -> Hresult;

const DLL_PROCESS_ATTACH: u32 = 1;
const DLL_PROCESS_DETACH: u32 = 0;
const DIERR_GENERIC: Hresult = 0x8000_4005_u32 as Hresult;

unsafe extern "system" {
    fn GetSystemDirectoryW(buffer: *mut u16, size: u32) -> u32;
    fn LoadLibraryW(name: *const u16) -> Hmodule;
    fn GetProcAddress(module: Hmodule, name: *const u8) -> *mut c_void;
    fn DisableThreadLibraryCalls(module: Hmodule) -> i32;
}

/// Keeps the marker in the binary for the installer.
#[used]
static MARKER: [u8; super::MARKER.len()] = {
    let mut bytes = [0; super::MARKER.len()];
    let src = super::MARKER.as_bytes();
    let mut i = 0;
    while i < src.len() {
        bytes[i] = src[i];
        i += 1;
    }
    bytes
};

/// Appends one line to the log next to PES6.exe. Errors are ignored: the mod
/// must never take the game down because of its log.
pub(crate) fn log(message: &str) {
    static PATH: OnceLock<Option<PathBuf>> = OnceLock::new();
    let Some(path) = PATH.get_or_init(|| {
        std::env::current_exe()
            .ok()
            .map(|exe| super::log_path(&exe))
    }) else {
        return;
    };
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0);
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "[{secs:.3}] {message}");
    }
}

fn system_directory() -> Option<PathBuf> {
    let mut buffer = [0u16; 260];
    // SAFETY: the buffer is valid for its whole length.
    let len = unsafe { GetSystemDirectoryW(buffer.as_mut_ptr(), buffer.len() as u32) } as usize;
    (len > 0 && len < buffer.len()).then(|| PathBuf::from(String::from_utf16_lossy(&buffer[..len])))
}

/// The real `DirectInput8Create`, loaded on first use (never under the loader
/// lock, i.e. never from `DllMain`).
fn real_create() -> Option<DirectInput8CreateFn> {
    static REAL: OnceLock<Option<usize>> = OnceLock::new();
    let address = *REAL.get_or_init(|| {
        let path = super::system_dinput8(&system_directory()?);
        let wide: Vec<u16> = path
            .as_os_str()
            .to_string_lossy()
            .encode_utf16()
            .chain([0])
            .collect();
        // SAFETY: `wide` is a null-terminated UTF-16 path.
        let module = unsafe { LoadLibraryW(wide.as_ptr()) };
        if module.is_null() {
            log(&format!("impossible de charger {}", path.display()));
            return None;
        }
        // SAFETY: `module` is a loaded module, the name is null-terminated.
        let proc = unsafe { GetProcAddress(module, c"DirectInput8Create".as_ptr().cast()) };
        if proc.is_null() {
            log("DirectInput8Create introuvable dans la DLL système");
            return None;
        }
        log(&format!(
            "DirectInput 8 système chargé : {}",
            path.display()
        ));
        Some(proc as usize)
    });
    // SAFETY: the address comes from GetProcAddress for this exact export,
    // whose signature is documented by DirectInput 8.
    address.map(|a| unsafe { std::mem::transmute::<usize, DirectInput8CreateFn>(a) })
}

/// Exported under its undecorated name through `dinput8.def`.
///
/// # Safety
/// Called by the game with the arguments of the real DirectInput 8 function.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn DirectInput8Create(
    hinst: *mut c_void,
    version: u32,
    riid: *const c_void,
    out: *mut *mut c_void,
    outer: *mut c_void,
) -> Hresult {
    match real_create() {
        // SAFETY: same arguments, same contract as the system function.
        Some(create) => unsafe { create(hinst, version, riid, out, outer) },
        None => DIERR_GENERIC,
    }
}

/// # Safety
/// Called by the Windows loader only.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn DllMain(module: Hmodule, reason: u32, _reserved: *mut c_void) -> i32 {
    match reason {
        DLL_PROCESS_ATTACH => {
            // SAFETY: `module` is this DLL's handle, given by the loader.
            unsafe { DisableThreadLibraryCalls(module) };
            log(&format!(
                "{} {} chargé dans {}",
                super::MARKER,
                env!("CARGO_PKG_VERSION"),
                std::env::current_exe()
                    .map(|p| p.display().to_string())
                    .unwrap_or_default()
            ));
            crate::render::install();
        }
        DLL_PROCESS_DETACH => log("déchargé"),
        _ => {}
    }
    1
}
