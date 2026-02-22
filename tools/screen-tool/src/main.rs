//! screen-tool — Secondary screen zoom/crop viewer and FPS monitor.
//!
//! Captures the primary emulator window via XComposite and displays it on the
//! bottom display (DP-2). Supports click-drag region selection for zoom, and
//! an FPS overlay toggle.
//!
//! Automatically yields to emulator secondary windows (e.g., 3DS bottom screen)
//! when they appear, and re-appears when they disappear.
//!
//! Env vars:
//!   SCREEN_TOOL_WINDOW    - Window title substring to capture (default: auto-detect emulator)
//!   SCREEN_TOOL_SECONDARY - Secondary window patterns to yield to (comma-separated)

use anyhow::{bail, Result};
use clap::Parser;
use glow::HasContext;
use signal_hook::flag;
use std::ffi::CStr;
use std::os::raw::{c_int, c_ulong, c_void};
use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use x11::glx;
use x11::xlib;

// ─── GLX texture_from_pixmap constants ───────────────────────────────────────

const GLX_BIND_TO_TEXTURE_RGB_EXT: c_int = 0x20D0;
const GLX_BIND_TO_TEXTURE_RGBA_EXT: c_int = 0x20D1;
const GLX_BIND_TO_TEXTURE_TARGETS_EXT: c_int = 0x20D3;
const GLX_Y_INVERTED_EXT: c_int = 0x20D4;
const GLX_TEXTURE_FORMAT_EXT: c_int = 0x20D5;
const GLX_TEXTURE_TARGET_EXT: c_int = 0x20D6;
const GLX_TEXTURE_FORMAT_RGB_EXT: c_int = 0x20D9;
const GLX_TEXTURE_FORMAT_RGBA_EXT: c_int = 0x20DA;
const GLX_TEXTURE_2D_EXT: c_int = 0x20DC;
const GLX_TEXTURE_2D_BIT_EXT: c_int = 0x0002;
const GLX_FRONT_LEFT_EXT: c_int = 0x20DE;

// ─── XComposite / XDamage FFI ────────────────────────────────────────────────

extern "C" {
    fn XCompositeRedirectWindow(display: *mut xlib::Display, window: c_ulong, update: c_int);
    fn XCompositeUnredirectWindow(display: *mut xlib::Display, window: c_ulong, update: c_int);
    fn XCompositeNameWindowPixmap(display: *mut xlib::Display, window: c_ulong) -> c_ulong;
    fn XCompositeQueryExtension(display: *mut xlib::Display, event_base: *mut c_int, error_base: *mut c_int) -> c_int;
    fn XDamageCreate(display: *mut xlib::Display, drawable: c_ulong, level: c_int) -> c_ulong;
    fn XDamageDestroy(display: *mut xlib::Display, damage: c_ulong);
    fn XDamageSubtract(display: *mut xlib::Display, damage: c_ulong, repair: c_ulong, parts: c_ulong);
    fn XDamageQueryExtension(display: *mut xlib::Display, event_base: *mut c_int, error_base: *mut c_int) -> c_int;
}

const COMPOSITE_REDIRECT_AUTOMATIC: c_int = 1;
const XDAMAGE_REPORT_NON_EMPTY: c_int = 1;

// ─── X11 error handler ──────────────────────────────────────────────────────

unsafe extern "C" fn x11_error_handler(_display: *mut xlib::Display, event: *mut xlib::XErrorEvent) -> c_int {
    let err = unsafe { &*event };
    if (err.request_code == 152 || err.request_code == 156) && (err.error_code == 9 || err.error_code == 161) {
        log::debug!("X11 error (benign): request={}, error={}, minor={}", err.request_code, err.error_code, err.minor_code);
    } else {
        log::warn!("X11 error: request={}, error={}, minor={}", err.request_code, err.error_code, err.minor_code);
    }
    0
}

// ─── GLX extension function pointers ─────────────────────────────────────────

struct GlxExtFns {
    bind_tex_image: unsafe extern "C" fn(*mut xlib::Display, glx::GLXDrawable, c_int, *const c_int),
    release_tex_image: unsafe extern "C" fn(*mut xlib::Display, glx::GLXDrawable, c_int),
}

// ─── GL state ────────────────────────────────────────────────────────────────

struct GlState {
    glow_ctx: Arc<glow::Context>,
    display: *mut xlib::Display,
    glx_context: glx::GLXContext,
    output_fb_config: glx::GLXFBConfig,
    glx_ext: GlxExtFns,
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

            // FBConfig for output windows — match screen depth
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

            let glx_ext = Self::load_glx_ext()?;

            Ok(Self {
                glow_ctx: Arc::new(glow_ctx),
                display, glx_context, output_fb_config, glx_ext, helper_window,
            })
        }
    }

    unsafe fn load_glx_ext() -> Result<GlxExtFns> {
        let bind = glx::glXGetProcAddress(b"glXBindTexImageEXT\0".as_ptr())
            .ok_or_else(|| anyhow::anyhow!("glXBindTexImageEXT not available"))?;
        let release = glx::glXGetProcAddress(b"glXReleaseTexImageEXT\0".as_ptr())
            .ok_or_else(|| anyhow::anyhow!("glXReleaseTexImageEXT not available"))?;
        Ok(GlxExtFns {
            bind_tex_image: std::mem::transmute(bind),
            release_tex_image: std::mem::transmute(release),
        })
    }

    fn find_fbconfig_for_visual(&self, visual_id: c_ulong) -> Result<glx::GLXFBConfig> {
        unsafe {
            let screen = xlib::XDefaultScreen(self.display);
            let mut num: c_int = 0;
            let configs = glx::glXGetFBConfigs(self.display, screen, &mut num);
            if configs.is_null() || num == 0 {
                bail!("No FBConfigs available");
            }
            let mut matched = None;
            for i in 0..num {
                let cfg = *configs.offset(i as isize);
                let mut vid: c_int = 0;
                glx::glXGetFBConfigAttrib(self.display, cfg, glx::GLX_VISUAL_ID, &mut vid);
                if vid as c_ulong != visual_id { continue; }
                let mut rgb: c_int = 0;
                let mut rgba: c_int = 0;
                glx::glXGetFBConfigAttrib(self.display, cfg, GLX_BIND_TO_TEXTURE_RGB_EXT, &mut rgb);
                glx::glXGetFBConfigAttrib(self.display, cfg, GLX_BIND_TO_TEXTURE_RGBA_EXT, &mut rgba);
                if rgb == 0 && rgba == 0 { continue; }
                let mut targets: c_int = 0;
                glx::glXGetFBConfigAttrib(self.display, cfg, GLX_BIND_TO_TEXTURE_TARGETS_EXT, &mut targets);
                if targets & GLX_TEXTURE_2D_BIT_EXT == 0 { continue; }
                matched = Some(cfg);
                break;
            }
            xlib::XFree(configs as *mut c_void);
            matched.ok_or_else(|| anyhow::anyhow!("No FBConfig for visual 0x{:x}", visual_id))
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

/// Find the primary emulator window by title pattern
fn find_emulator_window(display: *mut xlib::Display, pattern: &str) -> Option<(c_ulong, u32, u32, i32, i32)> {
    let root = unsafe { xlib::XDefaultRootWindow(display) };
    let pat_lower = pattern.to_lowercase();
    let mut best: Option<(c_ulong, u32, u32, i32, i32)> = None;
    let mut best_area: u64 = 0;

    if let Some(clients) = get_client_list(display, root) {
        for wid in clients {
            if let Some(name) = get_window_name(display, wid) {
                if name.starts_with("Shader: ") || name.starts_with("ScreenTool") { continue; }
                if name.contains("Secondary Window") { continue; }
                if name.to_lowercase().contains(&pat_lower) {
                    unsafe {
                        let mut attrs: xlib::XWindowAttributes = std::mem::zeroed();
                        if xlib::XGetWindowAttributes(display, wid, &mut attrs) != 0
                            && attrs.width >= 32 && attrs.height >= 32
                        {
                            let mut x = 0i32;
                            let mut y = 0i32;
                            let mut child: c_ulong = 0;
                            let r = xlib::XDefaultRootWindow(display);
                            xlib::XTranslateCoordinates(display, wid, r, 0, 0, &mut x, &mut y, &mut child);
                            let area = attrs.width as u64 * attrs.height as u64;
                            if area > best_area {
                                best = Some((wid, attrs.width as u32, attrs.height as u32, x, y));
                                best_area = area;
                            }
                        }
                    }
                }
            }
        }
    }
    best
}

/// Check if a secondary emulator window exists.
/// Checks _NET_CLIENT_LIST first, then falls back to recursive XQueryTree
/// (some emulators create secondary windows as transient/child windows
/// that openbox doesn't include in _NET_CLIENT_LIST).
fn secondary_window_exists(display: *mut xlib::Display, patterns: &[String]) -> bool {
    let root = unsafe { xlib::XDefaultRootWindow(display) };

    // First try _NET_CLIENT_LIST (fast, reliable for managed windows)
    if let Some(clients) = get_client_list(display, root) {
        for wid in clients {
            if check_window_matches(display, wid, patterns) {
                return true;
            }
        }
    }

    // Fallback: check direct children of root (covers transient/unmanaged windows)
    unsafe {
        let mut root_ret: c_ulong = 0;
        let mut parent_ret: c_ulong = 0;
        let mut children: *mut c_ulong = ptr::null_mut();
        let mut nchildren: u32 = 0;
        if xlib::XQueryTree(display, root, &mut root_ret, &mut parent_ret, &mut children, &mut nchildren) != 0 {
            let result = if !children.is_null() && nchildren > 0 {
                let child_slice = std::slice::from_raw_parts(children, nchildren as usize);
                child_slice.iter().any(|&wid| check_window_matches(display, wid, patterns))
            } else {
                false
            };
            if !children.is_null() {
                xlib::XFree(children as *mut c_void);
            }
            result
        } else {
            false
        }
    }
}

fn check_window_matches(display: *mut xlib::Display, wid: c_ulong, patterns: &[String]) -> bool {
    if let Some(name) = get_window_name(display, wid) {
        for pat in patterns {
            if name.contains(pat.as_str()) {
                unsafe {
                    let mut attrs: xlib::XWindowAttributes = std::mem::zeroed();
                    if xlib::XGetWindowAttributes(display, wid, &mut attrs) != 0
                        && attrs.width >= 32 && attrs.height >= 32
                        && attrs.map_state == xlib::IsViewable
                    {
                        return true;
                    }
                }
            }
        }
    }
    false
}

// ─── XComposite capture ─────────────────────────────────────────────────────

struct WindowCapture {
    display: *mut xlib::Display,
    source_window: c_ulong,
    x_pixmap: c_ulong,
    glx_pixmap: glx::GLXPixmap,
    texture: glow::Texture,
    width: u32,
    height: u32,
    dirty: bool,
    capture_fb_config: glx::GLXFBConfig,
    texture_format: c_int,
    y_inverted: bool,
    damage: c_ulong,
    damage_event_base: c_int,
}

impl WindowCapture {
    fn new(gl: &GlState, source_window: c_ulong) -> Result<Self> {
        unsafe {
            let display = gl.display;
            let mut comp_event = 0;
            let mut comp_error = 0;
            if XCompositeQueryExtension(display, &mut comp_event, &mut comp_error) == 0 {
                bail!("XComposite not available");
            }
            let mut damage_event_base = 0;
            let mut damage_error = 0;
            if XDamageQueryExtension(display, &mut damage_event_base, &mut damage_error) == 0 {
                bail!("XDamage not available");
            }

            let mut attrs: xlib::XWindowAttributes = std::mem::zeroed();
            if xlib::XGetWindowAttributes(display, source_window, &mut attrs) == 0 {
                bail!("Failed to get window attributes for 0x{:x}", source_window);
            }
            let width = attrs.width as u32;
            let height = attrs.height as u32;
            let visual_id = (*attrs.visual).visualid;
            let depth = attrs.depth;

            let capture_fb_config = gl.find_fbconfig_for_visual(visual_id)?;
            let texture_format = if depth >= 32 { GLX_TEXTURE_FORMAT_RGBA_EXT } else { GLX_TEXTURE_FORMAT_RGB_EXT };

            let mut y_inv: c_int = 0;
            glx::glXGetFBConfigAttrib(display, capture_fb_config, GLX_Y_INVERTED_EXT, &mut y_inv);
            let y_inverted = y_inv != 0;

            xlib::XSelectInput(display, source_window, xlib::StructureNotifyMask);

            // Use AUTOMATIC redirect — doesn't steal from compositor, NVFBC-safe
            XCompositeRedirectWindow(display, source_window, COMPOSITE_REDIRECT_AUTOMATIC);
            xlib::XSync(display, 0);

            let x_pixmap = XCompositeNameWindowPixmap(display, source_window);
            xlib::XSync(display, 0);
            if x_pixmap == 0 {
                bail!("Failed to get composite pixmap");
            }

            let pixmap_attribs: [c_int; 5] = [
                GLX_TEXTURE_TARGET_EXT, GLX_TEXTURE_2D_EXT,
                GLX_TEXTURE_FORMAT_EXT, texture_format,
                0,
            ];
            let glx_pixmap = glx::glXCreatePixmap(display, capture_fb_config, x_pixmap, pixmap_attribs.as_ptr());
            xlib::XSync(display, 0);
            if glx_pixmap == 0 {
                bail!("glXCreatePixmap failed");
            }

            let texture = gl.glow_ctx.create_texture().map_err(|e| anyhow::anyhow!("{}", e))?;
            gl.glow_ctx.bind_texture(glow::TEXTURE_2D, Some(texture));
            gl.glow_ctx.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
            gl.glow_ctx.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
            (gl.glx_ext.bind_tex_image)(display, glx_pixmap, GLX_FRONT_LEFT_EXT, ptr::null());
            xlib::XSync(display, 0);
            gl.glow_ctx.bind_texture(glow::TEXTURE_2D, None);

            let damage = XDamageCreate(display, source_window, XDAMAGE_REPORT_NON_EMPTY);
            if damage == 0 {
                bail!("Failed to create damage tracker");
            }

            log::info!("Capturing window 0x{:x} ({}x{}) via XComposite (automatic redirect)", source_window, width, height);

            Ok(Self {
                display, source_window, x_pixmap, glx_pixmap, texture,
                width, height, dirty: true, capture_fb_config, texture_format,
                y_inverted, damage, damage_event_base,
            })
        }
    }

    fn update_if_dirty(&mut self, gl: &GlState) {
        if !self.dirty { return; }
        unsafe {
            glx::glXWaitX();
            (gl.glx_ext.release_tex_image)(self.display, self.glx_pixmap, GLX_FRONT_LEFT_EXT);
            gl.glow_ctx.bind_texture(glow::TEXTURE_2D, Some(self.texture));
            (gl.glx_ext.bind_tex_image)(self.display, self.glx_pixmap, GLX_FRONT_LEFT_EXT, ptr::null());
            gl.glow_ctx.bind_texture(glow::TEXTURE_2D, None);
            self.dirty = false;
        }
    }

    fn mark_dirty(&mut self) { self.dirty = true; }

    fn acknowledge_damage(&self) {
        unsafe { XDamageSubtract(self.display, self.damage, 0, 0); }
    }
}

impl Drop for WindowCapture {
    fn drop(&mut self) {
        unsafe {
            XDamageDestroy(self.display, self.damage);
            glx::glXDestroyPixmap(self.display, self.glx_pixmap);
            xlib::XFreePixmap(self.display, self.x_pixmap);
            XCompositeUnredirectWindow(self.display, self.source_window, COMPOSITE_REDIRECT_AUTOMATIC);
        }
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
            // Accept mouse, keyboard, and structure events
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

            // Set window title
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

            // Create a persistent FBO for reading capture textures (avoids per-frame allocation)
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

/// Simple 8x8 bitmap font — digits 0-9, space, period, F, P, S, m, s, %, colon
/// Each char is 8 bytes (one per row, MSB first)
const FONT_CHARS: &str = "0123456789 .FPSms%:";

#[rustfmt::skip]
const FONT_DATA: &[[u8; 8]] = &[
    // 0
    [0x3C, 0x66, 0x6E, 0x76, 0x66, 0x66, 0x3C, 0x00],
    // 1
    [0x18, 0x38, 0x18, 0x18, 0x18, 0x18, 0x7E, 0x00],
    // 2
    [0x3C, 0x66, 0x06, 0x0C, 0x18, 0x30, 0x7E, 0x00],
    // 3
    [0x3C, 0x66, 0x06, 0x1C, 0x06, 0x66, 0x3C, 0x00],
    // 4
    [0x0C, 0x1C, 0x3C, 0x6C, 0x7E, 0x0C, 0x0C, 0x00],
    // 5
    [0x7E, 0x60, 0x7C, 0x06, 0x06, 0x66, 0x3C, 0x00],
    // 6
    [0x3C, 0x66, 0x60, 0x7C, 0x66, 0x66, 0x3C, 0x00],
    // 7
    [0x7E, 0x06, 0x0C, 0x18, 0x18, 0x18, 0x18, 0x00],
    // 8
    [0x3C, 0x66, 0x66, 0x3C, 0x66, 0x66, 0x3C, 0x00],
    // 9
    [0x3C, 0x66, 0x66, 0x3E, 0x06, 0x66, 0x3C, 0x00],
    // space
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
    // .
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x18, 0x18, 0x00],
    // F
    [0x7E, 0x60, 0x60, 0x7C, 0x60, 0x60, 0x60, 0x00],
    // P
    [0x7C, 0x66, 0x66, 0x7C, 0x60, 0x60, 0x60, 0x00],
    // S
    [0x3C, 0x66, 0x60, 0x3C, 0x06, 0x66, 0x3C, 0x00],
    // m
    [0x00, 0x00, 0x76, 0x7F, 0x6B, 0x63, 0x63, 0x00],
    // s
    [0x00, 0x00, 0x3E, 0x60, 0x3C, 0x06, 0x7C, 0x00],
    // %
    [0x62, 0x64, 0x08, 0x10, 0x26, 0x46, 0x00, 0x00],
    // :
    [0x00, 0x18, 0x18, 0x00, 0x18, 0x18, 0x00, 0x00],
];

/// Render text into an RGBA pixel buffer. Returns (pixels, width, height).
fn render_text(text: &str, scale: u32) -> (Vec<u8>, u32, u32) {
    let char_w = 8 * scale;
    let char_h = 8 * scale;
    let width = text.len() as u32 * char_w;
    let height = char_h;
    let mut pixels = vec![0u8; (width * height * 4) as usize];

    for (ci, ch) in text.chars().enumerate() {
        let font_idx = FONT_CHARS.find(ch).unwrap_or(10); // default to space
        if font_idx >= FONT_DATA.len() { continue; }
        let glyph = &FONT_DATA[font_idx];

        for row in 0..8u32 {
            for col in 0..8u32 {
                let lit = (glyph[row as usize] >> (7 - col)) & 1 == 1;
                if !lit { continue; }
                // Scale up
                for sy in 0..scale {
                    for sx in 0..scale {
                        let px = ci as u32 * char_w + col * scale + sx;
                        let py = row * scale + sy;
                        let offset = ((py * width + px) * 4) as usize;
                        if offset + 3 < pixels.len() {
                            pixels[offset] = 255;     // R
                            pixels[offset + 1] = 255; // G
                            pixels[offset + 2] = 255; // B
                            pixels[offset + 3] = 200; // A (semi-transparent)
                        }
                    }
                }
            }
        }
    }
    (pixels, width, height)
}

// ─── Zoom state machine ─────────────────────────────────────────────────────

/// Zoom region in normalized source coordinates (0.0 to 1.0)
#[derive(Debug, Clone, Copy)]
struct ZoomRegion {
    sx: f32,
    sy: f32,
    sw: f32,
    sh: f32,
}

impl Default for ZoomRegion {
    fn default() -> Self {
        Self { sx: 0.0, sy: 0.0, sw: 1.0, sh: 1.0 }
    }
}

impl ZoomRegion {
    #[allow(dead_code)]
    fn is_full(&self) -> bool {
        self.sx == 0.0 && self.sy == 0.0 && self.sw == 1.0 && self.sh == 1.0
    }
}

// ─── Blit helpers ────────────────────────────────────────────────────────────

/// Blit a sub-region of capture texture to the output window via persistent FBO.
/// Does NOT call glXSwapBuffers — caller must swap after all rendering is done.
fn blit_texture_region(
    gl: &GlState,
    texture: glow::Texture,
    _src_w: u32, src_h: u32,
    // Source region in texture coords (0..src_w, 0..src_h)
    sx: i32, sy: i32, sw: i32, sh: i32,
    // Output
    output: &OutputWindow,
    y_inverted: bool,
) {
    unsafe {
        gl.make_current(output.glx_window);
        let g = &gl.glow_ctx;

        // Use persistent FBO to read from the capture texture
        g.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(output.read_fbo));
        g.framebuffer_texture_2d(glow::READ_FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(texture), 0);

        g.bind_framebuffer(glow::DRAW_FRAMEBUFFER, None);
        g.viewport(0, 0, output.width as i32, output.height as i32);

        // Handle y-inversion (common on NVIDIA)
        let (src_y0, src_y1) = if y_inverted {
            (src_h as i32 - sy, src_h as i32 - (sy + sh))
        } else {
            (sy, sy + sh)
        };

        g.blit_framebuffer(
            sx, src_y0, sx + sw, src_y1,
            0, 0, output.width as i32, output.height as i32,
            glow::COLOR_BUFFER_BIT, glow::LINEAR,
        );
    }
}

/// Cached FPS overlay — only recreates texture when text changes.
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

    /// Render FPS text overlay onto the default framebuffer (back buffer) at top-left.
    /// Must be called AFTER blit_texture_region and BEFORE glXSwapBuffers.
    fn render(&mut self, gl: &GlState, output: &OutputWindow, fps_text: &str) {
        unsafe {
            gl.make_current(output.glx_window);
            let g = &gl.glow_ctx;
            let scale = 3u32;

            // Only recreate texture when text changes
            if fps_text != self.cached_text || self.texture.is_none() {
                let (pixels, tw, th) = render_text(fps_text, scale);

                if let Some(old_tex) = self.texture.take() {
                    g.delete_texture(old_tex);
                }

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

            // Create FBO once
            if self.fbo.is_none() {
                self.fbo = Some(g.create_framebuffer().unwrap());
            }

            let tw = self.tex_width;
            let th = self.tex_height;
            let margin = 8i32;
            let bg_pad = 4i32;

            // Draw dark background
            g.enable(glow::SCISSOR_TEST);
            g.scissor(margin - bg_pad, (output.height as i32) - (margin + th as i32 + bg_pad),
                tw as i32 + bg_pad * 2, th as i32 + bg_pad * 2);
            g.clear_color(0.0, 0.0, 0.0, 0.7);
            g.clear(glow::COLOR_BUFFER_BIT);
            g.disable(glow::SCISSOR_TEST);

            // Blit cached text texture
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
#[command(name = "screen-tool", about = "Secondary screen zoom/crop viewer and FPS monitor")]
struct Args {
    /// Window title pattern to capture (auto-detects emulator if not set)
    #[arg(long, default_value = "")]
    window: String,

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

    /// Timeout waiting for emulator window (seconds)
    #[arg(long, default_value = "120")]
    timeout: u32,
}

// ─── Auto-detect emulator window patterns ────────────────────────────────────

const EMULATOR_PATTERNS: &[&str] = &[
    "Azahar",
    "melonDS",
    "Dolphin",
    "PPSSPP",
    "Ryujinx",
    "Steam",
];

// ─── Main ────────────────────────────────────────────────────────────────────

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let args = Args::parse();

    let shutdown = Arc::new(AtomicBool::new(false));
    flag::register(signal_hook::consts::SIGTERM, Arc::clone(&shutdown))?;
    flag::register(signal_hook::consts::SIGINT, Arc::clone(&shutdown))?;

    unsafe { xlib::XSetErrorHandler(Some(x11_error_handler)); }

    let gl = GlState::new()?;

    // Parse secondary patterns
    let secondary_patterns: Vec<String> = args.secondary_patterns
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    // Determine emulator window pattern
    let window_pattern = if !args.window.is_empty() {
        args.window.clone()
    } else {
        // Auto-detect: wait for any known emulator window
        log::info!("No --window specified, auto-detecting emulator...");
        String::new()
    };

    // Wait for emulator window
    log::info!("Waiting for emulator window (pattern: '{}')...", if window_pattern.is_empty() { "<any>" } else { &window_pattern });
    let deadline = Instant::now() + Duration::from_secs(args.timeout as u64);
    let mut source_info: Option<(c_ulong, u32, u32, i32, i32)> = None;
    let mut matched_pattern = window_pattern.clone();

    while source_info.is_none() {
        if shutdown.load(Ordering::Relaxed) {
            log::info!("Shutdown before window found");
            return Ok(());
        }
        if Instant::now() > deadline {
            bail!("Timed out waiting for emulator window");
        }

        if window_pattern.is_empty() {
            // Try each known emulator pattern
            for pat in EMULATOR_PATTERNS {
                if let Some(info) = find_emulator_window(gl.display, pat) {
                    log::info!("Auto-detected emulator: '{}'", pat);
                    matched_pattern = pat.to_string();
                    source_info = Some(info);
                    break;
                }
            }
        } else {
            source_info = find_emulator_window(gl.display, &window_pattern);
        }

        if source_info.is_none() {
            thread::sleep(Duration::from_millis(500));
        }
    }

    let (source_wid, src_w, src_h, _src_x, _src_y) = source_info.unwrap();
    log::info!("Capturing window 0x{:x} ({}x{}) pattern='{}'", source_wid, src_w, src_h, matched_pattern);

    // Determine output position (default: below top display)
    let output_y = args.output_y.unwrap_or_else(|| {
        // Try to read DP-0 height from xrandr output via the source window position
        // Default to 1080 if we can't determine
        log::info!("No --output-y specified, defaulting to source window height or 1080");
        src_h as i32
    });

    // Create capture
    unsafe { gl.make_current_offscreen(); }
    let mut capture = WindowCapture::new(&gl, source_wid)?;

    // Create output window on DP-2
    let output = OutputWindow::new(&gl, "ScreenTool: Zoom",
        args.output_x, output_y, args.output_width, args.output_height)?;

    // State — zoom uses separate struct + selecting flag (not an enum variant)
    // so we can correctly read the current zoom region during selection
    let mut zoom = ZoomRegion::default();
    let mut selecting: Option<(i32, i32)> = None; // (x1, y1) during drag
    let mut show_fps = false;
    let mut hidden = false;
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

                // Damage events
                if etype == capture.damage_event_base {
                    capture.mark_dirty();
                    capture.acknowledge_damage();
                    continue;
                }

                match etype {
                    xlib::ButtonPress if !hidden => {
                        let e = event.button;
                        if e.window == output.window {
                            match e.button {
                                1 => {
                                    // Left click: start selection
                                    selecting = Some((e.x, e.y));
                                    log::debug!("Selection start: ({}, {})", e.x, e.y);
                                }
                                3 => {
                                    // Right click: reset to full view
                                    zoom = ZoomRegion::default();
                                    selecting = None;
                                    log::info!("Reset to full view");
                                }
                                _ => {}
                            }
                        }
                    }
                    xlib::ButtonRelease if !hidden => {
                        let e = event.button;
                        if e.window == output.window && e.button == 1 {
                            if let Some((x1, y1)) = selecting.take() {
                                let x2 = e.x;
                                let y2 = e.y;
                                // Convert screen coords to normalized output coords (0..1)
                                let ow = output.width as f32;
                                let oh = output.height as f32;

                                let sel_x1 = (x1.min(x2) as f32 / ow).clamp(0.0, 1.0);
                                let sel_y1 = (y1.min(y2) as f32 / oh).clamp(0.0, 1.0);
                                let sel_x2 = (x1.max(x2) as f32 / ow).clamp(0.0, 1.0);
                                let sel_y2 = (y1.max(y2) as f32 / oh).clamp(0.0, 1.0);

                                let sel_w = sel_x2 - sel_x1;
                                let sel_h = sel_y2 - sel_y1;

                                if sel_w > 0.01 && sel_h > 0.01 {
                                    // Map selection from current view to source coords
                                    // (supports nested zoom — selection within an already-zoomed view)
                                    let new_sx = zoom.sx + sel_x1 * zoom.sw;
                                    let new_sy = zoom.sy + sel_y1 * zoom.sh;
                                    let new_sw = sel_w * zoom.sw;
                                    let new_sh = sel_h * zoom.sh;
                                    zoom = ZoomRegion { sx: new_sx, sy: new_sy, sw: new_sw, sh: new_sh };
                                    log::info!("Zoomed to region: ({:.3}, {:.3}) {:.3}x{:.3}", new_sx, new_sy, new_sw, new_sh);
                                }
                                // If selection too small, just ignore (keep current zoom)
                            }
                        }
                    }
                    xlib::KeyPress if !hidden => {
                        let keysym = xlib::XLookupKeysym(&mut event.key as *mut _, 0);
                        if keysym == x11::keysym::XK_F1 as c_ulong {
                            show_fps = !show_fps;
                            log::info!("FPS overlay: {}", if show_fps { "ON" } else { "OFF" });
                        }
                    }
                    xlib::DestroyNotify => {
                        let e = event.destroy_window;
                        if e.window == capture.source_window {
                            log::info!("Source window destroyed, exiting");
                            return Ok(());
                        }
                    }
                    _ => {}
                }
            }
        }

        // Check for secondary window every ~1 second
        if last_secondary_check.elapsed() >= Duration::from_secs(1) {
            last_secondary_check = Instant::now();
            let secondary_present = secondary_window_exists(gl.display, &secondary_patterns);

            if secondary_present && !hidden {
                log::info!("Secondary window detected, hiding screen-tool");
                output.hide();
                hidden = true;
            } else if !secondary_present && hidden {
                log::info!("Secondary window gone, showing screen-tool");
                output.show(args.output_x, output_y, args.output_width, args.output_height);
                hidden = false;
                zoom = ZoomRegion::default();
                selecting = None;
            }
        }

        // Render
        if !hidden {
            unsafe { gl.make_current_offscreen(); }
            capture.update_if_dirty(&gl);

            // Compute source pixel region from normalized zoom coords
            let cw = capture.width as f32;
            let ch = capture.height as f32;
            let sx = (zoom.sx * cw) as i32;
            let sy = (zoom.sy * ch) as i32;
            let sw = (zoom.sw * cw) as i32;
            let sh = (zoom.sh * ch) as i32;

            // Blit captured region to back buffer (no swap yet)
            blit_texture_region(&gl, capture.texture, capture.width, capture.height,
                sx, sy, sw, sh, &output, capture.y_inverted);

            // FPS counter
            fps_counter += 1;
            if fps_timer.elapsed() >= Duration::from_secs(1) {
                fps_value = fps_counter as f32 / fps_timer.elapsed().as_secs_f32();
                fps_counter = 0;
                fps_timer = Instant::now();
            }

            // Render FPS overlay on top of the blitted content (if enabled)
            if show_fps {
                let fps_text = format!("{:.0} FPS", fps_value);
                fps_overlay.render(&gl, &output, &fps_text);
            }

            // Single swap after all rendering is done
            unsafe { glx::glXSwapBuffers(gl.display, output.glx_window); }
        } else {
            // When hidden, sleep longer to avoid wasting CPU
            thread::sleep(Duration::from_millis(100));
            continue;
        }

        thread::sleep(Duration::from_millis(16)); // ~60fps
    }

    Ok(())
}
