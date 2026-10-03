//! Hooks into PES 6's Direct3D 8 rendering.
//!
//! Chain: the `Direct3DCreate8` slot of PES6.exe's import table is replaced,
//! the returned `IDirect3D8` gets its `CreateDevice` replaced, and the device
//! gets `Present` (the mod draws just before the frame is shown), `Reset`,
//! `SetTransform` and the four draw calls (to find PES's pitch camera, see
//! `scene`).
//!
//! Vtable indices and constants are checked against `d3d8.h` / `d3d8types.h`
//! (Wine's public headers, identical interface layout to Microsoft's).

use std::ffi::c_void;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering::Relaxed};
use std::time::Instant;

use crate::marker::{self, WorldVertex};
use crate::overlay::{self, Vertex};
use crate::pe::{self, Image};
use crate::proxy::log;
use crate::scene::{self, Camera, FrameCameras, Matrix};

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
const DEV_DRAW_PRIMITIVE: usize = 70;
const DEV_DRAW_INDEXED_PRIMITIVE: usize = 71;
const DEV_DRAW_PRIMITIVE_UP: usize = 72;
const DEV_DRAW_INDEXED_PRIMITIVE_UP: usize = 73;
const DEV_SET_VERTEX_SHADER: usize = 76;
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
const D3DRS_ZFUNC: u32 = 23;
const D3DRS_ALPHABLENDENABLE: u32 = 27;
const D3DRS_FOGENABLE: u32 = 28;
const D3DRS_ZBIAS: u32 = 47;
const D3DRS_STENCILENABLE: u32 = 52;
const D3DRS_LIGHTING: u32 = 137;
const D3DRS_COLORWRITEENABLE: u32 = 168;
const D3DFILL_SOLID: u32 = 3;
const D3DBLEND_SRCALPHA: u32 = 5;
const D3DBLEND_INVSRCALPHA: u32 = 6;
const D3DCULL_NONE: u32 = 1;
const D3DCMP_LESSEQUAL: u32 = 4;

const D3DTSS_COLOROP: u32 = 1;
const D3DTSS_COLORARG2: u32 = 3;
const D3DTSS_ALPHAOP: u32 = 4;
const D3DTSS_ALPHAARG2: u32 = 6;
const D3DTOP_DISABLE: u32 = 1;
const D3DTOP_SELECTARG2: u32 = 3;
const D3DTA_DIFFUSE: u32 = 0;

const PAGE_READWRITE: u32 = 0x04;

// HYPOTHÈSE: a camera drawing fewer primitives is a menu or the title
// screen, not the stadium. Measured in the M2b log: title screen 8 to 10,
// menus up to 519, match 12 000 and more. Replaced by the real match state
// once it is read from memory (M3).
const MIN_SCENE_PRIMITIVES: u64 = 5_000;

/// How often the camera and frame rate are written to the log.
const LOG_EVERY_SECONDS: u64 = 10;

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
/// `slot` must point to a pointer-sized location that may be made writable.
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
static REAL_SET_TRANSFORM: AtomicUsize = AtomicUsize::new(0);
static REAL_DRAW_PRIMITIVE: AtomicUsize = AtomicUsize::new(0);
static REAL_DRAW_INDEXED_PRIMITIVE: AtomicUsize = AtomicUsize::new(0);
static REAL_DRAW_PRIMITIVE_UP: AtomicUsize = AtomicUsize::new(0);
static REAL_DRAW_INDEXED_PRIMITIVE_UP: AtomicUsize = AtomicUsize::new(0);

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
        let hooks: [(usize, usize, &AtomicUsize); 7] = [
            (
                DEV_PRESENT,
                hooked_present as *const () as usize,
                &REAL_PRESENT,
            ),
            (DEV_RESET, hooked_reset as *const () as usize, &REAL_RESET),
            (
                DEV_SET_TRANSFORM,
                hooked_set_transform as *const () as usize,
                &REAL_SET_TRANSFORM,
            ),
            (
                DEV_DRAW_PRIMITIVE,
                hooked_draw_primitive as *const () as usize,
                &REAL_DRAW_PRIMITIVE,
            ),
            (
                DEV_DRAW_INDEXED_PRIMITIVE,
                hooked_draw_indexed_primitive as *const () as usize,
                &REAL_DRAW_INDEXED_PRIMITIVE,
            ),
            (
                DEV_DRAW_PRIMITIVE_UP,
                hooked_draw_primitive_up as *const () as usize,
                &REAL_DRAW_PRIMITIVE_UP,
            ),
            (
                DEV_DRAW_INDEXED_PRIMITIVE_UP,
                hooked_draw_indexed_primitive_up as *const () as usize,
                &REAL_DRAW_INDEXED_PRIMITIVE_UP,
            ),
        ];
        for (index, hook, original) in hooks {
            // SAFETY: on success `out` holds a live IDirect3DDevice8.
            unsafe { hook_vtable(*out, index, hook, original) };
        }
        log("périphérique Direct3D 8 accroché");
    }
    hr
}

// ---------------------------------------------------------------------------
// Camera tracking
// ---------------------------------------------------------------------------

/// Cameras of the frame being drawn by PES.
static FRAME: Mutex<FrameCameras> = Mutex::new(FrameCameras::new());
/// True while the mod itself draws: its calls are not PES's.
static MOD_DRAWING: AtomicBool = AtomicBool::new(false);

fn count_primitives(primitives: u32) {
    if !MOD_DRAWING.load(Relaxed)
        && let Ok(mut frame) = FRAME.lock()
    {
        frame.draw(primitives);
    }
}

unsafe extern "system" fn hooked_set_transform(
    this: Com,
    state: u32,
    matrix: *const Matrix,
) -> Hresult {
    if !matrix.is_null()
        && !MOD_DRAWING.load(Relaxed)
        && let Ok(mut frame) = FRAME.lock()
    {
        // SAFETY: D3DMATRIX is 16 floats.
        let m = unsafe { *matrix };
        match state {
            D3DTS_VIEW => frame.set_view(m),
            D3DTS_PROJECTION => frame.set_projection(m),
            _ => {}
        }
    }
    type SetTransform = unsafe extern "system" fn(Com, u32, *const Matrix) -> Hresult;
    // SAFETY: real IDirect3DDevice8::SetTransform.
    unsafe {
        std::mem::transmute::<usize, SetTransform>(REAL_SET_TRANSFORM.load(Relaxed))(
            this, state, matrix,
        )
    }
}

unsafe extern "system" fn hooked_draw_primitive(
    this: Com,
    kind: u32,
    start: u32,
    count: u32,
) -> Hresult {
    count_primitives(count);
    type Draw = unsafe extern "system" fn(Com, u32, u32, u32) -> Hresult;
    // SAFETY: real IDirect3DDevice8::DrawPrimitive.
    unsafe {
        std::mem::transmute::<usize, Draw>(REAL_DRAW_PRIMITIVE.load(Relaxed))(
            this, kind, start, count,
        )
    }
}

unsafe extern "system" fn hooked_draw_indexed_primitive(
    this: Com,
    kind: u32,
    min_index: u32,
    vertices: u32,
    start: u32,
    count: u32,
) -> Hresult {
    count_primitives(count);
    type Draw = unsafe extern "system" fn(Com, u32, u32, u32, u32, u32) -> Hresult;
    // SAFETY: real IDirect3DDevice8::DrawIndexedPrimitive.
    unsafe {
        std::mem::transmute::<usize, Draw>(REAL_DRAW_INDEXED_PRIMITIVE.load(Relaxed))(
            this, kind, min_index, vertices, start, count,
        )
    }
}

unsafe extern "system" fn hooked_draw_primitive_up(
    this: Com,
    kind: u32,
    count: u32,
    data: *const c_void,
    stride: u32,
) -> Hresult {
    count_primitives(count);
    type Draw = unsafe extern "system" fn(Com, u32, u32, *const c_void, u32) -> Hresult;
    // SAFETY: real IDirect3DDevice8::DrawPrimitiveUP.
    unsafe {
        std::mem::transmute::<usize, Draw>(REAL_DRAW_PRIMITIVE_UP.load(Relaxed))(
            this, kind, count, data, stride,
        )
    }
}

#[allow(clippy::too_many_arguments)]
unsafe extern "system" fn hooked_draw_indexed_primitive_up(
    this: Com,
    kind: u32,
    min_index: u32,
    vertices: u32,
    count: u32,
    indices: *const c_void,
    index_format: u32,
    data: *const c_void,
    stride: u32,
) -> Hresult {
    count_primitives(count);
    type Draw = unsafe extern "system" fn(
        Com,
        u32,
        u32,
        u32,
        u32,
        *const c_void,
        u32,
        *const c_void,
        u32,
    ) -> Hresult;
    // SAFETY: real IDirect3DDevice8::DrawIndexedPrimitiveUP.
    unsafe {
        std::mem::transmute::<usize, Draw>(REAL_DRAW_INDEXED_PRIMITIVE_UP.load(Relaxed))(
            this,
            kind,
            min_index,
            vertices,
            count,
            indices,
            index_format,
            data,
            stride,
        )
    }
}

// ---------------------------------------------------------------------------
// Present: draw the mod on top of PES's finished frame
// ---------------------------------------------------------------------------

static FRAMES: AtomicU64 = AtomicU64::new(0);

struct LogClock {
    last: Instant,
    frames_at_last: u64,
}

static LOG_CLOCK: Mutex<Option<LogClock>> = Mutex::new(None);

fn log_camera(frame: u64, camera: Option<&Camera>, cameras: usize) {
    let Ok(mut clock) = LOG_CLOCK.lock() else {
        return;
    };
    let now = Instant::now();
    let clock = clock.get_or_insert(LogClock {
        last: now,
        frames_at_last: frame,
    });
    let elapsed = now.duration_since(clock.last).as_secs_f64();
    if elapsed < LOG_EVERY_SECONDS as f64 {
        return;
    }
    let fps = (frame - clock.frames_at_last) as f64 / elapsed;
    clock.last = now;
    clock.frames_at_last = frame;
    match camera {
        Some(c) => {
            let origin = scene::project([0.0; 3], &c.view, &c.projection);
            log(&format!(
                "image {frame}, {fps:.1} images/s, {cameras} caméra(s) ; principale : {} primitives, origine à l'écran {origin:?}",
                c.primitives
            ));
            log(&format!("  VIEW {:?}", c.view));
            log(&format!("  PROJECTION {:?}", c.projection));
        }
        None => log(&format!(
            "image {frame}, {fps:.1} images/s, aucune caméra 3D"
        )),
    }
}

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
    let (camera, cameras) = FRAME.lock().map(|mut f| f.finish()).unwrap_or((None, 0));
    log_camera(frame, camera.as_ref(), cameras);
    MOD_DRAWING.store(true, Relaxed);
    // SAFETY: `this` is the game's live device, outside any scene.
    unsafe { draw_mod(this, camera.as_ref(), &overlay::banner(frame)) };
    MOD_DRAWING.store(false, Relaxed);
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

/// Draws the world marker with PES's pitch camera (if the frame had one) and
/// the 2D banner, then restores every state the game had set.
///
/// # Safety
/// `device` must be a live IDirect3DDevice8, called outside BeginScene/EndScene.
unsafe fn draw_mod(device: Com, camera: Option<&Camera>, banner: &[Vertex]) {
    // SAFETY (whole block): every call goes through the device's own vtable
    // with the argument types of d3d8.h.
    unsafe {
        let create_state_block: unsafe extern "system" fn(Com, u32, *mut u32) -> Hresult =
            std::mem::transmute(method(device, DEV_CREATE_STATE_BLOCK));
        let apply_state_block: unsafe extern "system" fn(Com, u32) -> Hresult =
            std::mem::transmute(method(device, DEV_APPLY_STATE_BLOCK));
        let delete_state_block: unsafe extern "system" fn(Com, u32) -> Hresult =
            std::mem::transmute(method(device, DEV_DELETE_STATE_BLOCK));
        let begin_scene: unsafe extern "system" fn(Com) -> Hresult =
            std::mem::transmute(method(device, DEV_BEGIN_SCENE));
        let end_scene: unsafe extern "system" fn(Com) -> Hresult =
            std::mem::transmute(method(device, DEV_END_SCENE));
        let set_render_state: unsafe extern "system" fn(Com, u32, u32) -> Hresult =
            std::mem::transmute(method(device, DEV_SET_RENDER_STATE));
        let set_transform: unsafe extern "system" fn(Com, u32, *const Matrix) -> Hresult =
            std::mem::transmute(method(device, DEV_SET_TRANSFORM));
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
                // Depth test against PES's own depth buffer, still intact at
                // Present (one scene per frame, the 2D HUD is pre-transformed):
                // players in front of the marker hide it.
                (D3DRS_ZENABLE, 1),
                (D3DRS_ZFUNC, D3DCMP_LESSEQUAL),
                (D3DRS_ZWRITEENABLE, 1),
                (D3DRS_ZBIAS, 2),
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

            if let Some(camera) = camera.filter(|c| c.primitives >= MIN_SCENE_PRIMITIVES) {
                let world: Vec<WorldVertex> = marker::marker([0.0; 3]);
                set_transform(device, D3DTS_WORLD, &scene::IDENTITY);
                set_transform(device, D3DTS_VIEW, &camera.view);
                set_transform(device, D3DTS_PROJECTION, &camera.projection);
                set_vertex_shader(device, marker::FVF);
                draw_primitive_up(
                    device,
                    D3DPT_TRIANGLELIST,
                    (world.len() / 3) as u32,
                    world.as_ptr().cast(),
                    std::mem::size_of::<WorldVertex>() as u32,
                );
            }

            // The banner is always on top.
            set_render_state(device, D3DRS_ZENABLE, 0);
            set_render_state(device, D3DRS_ZWRITEENABLE, 0);
            set_vertex_shader(device, overlay::FVF);
            draw_primitive_up(
                device,
                D3DPT_TRIANGLELIST,
                (banner.len() / 3) as u32,
                banner.as_ptr().cast(),
                std::mem::size_of::<Vertex>() as u32,
            );
            end_scene(device);
        }
        apply_state_block(device, saved);
        delete_state_block(device, saved);
    }
}
