//! NVFBC FFI bindings and zero-copy GPU capture.

use anyhow::{bail, Result};
use std::ffi::c_char;
use std::ffi::{CStr, CString};
use std::mem::MaybeUninit;
use std::os::raw::c_void;
use std::ptr;

use crate::gl_state::GlState;

// ─── Types ───────────────────────────────────────────────────────────────────

pub(crate) type NvfbcSessionHandle = u64;
pub(crate) type NvfbcBool = u32;
pub(crate) type NvfbcStatus = u32;

pub(crate) const NVFBC_SUCCESS: NvfbcStatus = 0;
const NVFBC_TRACKING_OUTPUT: u32 = 1;
const NVFBC_CAPTURE_TO_GL: u32 = 3;
// Native X11/NVIDIA desktop format for NvFBC is BGRA.
const NVFBC_BUFFER_FORMAT_BGRA: u32 = 5;
const NVFBC_TOGL_TEXTURES_MAX: usize = 2;

// NVFBC API version: major=1, minor=7 => (7 | (1 << 8)) = 0x107
const NVFBC_VERSION: u32 = 0x107;

fn nvfbc_struct_version<T>(ver: u32) -> u32 {
    (std::mem::size_of::<T>() as u32) | (ver << 16) | (NVFBC_VERSION << 24)
}

// ─── FFI structs ─────────────────────────────────────────────────────────────

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

#[repr(C)]
struct NvfbcDestroyCaptureSessionParams {
    dw_version: u32,
}

#[repr(C)]
struct NvfbcBindContextParams {
    dw_version: u32,
}

#[repr(C)]
struct NvfbcReleaseContextParams {
    dw_version: u32,
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
type FnCreateHandle =
    unsafe extern "C" fn(*mut NvfbcSessionHandle, *mut NvfbcCreateHandleParams) -> NvfbcStatus;
type FnDestroyHandle = unsafe extern "C" fn(NvfbcSessionHandle, *mut c_void) -> NvfbcStatus;
type FnGetStatus =
    unsafe extern "C" fn(NvfbcSessionHandle, *mut NvfbcGetStatusParams) -> NvfbcStatus;
type FnCreateCaptureSession =
    unsafe extern "C" fn(NvfbcSessionHandle, *mut NvfbcCreateCaptureSessionParams) -> NvfbcStatus;
type FnDestroyCaptureSession =
    unsafe extern "C" fn(NvfbcSessionHandle, *mut NvfbcDestroyCaptureSessionParams) -> NvfbcStatus;
type FnToGlSetup =
    unsafe extern "C" fn(NvfbcSessionHandle, *mut NvfbcToGlSetupParams) -> NvfbcStatus;
type FnToGlGrabFrame =
    unsafe extern "C" fn(NvfbcSessionHandle, *mut NvfbcToGlGrabFrameParams) -> NvfbcStatus;
type FnBindContext =
    unsafe extern "C" fn(NvfbcSessionHandle, *mut NvfbcBindContextParams) -> NvfbcStatus;
type FnReleaseContext =
    unsafe extern "C" fn(NvfbcSessionHandle, *mut NvfbcReleaseContextParams) -> NvfbcStatus;
type FnGetLastErrorStr = unsafe extern "C" fn(NvfbcSessionHandle) -> *const c_char;

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

// ─── NvfbcCapture ────────────────────────────────────────────────────────────

pub struct NvfbcCapture {
    fns: NvfbcFunctionList,
    handle: NvfbcSessionHandle,
    textures: [u32; NVFBC_TOGL_TEXTURES_MAX],
    output_id: u32,
    capture_x: u32,
    capture_y: u32,
    capture_w: u32,
    capture_h: u32,
    pub width: u32,
    pub height: u32,
    pub screen_width: u32,
    pub screen_height: u32,
    _lib: *mut c_void,
}

impl NvfbcCapture {
    pub fn new(gl: &GlState, output_name: &str) -> Result<Self> {
        unsafe {
            log::debug!("[NVFBC] gl.display at entry: {:?}", gl.display);

            // Load libnvidia-fbc.so.1
            let lib_name = CString::new("libnvidia-fbc.so.1").unwrap();
            let lib = libc::dlopen(lib_name.as_ptr(), libc::RTLD_NOW);
            if lib.is_null() {
                let err = CStr::from_ptr(libc::dlerror());
                bail!(
                    "Failed to load libnvidia-fbc.so.1: {}",
                    err.to_string_lossy()
                );
            }
            log::debug!("[NVFBC] gl.display after dlopen: {:?}", gl.display);

            let sym_name = CString::new("NvFBCCreateInstance").unwrap();
            let create_instance: FnNvFBCCreateInstance = {
                let sym = libc::dlsym(lib, sym_name.as_ptr());
                if sym.is_null() {
                    bail!("NvFBCCreateInstance not found in libnvidia-fbc.so.1");
                }
                std::mem::transmute(sym)
            };
            log::debug!("[NVFBC] gl.display after dlsym: {:?}", gl.display);

            let mut fns_uninit = MaybeUninit::<NvfbcFunctionList>::uninit();
            (fns_uninit.as_mut_ptr() as *mut u32).write(NVFBC_VERSION);
            let status = create_instance(fns_uninit.as_mut_ptr());
            if status != NVFBC_SUCCESS {
                bail!("NvFBCCreateInstance failed: {}", status);
            }
            let fns = fns_uninit.assume_init();
            log::debug!(
                "[NVFBC] gl.display after NvFBCCreateInstance: {:?}",
                gl.display
            );

            // Magic key to enable NVFBC on consumer GPUs (same as Sunshine)
            let magic_key: [u32; 4] = [0xAEF57AC5, 0x401D1A39, 0x1B856BBE, 0x9ED0CEBA];

            gl.make_current_offscreen();
            log::debug!(
                "[NVFBC] gl.display after make_current_offscreen: {:?}",
                gl.display
            );

            let mut handle: NvfbcSessionHandle = 0;
            let mut create_params: NvfbcCreateHandleParams = std::mem::zeroed();
            create_params.dw_version = nvfbc_struct_version::<NvfbcCreateHandleParams>(2);
            create_params.private_data = magic_key.as_ptr() as *const c_void;
            create_params.private_data_size = std::mem::size_of_val(&magic_key) as u32;
            create_params.externally_managed_context = 1;
            create_params.glx_ctx = gl.glx_context as *mut c_void;
            create_params.glx_fb_config = gl.output_fb_config as *mut c_void;

            log::debug!("[NVFBC] NvfbcCreateHandleParams:");
            log::debug!("  size: {}", std::mem::size_of::<NvfbcCreateHandleParams>());
            log::debug!("  dw_version: 0x{:08x}", create_params.dw_version);
            log::debug!("  private_data: {:?}", create_params.private_data);
            log::debug!("  private_data_size: {}", create_params.private_data_size);
            log::debug!(
                "  externally_managed_context: {}",
                create_params.externally_managed_context
            );
            log::debug!("  glx_ctx: {:?}", create_params.glx_ctx);
            log::debug!("  glx_fb_config: {:?}", create_params.glx_fb_config);
            log::debug!("[NVFBC] gl.display before create_handle: {:?}", gl.display);

            let status = (fns.create_handle)(&mut handle, &mut create_params);
            log::debug!("[NVFBC] gl.display AFTER create_handle: {:?}", gl.display);

            if status != NVFBC_SUCCESS {
                let err = CStr::from_ptr((fns.get_last_error_str)(handle));
                bail!("nvFBCCreateHandle failed: {}", err.to_string_lossy());
            }

            // Bind NvFBC context to this thread for subsequent calls.
            let mut bind_params = NvfbcBindContextParams {
                dw_version: nvfbc_struct_version::<NvfbcBindContextParams>(1),
            };
            let status = (fns.bind_context)(handle, &mut bind_params);
            if status != NVFBC_SUCCESS {
                let err = CStr::from_ptr((fns.get_last_error_str)(handle));
                bail!("nvFBCBindContext failed: {}", err.to_string_lossy());
            }

            // Get status to find output ID
            log::debug!(
                "[NVFBC] Preparing NvfbcGetStatusParams (size={})",
                std::mem::size_of::<NvfbcGetStatusParams>()
            );
            let mut status_params: NvfbcGetStatusParams = std::mem::zeroed();
            status_params.dw_version = nvfbc_struct_version::<NvfbcGetStatusParams>(2);
            log::debug!("[NVFBC] gl.display before get_status: {:?}", gl.display);

            let status = (fns.get_status)(handle, &mut status_params);
            log::debug!("[NVFBC] gl.display AFTER get_status: {:?}", gl.display);

            if status != NVFBC_SUCCESS {
                let err = CStr::from_ptr((fns.get_last_error_str)(handle));
                bail!("nvFBCGetStatus failed: {}", err.to_string_lossy());
            }

            log::info!(
                "NVFBC: {} outputs, screen {}x{}",
                status_params.output_num,
                status_params.screen_size.w,
                status_params.screen_size.h
            );

            let mut output_id = 0u32;
            let mut capture_x = 0u32;
            let mut capture_y = 0u32;
            let mut capture_w = status_params.screen_size.w;
            let mut capture_h = status_params.screen_size.h;
            let mut found = false;
            for i in 0..status_params.output_num as usize {
                let out = &status_params.outputs[i];
                let name_end = out
                    .name
                    .iter()
                    .position(|&b| b == 0)
                    .unwrap_or(out.name.len());
                let name = String::from_utf8_lossy(&out.name[..name_end]);
                log::info!(
                    "  Output {}: '{}' id={} {}x{}+{}+{}",
                    i,
                    name,
                    out.dw_id,
                    out.tracker_box.w,
                    out.tracker_box.h,
                    out.tracker_box.x,
                    out.tracker_box.y
                );
                if name.trim() == output_name {
                    output_id = out.dw_id;
                    capture_x = out.tracker_box.x;
                    capture_y = out.tracker_box.y;
                    capture_w = out.tracker_box.w;
                    capture_h = out.tracker_box.h;
                    found = true;
                }
            }

            if !found {
                bail!("NVFBC output '{}' not found", output_name);
            }
            log::info!(
                "NVFBC: tracking output '{}' (id={})",
                output_name,
                output_id
            );

            // Create capture session
            let mut session_params: NvfbcCreateCaptureSessionParams = std::mem::zeroed();
            session_params.dw_version = nvfbc_struct_version::<NvfbcCreateCaptureSessionParams>(6);
            session_params.capture_type = NVFBC_CAPTURE_TO_GL;
            session_params.tracking_type = NVFBC_TRACKING_OUTPUT;
            session_params.output_id = output_id;
            session_params.capture_box = NvfbcBox {
                x: capture_x,
                y: capture_y,
                w: capture_w,
                h: capture_h,
            };
            session_params.frame_size = NvfbcSize {
                w: capture_w,
                h: capture_h,
            };
            session_params.round_frame_size = 0;
            session_params.with_cursor = 0;
            session_params.allow_direct_capture = 1;

            let status = (fns.create_capture_session)(handle, &mut session_params);
            if status != NVFBC_SUCCESS {
                let err = CStr::from_ptr((fns.get_last_error_str)(handle));
                bail!(
                    "nvFBCCreateCaptureSession failed: {}",
                    err.to_string_lossy()
                );
            }

            // Setup ToGL
            let mut gl_params: NvfbcToGlSetupParams = std::mem::zeroed();
            gl_params.dw_version = nvfbc_struct_version::<NvfbcToGlSetupParams>(2);
            gl_params.buffer_format = NVFBC_BUFFER_FORMAT_BGRA;

            let status = (fns.to_gl_setup)(handle, &mut gl_params);
            if status != NVFBC_SUCCESS {
                let err = CStr::from_ptr((fns.get_last_error_str)(handle));
                bail!("nvFBCToGLSetUp failed: {}", err.to_string_lossy());
            }

            log::info!(
                "NVFBC ToGL: textures=[{}, {}] target=0x{:x} format=0x{:x}",
                gl_params.textures[0],
                gl_params.textures[1],
                gl_params.tex_target,
                gl_params.tex_format
            );

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
                bail!(
                    "Initial nvFBCToGLGrabFrame failed: {}",
                    err.to_string_lossy()
                );
            }

            let width = grab_info.dw_width;
            let height = grab_info.dw_height;
            log::info!(
                "NVFBC: capturing {}x{} from '{}'",
                width,
                height,
                output_name
            );

            let mut release_params = NvfbcReleaseContextParams {
                dw_version: nvfbc_struct_version::<NvfbcReleaseContextParams>(1),
            };
            let _ = (fns.release_context)(handle, &mut release_params);

            Ok(Self {
                fns,
                handle,
                textures: gl_params.textures,
                output_id,
                capture_x,
                capture_y,
                capture_w,
                capture_h,
                width,
                height,
                screen_width: status_params.screen_size.w,
                screen_height: status_params.screen_size.h,
                _lib: lib,
            })
        }
    }

    /// List available NVFBC outputs (id, name, width, height).
    pub fn list_outputs(&self) -> Vec<(u32, String, u32, u32)> {
        unsafe {
            let mut status_params: NvfbcGetStatusParams = std::mem::zeroed();
            status_params.dw_version = nvfbc_struct_version::<NvfbcGetStatusParams>(2);
            let status = (self.fns.get_status)(self.handle, &mut status_params);
            if status != NVFBC_SUCCESS {
                return vec![];
            }
            let mut results = Vec::new();
            for i in 0..status_params.output_num as usize {
                let out = &status_params.outputs[i];
                let name_end = out
                    .name
                    .iter()
                    .position(|&b| b == 0)
                    .unwrap_or(out.name.len());
                let name = String::from_utf8_lossy(&out.name[..name_end])
                    .trim()
                    .to_string();
                results.push((out.dw_id, name, out.tracker_box.w, out.tracker_box.h));
            }
            results
        }
    }

    /// Switch capture output by NVFBC output id.
    pub fn switch_output_by_id(&mut self, output_id: u32) -> Result<()> {
        self.output_id = output_id;
        if let Some((_, _, w, h)) = self
            .list_outputs()
            .into_iter()
            .find(|(id, _, _, _)| *id == output_id)
        {
            self.capture_x = 0;
            self.capture_y = 0;
            self.capture_w = w;
            self.capture_h = h;
        }
        self.recreate_session()
    }

    /// Recreate the capture session and ToGL setup. Called when the display
    /// resolution changes (e.g., client connects with different resolution),
    /// or when switching outputs.
    pub fn recreate_session(&mut self) -> Result<()> {
        unsafe {
            // Destroy existing capture session
            let mut bind_params = NvfbcBindContextParams {
                dw_version: nvfbc_struct_version::<NvfbcBindContextParams>(1),
            };
            let status = (self.fns.bind_context)(self.handle, &mut bind_params);
            if status != NVFBC_SUCCESS {
                let err = CStr::from_ptr((self.fns.get_last_error_str)(self.handle));
                bail!("recreate bind_context failed: {}", err.to_string_lossy());
            }

            let mut destroy_params = NvfbcDestroyCaptureSessionParams {
                dw_version: nvfbc_struct_version::<NvfbcDestroyCaptureSessionParams>(1),
            };
            let status = (self.fns.destroy_capture_session)(self.handle, &mut destroy_params);
            if status != NVFBC_SUCCESS {
                let err = CStr::from_ptr((self.fns.get_last_error_str)(self.handle));
                log::warn!("destroy_capture_session: {}", err.to_string_lossy());
            }

            // Create new capture session with same output
            let mut session_params: NvfbcCreateCaptureSessionParams = std::mem::zeroed();
            session_params.dw_version = nvfbc_struct_version::<NvfbcCreateCaptureSessionParams>(6);
            session_params.capture_type = NVFBC_CAPTURE_TO_GL;
            session_params.tracking_type = NVFBC_TRACKING_OUTPUT;
            session_params.output_id = self.output_id;
            session_params.capture_box = NvfbcBox {
                x: self.capture_x,
                y: self.capture_y,
                w: self.capture_w,
                h: self.capture_h,
            };
            session_params.frame_size = NvfbcSize {
                w: self.capture_w,
                h: self.capture_h,
            };
            session_params.round_frame_size = 0;
            session_params.with_cursor = 0;
            session_params.allow_direct_capture = 1;

            let status = (self.fns.create_capture_session)(self.handle, &mut session_params);
            if status != NVFBC_SUCCESS {
                let err = CStr::from_ptr((self.fns.get_last_error_str)(self.handle));
                bail!(
                    "recreate nvFBCCreateCaptureSession failed: {}",
                    err.to_string_lossy()
                );
            }

            // Re-setup ToGL
            let mut gl_params: NvfbcToGlSetupParams = std::mem::zeroed();
            gl_params.dw_version = nvfbc_struct_version::<NvfbcToGlSetupParams>(2);
            gl_params.buffer_format = NVFBC_BUFFER_FORMAT_BGRA;

            let status = (self.fns.to_gl_setup)(self.handle, &mut gl_params);
            if status != NVFBC_SUCCESS {
                let err = CStr::from_ptr((self.fns.get_last_error_str)(self.handle));
                bail!("recreate nvFBCToGLSetUp failed: {}", err.to_string_lossy());
            }

            self.textures = gl_params.textures;

            // Grab one frame to get new dimensions
            let mut grab_info: NvfbcFrameGrabInfo = std::mem::zeroed();
            let mut grab_params: NvfbcToGlGrabFrameParams = std::mem::zeroed();
            grab_params.dw_version = nvfbc_struct_version::<NvfbcToGlGrabFrameParams>(2);
            grab_params.dw_flags = 0;
            grab_params.frame_grab_info = &mut grab_info;
            grab_params.dw_timeout_ms = 5000;

            let status = (self.fns.to_gl_grab_frame)(self.handle, &mut grab_params);
            if status != NVFBC_SUCCESS {
                let err = CStr::from_ptr((self.fns.get_last_error_str)(self.handle));
                bail!("recreate grab failed: {}", err.to_string_lossy());
            }

            self.width = grab_info.dw_width;
            self.height = grab_info.dw_height;
            log::info!(
                "NVFBC: session recreated, now capturing {}x{}",
                self.width,
                self.height
            );

            let mut release_params = NvfbcReleaseContextParams {
                dw_version: nvfbc_struct_version::<NvfbcReleaseContextParams>(1),
            };
            let _ = (self.fns.release_context)(self.handle, &mut release_params);

            Ok(())
        }
    }

    /// Grab a frame, returns (GL texture ID, width, height, is_new).
    pub fn grab_frame(&mut self) -> Result<(u32, u32, u32, bool)> {
        unsafe {
            let mut bind_params = NvfbcBindContextParams {
                dw_version: nvfbc_struct_version::<NvfbcBindContextParams>(1),
            };
            let status = (self.fns.bind_context)(self.handle, &mut bind_params);
            if status != NVFBC_SUCCESS {
                let err = CStr::from_ptr((self.fns.get_last_error_str)(self.handle));
                bail!("nvFBCBindContext: {}", err.to_string_lossy());
            }

            let mut grab_info: NvfbcFrameGrabInfo = std::mem::zeroed();
            let mut grab_params: NvfbcToGlGrabFrameParams = std::mem::zeroed();
            grab_params.dw_version = nvfbc_struct_version::<NvfbcToGlGrabFrameParams>(2);
            grab_params.dw_flags = NVFBC_TOGL_GRAB_FLAGS_NOWAIT;
            grab_params.frame_grab_info = &mut grab_info;

            let status = (self.fns.to_gl_grab_frame)(self.handle, &mut grab_params);
            let mut release_params = NvfbcReleaseContextParams {
                dw_version: nvfbc_struct_version::<NvfbcReleaseContextParams>(1),
            };
            let _ = (self.fns.release_context)(self.handle, &mut release_params);
            if status != NVFBC_SUCCESS {
                let err = CStr::from_ptr((self.fns.get_last_error_str)(self.handle));
                bail!("nvFBCToGLGrabFrame: {}", err.to_string_lossy());
            }

            self.width = grab_info.dw_width;
            self.height = grab_info.dw_height;
            let tex_id = self.textures[grab_params.dw_texture_index as usize];
            Ok((
                tex_id,
                grab_info.dw_width,
                grab_info.dw_height,
                grab_info.b_is_new_frame != 0,
            ))
        }
    }
}

impl Drop for NvfbcCapture {
    fn drop(&mut self) {
        unsafe {
            let mut destroy_params = NvfbcDestroyCaptureSessionParams {
                dw_version: nvfbc_struct_version::<NvfbcDestroyCaptureSessionParams>(1),
            };
            (self.fns.destroy_capture_session)(self.handle, &mut destroy_params);
            (self.fns.destroy_handle)(self.handle, ptr::null_mut());
        }
    }
}
