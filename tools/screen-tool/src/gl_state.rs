//! OpenGL/GLX context management.

use anyhow::{bail, Result};
use glow::HasContext;
use std::ffi::CStr;
use std::os::raw::{c_int, c_void};
use std::ptr;
use std::sync::Arc;
use x11::glx;
use x11::xlib;

pub(crate) unsafe extern "C" fn x11_error_handler(_display: *mut xlib::Display, event: *mut xlib::XErrorEvent) -> c_int {
    let err = unsafe { &*event };
    log::debug!("X11 error: request={}, error={}, minor={}", err.request_code, err.error_code, err.minor_code);
    0
}

pub struct GlState {
    pub glow_ctx: Arc<glow::Context>,
    pub display: *mut xlib::Display,
    pub glx_context: glx::GLXContext,
    pub output_fb_config: glx::GLXFBConfig,
    pub helper_window: xlib::Window,
}

impl GlState {
    pub fn new() -> Result<Self> {
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

    pub unsafe fn make_current(&self, drawable: glx::GLXDrawable) {
        glx::glXMakeCurrent(self.display, drawable, self.glx_context);
    }

    pub unsafe fn make_current_offscreen(&self) {
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
