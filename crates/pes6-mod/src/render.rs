//! Hooks into PES 6's Direct3D 8 rendering.
//!
//! Chain: the `Direct3DCreate8` slot of PES6.exe's import table is replaced,
//! the returned `IDirect3D8` gets its `CreateDevice` replaced, and the device
//! gets `Present` (overlay drawn just before the frame is shown), `Reset`, and
//! a few diagnostic hooks (how the game sends its matrices, for M2b).
//!
//! Vtable indices and constants are checked against `d3d8.h` / `d3d8types.h`
//! (Wine's public headers, identical interface layout to Microsoft's).

use std::ffi::c_void;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, AtomicU64, AtomicUsize, Ordering::Relaxed};

use crate::overlay::{self, Vertex};
use crate::pe::{self, Image};
use crate::proxy::log;

type Hresult = i32;
type Com = *mut c_void;

// IDirect3D8
const D3D_CREATE_DEVICE: usize = 15;
// IDirect3DDevice8
const DEV_RESET: usize = 14;
const DEV_PRESENT: usize = 15;
const DEV_BEGIN_SCENE: usize = 34;
const DEV_END_SCENE: usize = 35;
const DEV_SET_TRANSFORM: usize = 37;
const DEV_GET_VIEWPORT: usize = 41;
const DEV_SET_RENDER_STATE: usize = 50;
const DEV_APPLY_STATE_BLOCK: usize = 54;
const DEV_DELETE_STATE_BLOCK: usize = 56;
const DEV_CREATE_STATE_BLOCK: usize = 57;
const DEV_SET_TEXTURE: usize = 61;
const DEV_SET_TEXTURE_STAGE_STATE: usize = 63;
const DEV_DRAW_PRIMITIVE_UP: usize = 72;
const DEV_CREATE_VERTEX_SHADER: usize = 75;
const DEV_SET_VERTEX_SHADER: usize = 76;
const DEV_SET_VERTEX_SHADER_CONSTANT: usize = 79;
const DEV_SET_PIXEL_SHADER: usize = 88;

const D3DSBT_ALL: u32 = 1;
const D3DPT_TRIANGLELIST: u32 = 4;
const D3DTS_VIEW: u32 = 2;
const D3DTS_PROJECTION: u32 = 3;
const D3DTS_WORLD: u32 = 256;

const D3DRS_FILLMODE: u32 = 8;
const D3DRS_ZENABLE: u32 = 7;
const D3DRS_ZWRITEENABLE: u32 = 14;
const D3DRS_ALPHATESTENABLE: u32 = 15;
const D3DRS_SRCBLEND: u32 = 19;
const D3DRS_DESTBLEND: u32 = 20;
const D3DRS_CULLMODE: u32 = 22;
const D3DRS_ALPHABLENDENABLE: u32 = 27;
const D3DRS_FOGENABLE: u32 = 28;
const D3DRS_STENCILENABLE: u32 = 52;
const D3DRS_LIGHTING: u32 = 137;
const D3DRS_COLORWRITEENABLE: u32 = 168;
const D3DFILL_SOLID: u32 = 3;
const D3DBLEND_SRCALPHA: u32 = 5;
const D3DBLEND_INVSRCALPHA: u32 = 6;
const D3DCULL_NONE: u32 = 1;

const D3DTSS_COLOROP: u32 = 1;
const D3DTSS_COLORARG2: u32 = 3;
const D3DTSS_ALPHAOP: u32 = 4;
const D3DTSS_ALPHAARG2: u32 = 6;
const D3DTOP_DISABLE: u32 = 1;
const D3DTOP_SELECTARG2: u32 = 3;
const D3DTA_DIFFUSE: u32 = 0;

const PAGE_READWRITE: u32 = 0x04;

unsafe extern "system" {
    fn GetModuleHandleW(name: *const u16) -> *mut c_void;
    fn VirtualProtect(address: *mut c_void, size: usize, protect: u32, old: *mut u32) -> i32;
}

// ---------------------------------------------------------------------------
// Patching helpers
// ---------------------------------------------------------------------------

/// Replaces the pointer at `slot`, returns the previous one.
///
/// # Safety
/// `slot` must point to a pointer-sized, writable-once-unprotected location.
unsafe fn patch(slot: *mut usize, new: usize) -> Option<usize> {
    let mut old_protect = 0;
    // SAFETY: protection change on the 4 bytes of the slot only.
    if unsafe { VirtualProtect(slot.cast(), 4, PAGE_READWRITE, &mut old_protect) } == 0 {
        return None;
    }
    // SAFETY: the page is now writable; slots are aligned pointers.
    let old = unsafe { slot.replace(new) };
    let mut ignored = 0;
    // SAFETY: restores the original protection.
    unsafe { VirtualProtect(slot.cast(), 4, old_protect, &mut ignored) };
    Some(old)
}

/// Replaces entry `index` of the vtable of `object` with `hook`, keeping the
/// original in `original`. Vtables are shared by every object of the class, so
/// this only happens once.
///
/// # Safety
/// `object` must be a live COM object whose vtable has more than `index` entries.
unsafe fn hook_vtable(object: Com, index: usize, hook: usize, original: &AtomicUsize) {
    // SAFETY: a COM object starts with its vtable pointer.
    let slot = unsafe { (*(object as *const *mut usize)).add(index) };
    // SAFETY: reading a vtable entry.
    if unsafe { *slot } == hook {
        return;
    }
    // SAFETY: see above.
    if let Some(old) = unsafe { patch(slot, hook) } {
        original.store(old, Relaxed);
    } else {
        log(&format!(
            "impossible de modifier l'entrée {index} d'une vtable"
        ));
    }
}

/// Entry `index` of the vtable of `object`.
///
/// # Safety
/// As for [`hook_vtable`].
unsafe fn method(object: Com, index: usize) -> usize {
    // SAFETY: a COM object starts with its vtable pointer.
    unsafe { *(*(object as *const *const usize)).add(index) }
}

/// PES6.exe as mapped in memory.
struct LoadedModule {
    base: *const u8,
    size: usize,
}

impl Image for LoadedModule {
    fn bytes(&self, rva: usize, len: usize) -> Option<&[u8]> {
        if rva.checked_add(len)? > self.size {
            return None;
        }
        // SAFETY: inside the module's mapped image; only headers and the
        // import tables (in .rdata) are read.
        Some(unsafe { std::slice::from_raw_parts(self.base.add(rva), len) })
    }
}

impl LoadedModule {
    fn main() -> Option<Self> {
        // SAFETY: null asks for the main executable's module.
        let base = unsafe { GetModuleHandleW(std::ptr::null()) } as *const u8;
        if base.is_null() {
            return None;
        }
        let mut probe = Self { base, size: 0x1000 };
        let pe = probe.u32_at(0x3c)? as usize;
        // SizeOfImage, in the optional header.
        probe.size = pe + 24 + 60;
        let size = probe.u32_at(pe + 24 + 56)? as usize;
        Some(Self { base, size })
    }
}

// ---------------------------------------------------------------------------
// Installation
// ---------------------------------------------------------------------------

static REAL_CREATE8: AtomicUsize = AtomicUsize::new(0);
static REAL_CREATE_DEVICE: AtomicUsize = AtomicUsize::new(0);
static REAL_PRESENT: AtomicUsize = AtomicUsize::new(0);
static REAL_RESET: AtomicUsize = AtomicUsize::new(0);
static REAL_BEGIN_SCENE: AtomicUsize = AtomicUsize::new(0);
static REAL_SET_TRANSFORM: AtomicUsize = AtomicUsize::new(0);
static REAL_CREATE_VERTEX_SHADER: AtomicUsize = AtomicUsize::new(0);
static REAL_SET_VERTEX_SHADER_CONSTANT: AtomicUsize = AtomicUsize::new(0);

/// Redirects PES6.exe's `Direct3DCreate8` import. Only writes memory: safe to
/// call from `DllMain`.
pub(crate) fn install() {
    let Some(module) = LoadedModule::main() else {
        log("module principal introuvable");
        return;
    };
    let Some(rva) = pe::import_slot(&module, "d3d8.dll", "Direct3DCreate8") else {
        log("import Direct3DCreate8 introuvable : pas d'affichage du mod");
        return;
    };
    // SAFETY: the slot lies in the module's import address table.
    let slot = unsafe { module.base.add(rva) } as *mut usize;
    // SAFETY: as above.
    match unsafe { patch(slot, hooked_create8 as *const () as usize) } {
        Some(real) => {
            REAL_CREATE8.store(real, Relaxed);
            log(&format!(
                "Direct3DCreate8 redirigé (emplacement RVA {rva:#x})"
            ));
        }
        None => log("impossible de modifier l'emplacement de Direct3DCreate8"),
    }
}

unsafe extern "system" fn hooked_create8(sdk_version: u32) -> Com {
    type Create8 = unsafe extern "system" fn(u32) -> Com;
    // SAFETY: the stored address is the real Direct3DCreate8.
    let real: Create8 = unsafe { std::mem::transmute(REAL_CREATE8.load(Relaxed)) };
    // SAFETY: same contract as the real function.
    let d3d = unsafe { real(sdk_version) };
    log(&format!("Direct3DCreate8({sdk_version}) -> {d3d:p}"));
    if !d3d.is_null() {
        // SAFETY: `d3d` is a live IDirect3D8.
        unsafe {
            hook_vtable(
                d3d,
                D3D_CREATE_DEVICE,
                hooked_create_device as *const () as usize,
                &REAL_CREATE_DEVICE,
            )
        };
    }
    d3d
}

unsafe extern "system" fn hooked_create_device(
    this: Com,
    adapter: u32,
    device_type: u32,
    window: *mut c_void,
    flags: u32,
    params: *const u32,
    out: *mut Com,
) -> Hresult {
    type CreateDevice =
        unsafe extern "system" fn(Com, u32, u32, *mut c_void, u32, *const u32, *mut Com) -> Hresult;
    // SAFETY: the stored address is the real IDirect3D8::CreateDevice.
    let real: CreateDevice = unsafe { std::mem::transmute(REAL_CREATE_DEVICE.load(Relaxed)) };
    // SAFETY: same contract as the real method.
    let hr = unsafe { real(this, adapter, device_type, window, flags, params, out) };
    let (width, height) = if params.is_null() {
        (0, 0)
    } else {
        // SAFETY: D3DPRESENT_PARAMETERS starts with BackBufferWidth, BackBufferHeight.
        unsafe { (*params, *params.add(1)) }
    };
    log(&format!(
        "CreateDevice(adaptateur {adapter}, type {device_type}, drapeaux {flags:#x}, {width}x{height}) -> {hr:#x}"
    ));
    if hr >= 0 && !out.is_null() {
        // SAFETY: on success `out` holds a live IDirect3DDevice8.
        unsafe {
            let device = *out;
            hook_vtable(
                device,
                DEV_PRESENT,
                hooked_present as *const () as usize,
                &REAL_PRESENT,
            );
            hook_vtable(
                device,
                DEV_RESET,
                hooked_reset as *const () as usize,
                &REAL_RESET,
            );
            hook_vtable(
                device,
                DEV_BEGIN_SCENE,
                hooked_begin_scene as *const () as usize,
                &REAL_BEGIN_SCENE,
            );
            hook_vtable(
                device,
                DEV_SET_TRANSFORM,
                hooked_set_transform as *const () as usize,
                &REAL_SET_TRANSFORM,
            );
            hook_vtable(
                device,
                DEV_CREATE_VERTEX_SHADER,
                hooked_create_vertex_shader as *const () as usize,
                &REAL_CREATE_VERTEX_SHADER,
            );
            hook_vtable(
                device,
                DEV_SET_VERTEX_SHADER_CONSTANT,
                hooked_set_vertex_shader_constant as *const () as usize,
                &REAL_SET_VERTEX_SHADER_CONSTANT,
            );
        }
        log("périphérique Direct3D 8 accroché");
    }
    hr
}

// ---------------------------------------------------------------------------
// Diagnostics: how PES sends its matrices (fixed pipeline or shaders)
// ---------------------------------------------------------------------------

static FRAMES: AtomicU64 = AtomicU64::new(0);
static SCENES: AtomicU64 = AtomicU64::new(0);
static VIEW_SETS: AtomicU64 = AtomicU64::new(0);
static PROJECTION_SETS: AtomicU64 = AtomicU64::new(0);
static WORLD_SETS: AtomicU64 = AtomicU64::new(0);
static SHADER_CONSTANT_SETS: AtomicU64 = AtomicU64::new(0);
static SHADERS_CREATED: AtomicU32 = AtomicU32::new(0);
static LAST_VIEW: Mutex<[f32; 16]> = Mutex::new([0.0; 16]);
static LAST_PROJECTION: Mutex<[f32; 16]> = Mutex::new([0.0; 16]);

unsafe extern "system" fn hooked_begin_scene(this: Com) -> Hresult {
    SCENES.fetch_add(1, Relaxed);
    type BeginScene = unsafe extern "system" fn(Com) -> Hresult;
    // SAFETY: real IDirect3DDevice8::BeginScene.
    unsafe { std::mem::transmute::<usize, BeginScene>(REAL_BEGIN_SCENE.load(Relaxed))(this) }
}

unsafe extern "system" fn hooked_set_transform(
    this: Com,
    state: u32,
    matrix: *const [f32; 16],
) -> Hresult {
    let target = match state {
        D3DTS_VIEW => Some((&VIEW_SETS, &LAST_VIEW)),
        D3DTS_PROJECTION => Some((&PROJECTION_SETS, &LAST_PROJECTION)),
        _ => None,
    };
    if let Some((count, last)) = target {
        count.fetch_add(1, Relaxed);
        if !matrix.is_null()
            && let Ok(mut last) = last.lock()
        {
            // SAFETY: D3DMATRIX is 16 floats.
            *last = unsafe { *matrix };
        }
    } else if state == D3DTS_WORLD {
        WORLD_SETS.fetch_add(1, Relaxed);
    }
    type SetTransform = unsafe extern "system" fn(Com, u32, *const [f32; 16]) -> Hresult;
    // SAFETY: real IDirect3DDevice8::SetTransform.
    unsafe {
        std::mem::transmute::<usize, SetTransform>(REAL_SET_TRANSFORM.load(Relaxed))(
            this, state, matrix,
        )
    }
}

unsafe extern "system" fn hooked_create_vertex_shader(
    this: Com,
    declaration: *const u32,
    function: *const u32,
    handle: *mut u32,
    usage: u32,
) -> Hresult {
    SHADERS_CREATED.fetch_add(1, Relaxed);
    type CreateVertexShader =
        unsafe extern "system" fn(Com, *const u32, *const u32, *mut u32, u32) -> Hresult;
    // SAFETY: real IDirect3DDevice8::CreateVertexShader.
    unsafe {
        std::mem::transmute::<usize, CreateVertexShader>(REAL_CREATE_VERTEX_SHADER.load(Relaxed))(
            this,
            declaration,
            function,
            handle,
            usage,
        )
    }
}

unsafe extern "system" fn hooked_set_vertex_shader_constant(
    this: Com,
    register: u32,
    data: *const c_void,
    count: u32,
) -> Hresult {
    SHADER_CONSTANT_SETS.fetch_add(1, Relaxed);
    type SetConstant = unsafe extern "system" fn(Com, u32, *const c_void, u32) -> Hresult;
    // SAFETY: real IDirect3DDevice8::SetVertexShaderConstant.
    unsafe {
        std::mem::transmute::<usize, SetConstant>(REAL_SET_VERTEX_SHADER_CONSTANT.load(Relaxed))(
            this, register, data, count,
        )
    }
}

fn log_statistics(frames: u64) {
    let per_frame = |n: &AtomicU64| n.load(Relaxed) as f64 / frames as f64;
    log(&format!(
        "{frames} images : par image {:.1} scènes, {:.1} VIEW, {:.1} PROJECTION, {:.1} WORLD, {:.1} constantes de shader ; {} vertex shaders créés",
        per_frame(&SCENES),
        per_frame(&VIEW_SETS),
        per_frame(&PROJECTION_SETS),
        per_frame(&WORLD_SETS),
        per_frame(&SHADER_CONSTANT_SETS),
        SHADERS_CREATED.load(Relaxed),
    ));
    for (name, matrix) in [("VIEW", &LAST_VIEW), ("PROJECTION", &LAST_PROJECTION)] {
        if let Ok(m) = matrix.lock() {
            log(&format!("dernière matrice {name} : {:?}", *m));
        }
    }
}

// ---------------------------------------------------------------------------
// Present: draw the overlay on top of PES's finished frame
// ---------------------------------------------------------------------------

unsafe extern "system" fn hooked_reset(this: Com, params: *mut c_void) -> Hresult {
    type Reset = unsafe extern "system" fn(Com, *mut c_void) -> Hresult;
    // SAFETY: real IDirect3DDevice8::Reset.
    let hr = unsafe { std::mem::transmute::<usize, Reset>(REAL_RESET.load(Relaxed))(this, params) };
    log(&format!("Reset -> {hr:#x}"));
    hr
}

unsafe extern "system" fn hooked_present(
    this: Com,
    source: *const c_void,
    dest: *const c_void,
    window: *mut c_void,
    dirty: *const c_void,
) -> Hresult {
    let frame = FRAMES.fetch_add(1, Relaxed) + 1;
    if frame == 300 || frame.is_multiple_of(3600) {
        log_statistics(frame);
    }
    // SAFETY: `this` is the game's live device, outside any scene.
    unsafe { draw_overlay(this, &overlay::banner(frame)) };
    type Present = unsafe extern "system" fn(
        Com,
        *const c_void,
        *const c_void,
        *mut c_void,
        *const c_void,
    ) -> Hresult;
    // SAFETY: real IDirect3DDevice8::Present.
    unsafe {
        std::mem::transmute::<usize, Present>(REAL_PRESENT.load(Relaxed))(
            this, source, dest, window, dirty,
        )
    }
}

/// Draws pre-transformed triangles over the back buffer, then restores every
/// state the game had set.
///
/// # Safety
/// `device` must be a live IDirect3DDevice8, called outside BeginScene/EndScene.
unsafe fn draw_overlay(device: Com, vertices: &[Vertex]) {
    if vertices.is_empty() {
        return;
    }
    // SAFETY (whole block): every call goes through the device's own vtable
    // with the argument types of d3d8.h.
    unsafe {
        let create_state_block: unsafe extern "system" fn(Com, u32, *mut u32) -> Hresult =
            std::mem::transmute(method(device, DEV_CREATE_STATE_BLOCK));
        let apply_state_block: unsafe extern "system" fn(Com, u32) -> Hresult =
            std::mem::transmute(method(device, DEV_APPLY_STATE_BLOCK));
        let delete_state_block: unsafe extern "system" fn(Com, u32) -> Hresult =
            std::mem::transmute(method(device, DEV_DELETE_STATE_BLOCK));
        // The real BeginScene, so the overlay's scene is not counted in the
        // statistics (the vtable entry, if the hook could not be installed).
        let begin_scene: unsafe extern "system" fn(Com) -> Hresult =
            std::mem::transmute(match REAL_BEGIN_SCENE.load(Relaxed) {
                0 => method(device, DEV_BEGIN_SCENE),
                real => real,
            });
        let end_scene: unsafe extern "system" fn(Com) -> Hresult =
            std::mem::transmute(method(device, DEV_END_SCENE));
        let set_render_state: unsafe extern "system" fn(Com, u32, u32) -> Hresult =
            std::mem::transmute(method(device, DEV_SET_RENDER_STATE));
        let set_texture: unsafe extern "system" fn(Com, u32, Com) -> Hresult =
            std::mem::transmute(method(device, DEV_SET_TEXTURE));
        let set_tss: unsafe extern "system" fn(Com, u32, u32, u32) -> Hresult =
            std::mem::transmute(method(device, DEV_SET_TEXTURE_STAGE_STATE));
        let set_vertex_shader: unsafe extern "system" fn(Com, u32) -> Hresult =
            std::mem::transmute(method(device, DEV_SET_VERTEX_SHADER));
        let set_pixel_shader: unsafe extern "system" fn(Com, u32) -> Hresult =
            std::mem::transmute(method(device, DEV_SET_PIXEL_SHADER));
        let draw_primitive_up: unsafe extern "system" fn(
            Com,
            u32,
            u32,
            *const c_void,
            u32,
        ) -> Hresult = std::mem::transmute(method(device, DEV_DRAW_PRIMITIVE_UP));
        let get_viewport: unsafe extern "system" fn(Com, *mut [u32; 6]) -> Hresult =
            std::mem::transmute(method(device, DEV_GET_VIEWPORT));

        let mut viewport = [0u32; 6];
        if get_viewport(device, &mut viewport) < 0 || viewport[2] == 0 {
            return;
        }

        let mut saved = 0;
        if create_state_block(device, D3DSBT_ALL, &mut saved) < 0 {
            return;
        }
        if begin_scene(device) >= 0 {
            for (state, value) in [
                (D3DRS_FILLMODE, D3DFILL_SOLID),
                (D3DRS_ZENABLE, 0),
                (D3DRS_ZWRITEENABLE, 0),
                (D3DRS_ALPHATESTENABLE, 0),
                (D3DRS_ALPHABLENDENABLE, 1),
                (D3DRS_SRCBLEND, D3DBLEND_SRCALPHA),
                (D3DRS_DESTBLEND, D3DBLEND_INVSRCALPHA),
                (D3DRS_CULLMODE, D3DCULL_NONE),
                (D3DRS_FOGENABLE, 0),
                (D3DRS_STENCILENABLE, 0),
                (D3DRS_LIGHTING, 0),
                (D3DRS_COLORWRITEENABLE, 0xf),
            ] {
                set_render_state(device, state, value);
            }
            set_texture(device, 0, std::ptr::null_mut());
            set_tss(device, 0, D3DTSS_COLOROP, D3DTOP_SELECTARG2);
            set_tss(device, 0, D3DTSS_COLORARG2, D3DTA_DIFFUSE);
            set_tss(device, 0, D3DTSS_ALPHAOP, D3DTOP_SELECTARG2);
            set_tss(device, 0, D3DTSS_ALPHAARG2, D3DTA_DIFFUSE);
            set_tss(device, 1, D3DTSS_COLOROP, D3DTOP_DISABLE);
            set_tss(device, 1, D3DTSS_ALPHAOP, D3DTOP_DISABLE);
            set_pixel_shader(device, 0);
            set_vertex_shader(device, overlay::FVF);
            draw_primitive_up(
                device,
                D3DPT_TRIANGLELIST,
                (vertices.len() / 3) as u32,
                vertices.as_ptr().cast(),
                std::mem::size_of::<Vertex>() as u32,
            );
            end_scene(device);
        }
        apply_state_block(device, saved);
        delete_state_block(device, saved);
    }
}
