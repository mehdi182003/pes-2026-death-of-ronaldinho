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

use crate::assets::{self, Tommy};
use crate::game;
use crate::marker::{self, WorldVertex};
use crate::overlay::{self, Vertex};
use crate::pe::{self, Image};
use crate::pes;
use crate::proxy::log;
use crate::scene::{self, Camera, FrameCameras, FrameSummary, Matrix};
use crate::trace::{self, Event};

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
const DEV_GET_DISPLAY_MODE: usize = 8;
const DEV_CREATE_IMAGE_SURFACE: usize = 27;
const DEV_GET_FRONT_BUFFER: usize = 30;
const DEV_SET_RENDER_TARGET: usize = 31;
const DEV_CLEAR: usize = 36;
// IUnknown
const COM_RELEASE: usize = 2;
// IDirect3DSurface8
const SURF_LOCK_RECT: usize = 9;
const SURF_UNLOCK_RECT: usize = 10;
const D3DFVF_XYZRHW: u32 = 0x0004;
const DEV_CREATE_TEXTURE: usize = 20;
// IDirect3DTexture8
const TEX_LOCK_RECT: usize = 16;
const TEX_UNLOCK_RECT: usize = 17;

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
const D3DTA_TEXTURE: u32 = 2;
const D3DTSS_COLORARG1: u32 = 2;
const D3DTSS_ALPHAARG1: u32 = 5;
const D3DTSS_ADDRESSU: u32 = 13;
const D3DTSS_ADDRESSV: u32 = 14;
const D3DTSS_MAGFILTER: u32 = 16;
const D3DTSS_MINFILTER: u32 = 17;
const D3DTSS_MIPFILTER: u32 = 18;
const D3DTOP_MODULATE: u32 = 4;
const D3DTEXF_NONE: u32 = 0;
const D3DTEXF_LINEAR: u32 = 2;
const D3DTADDRESS_WRAP: u32 = 1;
const D3DRS_ALPHAREF: u32 = 24;
const D3DRS_ALPHAFUNC: u32 = 25;
const D3DCMP_GREATER: u32 = 5;
const D3DFMT_A8R8G8B8: u32 = 21;
const D3DFMT_INDEX16: u32 = 101;
const D3DPOOL_MANAGED: u32 = 1;

const PAGE_READWRITE: u32 = 0x04;

// HYPOTHÈSE: a camera drawing fewer primitives is a menu or the title
// screen, not the stadium. Measured in the M2b log: title screen 8 to 10,
// menus up to 519, match 12 000 and more. Replaced by the real match state
// once it is read from memory (M3).
const MIN_SCENE_PRIMITIVES: u64 = 5_000;

/// How often the camera and frame rate are written to the log.
const LOG_EVERY_SECONDS: u64 = 10;

#[link(name = "user32")]
unsafe extern "system" {
    fn GetAsyncKeyState(key: i32) -> i16;
}

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
static REAL_BEGIN_SCENE: AtomicUsize = AtomicUsize::new(0);
static REAL_END_SCENE: AtomicUsize = AtomicUsize::new(0);
static REAL_CLEAR: AtomicUsize = AtomicUsize::new(0);
static REAL_SET_RENDER_TARGET: AtomicUsize = AtomicUsize::new(0);
static REAL_SET_RENDER_STATE: AtomicUsize = AtomicUsize::new(0);
static REAL_SET_VERTEX_SHADER: AtomicUsize = AtomicUsize::new(0);

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
    log(&format!(
        "CreateDevice(adaptateur {adapter}, type {device_type}, drapeaux {flags:#x}) -> {hr:#x}"
    ));
    if !params.is_null() {
        // SAFETY: D3DPRESENT_PARAMETERS is 13 DWORDs (d3d8types.h).
        let p = unsafe { std::slice::from_raw_parts(params, 13) };
        log(&format!(
            "  {}x{}, format {}, {} tampon(s), multi-échantillon {}, échange {}, fenêtré {}, profondeur auto {} format {}, drapeaux {:#x}, {} Hz, intervalle {:#x}",
            p[0], p[1], p[2], p[3], p[4], p[5], p[7], p[8], p[9], p[10], p[11], p[12]
        ));
    }
    if hr >= 0 && !out.is_null() {
        let hooks: [(usize, usize, &AtomicUsize); 13] = [
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
            (
                DEV_BEGIN_SCENE,
                hooked_begin_scene as *const () as usize,
                &REAL_BEGIN_SCENE,
            ),
            (
                DEV_END_SCENE,
                hooked_end_scene as *const () as usize,
                &REAL_END_SCENE,
            ),
            (DEV_CLEAR, hooked_clear as *const () as usize, &REAL_CLEAR),
            (
                DEV_SET_RENDER_TARGET,
                hooked_set_render_target as *const () as usize,
                &REAL_SET_RENDER_TARGET,
            ),
            (
                DEV_SET_RENDER_STATE,
                hooked_set_render_state as *const () as usize,
                &REAL_SET_RENDER_STATE,
            ),
            (
                DEV_SET_VERTEX_SHADER,
                hooked_set_vertex_shader as *const () as usize,
                &REAL_SET_VERTEX_SHADER,
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
// Camera tracking, frame trace
// ---------------------------------------------------------------------------

/// Cameras of the frame being drawn by PES.
static FRAME: Mutex<FrameCameras> = Mutex::new(FrameCameras::new());
/// True while the mod itself draws: its calls are not PES's.
static MOD_DRAWING: AtomicBool = AtomicBool::new(false);
/// The calls of the frame being traced, if one is.
static TRACE: Mutex<Option<Vec<Event>>> = Mutex::new(None);
/// Trace the next frame.
static TRACE_NEXT: AtomicBool = AtomicBool::new(false);

/// Most events a trace keeps (a match frame has a few thousand calls).
const TRACE_LIMIT: usize = 20_000;

fn record(event: Event) {
    if MOD_DRAWING.load(Relaxed) {
        return;
    }
    record_always(event);
}

fn record_always(event: Event) {
    if let Ok(mut trace) = TRACE.lock()
        && let Some(events) = trace.as_mut()
        && events.len() < TRACE_LIMIT
    {
        events.push(event);
    }
}

fn count_primitives(api: &'static str, primitives: u32) {
    if MOD_DRAWING.load(Relaxed) {
        return;
    }
    if let Ok(mut frame) = FRAME.lock() {
        frame.draw(primitives);
    }
    record(Event::Draw { api, primitives });
}

/// The real method stored in `original`, with the signature `F`.
///
/// # Safety
/// `original` must hold the address of a function of type `F`.
unsafe fn real<F: Copy>(original: &AtomicUsize) -> F {
    let address = original.load(Relaxed);
    // SAFETY: as required by this function; F is a function pointer type.
    unsafe { std::mem::transmute_copy(&address) }
}

unsafe extern "system" fn hooked_set_transform(
    this: Com,
    state: u32,
    matrix: *const Matrix,
) -> Hresult {
    if !matrix.is_null() && !MOD_DRAWING.load(Relaxed) {
        // SAFETY: D3DMATRIX is 16 floats.
        let m = unsafe { *matrix };
        if matches!(state, D3DTS_VIEW | D3DTS_PROJECTION) {
            let changes = FRAME
                .lock()
                .ok()
                .and_then(|f| f.current())
                .is_some_and(|c| {
                    if state == D3DTS_VIEW {
                        c.view != m
                    } else {
                        c.projection != m
                    }
                });
            if changes {
                // SAFETY: `this` is the game's device, inside its scene.
                unsafe { draw_world_once(this, "PES change de caméra") };
            }
        }
        if let Ok(mut frame) = FRAME.lock() {
            match state {
                D3DTS_VIEW => frame.set_view(m),
                D3DTS_PROJECTION => frame.set_projection(m),
                D3DTS_WORLD.. => frame.set_world(state - D3DTS_WORLD, m),
                _ => {}
            }
        }
        record(match state {
            D3DTS_VIEW => Event::View {
                translation: [m[12], m[13], m[14]],
            },
            D3DTS_PROJECTION => Event::Projection { scale_x: m[0] },
            _ => Event::World {
                translation: [m[12], m[13], m[14]],
                scale: (m[0] * m[0] + m[1] * m[1] + m[2] * m[2]).sqrt(),
            },
        });
    }
    // SAFETY: real IDirect3DDevice8::SetTransform.
    unsafe {
        real::<unsafe extern "system" fn(Com, u32, *const Matrix) -> Hresult>(&REAL_SET_TRANSFORM)(
            this, state, matrix,
        )
    }
}

unsafe extern "system" fn hooked_set_vertex_shader(this: Com, handle: u32) -> Hresult {
    // HYPOTHÈSE: PES draws its HUD and menus with pre-transformed vertices
    // (D3DFVF_XYZRHW), like the mod's banner; switching to them ends its 3D.
    // The frame trace shows whether that holds.
    if handle & D3DFVF_XYZRHW != 0 && handle < 0x1_0000 && !MOD_DRAWING.load(Relaxed) {
        // SAFETY: `this` is the game's device, inside its scene.
        unsafe { draw_world_once(this, "PES passe aux sommets 2D") };
    }
    record(Event::VertexShader(handle));
    // SAFETY: real IDirect3DDevice8::SetVertexShader.
    unsafe {
        real::<unsafe extern "system" fn(Com, u32) -> Hresult>(&REAL_SET_VERTEX_SHADER)(
            this, handle,
        )
    }
}

unsafe extern "system" fn hooked_begin_scene(this: Com) -> Hresult {
    record(Event::BeginScene);
    // SAFETY: real IDirect3DDevice8::BeginScene.
    unsafe { real::<unsafe extern "system" fn(Com) -> Hresult>(&REAL_BEGIN_SCENE)(this) }
}

unsafe extern "system" fn hooked_end_scene(this: Com) -> Hresult {
    if !MOD_DRAWING.load(Relaxed) {
        // SAFETY: `this` is the game's device, inside its scene.
        unsafe { draw_world_once(this, "fin de la scène de PES") };
    }
    record(Event::EndScene);
    // SAFETY: real IDirect3DDevice8::EndScene.
    unsafe { real::<unsafe extern "system" fn(Com) -> Hresult>(&REAL_END_SCENE)(this) }
}

unsafe extern "system" fn hooked_clear(
    this: Com,
    count: u32,
    rects: *const c_void,
    flags: u32,
    color: u32,
    z: f32,
    stencil: u32,
) -> Hresult {
    record(Event::Clear { flags });
    // SAFETY: real IDirect3DDevice8::Clear.
    unsafe {
        real::<unsafe extern "system" fn(Com, u32, *const c_void, u32, u32, f32, u32) -> Hresult>(
            &REAL_CLEAR,
        )(this, count, rects, flags, color, z, stencil)
    }
}

unsafe extern "system" fn hooked_set_render_target(this: Com, target: Com, depth: Com) -> Hresult {
    record(Event::SetRenderTarget);
    // SAFETY: real IDirect3DDevice8::SetRenderTarget.
    unsafe {
        real::<unsafe extern "system" fn(Com, Com, Com) -> Hresult>(&REAL_SET_RENDER_TARGET)(
            this, target, depth,
        )
    }
}

unsafe extern "system" fn hooked_set_render_state(this: Com, state: u32, value: u32) -> Hresult {
    // Only the states that tell 3D from 2D.
    if matches!(
        state,
        D3DRS_ZENABLE | D3DRS_ZWRITEENABLE | D3DRS_ZFUNC | D3DRS_ALPHABLENDENABLE | D3DRS_FOGENABLE
    ) {
        record(Event::RenderState { state, value });
    }
    // SAFETY: real IDirect3DDevice8::SetRenderState.
    unsafe {
        real::<unsafe extern "system" fn(Com, u32, u32) -> Hresult>(&REAL_SET_RENDER_STATE)(
            this, state, value,
        )
    }
}

unsafe extern "system" fn hooked_draw_primitive(
    this: Com,
    kind: u32,
    start: u32,
    count: u32,
) -> Hresult {
    count_primitives("DrawPrimitive", count);
    // SAFETY: real IDirect3DDevice8::DrawPrimitive.
    unsafe {
        real::<unsafe extern "system" fn(Com, u32, u32, u32) -> Hresult>(&REAL_DRAW_PRIMITIVE)(
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
    count_primitives("DrawIndexedPrimitive", count);
    // SAFETY: real IDirect3DDevice8::DrawIndexedPrimitive.
    unsafe {
        real::<unsafe extern "system" fn(Com, u32, u32, u32, u32, u32) -> Hresult>(
            &REAL_DRAW_INDEXED_PRIMITIVE,
        )(this, kind, min_index, vertices, start, count)
    }
}

unsafe extern "system" fn hooked_draw_primitive_up(
    this: Com,
    kind: u32,
    count: u32,
    data: *const c_void,
    stride: u32,
) -> Hresult {
    count_primitives("DrawPrimitiveUP", count);
    // SAFETY: real IDirect3DDevice8::DrawPrimitiveUP.
    unsafe {
        real::<unsafe extern "system" fn(Com, u32, u32, *const c_void, u32) -> Hresult>(
            &REAL_DRAW_PRIMITIVE_UP,
        )(this, kind, count, data, stride)
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
    count_primitives("DrawIndexedPrimitiveUP", count);
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
        real::<Draw>(&REAL_DRAW_INDEXED_PRIMITIVE_UP)(
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
// The mod's 3D objects, drawn inside PES's scene
// ---------------------------------------------------------------------------

/// The mod's 3D objects were drawn in this frame.
static WORLD_DRAWN: AtomicBool = AtomicBool::new(false);
/// Last WORLD matrix seen for logic coordinates (see `FrameSummary::logic_world`).
static LOGIC_WORLD: Mutex<Option<Matrix>> = Mutex::new(None);

/// Draws the mod's 3D objects once per frame, at the first point where PES's
/// 3D scene is over: its camera is the current one and has drawn the
/// stadium, the depth buffer still holds the scene, and PES's 2D (HUD,
/// menus) will be drawn over them.
///
/// # Safety
/// `device` must be the game's device, inside its scene.
unsafe fn draw_world_once(device: Com, reason: &'static str) {
    if WORLD_DRAWN.load(Relaxed) {
        return;
    }
    let Some(camera) = FRAME.lock().ok().and_then(|f| f.current()) else {
        return;
    };
    if camera.primitives < MIN_SCENE_PRIMITIVES {
        return;
    }
    WORLD_DRAWN.store(true, Relaxed);
    record_always(Event::ModWorld(reason));
    let logic_world = LOGIC_WORLD.lock().ok().and_then(|last| *last);
    MOD_DRAWING.store(true, Relaxed);
    // SAFETY: as required by this function.
    unsafe { draw_world(device, &camera, logic_world) };
    MOD_DRAWING.store(false, Relaxed);
}

/// Sets the states for untextured, coloured triangles.
///
/// # Safety
/// `device` must be a live IDirect3DDevice8.
unsafe fn untextured_states(device: Com) {
    // SAFETY (whole block): calls through the vtable, types of d3d8.h.
    unsafe {
        let set_render_state: unsafe extern "system" fn(Com, u32, u32) -> Hresult =
            std::mem::transmute(method(device, DEV_SET_RENDER_STATE));
        let set_texture: unsafe extern "system" fn(Com, u32, Com) -> Hresult =
            std::mem::transmute(method(device, DEV_SET_TEXTURE));
        let set_tss: unsafe extern "system" fn(Com, u32, u32, u32) -> Hresult =
            std::mem::transmute(method(device, DEV_SET_TEXTURE_STAGE_STATE));
        let set_pixel_shader: unsafe extern "system" fn(Com, u32) -> Hresult =
            std::mem::transmute(method(device, DEV_SET_PIXEL_SHADER));
        for (state, value) in [
            (D3DRS_FILLMODE, D3DFILL_SOLID),
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
    }
}

/// Draws the markers and Tommy with PES's camera, then restores every state.
///
/// # Safety
/// `device` must be a live IDirect3DDevice8, inside a scene.
unsafe fn draw_world(device: Com, camera: &Camera, logic_world: Option<Matrix>) {
    // SAFETY (whole block): calls through the vtable, types of d3d8.h.
    unsafe {
        let create_state_block: unsafe extern "system" fn(Com, u32, *mut u32) -> Hresult =
            std::mem::transmute(method(device, DEV_CREATE_STATE_BLOCK));
        let apply_state_block: unsafe extern "system" fn(Com, u32) -> Hresult =
            std::mem::transmute(method(device, DEV_APPLY_STATE_BLOCK));
        let delete_state_block: unsafe extern "system" fn(Com, u32) -> Hresult =
            std::mem::transmute(method(device, DEV_DELETE_STATE_BLOCK));
        let set_render_state: unsafe extern "system" fn(Com, u32, u32) -> Hresult =
            std::mem::transmute(method(device, DEV_SET_RENDER_STATE));
        let set_transform: unsafe extern "system" fn(Com, u32, *const Matrix) -> Hresult =
            std::mem::transmute(method(device, DEV_SET_TRANSFORM));
        let set_vertex_shader: unsafe extern "system" fn(Com, u32) -> Hresult =
            std::mem::transmute(method(device, DEV_SET_VERTEX_SHADER));
        let draw_primitive_up: unsafe extern "system" fn(
            Com,
            u32,
            u32,
            *const c_void,
            u32,
        ) -> Hresult = std::mem::transmute(method(device, DEV_DRAW_PRIMITIVE_UP));

        let mut saved = 0;
        if create_state_block(device, D3DSBT_ALL, &mut saved) < 0 {
            return;
        }
        untextured_states(device);
        for (state, value) in [
            // Depth test against PES's depth buffer, which still holds the
            // scene: players in front of the objects hide them.
            (D3DRS_ZENABLE, 1),
            (D3DRS_ZFUNC, D3DCMP_LESSEQUAL),
            (D3DRS_ZWRITEENABLE, 1),
            (D3DRS_ZBIAS, 2),
        ] {
            set_render_state(device, state, value);
        }

        let marker: Vec<WorldVertex> = marker::marker([0.0; 3]);
        set_transform(device, D3DTS_WORLD, &scene::IDENTITY);
        set_transform(device, D3DTS_VIEW, &camera.view);
        set_transform(device, D3DTS_PROJECTION, &camera.projection);
        set_vertex_shader(device, marker::FVF);
        draw_primitive_up(
            device,
            D3DPT_TRIANGLELIST,
            (marker.len() / 3) as u32,
            marker.as_ptr().cast(),
            std::mem::size_of::<WorldVertex>() as u32,
        );

        if let Some(logic) = logic_world {
            let mut pins = game::ball_position()
                .map(|ball| marker::pin(ball, pes::LOGIC_UNITS_PER_METRE))
                .unwrap_or_default();
            for player in game::players() {
                if player.on_pitch() {
                    pins.extend(marker::flag(
                        player.position,
                        pes::LOGIC_UNITS_PER_METRE,
                        marker::role_color(player.role()),
                    ));
                }
            }
            set_transform(device, D3DTS_WORLD, &logic);
            if !pins.is_empty() {
                draw_primitive_up(
                    device,
                    D3DPT_TRIANGLELIST,
                    (pins.len() / 3) as u32,
                    pins.as_ptr().cast(),
                    std::mem::size_of::<WorldVertex>() as u32,
                );
            }
            if let Some(tommy) = assets::tommy() {
                draw_tommy(device, tommy);
            }
        }
        apply_state_block(device, saved);
        delete_state_block(device, saved);
    }
}

// ---------------------------------------------------------------------------
// Present: banner, frame statistics, trace and capture
// ---------------------------------------------------------------------------

static FRAMES: AtomicU64 = AtomicU64::new(0);
/// Frames with a match camera, to trace one about 10 s into the match.
static MATCH_FRAMES: AtomicU64 = AtomicU64::new(0);
const TRACE_AT_MATCH_FRAME: u64 = 600;
/// Virtual-key code of the capture key, F9.
const CAPTURE_KEY: i32 = 0x78;
static CAPTURE_KEY_DOWN: AtomicBool = AtomicBool::new(false);
static CAPTURES: AtomicU64 = AtomicU64::new(0);

struct LogClock {
    last: Instant,
    frames_at_last: u64,
}

static LOG_CLOCK: Mutex<Option<LogClock>> = Mutex::new(None);

/// Objects listed per log entry (the biggest ones).
const LOGGED_OBJECTS: usize = 8;

fn log_frame(frame: u64, summary: &FrameSummary) {
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
    let Some(c) = &summary.main else {
        log(&format!(
            "image {frame}, {fps:.1} images/s, aucune caméra 3D"
        ));
        return;
    };
    let origin = scene::project([0.0; 3], &c.view, &c.projection);
    log(&format!(
        "image {frame}, {fps:.1} images/s, {} caméra(s) ; principale : {} primitives, origine à l'écran {origin:?}",
        summary.cameras, c.primitives
    ));
    if c.primitives < MIN_SCENE_PRIMITIVES {
        return;
    }
    log(&format!("  ballon : {:?}", game::ball_position()));
    for p in game::players().iter().filter(|p| p.on_pitch()) {
        let [x, y, z] = p.position;
        log(&format!(
            "    emplacement {:2} : équipe {}, n° {:2}, pos ({x:9.1}, {y:7.1}, {z:9.1})",
            p.slot, p.team, p.number
        ));
    }
    for object in summary.objects.iter().take(LOGGED_OBJECTS) {
        let [x, y, z] = object.position();
        log(&format!(
            "    objet pos ({x:9.1}, {y:9.1}, {z:9.1}) échelle {:.3} : {} primitives, {} appels",
            object.scale(),
            object.primitives,
            object.draws
        ));
    }
}

unsafe extern "system" fn hooked_reset(this: Com, params: *mut c_void) -> Hresult {
    // SAFETY: real IDirect3DDevice8::Reset.
    let hr = unsafe {
        real::<unsafe extern "system" fn(Com, *mut c_void) -> Hresult>(&REAL_RESET)(this, params)
    };
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
    let summary = FRAME.lock().map(|mut f| f.finish()).unwrap_or_default();
    log_frame(frame, &summary);
    if let Some(found) = summary.logic_world()
        && let Ok(mut last) = LOGIC_WORLD.lock()
    {
        *last = Some(found);
    }
    if summary
        .main
        .is_some_and(|c| c.primitives >= MIN_SCENE_PRIMITIVES)
        && MATCH_FRAMES.fetch_add(1, Relaxed) + 1 == TRACE_AT_MATCH_FRAME
    {
        TRACE_NEXT.store(true, Relaxed);
    }
    if !WORLD_DRAWN.load(Relaxed)
        && summary
            .main
            .is_some_and(|c| c.primitives >= MIN_SCENE_PRIMITIVES)
    {
        record_always(Event::ModWorld(
            "aucun point d'insertion : objets 3D non dessinés",
        ));
    }
    // End of the traced frame: write it to the log.
    if let Ok(mut trace) = TRACE.lock()
        && let Some(events) = trace.take()
    {
        log(&format!(
            "trace de l'image {frame} ({} appels) :",
            events.len()
        ));
        for line in trace::summarize(&events) {
            log(&format!("  {line}"));
        }
    }

    MOD_DRAWING.store(true, Relaxed);
    // SAFETY: `this` is the game's live device, outside any scene.
    unsafe { draw_banner(this, &overlay::banner(frame)) };
    MOD_DRAWING.store(false, Relaxed);
    type Present = unsafe extern "system" fn(
        Com,
        *const c_void,
        *const c_void,
        *mut c_void,
        *const c_void,
    ) -> Hresult;
    // SAFETY: real IDirect3DDevice8::Present.
    let hr = unsafe { real::<Present>(&REAL_PRESENT)(this, source, dest, window, dirty) };

    // The next frame starts here.
    WORLD_DRAWN.store(false, Relaxed);
    // SAFETY: GetAsyncKeyState only reads the keyboard state.
    let key_down = unsafe { GetAsyncKeyState(CAPTURE_KEY) } as u16 & 0x8000 != 0;
    if key_down && !CAPTURE_KEY_DOWN.swap(true, Relaxed) {
        // SAFETY: `this` is the game's live device, outside any scene.
        unsafe { capture(this) };
        TRACE_NEXT.store(true, Relaxed);
    } else if !key_down {
        CAPTURE_KEY_DOWN.store(false, Relaxed);
    }
    if TRACE_NEXT.swap(false, Relaxed)
        && let Ok(mut trace) = TRACE.lock()
    {
        *trace = Some(Vec::new());
    }
    hr
}

/// Draws the 2D banner over the finished frame, then restores every state.
///
/// # Safety
/// `device` must be a live IDirect3DDevice8, called outside BeginScene/EndScene.
unsafe fn draw_banner(device: Com, banner: &[Vertex]) {
    // SAFETY (whole block): calls through the vtable, types of d3d8.h.
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
        let set_vertex_shader: unsafe extern "system" fn(Com, u32) -> Hresult =
            std::mem::transmute(method(device, DEV_SET_VERTEX_SHADER));
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
        if banner.is_empty() || get_viewport(device, &mut viewport) < 0 || viewport[2] == 0 {
            return;
        }
        let mut saved = 0;
        if create_state_block(device, D3DSBT_ALL, &mut saved) < 0 {
            return;
        }
        if begin_scene(device) >= 0 {
            untextured_states(device);
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

/// Saves what is on screen (the front buffer, i.e. the frame just shown) to
/// `chaos-fc-capture-<n>.bmp` next to PES6.exe.
///
/// # Safety
/// `device` must be a live IDirect3DDevice8, outside any scene.
unsafe fn capture(device: Com) {
    // SAFETY (whole block): calls through the vtables, types of d3d8.h.
    unsafe {
        let get_display_mode: unsafe extern "system" fn(Com, *mut [u32; 4]) -> Hresult =
            std::mem::transmute(method(device, DEV_GET_DISPLAY_MODE));
        let create_image_surface: unsafe extern "system" fn(
            Com,
            u32,
            u32,
            u32,
            *mut Com,
        ) -> Hresult = std::mem::transmute(method(device, DEV_CREATE_IMAGE_SURFACE));
        let get_front_buffer: unsafe extern "system" fn(Com, Com) -> Hresult =
            std::mem::transmute(method(device, DEV_GET_FRONT_BUFFER));

        // D3DDISPLAYMODE: width, height, refresh rate, format. The front
        // buffer copy has the size of the display.
        let mut mode = [0u32; 4];
        if get_display_mode(device, &mut mode) < 0 {
            log("capture impossible : GetDisplayMode");
            return;
        }
        let (width, height) = (mode[0], mode[1]);
        let mut surface: Com = std::ptr::null_mut();
        if create_image_surface(device, width, height, D3DFMT_A8R8G8B8, &mut surface) < 0
            || surface.is_null()
        {
            log("capture impossible : CreateImageSurface");
            return;
        }
        let release: unsafe extern "system" fn(Com) -> u32 =
            std::mem::transmute(method(surface, COM_RELEASE));
        let hr = get_front_buffer(device, surface);
        if hr < 0 {
            log(&format!("capture impossible : GetFrontBuffer -> {hr:#x}"));
            release(surface);
            return;
        }
        let lock: unsafe extern "system" fn(Com, *mut [usize; 2], *const c_void, u32) -> Hresult =
            std::mem::transmute(method(surface, SURF_LOCK_RECT));
        let unlock: unsafe extern "system" fn(Com) -> Hresult =
            std::mem::transmute(method(surface, SURF_UNLOCK_RECT));
        let mut locked = [0usize; 2];
        if lock(surface, &mut locked, std::ptr::null(), 0) >= 0 {
            let (pitch, bits) = (locked[0], locked[1] as *const u8);
            let row = width as usize * 4;
            let mut pixels = Vec::with_capacity(row * height as usize);
            for y in 0..height as usize {
                pixels.extend_from_slice(std::slice::from_raw_parts(bits.add(y * pitch), row));
            }
            unlock(surface);
            let n = CAPTURES.fetch_add(1, Relaxed) + 1;
            let path = std::env::current_exe()
                .map(|exe| exe.with_file_name(format!("chaos-fc-capture-{n}.bmp")))
                .unwrap_or_default();
            match std::fs::write(&path, crate::bmp::encode_bgra(width, height, &pixels)) {
                Ok(()) => log(&format!("capture {width}x{height} : {}", path.display())),
                Err(err) => log(&format!("capture impossible : {err}")),
            }
        }
        release(surface);
    }
}

// ---------------------------------------------------------------------------
// Tommy
// ---------------------------------------------------------------------------

/// Tommy's textures on the device, created on first use, by name. In the
/// managed pool: Direct3D keeps them across `Reset`.
static TEXTURES: Mutex<Vec<(String, usize)>> = Mutex::new(Vec::new());

/// Creates a texture from RGBA pixels, or null.
///
/// # Safety
/// `device` must be a live IDirect3DDevice8.
unsafe fn create_texture(device: Com, texture: &asset_bridge::model::Texture) -> Com {
    // SAFETY (whole block): calls through the vtables, types of d3d8.h.
    unsafe {
        let create: unsafe extern "system" fn(
            Com,
            u32,
            u32,
            u32,
            u32,
            u32,
            u32,
            *mut Com,
        ) -> Hresult = std::mem::transmute(method(device, DEV_CREATE_TEXTURE));
        let mut out: Com = std::ptr::null_mut();
        if create(
            device,
            texture.width,
            texture.height,
            1,
            0,
            D3DFMT_A8R8G8B8,
            D3DPOOL_MANAGED,
            &mut out,
        ) < 0
            || out.is_null()
        {
            return std::ptr::null_mut();
        }
        let lock: unsafe extern "system" fn(
            Com,
            u32,
            *mut [usize; 2],
            *const c_void,
            u32,
        ) -> Hresult = std::mem::transmute(method(out, TEX_LOCK_RECT));
        let unlock: unsafe extern "system" fn(Com, u32) -> Hresult =
            std::mem::transmute(method(out, TEX_UNLOCK_RECT));
        // D3DLOCKED_RECT: pitch (INT), then a pointer to the pixels.
        let mut locked = [0usize; 2];
        if lock(out, 0, &mut locked, std::ptr::null(), 0) >= 0 {
            let (pitch, bits) = (locked[0], locked[1] as *mut u8);
            let width = texture.width as usize;
            for row in 0..texture.height as usize {
                let src = &texture.rgba8[row * width * 4..(row + 1) * width * 4];
                let dst = std::slice::from_raw_parts_mut(bits.add(row * pitch), width * 4);
                // A8R8G8B8 is B, G, R, A in memory.
                for (d, s) in dst
                    .as_chunks_mut::<4>()
                    .0
                    .iter_mut()
                    .zip(src.as_chunks::<4>().0)
                {
                    *d = [s[2], s[1], s[0], s[3]];
                }
            }
            unlock(out, 0);
        }
        out
    }
}

/// The device texture called `name`, created if needed.
///
/// # Safety
/// `device` must be a live IDirect3DDevice8.
unsafe fn texture_for(device: Com, tommy: &Tommy, name: &str) -> Com {
    let Ok(mut cache) = TEXTURES.lock() else {
        return std::ptr::null_mut();
    };
    if let Some((_, texture)) = cache.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)) {
        return *texture as Com;
    }
    let texture = tommy
        .textures
        .iter()
        .find(|t| t.name.eq_ignore_ascii_case(name))
        // SAFETY: as required by this function.
        .map_or(std::ptr::null_mut(), |t| unsafe {
            create_texture(device, t)
        });
    log(&format!("texture {name} : {texture:p}"));
    cache.push((name.to_owned(), texture as usize));
    texture
}

/// Draws Tommy. The WORLD matrix must already be PES's logic matrix.
///
/// # Safety
/// `device` must be a live IDirect3DDevice8, inside a scene.
unsafe fn draw_tommy(device: Com, tommy: &Tommy) {
    // SAFETY (whole block): calls through the vtables, types of d3d8.h.
    unsafe {
        let set_render_state: unsafe extern "system" fn(Com, u32, u32) -> Hresult =
            std::mem::transmute(method(device, DEV_SET_RENDER_STATE));
        let set_texture: unsafe extern "system" fn(Com, u32, Com) -> Hresult =
            std::mem::transmute(method(device, DEV_SET_TEXTURE));
        let set_tss: unsafe extern "system" fn(Com, u32, u32, u32) -> Hresult =
            std::mem::transmute(method(device, DEV_SET_TEXTURE_STAGE_STATE));
        let set_vertex_shader: unsafe extern "system" fn(Com, u32) -> Hresult =
            std::mem::transmute(method(device, DEV_SET_VERTEX_SHADER));
        let draw: unsafe extern "system" fn(
            Com,
            u32,
            u32,
            u32,
            u32,
            *const c_void,
            u32,
            *const c_void,
            u32,
        ) -> Hresult = std::mem::transmute(method(device, DEV_DRAW_INDEXED_PRIMITIVE_UP));

        for (state, value) in [
            (D3DRS_ZENABLE, 1),
            (D3DRS_ZWRITEENABLE, 1),
            (D3DRS_ZBIAS, 0),
            (D3DRS_ALPHATESTENABLE, 1),
            (D3DRS_ALPHAREF, 0x7f),
            (D3DRS_ALPHAFUNC, D3DCMP_GREATER),
            // HYPOTHÈSE: no culling until the handedness of PES's chain is
            // known (M4b).
            (D3DRS_CULLMODE, D3DCULL_NONE),
        ] {
            set_render_state(device, state, value);
        }
        for (state, value) in [
            (D3DTSS_COLOROP, D3DTOP_MODULATE),
            (D3DTSS_COLORARG1, D3DTA_TEXTURE),
            (D3DTSS_COLORARG2, D3DTA_DIFFUSE),
            (D3DTSS_ALPHAOP, D3DTOP_MODULATE),
            (D3DTSS_ALPHAARG1, D3DTA_TEXTURE),
            (D3DTSS_ALPHAARG2, D3DTA_DIFFUSE),
            (D3DTSS_MAGFILTER, D3DTEXF_LINEAR),
            (D3DTSS_MINFILTER, D3DTEXF_LINEAR),
            (D3DTSS_MIPFILTER, D3DTEXF_NONE),
            (D3DTSS_ADDRESSU, D3DTADDRESS_WRAP),
            (D3DTSS_ADDRESSV, D3DTADDRESS_WRAP),
        ] {
            set_tss(device, 0, state, value);
        }
        set_vertex_shader(device, crate::tommy::FVF);
        for batch in &tommy.batches {
            let texture = batch
                .texture
                .as_deref()
                .map_or(std::ptr::null_mut(), |name| {
                    texture_for(device, tommy, name)
                });
            set_texture(device, 0, texture);
            set_render_state(device, D3DRS_ALPHABLENDENABLE, u32::from(batch.blend));
            draw(
                device,
                D3DPT_TRIANGLELIST,
                0,
                batch.vertices.len() as u32,
                (batch.indices.len() / 3) as u32,
                batch.indices.as_ptr().cast(),
                D3DFMT_INDEX16,
                batch.vertices.as_ptr().cast(),
                std::mem::size_of::<crate::tommy::TexturedVertex>() as u32,
            );
        }
        set_texture(device, 0, std::ptr::null_mut());
    }
}
