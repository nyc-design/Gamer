use anyhow::{bail, Result};
use glow::HasContext;
use std::os::raw::{c_int, c_ulong, c_void};
use std::ptr;
use x11::glx;
use x11::xlib;

use crate::gl_context::GlState;

/// An overlay window positioned on top of a source window, used to display shader output
pub struct OverlayWindow {
    display: *mut xlib::Display,
    window: c_ulong,
    glx_window: glx::GLXDrawable,
    pub width: u32,
    pub height: u32,
}

// Atoms for window type
extern "C" {
    fn XInternAtom(display: *mut xlib::Display, name: *const i8, only_if_exists: c_int) -> c_ulong;
}

impl OverlayWindow {
    pub fn new(gl: &GlState, x: i32, y: i32, width: u32, height: u32) -> Result<Self> {
        unsafe {
            let display = gl.display;
            let screen = xlib::XDefaultScreen(display);
            let root = xlib::XRootWindow(display, screen);

            let visual = glx::glXGetVisualFromFBConfig(display, gl.fb_config);
            if visual.is_null() {
                bail!("Failed to get visual for overlay window");
            }

            let mut swa: xlib::XSetWindowAttributes = std::mem::zeroed();
            swa.colormap = xlib::XCreateColormap(display, root, (*visual).visual, xlib::AllocNone);
            swa.override_redirect = 1; // bypass window manager
            swa.event_mask = xlib::StructureNotifyMask | xlib::ExposureMask;

            let window = xlib::XCreateWindow(
                display, root,
                x, y, width, height, 0,
                (*visual).depth,
                xlib::InputOutput as u32,
                (*visual).visual,
                xlib::CWColormap | xlib::CWOverrideRedirect | xlib::CWEventMask,
                &mut swa,
            );
            xlib::XFree(visual as *mut c_void);

            if window == 0 {
                bail!("Failed to create overlay window");
            }

            // Set window type to DOCK (always on top)
            let atom_wm_type = XInternAtom(display, b"_NET_WM_WINDOW_TYPE\0".as_ptr() as *const i8, 0);
            let atom_dock = XInternAtom(display, b"_NET_WM_WINDOW_TYPE_DOCK\0".as_ptr() as *const i8, 0);
            xlib::XChangeProperty(
                display, window, atom_wm_type,
                xlib::XA_ATOM, 32, xlib::PropModeReplace,
                &atom_dock as *const c_ulong as *const u8, 1,
            );

            // Make window click-through (input passes to window below)
            let atom_state = XInternAtom(display, b"_NET_WM_STATE\0".as_ptr() as *const i8, 0);
            let atom_above = XInternAtom(display, b"_NET_WM_STATE_ABOVE\0".as_ptr() as *const i8, 0);
            xlib::XChangeProperty(
                display, window, atom_state,
                xlib::XA_ATOM, 32, xlib::PropModeReplace,
                &atom_above as *const c_ulong as *const u8, 1,
            );

            // Create GLX drawable for this window
            let glx_window = glx::glXCreateWindow(display, gl.fb_config, window, ptr::null());
            if glx_window == 0 {
                bail!("Failed to create GLX window for overlay");
            }

            // Map the window
            xlib::XMapWindow(display, window);
            xlib::XFlush(display);

            log::info!("Created overlay window 0x{:x} at ({},{}) {}x{}", window, x, y, width, height);

            Ok(Self {
                display,
                window,
                glx_window,
                width,
                height,
            })
        }
    }

    /// Blit the shader output FBO to this overlay window
    pub fn present(&self, gl: &GlState, shader_fbo: glow::Framebuffer, src_width: u32, src_height: u32) {
        unsafe {
            // Switch GLX drawable to this overlay window
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

    /// Reposition and resize the overlay to match the source window
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

    /// Raise this overlay window to the top of the stacking order
    pub fn raise_above(&self, _sibling: c_ulong) {
        unsafe {
            xlib::XRaiseWindow(self.display, self.window);
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
