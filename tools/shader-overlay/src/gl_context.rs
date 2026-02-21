use anyhow::{bail, Result};
use glow::HasContext;
use std::ffi::{CStr, CString};
use std::os::raw::{c_int, c_void};
use std::ptr;
use std::sync::Arc;
use x11::glx;
use x11::xlib;

/// GLX extension function pointers for texture_from_pixmap
pub struct GlxExtFns {
    pub bind_tex_image: unsafe extern "C" fn(*mut xlib::Display, glx::GLXDrawable, c_int, *const c_int),
    pub release_tex_image: unsafe extern "C" fn(*mut xlib::Display, glx::GLXDrawable, c_int),
}

/// Holds the GLX context, X display, and glow context
pub struct GlState {
    pub glow_ctx: Arc<glow::Context>,
    pub display: *mut xlib::Display,
    pub glx_context: glx::GLXContext,
    pub fb_config: glx::GLXFBConfig,
    pub glx_ext: GlxExtFns,
    _helper_window: xlib::Window,
}

// GLX_EXT_texture_from_pixmap constants
pub const GLX_BIND_TO_TEXTURE_RGBA_EXT: c_int = 0x20D1;
pub const GLX_TEXTURE_TARGET_EXT: c_int = 0x20D6;
pub const GLX_TEXTURE_2D_EXT: c_int = 0x20DC;
pub const GLX_TEXTURE_FORMAT_EXT: c_int = 0x20D5;
pub const GLX_TEXTURE_FORMAT_RGBA_EXT: c_int = 0x20DA;
pub const GLX_FRONT_EXT: c_int = 0x20DE;
pub const GLX_BIND_TO_TEXTURE_TARGETS_EXT: c_int = 0x20D3;
pub const GLX_TEXTURE_2D_BIT_EXT: c_int = 0x0002;
pub const GLX_Y_INVERTED_EXT: c_int = 0x20D4;

impl GlState {
    pub fn new() -> Result<Self> {
        unsafe {
            // Open X display
            let display = xlib::XOpenDisplay(ptr::null());
            if display.is_null() {
                bail!("Failed to open X display");
            }

            let screen = xlib::XDefaultScreen(display);
            let root = xlib::XRootWindow(display, screen);

            // Choose FBConfig with texture_from_pixmap support
            let fb_attribs: Vec<c_int> = vec![
                glx::GLX_X_RENDERABLE, 1,
                glx::GLX_DRAWABLE_TYPE, glx::GLX_WINDOW_BIT | glx::GLX_PIXMAP_BIT,
                glx::GLX_RENDER_TYPE, glx::GLX_RGBA_BIT,
                GLX_BIND_TO_TEXTURE_RGBA_EXT, 1,
                GLX_BIND_TO_TEXTURE_TARGETS_EXT, GLX_TEXTURE_2D_BIT_EXT,
                glx::GLX_RED_SIZE, 8,
                glx::GLX_GREEN_SIZE, 8,
                glx::GLX_BLUE_SIZE, 8,
                glx::GLX_ALPHA_SIZE, 8,
                glx::GLX_DOUBLEBUFFER, 1,
                0, // None terminator
            ];

            let mut num_configs: c_int = 0;
            let configs = glx::glXChooseFBConfig(display, screen, fb_attribs.as_ptr(), &mut num_configs);
            if configs.is_null() || num_configs == 0 {
                bail!("No suitable GLX FBConfig found (need texture_from_pixmap support)");
            }

            let fb_config = *configs;
            xlib::XFree(configs as *mut c_void);

            let visual = glx::glXGetVisualFromFBConfig(display, fb_config);
            if visual.is_null() {
                bail!("Failed to get visual from FBConfig");
            }

            // Create small invisible helper window for GLX context binding
            let mut swa: xlib::XSetWindowAttributes = std::mem::zeroed();
            swa.colormap = xlib::XCreateColormap(display, root, (*visual).visual, xlib::AllocNone);
            swa.override_redirect = 1;

            let helper_window = xlib::XCreateWindow(
                display, root,
                0, 0, 1, 1, 0,
                (*visual).depth, xlib::InputOutput as u32,
                (*visual).visual,
                xlib::CWColormap | xlib::CWOverrideRedirect,
                &mut swa,
            );
            xlib::XFree(visual as *mut c_void);

            if helper_window == 0 {
                bail!("Failed to create helper window");
            }

            // Create GLX context
            let glx_context = glx::glXCreateNewContext(
                display, fb_config, glx::GLX_RGBA_TYPE, ptr::null_mut(), 1,
            );
            if glx_context.is_null() {
                bail!("Failed to create GLX context");
            }

            // Make context current on helper window
            if glx::glXMakeCurrent(display, helper_window, glx_context) == 0 {
                bail!("Failed to make GLX context current");
            }

            // Load GLX extension function pointers
            let bind_name = CString::new("glXBindTexImageEXT").unwrap();
            let release_name = CString::new("glXReleaseTexImageEXT").unwrap();

            let bind_fn = glx::glXGetProcAddress(bind_name.as_ptr() as *const u8);
            let release_fn = glx::glXGetProcAddress(release_name.as_ptr() as *const u8);

            if bind_fn.is_none() || release_fn.is_none() {
                bail!("GLX_EXT_texture_from_pixmap not supported");
            }

            let glx_ext = GlxExtFns {
                bind_tex_image: std::mem::transmute(bind_fn.unwrap()),
                release_tex_image: std::mem::transmute(release_fn.unwrap()),
            };

            // Create glow context from GLX function loader
            let glow_ctx = glow::Context::from_loader_function_cstr(|name: &CStr| {
                let ptr = glx::glXGetProcAddress(name.as_ptr() as *const u8);
                match ptr {
                    Some(f) => f as *const c_void,
                    None => ptr::null(),
                }
            });

            let version = glow_ctx.get_parameter_string(glow::VERSION);
            log::info!("OpenGL version: {}", version);

            Ok(Self {
                glow_ctx: Arc::new(glow_ctx),
                display,
                glx_context,
                fb_config,
                glx_ext,
                _helper_window: helper_window,
            })
        }
    }

    /// Make the GLX context current on a specific drawable
    pub unsafe fn make_current(&self, drawable: glx::GLXDrawable) {
        glx::glXMakeCurrent(self.display, drawable, self.glx_context);
    }

    /// Make the GLX context current on the helper window (for off-screen rendering)
    pub unsafe fn make_current_offscreen(&self) {
        glx::glXMakeCurrent(self.display, self._helper_window, self.glx_context);
    }
}

impl Drop for GlState {
    fn drop(&mut self) {
        unsafe {
            glx::glXMakeCurrent(self.display, 0, ptr::null_mut());
            glx::glXDestroyContext(self.display, self.glx_context);
            xlib::XDestroyWindow(self.display, self._helper_window);
            xlib::XCloseDisplay(self.display);
        }
    }
}
