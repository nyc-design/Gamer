//! screen-tool — Secondary screen magnification tool.
//!
//! Mirrors the top display (DP-0) onto the bottom display (DP-2) using NVFBC
//! for zero-copy GPU capture. Supports click-drag region selection for zoom,
//! and an FPS overlay toggle.
//!
//! Automatically hides when emulator secondary windows appear (e.g., 3DS bottom
//! screen in dual-window mode), and re-appears when they disappear.
//!
//! Env vars:
//!   SCREEN_TOOL_WINDOW    - Window title substring to capture (default: auto-detect emulator)
//!   SCREEN_TOOL_SECONDARY - Secondary window patterns to yield to (comma-separated)

use anyhow::{bail, Result};
use clap::Parser;
use glow::HasContext;
use signal_hook::flag;
use std::ffi::{CStr, CString};
use std::mem::MaybeUninit;
use std::os::raw::{c_int, c_ulong, c_void};
use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use x11::glx;
use x11::xlib;

// ─── NVFBC FFI ──────────────────────────────────────────────────────────────

type NvfbcSessionHandle = u64;
type NvfbcBool = u32;
type NvfbcStatus = u32;

const NVFBC_SUCCESS: NvfbcStatus = 0;
const NVFBC_TRACKING_OUTPUT: u32 = 1;
const NVFBC_CAPTURE_TO_GL: u32 = 3;
const NVFBC_BUFFER_FORMAT_RGBA: u32 = 4;
const NVFBC_TOGL_TEXTURES_MAX: usize = 2;

// NVFBC API version: major=1, minor=7 => (7 | (1 << 8)) = 0x107
const NVFBC_VERSION: u32 = 0x107;

fn nvfbc_struct_version<T>(ver: u32) -> u32 {
    (std::mem::size_of::<T>() as u32) | (ver << 16) | (NVFBC_VERSION << 24)
}

#[repr(C)]
struct NvfbcSize {
    w: u32,
    h: u32,
}

#[repr(C)]
struct NvfbcBox {
    x: u32,
    y: u32,
    w: u32,
    h: u32,
}

#[repr(C)]
struct NvfbcRandrOutputInfo {
    dw_id: u32,
    name: [u8; 128],
    tracker_box: NvfbcBox,
}

const NVFBC_OUTPUT_MAX: usize = 5;

#[repr(C)]
struct NvfbcCreateHandleParams {
    dw_version: u32,
    private_data: *const c_void,
    private_data_size: u32,
    externally_managed_context: NvfbcBool,
    glx_ctx: *mut c_void,
    glx_fb_config: *mut c_void,
}

#[repr(C)]
struct NvfbcGetStatusParams {
    dw_version: u32,
    is_capture_possible: NvfbcBool,
    currently_capturing: NvfbcBool,
    can_create_now: NvfbcBool,
    screen_size: NvfbcSize,
    xrandr_available: NvfbcBool,
    outputs: [NvfbcRandrOutputInfo; NVFBC_OUTPUT_MAX],
    output_num: u32,
    nvfbc_version: u32,
    in_modeset: NvfbcBool,
}

#[repr(C)]
struct NvfbcCreateCaptureSessionParams {
    dw_version: u32,
    capture_type: u32,
    tracking_type: u32,
    output_id: u32,
    capture_box: NvfbcBox,
    frame_size: NvfbcSize,
    with_cursor: NvfbcBool,
    disable_auto_modeset_recovery: NvfbcBool,
    round_frame_size: NvfbcBool,
    sampling_rate_ms: u32,
    push_model: NvfbcBool,
    allow_direct_capture: NvfbcBool,
}

#[repr(C)]
struct NvfbcFrameGrabInfo {
    dw_width: u32,
    dw_height: u32,
    dw_byte_size: u32,
    dw_current_frame: u32,
    b_is_new_frame: NvfbcBool,
}

#[repr(C)]
struct NvfbcToGlSetupParams {
    dw_version: u32,
    buffer_format: u32,
    with_diff_map: NvfbcBool,
    pp_diff_map: *mut *mut c_void,
    diff_map_scaling_factor: u32,
    textures: [u32; NVFBC_TOGL_TEXTURES_MAX],
    tex_target: u32,
    tex_format: u32,
    tex_type: u32,
    diff_map_size: NvfbcSize,
}

const NVFBC_TOGL_GRAB_FLAGS_NOWAIT: u32 = 1;

#[repr(C)]
struct NvfbcToGlGrabFrameParams {
    dw_version: u32,
    dw_flags: u32,
    dw_texture_index: u32,
    frame_grab_info: *mut NvfbcFrameGrabInfo,
    dw_timeout_ms: u32,
}

// Function pointer types
type FnCreateHandle = unsafe extern "C" fn(*mut NvfbcSessionHandle, *mut NvfbcCreateHandleParams) -> NvfbcStatus;
type FnDestroyHandle = unsafe extern "C" fn(NvfbcSessionHandle, *mut c_void) -> NvfbcStatus;
type FnGetStatus = unsafe extern "C" fn(NvfbcSessionHandle, *mut NvfbcGetStatusParams) -> NvfbcStatus;
type FnCreateCaptureSession = unsafe extern "C" fn(NvfbcSessionHandle, *mut NvfbcCreateCaptureSessionParams) -> NvfbcStatus;
type FnDestroyCaptureSession = unsafe extern "C" fn(NvfbcSessionHandle, *mut c_void) -> NvfbcStatus;
type FnToGlSetup = unsafe extern "C" fn(NvfbcSessionHandle, *mut NvfbcToGlSetupParams) -> NvfbcStatus;
type FnToGlGrabFrame = unsafe extern "C" fn(NvfbcSessionHandle, *mut NvfbcToGlGrabFrameParams) -> NvfbcStatus;
type FnBindContext = unsafe extern "C" fn(NvfbcSessionHandle, *mut c_void) -> NvfbcStatus;
type FnReleaseContext = unsafe extern "C" fn(NvfbcSessionHandle, *mut c_void) -> NvfbcStatus;
type FnGetLastErrorStr = unsafe extern "C" fn(NvfbcSessionHandle) -> *const i8;

#[repr(C)]
struct NvfbcFunctionList {
    dw_version: u32,
    get_last_error_str: FnGetLastErrorStr,
    create_handle: FnCreateHandle,
    destroy_handle: FnDestroyHandle,
    get_status: FnGetStatus,
    create_capture_session: FnCreateCaptureSession,
    destroy_capture_session: FnDestroyCaptureSession,
    to_sys_setup: *const c_void,
    to_sys_grab_frame: *const c_void,
    to_cuda_setup: *const c_void,
    to_cuda_grab_frame: *const c_void,
    _pad1: *const c_void,
    _pad2: *const c_void,
    _pad3: *const c_void,
    bind_context: FnBindContext,
    release_context: FnReleaseContext,
    _pad4: *const c_void,
    _pad5: *const c_void,
    _pad6: *const c_void,
    _pad7: *const c_void,
    to_gl_setup: FnToGlSetup,
    to_gl_grab_frame: FnToGlGrabFrame,
}

type FnNvFBCCreateInstance = unsafe extern "C" fn(*mut NvfbcFunctionList) -> NvfbcStatus;

// ─── X11 error handler ──────────────────────────────────────────────────────

unsafe extern "C" fn x11_error_handler(_display: *mut xlib::Display, event: *mut xlib::XErrorEvent) -> c_int {
    let err = unsafe { &*event };
    log::debug!("X11 error: request={}, error={}, minor={}", err.request_code, err.error_code, err.minor_code);
    0
}

// ─── GL state ────────────────────────────────────────────────────────────────

struct GlState {
    glow_ctx: Arc<glow::Context>,
    display: *mut xlib::Display,
    glx_context: glx::GLXContext,
    output_fb_config: glx::GLXFBConfig,
    helper_window: xlib::Window,
}

impl GlState {
    fn new() -> Result<Self> {
        unsafe {
            let display = xlib::XOpenDisplay(ptr::null());
            if display.is_null() {
                bail!("Failed to open X display");
            }

            let screen = xlib::XDefaultScreen(display);
            let root = xlib::XRootWindow(display, screen);
            let screen_depth = xlib::XDefaultDepth(display, screen);

            let output_attribs: Vec<c_int> = vec![
                glx::GLX_X_RENDERABLE, 1,
                glx::GLX_DRAWABLE_TYPE, glx::GLX_WINDOW_BIT,
                glx::GLX_RENDER_TYPE, glx::GLX_RGBA_BIT,
                glx::GLX_RED_SIZE, 8, glx::GLX_GREEN_SIZE, 8, glx::GLX_BLUE_SIZE, 8,
                glx::GLX_DOUBLEBUFFER, 1,
                glx::GLX_BUFFER_SIZE, screen_depth,
                0,
            ];

            let mut num_configs: c_int = 0;
            let configs = glx::glXChooseFBConfig(display, screen, output_attribs.as_ptr(), &mut num_configs);
            if configs.is_null() || num_configs == 0 {
                bail!("No suitable GLX FBConfig found");
            }

            let mut output_fb_config = *configs;
            for i in 0..num_configs {
                let cfg = *configs.offset(i as isize);
                let vis = glx::glXGetVisualFromFBConfig(display, cfg);
                if !vis.is_null() {
                    let depth = (*vis).depth;
                    xlib::XFree(vis as *mut c_void);
                    if depth == screen_depth {
                        output_fb_config = cfg;
                        break;
                    }
                }
            }
            xlib::XFree(configs as *mut c_void);

            let visual = glx::glXGetVisualFromFBConfig(display, output_fb_config);
            if visual.is_null() {
                bail!("Failed to get visual from FBConfig");
            }

            let mut swa: xlib::XSetWindowAttributes = std::mem::zeroed();
            swa.colormap = xlib::XCreateColormap(display, root, (*visual).visual, xlib::AllocNone);
            swa.override_redirect = 1;
            let helper_window = xlib::XCreateWindow(
                display, root, 0, 0, 1, 1, 0,
                (*visual).depth, xlib::InputOutput as u32, (*visual).visual,
                xlib::CWColormap | xlib::CWOverrideRedirect, &mut swa,
            );
            xlib::XFree(visual as *mut c_void);

            let glx_context = glx::glXCreateNewContext(display, output_fb_config, glx::GLX_RGBA_TYPE, ptr::null_mut(), 1);
            if glx_context.is_null() {
                bail!("Failed to create GLX context");
            }
            glx::glXMakeCurrent(display, helper_window, glx_context);

            let glow_ctx = glow::Context::from_loader_function_cstr(|name: &CStr| {
                match glx::glXGetProcAddress(name.as_ptr() as *const u8) {
                    Some(f) => f as *const c_void,
                    None => ptr::null(),
                }
            });

            let version = glow_ctx.get_parameter_string(glow::VERSION);
            log::info!("OpenGL version: {}", version);

            Ok(Self {
                glow_ctx: Arc::new(glow_ctx),
                display, glx_context, output_fb_config, helper_window,
            })
        }
    }

    unsafe fn make_current(&self, drawable: glx::GLXDrawable) {
        glx::glXMakeCurrent(self.display, drawable, self.glx_context);
    }

    unsafe fn make_current_offscreen(&self) {
        glx::glXMakeCurrent(self.display, self.helper_window, self.glx_context);
    }
}

impl Drop for GlState {
    fn drop(&mut self) {
        unsafe {
            glx::glXMakeCurrent(self.display, 0, ptr::null_mut());
            glx::glXDestroyContext(self.display, self.glx_context);
            xlib::XDestroyWindow(self.display, self.helper_window);
            xlib::XCloseDisplay(self.display);
        }
    }
}

// ─── NVFBC capture ──────────────────────────────────────────────────────────

struct NvfbcCapture {
    fns: NvfbcFunctionList,
    handle: NvfbcSessionHandle,
    textures: [u32; NVFBC_TOGL_TEXTURES_MAX],
    width: u32,
    height: u32,
    _lib: *mut c_void, // dlopen handle
}

impl NvfbcCapture {
    fn new(gl: &GlState, output_name: &str) -> Result<Self> {
        unsafe {
            // Load libnvidia-fbc.so.1
            let lib_name = CString::new("libnvidia-fbc.so.1").unwrap();
            let lib = libc::dlopen(lib_name.as_ptr(), libc::RTLD_NOW);
            if lib.is_null() {
                let err = CStr::from_ptr(libc::dlerror());
                bail!("Failed to load libnvidia-fbc.so.1: {}", err.to_string_lossy());
            }

            let sym_name = CString::new("NvFBCCreateInstance").unwrap();
            let create_instance: FnNvFBCCreateInstance = {
                let sym = libc::dlsym(lib, sym_name.as_ptr());
                if sym.is_null() {
                    bail!("NvFBCCreateInstance not found in libnvidia-fbc.so.1");
                }
                std::mem::transmute(sym)
            };

            // Get function table — use MaybeUninit because the struct contains
            // function pointers that can't be zero-initialized in Rust
            let mut fns_uninit = MaybeUninit::<NvfbcFunctionList>::uninit();
            // Only set version field (at the start of the struct)
            // Function list version uses NVFBC_VERSION directly (not struct version)
            (fns_uninit.as_mut_ptr() as *mut u32).write(NVFBC_VERSION);
            let status = create_instance(fns_uninit.as_mut_ptr());
            if status != NVFBC_SUCCESS {
                bail!("NvFBCCreateInstance failed: {}", status);
            }
            let fns = fns_uninit.assume_init();

            // Create handle with our GLX context
            // Magic key to enable NVFBC on consumer GPUs (same as Sunshine)
            // See: https://github.com/keylase/nvidia-patch
            let magic_key: [u32; 4] = [0xAEF57AC5, 0x401D1A39, 0x1B856BBE, 0x9ED0CEBA];

            gl.make_current_offscreen();
            let mut handle: NvfbcSessionHandle = 0;
            let mut create_params: NvfbcCreateHandleParams = std::mem::zeroed();
            create_params.dw_version = nvfbc_struct_version::<NvfbcCreateHandleParams>(2);
            create_params.private_data = magic_key.as_ptr() as *const c_void;
            create_params.private_data_size = std::mem::size_of_val(&magic_key) as u32;
            create_params.externally_managed_context = 1;
            create_params.glx_ctx = gl.glx_context as *mut c_void;
            create_params.glx_fb_config = gl.output_fb_config as *mut c_void;

            let status = (fns.create_handle)(&mut handle, &mut create_params);
            if status != NVFBC_SUCCESS {
                let err = CStr::from_ptr((fns.get_last_error_str)(handle));
                bail!("nvFBCCreateHandle failed: {}", err.to_string_lossy());
            }

            // Get status to find output ID
            let mut status_params: NvfbcGetStatusParams = std::mem::zeroed();
            status_params.dw_version = nvfbc_struct_version::<NvfbcGetStatusParams>(2);
            let status = (fns.get_status)(handle, &mut status_params);
            if status != NVFBC_SUCCESS {
                let err = CStr::from_ptr((fns.get_last_error_str)(handle));
                bail!("nvFBCGetStatus failed: {}", err.to_string_lossy());
            }

            log::info!("NVFBC: {} outputs, screen {}x{}", status_params.output_num,
                status_params.screen_size.w, status_params.screen_size.h);

            let mut output_id = 0u32;
            let mut found = false;
            for i in 0..status_params.output_num as usize {
                let out = &status_params.outputs[i];
                let name_end = out.name.iter().position(|&b| b == 0).unwrap_or(out.name.len());
                let name = String::from_utf8_lossy(&out.name[..name_end]);
                log::info!("  Output {}: '{}' id={} {}x{}+{}+{}",
                    i, name, out.dw_id, out.tracker_box.w, out.tracker_box.h,
                    out.tracker_box.x, out.tracker_box.y);
                if name.trim() == output_name {
                    output_id = out.dw_id;
                    found = true;
                }
            }

            if !found {
                bail!("NVFBC output '{}' not found", output_name);
            }
            log::info!("NVFBC: tracking output '{}' (id={})", output_name, output_id);

            // Create capture session
            let mut session_params: NvfbcCreateCaptureSessionParams = std::mem::zeroed();
            session_params.dw_version = nvfbc_struct_version::<NvfbcCreateCaptureSessionParams>(6);
            session_params.capture_type = NVFBC_CAPTURE_TO_GL;
            session_params.tracking_type = NVFBC_TRACKING_OUTPUT;
            session_params.output_id = output_id;
            session_params.with_cursor = 0;
            session_params.allow_direct_capture = 1;

            let status = (fns.create_capture_session)(handle, &mut session_params);
            if status != NVFBC_SUCCESS {
                let err = CStr::from_ptr((fns.get_last_error_str)(handle));
                bail!("nvFBCCreateCaptureSession failed: {}", err.to_string_lossy());
            }

            // Setup ToGL
            let mut gl_params: NvfbcToGlSetupParams = std::mem::zeroed();
            gl_params.dw_version = nvfbc_struct_version::<NvfbcToGlSetupParams>(2);
            gl_params.buffer_format = NVFBC_BUFFER_FORMAT_RGBA;

            let status = (fns.to_gl_setup)(handle, &mut gl_params);
            if status != NVFBC_SUCCESS {
                let err = CStr::from_ptr((fns.get_last_error_str)(handle));
                bail!("nvFBCToGLSetUp failed: {}", err.to_string_lossy());
            }

            log::info!("NVFBC ToGL: textures=[{}, {}] target=0x{:x} format=0x{:x}",
                gl_params.textures[0], gl_params.textures[1],
                gl_params.tex_target, gl_params.tex_format);

            // Grab one frame to get dimensions
            let mut grab_info: NvfbcFrameGrabInfo = std::mem::zeroed();
            let mut grab_params: NvfbcToGlGrabFrameParams = std::mem::zeroed();
            grab_params.dw_version = nvfbc_struct_version::<NvfbcToGlGrabFrameParams>(2);
            grab_params.dw_flags = 0; // blocking first frame
            grab_params.frame_grab_info = &mut grab_info;
            grab_params.dw_timeout_ms = 5000;

            let status = (fns.to_gl_grab_frame)(handle, &mut grab_params);
            if status != NVFBC_SUCCESS {
                let err = CStr::from_ptr((fns.get_last_error_str)(handle));
                bail!("Initial nvFBCToGLGrabFrame failed: {}", err.to_string_lossy());
            }

            let width = grab_info.dw_width;
            let height = grab_info.dw_height;
            log::info!("NVFBC: capturing {}x{} from '{}'", width, height, output_name);

            Ok(Self {
                fns,
                handle,
                textures: gl_params.textures,
                width,
                height,
                _lib: lib,
            })
        }
    }

    /// Grab a frame, returns (GL texture ID, width, height, is_new).
    fn grab_frame(&mut self) -> Result<(u32, u32, u32, bool)> {
        unsafe {
            let mut grab_info: NvfbcFrameGrabInfo = std::mem::zeroed();
            let mut grab_params: NvfbcToGlGrabFrameParams = std::mem::zeroed();
            grab_params.dw_version = nvfbc_struct_version::<NvfbcToGlGrabFrameParams>(2);
            grab_params.dw_flags = NVFBC_TOGL_GRAB_FLAGS_NOWAIT;
            grab_params.frame_grab_info = &mut grab_info;

            let status = (self.fns.to_gl_grab_frame)(self.handle, &mut grab_params);
            if status != NVFBC_SUCCESS {
                let err = CStr::from_ptr((self.fns.get_last_error_str)(self.handle));
                bail!("nvFBCToGLGrabFrame: {}", err.to_string_lossy());
            }

            self.width = grab_info.dw_width;
            self.height = grab_info.dw_height;
            let tex_id = self.textures[grab_params.dw_texture_index as usize];
            Ok((tex_id, grab_info.dw_width, grab_info.dw_height, grab_info.b_is_new_frame != 0))
        }
    }
}

impl Drop for NvfbcCapture {
    fn drop(&mut self) {
        unsafe {
            (self.fns.destroy_capture_session)(self.handle, ptr::null_mut());
            (self.fns.destroy_handle)(self.handle, ptr::null_mut());
            // Note: we don't dlclose — NVFBC may have background threads
        }
    }
}

// ─── Window discovery ────────────────────────────────────────────────────────

fn get_client_list(display: *mut xlib::Display, root: c_ulong) -> Option<Vec<c_ulong>> {
    unsafe {
        let atom = xlib::XInternAtom(display, b"_NET_CLIENT_LIST\0".as_ptr() as *const _, 0);
        let mut actual_type: c_ulong = 0;
        let mut actual_format: i32 = 0;
        let mut nitems: c_ulong = 0;
        let mut bytes_after: c_ulong = 0;
        let mut prop: *mut u8 = ptr::null_mut();
        if xlib::XGetWindowProperty(
            display, root, atom, 0, 1024, 0, xlib::XA_WINDOW,
            &mut actual_type, &mut actual_format, &mut nitems, &mut bytes_after, &mut prop,
        ) == 0 && !prop.is_null() && nitems > 0 && actual_format == 32 {
            let windows = std::slice::from_raw_parts(prop as *const c_ulong, nitems as usize).to_vec();
            xlib::XFree(prop as *mut c_void);
            Some(windows)
        } else {
            if !prop.is_null() { xlib::XFree(prop as *mut c_void); }
            None
        }
    }
}

fn get_window_name(display: *mut xlib::Display, window: c_ulong) -> Option<String> {
    unsafe {
        let utf8 = xlib::XInternAtom(display, b"UTF8_STRING\0".as_ptr() as *const _, 0);
        let net_name = xlib::XInternAtom(display, b"_NET_WM_NAME\0".as_ptr() as *const _, 0);
        let mut atype: c_ulong = 0;
        let mut afmt: i32 = 0;
        let mut nitems: c_ulong = 0;
        let mut after: c_ulong = 0;
        let mut prop: *mut u8 = ptr::null_mut();
        if xlib::XGetWindowProperty(display, window, net_name, 0, 1024, 0, utf8,
            &mut atype, &mut afmt, &mut nitems, &mut after, &mut prop,
        ) == 0 && !prop.is_null() && nitems > 0 {
            let name = String::from_utf8_lossy(std::slice::from_raw_parts(prop, nitems as usize)).to_string();
            xlib::XFree(prop as *mut c_void);
            return Some(name);
        }
        if !prop.is_null() { xlib::XFree(prop as *mut c_void); }
        let mut name_ptr: *mut i8 = ptr::null_mut();
        if xlib::XFetchName(display, window, &mut name_ptr) != 0 && !name_ptr.is_null() {
            let name = CStr::from_ptr(name_ptr).to_string_lossy().to_string();
            xlib::XFree(name_ptr as *mut c_void);
            return Some(name);
        }
        None
    }
}

/// Find a secondary emulator window matching the given patterns.
/// Returns the window ID if found, or None.
fn find_secondary_window(display: *mut xlib::Display, patterns: &[String]) -> Option<c_ulong> {
    let root = unsafe { xlib::XDefaultRootWindow(display) };

    if let Some(clients) = get_client_list(display, root) {
        for wid in clients {
            if check_window_matches(display, wid, patterns) {
                return Some(wid);
            }
        }
    }
    None
}

fn check_window_matches(display: *mut xlib::Display, wid: c_ulong, patterns: &[String]) -> bool {
    if let Some(name) = get_window_name(display, wid) {
        for pat in patterns {
            if name.contains(pat.as_str()) {
                unsafe {
                    let mut attrs: xlib::XWindowAttributes = std::mem::zeroed();
                    if xlib::XGetWindowAttributes(display, wid, &mut attrs) != 0
                        && attrs.map_state == 2 // IsViewable
                        && attrs.width >= 32 && attrs.height >= 32
                    {
                        return true;
                    }
                }
            }
        }
    }
    false
}

/// Check if a window is rendering non-black content by sampling a few pixels.
/// Samples 5 points across the window center row. Returns true if any pixel
/// has a non-zero RGB value.
fn window_has_content(display: *mut xlib::Display, wid: c_ulong) -> bool {
    unsafe {
        let mut attrs: xlib::XWindowAttributes = std::mem::zeroed();
        if xlib::XGetWindowAttributes(display, wid, &mut attrs) == 0 {
            return false;
        }
        let w = attrs.width;
        let h = attrs.height;
        if w < 32 || h < 32 { return false; }

        // Sample 5 pixels along the center row at 20%, 35%, 50%, 65%, 80% width
        let cy = h / 2;
        let sample_xs = [w / 5, w * 35 / 100, w / 2, w * 65 / 100, w * 4 / 5];

        for &sx in &sample_xs {
            let img = xlib::XGetImage(display, wid, sx, cy, 1, 1, !0, xlib::ZPixmap);
            if img.is_null() { continue; }
            let pixel = xlib::XGetPixel(img, 0, 0);
            xlib::XDestroyImage(img);
            // Check if any color channel is non-zero (not black)
            if pixel & 0x00FFFFFF != 0 {
                return true;
            }
        }
        false
    }
}

// ─── Output window on DP-2 ──────────────────────────────────────────────────

struct OutputWindow {
    display: *mut xlib::Display,
    window: c_ulong,
    glx_window: glx::GLXDrawable,
    width: u32,
    height: u32,
    read_fbo: glow::Framebuffer,
}

impl OutputWindow {
    fn new(gl: &GlState, title: &str, x: i32, y: i32, width: u32, height: u32) -> Result<Self> {
        unsafe {
            let display = gl.display;
            let screen = xlib::XDefaultScreen(display);
            let root = xlib::XRootWindow(display, screen);
            let visual = glx::glXGetVisualFromFBConfig(display, gl.output_fb_config);
            if visual.is_null() { bail!("No visual for output window"); }

            let mut swa: xlib::XSetWindowAttributes = std::mem::zeroed();
            swa.colormap = xlib::XCreateColormap(display, root, (*visual).visual, xlib::AllocNone);
            swa.event_mask = xlib::StructureNotifyMask | xlib::ExposureMask
                | xlib::ButtonPressMask | xlib::ButtonReleaseMask
                | xlib::PointerMotionMask | xlib::KeyPressMask;

            let window = xlib::XCreateWindow(
                display, root, x, y, width, height, 0,
                (*visual).depth, xlib::InputOutput as u32, (*visual).visual,
                xlib::CWColormap | xlib::CWEventMask, &mut swa,
            );
            xlib::XFree(visual as *mut c_void);
            if window == 0 { bail!("Failed to create output window"); }

            let title_c = std::ffi::CString::new(title).unwrap();
            xlib::XStoreName(display, window, title_c.as_ptr());
            let atom_name = xlib::XInternAtom(display, b"_NET_WM_NAME\0".as_ptr() as *const _, 0);
            let atom_utf8 = xlib::XInternAtom(display, b"UTF8_STRING\0".as_ptr() as *const _, 0);
            xlib::XChangeProperty(display, window, atom_name, atom_utf8, 8, xlib::PropModeReplace,
                title.as_ptr(), title.len() as i32);

            let atom_type = xlib::XInternAtom(display, b"_NET_WM_WINDOW_TYPE\0".as_ptr() as *const _, 0);
            let atom_normal = xlib::XInternAtom(display, b"_NET_WM_WINDOW_TYPE_NORMAL\0".as_ptr() as *const _, 0);
            xlib::XChangeProperty(display, window, atom_type, xlib::XA_ATOM, 32, xlib::PropModeReplace,
                &atom_normal as *const c_ulong as *const u8, 1);

            let glx_window = glx::glXCreateWindow(display, gl.output_fb_config, window, ptr::null());
            if glx_window == 0 { bail!("Failed to create GLX window"); }

            glx::glXMakeCurrent(display, glx_window, gl.glx_context);
            let read_fbo = gl.glow_ctx.create_framebuffer().map_err(|e| anyhow::anyhow!("{}", e))?;

            xlib::XMapWindow(display, window);
            xlib::XRaiseWindow(display, window);
            xlib::XFlush(display);

            log::info!("Output window 0x{:x} '{}' at ({},{}) {}x{}", window, title, x, y, width, height);

            Ok(Self { display, window, glx_window, width, height, read_fbo })
        }
    }

    fn hide(&self) {
        unsafe {
            xlib::XUnmapWindow(self.display, self.window);
            xlib::XFlush(self.display);
        }
    }

    fn show(&self, x: i32, y: i32, width: u32, height: u32) {
        unsafe {
            let mut changes: xlib::XWindowChanges = std::mem::zeroed();
            changes.x = x;
            changes.y = y;
            changes.width = width as c_int;
            changes.height = height as c_int;
            xlib::XConfigureWindow(self.display, self.window,
                (xlib::CWX | xlib::CWY | xlib::CWWidth | xlib::CWHeight) as u32, &mut changes);
            xlib::XMapWindow(self.display, self.window);
            xlib::XRaiseWindow(self.display, self.window);
            xlib::XFlush(self.display);
        }
    }
}

impl Drop for OutputWindow {
    fn drop(&mut self) {
        unsafe {
            glx::glXDestroyWindow(self.display, self.glx_window);
            xlib::XDestroyWindow(self.display, self.window);
        }
    }
}

// ─── Bitmap font for FPS overlay ─────────────────────────────────────────────

const FONT_CHARS: &str = "0123456789 .FPSms%:";

#[rustfmt::skip]
const FONT_DATA: &[[u8; 8]] = &[
    [0x3C, 0x66, 0x6E, 0x76, 0x66, 0x66, 0x3C, 0x00], // 0
    [0x18, 0x38, 0x18, 0x18, 0x18, 0x18, 0x7E, 0x00], // 1
    [0x3C, 0x66, 0x06, 0x0C, 0x18, 0x30, 0x7E, 0x00], // 2
    [0x3C, 0x66, 0x06, 0x1C, 0x06, 0x66, 0x3C, 0x00], // 3
    [0x0C, 0x1C, 0x3C, 0x6C, 0x7E, 0x0C, 0x0C, 0x00], // 4
    [0x7E, 0x60, 0x7C, 0x06, 0x06, 0x66, 0x3C, 0x00], // 5
    [0x3C, 0x66, 0x60, 0x7C, 0x66, 0x66, 0x3C, 0x00], // 6
    [0x7E, 0x06, 0x0C, 0x18, 0x18, 0x18, 0x18, 0x00], // 7
    [0x3C, 0x66, 0x66, 0x3C, 0x66, 0x66, 0x3C, 0x00], // 8
    [0x3C, 0x66, 0x66, 0x3E, 0x06, 0x66, 0x3C, 0x00], // 9
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00], // space
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x18, 0x18, 0x00], // .
    [0x7E, 0x60, 0x60, 0x7C, 0x60, 0x60, 0x60, 0x00], // F
    [0x7C, 0x66, 0x66, 0x7C, 0x60, 0x60, 0x60, 0x00], // P
    [0x3C, 0x66, 0x60, 0x3C, 0x06, 0x66, 0x3C, 0x00], // S
    [0x00, 0x00, 0x76, 0x7F, 0x6B, 0x63, 0x63, 0x00], // m
    [0x00, 0x00, 0x3E, 0x60, 0x3C, 0x06, 0x7C, 0x00], // s
    [0x62, 0x64, 0x08, 0x10, 0x26, 0x46, 0x00, 0x00], // %
    [0x00, 0x18, 0x18, 0x00, 0x18, 0x18, 0x00, 0x00], // :
];

fn render_text(text: &str, scale: u32) -> (Vec<u8>, u32, u32) {
    let char_w = 8 * scale;
    let char_h = 8 * scale;
    let width = text.len() as u32 * char_w;
    let height = char_h;
    let mut pixels = vec![0u8; (width * height * 4) as usize];

    for (ci, ch) in text.chars().enumerate() {
        let font_idx = FONT_CHARS.find(ch).unwrap_or(10);
        if font_idx >= FONT_DATA.len() { continue; }
        let glyph = &FONT_DATA[font_idx];

        for row in 0..8u32 {
            for col in 0..8u32 {
                let lit = (glyph[row as usize] >> (7 - col)) & 1 == 1;
                if !lit { continue; }
                for sy in 0..scale {
                    for sx in 0..scale {
                        let px = ci as u32 * char_w + col * scale + sx;
                        let py = row * scale + sy;
                        let offset = ((py * width + px) * 4) as usize;
                        if offset + 3 < pixels.len() {
                            pixels[offset] = 255;
                            pixels[offset + 1] = 255;
                            pixels[offset + 2] = 255;
                            pixels[offset + 3] = 200;
                        }
                    }
                }
            }
        }
    }
    (pixels, width, height)
}

// ─── Zoom state ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
struct ZoomRegion { sx: f32, sy: f32, sw: f32, sh: f32 }

impl Default for ZoomRegion {
    fn default() -> Self { Self { sx: 0.0, sy: 0.0, sw: 1.0, sh: 1.0 } }
}

// ─── Blit helper ─────────────────────────────────────────────────────────────

/// Blit a sub-region of an NVFBC texture to the output window.
/// NVFBC textures are NOT y-inverted (origin is top-left).
fn blit_nvfbc_region(
    gl: &GlState, tex_id: u32, src_w: u32, src_h: u32,
    sx: i32, sy: i32, sw: i32, sh: i32,
    output: &OutputWindow,
) {
    unsafe {
        gl.make_current(output.glx_window);
        let g = &gl.glow_ctx;

        // NVFBC gives us a raw GL texture ID (u32), wrap it for glow
        let texture = glow::NativeTexture(std::num::NonZeroU32::new(tex_id).unwrap());

        g.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(output.read_fbo));
        g.framebuffer_texture_2d(glow::READ_FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(texture), 0);

        g.bind_framebuffer(glow::DRAW_FRAMEBUFFER, None);
        g.viewport(0, 0, output.width as i32, output.height as i32);

        // NVFBC: origin is top-left, GL blit origin is bottom-left, so flip Y
        let src_y0 = src_h as i32 - sy;
        let src_y1 = src_h as i32 - (sy + sh);

        g.blit_framebuffer(
            sx, src_y0, sx + sw, src_y1,
            0, 0, output.width as i32, output.height as i32,
            glow::COLOR_BUFFER_BIT, glow::LINEAR,
        );
    }
}

/// Cached FPS overlay
struct FpsOverlay {
    texture: Option<glow::Texture>,
    fbo: Option<glow::Framebuffer>,
    cached_text: String,
    tex_width: u32,
    tex_height: u32,
}

impl FpsOverlay {
    fn new() -> Self {
        Self { texture: None, fbo: None, cached_text: String::new(), tex_width: 0, tex_height: 0 }
    }

    fn render(&mut self, gl: &GlState, output: &OutputWindow, fps_text: &str) {
        unsafe {
            gl.make_current(output.glx_window);
            let g = &gl.glow_ctx;
            let scale = 3u32;

            if fps_text != self.cached_text || self.texture.is_none() {
                let (pixels, tw, th) = render_text(fps_text, scale);
                if let Some(old_tex) = self.texture.take() { g.delete_texture(old_tex); }
                let tex = g.create_texture().unwrap();
                g.bind_texture(glow::TEXTURE_2D, Some(tex));
                g.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA as i32, tw as i32, th as i32, 0,
                    glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelUnpackData::Slice(Some(&pixels)));
                g.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::NEAREST as i32);
                g.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::NEAREST as i32);
                g.bind_texture(glow::TEXTURE_2D, None);
                self.texture = Some(tex);
                self.tex_width = tw;
                self.tex_height = th;
                self.cached_text = fps_text.to_string();
            }

            if self.fbo.is_none() { self.fbo = Some(g.create_framebuffer().unwrap()); }

            let tw = self.tex_width;
            let th = self.tex_height;
            let margin = 8i32;
            let bg_pad = 4i32;

            g.enable(glow::SCISSOR_TEST);
            g.scissor(margin - bg_pad, (output.height as i32) - (margin + th as i32 + bg_pad),
                tw as i32 + bg_pad * 2, th as i32 + bg_pad * 2);
            g.clear_color(0.0, 0.0, 0.0, 0.7);
            g.clear(glow::COLOR_BUFFER_BIT);
            g.disable(glow::SCISSOR_TEST);

            g.bind_framebuffer(glow::READ_FRAMEBUFFER, self.fbo);
            g.framebuffer_texture_2d(glow::READ_FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, self.texture, 0);
            g.bind_framebuffer(glow::DRAW_FRAMEBUFFER, None);

            let dst_y0 = (output.height as i32) - margin - th as i32;
            let dst_y1 = (output.height as i32) - margin;
            g.blit_framebuffer(
                0, 0, tw as i32, th as i32,
                margin, dst_y0, margin + tw as i32, dst_y1,
                glow::COLOR_BUFFER_BIT, glow::NEAREST,
            );
        }
    }
}

// ─── CLI args ────────────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(name = "screen-tool", about = "Secondary screen magnification tool (NVFBC)")]
struct Args {
    /// NVFBC output name to capture (e.g. "DP-0")
    #[arg(long, default_value = "DP-0")]
    capture_output: String,

    /// Secondary window patterns to yield to (comma-separated)
    #[arg(long, default_value = "Secondary Window")]
    secondary_patterns: String,

    /// Output position X (DP-2 X offset)
    #[arg(long, default_value = "0")]
    output_x: i32,

    /// Output position Y (DP-2 Y offset)
    #[arg(long)]
    output_y: Option<i32>,

    /// Output width
    #[arg(long, default_value = "1920")]
    output_width: u32,

    /// Output height
    #[arg(long, default_value = "1080")]
    output_height: u32,
}

// ─── Main ────────────────────────────────────────────────────────────────────

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let args = Args::parse();

    let shutdown = Arc::new(AtomicBool::new(false));
    flag::register(signal_hook::consts::SIGTERM, Arc::clone(&shutdown))?;
    flag::register(signal_hook::consts::SIGINT, Arc::clone(&shutdown))?;

    unsafe { xlib::XSetErrorHandler(Some(x11_error_handler)); }

    let gl = GlState::new()?;

    let secondary_patterns: Vec<String> = args.secondary_patterns
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    // Default output_y: read DP-0 height from xrandr
    let output_y = args.output_y.unwrap_or(1080);

    // Initialize NVFBC capture of DP-0
    unsafe { gl.make_current_offscreen(); }
    let mut capture = NvfbcCapture::new(&gl, &args.capture_output)?;

    // Create output window on DP-2
    let mut output = OutputWindow::new(&gl, "ScreenTool: Zoom",
        args.output_x, output_y, args.output_width, args.output_height)?;

    let mut zoom = ZoomRegion::default();
    let mut selecting: Option<(i32, i32)> = None;
    let mut show_fps = false;
    let mut hidden = false; // true = unmapped, secondary window is active
    let mut fps_counter: usize = 0;
    let mut fps_timer = Instant::now();
    let mut fps_value: f32 = 0.0;
    let mut fps_overlay = FpsOverlay::new();
    let mut last_secondary_check = Instant::now();

    let mut event: xlib::XEvent = unsafe { std::mem::zeroed() };

    log::info!("screen-tool running. Left-drag to zoom, right-click to reset, F1 for FPS.");

    loop {
        if shutdown.load(Ordering::Relaxed) {
            log::info!("Shutdown");
            break;
        }

        // Process X events
        unsafe {
            while xlib::XPending(gl.display) > 0 {
                xlib::XNextEvent(gl.display, &mut event);
                let etype = event.type_;

                match etype {
                    xlib::ButtonPress => {
                        if !hidden {
                            let e = event.button;
                            if e.window == output.window {
                                match e.button {
                                    1 => { selecting = Some((e.x, e.y)); }
                                    3 => { zoom = ZoomRegion::default(); selecting = None; log::info!("Reset to full view"); }
                                    _ => {}
                                }
                            }
                        }
                    }
                    xlib::ButtonRelease => {
                        if !hidden {
                            let e = event.button;
                            if e.window == output.window && e.button == 1 {
                                if let Some((x1, y1)) = selecting.take() {
                                    let x2 = e.x;
                                    let y2 = e.y;
                                    let ow = output.width as f32;
                                    let oh = output.height as f32;
                                    let sel_x1 = (x1.min(x2) as f32 / ow).clamp(0.0, 1.0);
                                    let sel_y1 = (y1.min(y2) as f32 / oh).clamp(0.0, 1.0);
                                    let sel_x2 = (x1.max(x2) as f32 / ow).clamp(0.0, 1.0);
                                    let sel_y2 = (y1.max(y2) as f32 / oh).clamp(0.0, 1.0);
                                    let sel_w = sel_x2 - sel_x1;
                                    let sel_h = sel_y2 - sel_y1;
                                    if sel_w > 0.01 && sel_h > 0.01 {
                                        let new_sx = zoom.sx + sel_x1 * zoom.sw;
                                        let new_sy = zoom.sy + sel_y1 * zoom.sh;
                                        let new_sw = sel_w * zoom.sw;
                                        let new_sh = sel_h * zoom.sh;
                                        zoom = ZoomRegion { sx: new_sx, sy: new_sy, sw: new_sw, sh: new_sh };
                                        log::info!("Zoomed to region: ({:.3}, {:.3}) {:.3}x{:.3}", new_sx, new_sy, new_sw, new_sh);
                                    }
                                }
                            }
                        }
                    }
                    xlib::ConfigureNotify => {
                        let e = event.configure;
                        if e.window == output.window {
                            let new_w = e.width as u32;
                            let new_h = e.height as u32;
                            if new_w != output.width || new_h != output.height {
                                log::info!("Window resized: {}x{} -> {}x{}", output.width, output.height, new_w, new_h);
                                output.width = new_w;
                                output.height = new_h;
                            }
                        }
                    }
                    xlib::KeyPress => {
                        if !hidden {
                            let keysym = xlib::XLookupKeysym(&mut event.key as *mut _, 0);
                            if keysym == x11::keysym::XK_F1 as c_ulong {
                                show_fps = !show_fps;
                                log::info!("FPS overlay: {}", if show_fps { "ON" } else { "OFF" });
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        // Check for secondary window every ~1 second.
        // Hide completely when secondary has content (dual-window mode).
        // Show when secondary is black/gone (single-window mode).
        if last_secondary_check.elapsed() >= Duration::from_secs(1) {
            last_secondary_check = Instant::now();

            let secondary_active = match find_secondary_window(gl.display, &secondary_patterns) {
                Some(sec_wid) => window_has_content(gl.display, sec_wid),
                None => false,
            };

            if secondary_active && !hidden {
                log::info!("Secondary window active, hiding screen-tool");
                output.hide();
                hidden = true;
            } else if !secondary_active && hidden {
                log::info!("Secondary window gone/black, showing screen-tool");
                // Read current DP-2 geometry for correct position/size
                let (bot_x, bot_y, bot_w, bot_h) = read_dp2_geometry(gl.display);
                output.show(bot_x, bot_y, bot_w, bot_h);
                hidden = false;
                zoom = ZoomRegion::default();
                selecting = None;
            }
        }

        if hidden {
            // Sleep longer when hidden — just polling for secondary window state
            thread::sleep(Duration::from_millis(100));
            continue;
        }

        // Render NVFBC capture to output window
        match capture.grab_frame() {
            Ok((tex_id, src_w, src_h, _is_new)) => {
                let cw = src_w as f32;
                let ch = src_h as f32;
                let sx = (zoom.sx * cw) as i32;
                let sy = (zoom.sy * ch) as i32;
                let sw = (zoom.sw * cw) as i32;
                let sh = (zoom.sh * ch) as i32;

                blit_nvfbc_region(&gl, tex_id, src_w, src_h, sx, sy, sw, sh, &output);

                fps_counter += 1;
                if fps_timer.elapsed() >= Duration::from_secs(1) {
                    fps_value = fps_counter as f32 / fps_timer.elapsed().as_secs_f32();
                    fps_counter = 0;
                    fps_timer = Instant::now();
                }

                if show_fps {
                    let fps_text = format!("{:.0} FPS", fps_value);
                    fps_overlay.render(&gl, &output, &fps_text);
                }

                unsafe { glx::glXSwapBuffers(gl.display, output.glx_window); }
            }
            Err(e) => {
                log::warn!("NVFBC grab failed: {}", e);
                thread::sleep(Duration::from_millis(100));
            }
        }

        // No fixed sleep — run as fast as possible, NVFBC NOWAIT returns immediately
        // if no new frame. The GPU blit is negligible.
        thread::sleep(Duration::from_millis(1));
    }

    Ok(())
}

/// Read DP-2 position and size from xrandr output.
fn read_dp2_geometry(display: *mut xlib::Display) -> (i32, i32, u32, u32) {
    // Use xrandr via command since we need the current modeline
    if let Ok(output) = std::process::Command::new("xrandr").arg("--current").output() {
        let text = String::from_utf8_lossy(&output.stdout);
        for line in text.lines() {
            if line.starts_with("DP-2") && line.contains("connected") {
                // Parse "DP-2 connected 2624x1206+0+1964"
                if let Some(geom) = line.split_whitespace()
                    .find(|s| s.contains('x') && s.contains('+'))
                {
                    let parts: Vec<&str> = geom.split(|c| c == 'x' || c == '+').collect();
                    if parts.len() >= 4 {
                        let w = parts[0].parse().unwrap_or(1920);
                        let h = parts[1].parse().unwrap_or(1080);
                        let x = parts[2].parse().unwrap_or(0);
                        let y = parts[3].parse().unwrap_or(1080);
                        return (x, y, w, h);
                    }
                }
            }
        }
    }
    // Fallback: read DP-0 height from root window geometry
    unsafe {
        let screen = xlib::XDefaultScreen(display);
        let h = xlib::XDisplayHeight(display, screen);
        (0, h / 2, 1920, h as u32 / 2)
    }
}
