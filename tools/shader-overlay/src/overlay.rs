use anyhow::{bail, Result};
use glow::HasContext;
use std::ffi::CString;
use std::os::raw::{c_int, c_ulong, c_void};
use std::ptr;
use x11::glx;
use x11::xlib;

use crate::gl_context::GlState;

/// A normal openbox-managed window that displays shader output.
/// Has a title for window-management scripts to find it.
pub struct OverlayWindow {
    display: *mut xlib::Display,
    window: c_ulong,
    glx_window: glx::GLXDrawable,
    pub width: u32,
    pub height: u32,
}

extern "C" {
    fn XInternAtom(display: *mut xlib::Display, name: *const i8, only_if_exists: c_int) -> c_ulong;
}

impl OverlayWindow {
    /// Create a normal managed window with the given title.
    /// Uses output_fb_config (screen-depth matching) so it renders visibly.
    pub fn new(gl: &GlState, title: &str, x: i32, y: i32, width: u32, height: u32) -> Result<Self> {
        unsafe {
            let display = gl.display;
            let screen = xlib::XDefaultScreen(display);
            let root = xlib::XRootWindow(display, screen);

            // Use the output FBConfig that matches screen depth
            let visual = glx::glXGetVisualFromFBConfig(display, gl.output_fb_config);
            if visual.is_null() {
                bail!("Failed to get visual for output window");
            }

            let mut swa: xlib::XSetWindowAttributes = std::mem::zeroed();
            swa.colormap = xlib::XCreateColormap(display, root, (*visual).visual, xlib::AllocNone);
            swa.event_mask = xlib::StructureNotifyMask | xlib::ExposureMask;

            // Normal managed window — no override_redirect
            let window = xlib::XCreateWindow(
                display, root,
                x, y, width, height, 0,
                (*visual).depth,
                xlib::InputOutput as u32,
                (*visual).visual,
                xlib::CWColormap | xlib::CWEventMask,
                &mut swa,
            );
            xlib::XFree(visual as *mut c_void);

            if window == 0 {
                bail!("Failed to create output window");
            }

            // Set window title (for xdotool/reposition-windows.sh to find)
            let title_c = CString::new(title).unwrap_or_else(|_| CString::new("Shader Output").unwrap());
            xlib::XStoreName(display, window, title_c.as_ptr());

            // Also set _NET_WM_NAME for UTF-8 support
            let atom_net_wm_name = XInternAtom(display, b"_NET_WM_NAME\0".as_ptr() as *const i8, 0);
            let atom_utf8 = XInternAtom(display, b"UTF8_STRING\0".as_ptr() as *const i8, 0);
            xlib::XChangeProperty(
                display, window, atom_net_wm_name,
                atom_utf8, 8, xlib::PropModeReplace,
                title.as_ptr(), title.len() as i32,
            );

            // Set normal window type
            let atom_wm_type = XInternAtom(display, b"_NET_WM_WINDOW_TYPE\0".as_ptr() as *const i8, 0);
            let atom_normal = XInternAtom(display, b"_NET_WM_WINDOW_TYPE_NORMAL\0".as_ptr() as *const i8, 0);
            xlib::XChangeProperty(
                display, window, atom_wm_type,
                xlib::XA_ATOM, 32, xlib::PropModeReplace,
                &atom_normal as *const c_ulong as *const u8, 1,
            );

            // Create GLX drawable for this window using output FBConfig
            let glx_window = glx::glXCreateWindow(display, gl.output_fb_config, window, ptr::null());
            if glx_window == 0 {
                bail!("Failed to create GLX window for output");
            }

            // Map the window
            xlib::XMapWindow(display, window);
            xlib::XFlush(display);

            log::info!("Created output window 0x{:x} '{}' at ({},{}) {}x{}", window, title, x, y, width, height);

            Ok(Self {
                display,
                window,
                glx_window,
                width,
                height,
            })
        }
    }

    /// Blit the shader output FBO to this window
    pub fn present(&self, gl: &GlState, shader_fbo: glow::Framebuffer, src_width: u32, src_height: u32) {
        unsafe {
            // Switch GLX drawable to this output window
            gl.make_current(self.glx_window);

            gl.glow_ctx.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(shader_fbo));
            gl.glow_ctx.bind_framebuffer(glow::DRAW_FRAMEBUFFER, None);

            gl.glow_ctx.viewport(0, 0, self.width as i32, self.height as i32);

            gl.glow_ctx.blit_framebuffer(
                0, 0, src_width as i32, src_height as i32,
                0, 0, self.width as i32, self.height as i32,
                glow::COLOR_BUFFER_BIT,
                glow::LINEAR,
            );

            glx::glXSwapBuffers(self.display, self.glx_window);
        }
    }

    /// Reposition and resize the window
    pub fn reposition(&mut self, x: i32, y: i32, width: u32, height: u32) {
        unsafe {
            let mut changes: xlib::XWindowChanges = std::mem::zeroed();
            changes.x = x;
            changes.y = y;
            changes.width = width as c_int;
            changes.height = height as c_int;
            xlib::XConfigureWindow(
                self.display, self.window,
                (xlib::CWX | xlib::CWY | xlib::CWWidth | xlib::CWHeight) as u32,
                &mut changes,
            );
            self.width = width;
            self.height = height;
        }
    }

    pub fn window_id(&self) -> c_ulong {
        self.window
    }
}

impl Drop for OverlayWindow {
    fn drop(&mut self) {
        unsafe {
            glx::glXDestroyWindow(self.display, self.glx_window);
            xlib::XDestroyWindow(self.display, self.window);
        }
    }
}
