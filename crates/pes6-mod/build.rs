//! On 32-bit Windows, `extern "system"` functions are stdcall and get exported
//! as `DirectInput8Create@20`. The module definition file exports them under
//! the plain name PES6.exe imports.

fn main() {
    println!("cargo:rerun-if-changed=dinput8.def");
    let windows = std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows");
    let x86 = std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() == Ok("x86");
    let msvc = std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc");
    if windows && x86 && msvc {
        let def =
            std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("dinput8.def");
        println!("cargo:rustc-cdylib-link-arg=/DEF:{}", def.display());
    }
}
