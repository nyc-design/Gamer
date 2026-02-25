//! Winit-based window with egui-winit integration.
//!
//! Creates an X11 window via winit, creates a GLX drawable on it using
//! the shared GlState context, and uses egui-winit for event translation.

use anyhow::{bail, Result};
use std::sync::Arc;
use winit::dpi::{LogicalSize, PhysicalPosition, PhysicalSize};
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::platform::x11::WindowAttributesExtX11;
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::{Window, WindowAttributes, WindowLevel};
use x11::glx;

use crate::gl_state::GlState;

/// Events the main loop cares about.
pub enum AppEvent {
    CloseRequested,
}

pub struct AppWindow {
    pub winit_window: Window,
    pub glx_window: glx::GLXDrawable,
    pub width: u32,
    pub height: u32,

    // egui integration
    egui_ctx: egui::Context,
    egui_winit: egui_winit::State,
    egui_painter: egui_glow::Painter,
    pending_output: Option<egui::FullOutput>,

    // Window state
    mapped: bool,
}

impl AppWindow {
    pub fn new(
        event_loop: &ActiveEventLoop,
        gl: &GlState,
        title: &str,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
    ) -> Result<Self> {
        let is_toggle = title == "ScreenToolToggle";

        // Build winit window attributes with the correct X11 visual
        let visual_id = gl.visual_id();
        let mut attrs = WindowAttributes::default()
            .with_title(title)
            .with_inner_size(LogicalSize::new(width, height))
            .with_position(PhysicalPosition::new(x, y))
            .with_decorations(false)
            .with_x11_visual(visual_id);

        if is_toggle {
            attrs = attrs
                .with_window_level(WindowLevel::AlwaysOnTop)
                .with_override_redirect(true);
        }

        let winit_window = event_loop.create_window(attrs)
            .map_err(|e| anyhow::anyhow!("Failed to create winit window: {}", e))?;

        // Extract X11 window handle to create GLX drawable
        let x_window = match winit_window.window_handle()
            .map_err(|e| anyhow::anyhow!("No window handle: {}", e))?
            .as_raw()
        {
            RawWindowHandle::Xlib(handle) => handle.window as x11::xlib::Window,
            _ => bail!("Expected Xlib window handle"),
        };

        // Create GLX drawable on winit's X11 window
        let glx_window = unsafe { gl.create_glx_window(x_window) };
        if glx_window == 0 {
            bail!("Failed to create GLX window on winit handle");
        }

        // Make context current on this window for painter init
        unsafe {
            gl.make_current(glx_window);
        }

        // Set EWMH properties for toggle button (winit doesn't expose these)
        if is_toggle {
            unsafe {
                Self::set_toggle_ewmh(gl.display, x_window);
            }
        }

        // Initialize egui
        let egui_ctx = egui::Context::default();

        let egui_winit = egui_winit::State::new(
            egui_ctx.clone(),
            egui_ctx.viewport_id(),
            event_loop,
            Some(winit_window.scale_factor() as f32),
            None, // theme
            None, // max_texture_side
        );

        let egui_painter =
            egui_glow::Painter::new(Arc::clone(&gl.glow_ctx), "", None, false)
                .map_err(|e| anyhow::anyhow!("egui Painter init failed: {}", e))?;

        let PhysicalSize { width, height } = winit_window.inner_size();

        log::info!(
            "AppWindow '{}' at ({},{}) {}x{} visual=0x{:x}",
            title,
            x,
            y,
            width,
            height,
            visual_id,
        );

        Ok(Self {
            winit_window,
            glx_window,
            width,
            height,
            egui_ctx,
            egui_winit,
            egui_painter,
            pending_output: None,
            mapped: true,
        })
    }

    /// Set EWMH window properties for the toggle button.
    /// winit doesn't expose _NET_WM_WINDOW_TYPE_DOCK etc., so we use raw X11.
    unsafe fn set_toggle_ewmh(display: *mut x11::xlib::Display, window: x11::xlib::Window) {
        use x11::xlib;

        let atom_type =
            xlib::XInternAtom(display, b"_NET_WM_WINDOW_TYPE\0".as_ptr() as *const _, 0);
        let atom_dock = xlib::XInternAtom(
            display,
            b"_NET_WM_WINDOW_TYPE_DOCK\0".as_ptr() as *const _,
            0,
        );
        xlib::XChangeProperty(
            display,
            window,
            atom_type,
            xlib::XA_ATOM,
            32,
            xlib::PropModeReplace,
            &atom_dock as *const u64 as *const u8,
            1,
        );

        let atom_state =
            xlib::XInternAtom(display, b"_NET_WM_STATE\0".as_ptr() as *const _, 0);
        let atom_above =
            xlib::XInternAtom(display, b"_NET_WM_STATE_ABOVE\0".as_ptr() as *const _, 0);
        let atom_sticky =
            xlib::XInternAtom(display, b"_NET_WM_STATE_STICKY\0".as_ptr() as *const _, 0);
        let atom_skip_taskbar = xlib::XInternAtom(
            display,
            b"_NET_WM_STATE_SKIP_TASKBAR\0".as_ptr() as *const _,
            0,
        );
        let atom_skip_pager = xlib::XInternAtom(
            display,
            b"_NET_WM_STATE_SKIP_PAGER\0".as_ptr() as *const _,
            0,
        );
        let mut states = [atom_above, atom_sticky, atom_skip_taskbar, atom_skip_pager];
        xlib::XChangeProperty(
            display,
            window,
            atom_state,
            xlib::XA_ATOM,
            32,
            xlib::PropModeReplace,
            states.as_mut_ptr() as *const u8,
            states.len() as i32,
        );
    }

    /// Process a winit WindowEvent, forwarding to egui-winit.
    /// Returns app-level events if any.
    pub fn handle_window_event(&mut self, event: &WindowEvent) -> Vec<AppEvent> {
        let mut app_events = Vec::new();

        match event {
            WindowEvent::CloseRequested => {
                app_events.push(AppEvent::CloseRequested);
            }
            WindowEvent::Resized(size) => {
                if size.width != self.width || size.height != self.height {
                    log::info!(
                        "Window resized: {}x{} -> {}x{}",
                        self.width,
                        self.height,
                        size.width,
                        size.height
                    );
                    self.width = size.width;
                    self.height = size.height;
                }
            }
            WindowEvent::Occluded(occluded) => {
                self.mapped = !occluded;
            }
            _ => {}
        }

        // Forward all events to egui-winit for translation
        let response = self.egui_winit.on_window_event(&self.winit_window, event);
        // response.repaint tells us if egui wants a repaint, but we repaint every frame anyway
        let _ = response;

        app_events
    }

    /// Run egui UI logic (collect shapes). Call the provided closure to build UI.
    pub fn begin_frame<F: FnMut(&egui::Context)>(&mut self, ui_fn: F) {
        let raw_input = self.egui_winit.take_egui_input(&self.winit_window);
        let full_output = self.egui_ctx.run(raw_input, ui_fn);
        self.pending_output = Some(full_output);
    }

    /// Paint egui overlay and swap buffers.
    /// GL context must already be current on this window's GLX drawable.
    pub fn end_frame(&mut self, gl: &GlState) {
        let full_output = self.pending_output.take().expect("begin_frame not called");

        // Handle platform output (cursor, clipboard, etc.)
        self.egui_winit
            .handle_platform_output(&self.winit_window, full_output.platform_output);

        let clipped_primitives = self
            .egui_ctx
            .tessellate(full_output.shapes, full_output.pixels_per_point);
        self.egui_painter.paint_and_update_textures(
            [self.width, self.height],
            full_output.pixels_per_point,
            &clipped_primitives,
            &full_output.textures_delta,
        );

        unsafe {
            gl.swap_buffers(self.glx_window);
        }
    }

    /// Check if the window is currently mapped (visible).
    pub fn is_mapped(&self) -> bool {
        self.mapped
    }

    /// Request winit to redraw this window.
    pub fn request_redraw(&self) {
        self.winit_window.request_redraw();
    }

    /// Get the winit window ID for event routing.
    pub fn id(&self) -> winit::window::WindowId {
        self.winit_window.id()
    }

    /// Destroy egui resources.
    pub fn destroy(&mut self) {
        self.egui_painter.destroy();
    }
}
