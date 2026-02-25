//! X11 window with egui integration.
//!
//! Creates a normal WM-managed X11 window, translates X11 events into egui
//! input, and renders egui via egui_glow::Painter. No winit dependency.

use anyhow::{bail, Result};
use std::os::raw::{c_ulong, c_void};
use std::ptr;
use std::sync::Arc;
use std::time::Instant;
use x11::glx;
use x11::xlib;

use crate::gl_state::GlState;

/// Events the main loop cares about (beyond what egui handles internally).
pub enum AppEvent {
    CloseRequested,
}

pub struct AppWindow {
    pub display: *mut xlib::Display,
    pub window: c_ulong,
    pub glx_window: glx::GLXDrawable,
    pub width: u32,
    pub height: u32,

    // GLX context for re-binding on resize
    glx_context: glx::GLXContext,

    // egui integration
    egui_ctx: egui::Context,
    egui_painter: egui_glow::Painter,
    raw_input: egui::RawInput,
    modifiers: egui::Modifiers,
    start_time: Instant,
    pending_output: Option<egui::FullOutput>,

    // Window state
    mapped: bool,

    // Atoms
    wm_delete_window: c_ulong,
}

impl AppWindow {
    pub fn new(gl: &GlState, title: &str, x: i32, y: i32, width: u32, height: u32) -> Result<Self> {
        unsafe {
            let display = gl.display;
            xlib::XSync(display, 0);
            let screen = xlib::XDefaultScreen(display);
            let root = xlib::XRootWindow(display, screen);
            let visual = glx::glXGetVisualFromFBConfig(display, gl.output_fb_config);
            if visual.is_null() {
                bail!("No visual for output window");
            }
            let is_toggle = title == "ScreenToolToggle";

            let mut swa: xlib::XSetWindowAttributes = std::mem::zeroed();
            swa.colormap = xlib::XCreateColormap(display, root, (*visual).visual, xlib::AllocNone);
            swa.event_mask = xlib::StructureNotifyMask
                | xlib::ExposureMask
                | xlib::ButtonPressMask
                | xlib::ButtonReleaseMask
                | xlib::PointerMotionMask
                | xlib::KeyPressMask
                | xlib::KeyReleaseMask
                | xlib::FocusChangeMask
                | xlib::EnterWindowMask
                | xlib::LeaveWindowMask;
            swa.override_redirect = if is_toggle { 1 } else { 0 };
            let mut valuemask = xlib::CWColormap | xlib::CWEventMask;
            if is_toggle {
                valuemask |= xlib::CWOverrideRedirect;
            }

            let window = xlib::XCreateWindow(
                display,
                root,
                x,
                y,
                width,
                height,
                0,
                (*visual).depth,
                xlib::InputOutput as u32,
                (*visual).visual,
                valuemask,
                &mut swa,
            );
            xlib::XFree(visual as *mut c_void);
            if window == 0 {
                bail!("Failed to create window");
            }

            // Set window title (both legacy and _NET_WM_NAME)
            let title_c = std::ffi::CString::new(title).unwrap();
            xlib::XStoreName(display, window, title_c.as_ptr());
            let atom_name = xlib::XInternAtom(display, b"_NET_WM_NAME\0".as_ptr() as *const _, 0);
            let atom_utf8 = xlib::XInternAtom(display, b"UTF8_STRING\0".as_ptr() as *const _, 0);
            xlib::XChangeProperty(
                display,
                window,
                atom_name,
                atom_utf8,
                8,
                xlib::PropModeReplace,
                title.as_ptr(),
                title.len() as i32,
            );

            // Set window type (toggle helper should behave like a tiny dock so it
            // stays above fullscreen emulator surfaces).
            let atom_type =
                xlib::XInternAtom(display, b"_NET_WM_WINDOW_TYPE\0".as_ptr() as *const _, 0);
            let atom_normal = xlib::XInternAtom(
                display,
                b"_NET_WM_WINDOW_TYPE_NORMAL\0".as_ptr() as *const _,
                0,
            );
            let atom_dock = xlib::XInternAtom(
                display,
                b"_NET_WM_WINDOW_TYPE_DOCK\0".as_ptr() as *const _,
                0,
            );
            let atom_kind = if is_toggle { atom_dock } else { atom_normal };
            xlib::XChangeProperty(
                display,
                window,
                atom_type,
                xlib::XA_ATOM,
                32,
                xlib::PropModeReplace,
                &atom_kind as *const c_ulong as *const u8,
                1,
            );

            if is_toggle {
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

            // Register for WM_DELETE_WINDOW
            let wm_delete_window =
                xlib::XInternAtom(display, b"WM_DELETE_WINDOW\0".as_ptr() as *const _, 0);
            let mut protocols = [wm_delete_window];
            xlib::XSetWMProtocols(display, window, protocols.as_mut_ptr(), 1);

            let glx_window =
                glx::glXCreateWindow(display, gl.output_fb_config, window, ptr::null());
            if glx_window == 0 {
                bail!("Failed to create GLX window");
            }
            glx::glXMakeCurrent(display, glx_window, gl.glx_context);

            // Map window and wait for MapNotify
            xlib::XMapWindow(display, window);
            xlib::XFlush(display);
            let mut ev: xlib::XEvent = std::mem::zeroed();
            loop {
                xlib::XMaskEvent(display, xlib::StructureNotifyMask, &mut ev);
                if ev.type_ == xlib::MapNotify && ev.map.window == window {
                    break;
                }
            }

            let egui_ctx = egui::Context::default();
            let egui_painter = egui_glow::Painter::new(Arc::clone(&gl.glow_ctx), "", None, false)
                .map_err(|e| anyhow::anyhow!("egui Painter init failed: {}", e))?;

            let raw_input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::pos2(0.0, 0.0),
                    egui::vec2(width as f32, height as f32),
                )),
                ..Default::default()
            };

            log::info!(
                "AppWindow 0x{:x} '{}' at ({},{}) {}x{}",
                window,
                title,
                x,
                y,
                width,
                height
            );

            Ok(Self {
                display,
                window,
                glx_window,
                width,
                height,
                glx_context: gl.glx_context,
                egui_ctx,
                egui_painter,
                raw_input,
                modifiers: egui::Modifiers::NONE,
                start_time: Instant::now(),
                pending_output: None,
                mapped: true,
                wm_delete_window,
            })
        }
    }

    /// Process all pending X11 events, translate to egui input.
    /// Returns app-level events (close, etc).
    pub fn process_events(&mut self) -> Vec<AppEvent> {
        let mut app_events = Vec::new();
        unsafe {
            while xlib::XPending(self.display) > 0 {
                let mut event: xlib::XEvent = std::mem::zeroed();
                xlib::XNextEvent(self.display, &mut event);
                let etype = event.type_;

                match etype {
                    xlib::ClientMessage => {
                        let cm = event.client_message;
                        if cm.data.get_long(0) as c_ulong == self.wm_delete_window {
                            app_events.push(AppEvent::CloseRequested);
                        }
                    }
                    xlib::MapNotify => {
                        if event.map.window == self.window {
                            self.mapped = true;
                        }
                    }
                    xlib::UnmapNotify => {
                        if event.unmap.window == self.window {
                            self.mapped = false;
                        }
                    }
                    xlib::ConfigureNotify => {
                        let e = event.configure;
                        if e.window == self.window {
                            let new_w = e.width as u32;
                            let new_h = e.height as u32;
                            if new_w != self.width || new_h != self.height {
                                log::info!(
                                    "Window resized: {}x{} -> {}x{}",
                                    self.width,
                                    self.height,
                                    new_w,
                                    new_h
                                );
                                self.width = new_w;
                                self.height = new_h;
                                self.raw_input.screen_rect = Some(egui::Rect::from_min_size(
                                    egui::pos2(0.0, 0.0),
                                    egui::vec2(new_w as f32, new_h as f32),
                                ));
                                // Re-bind GLX context to force framebuffer resize on NVIDIA
                                glx::glXMakeCurrent(
                                    self.display,
                                    self.glx_window,
                                    self.glx_context,
                                );
                            }
                        }
                    }
                    xlib::ButtonPress => {
                        let e = event.button;
                        if e.window == self.window {
                            match e.button {
                                1 => self.raw_input.events.push(egui::Event::PointerButton {
                                    pos: egui::pos2(e.x as f32, e.y as f32),
                                    button: egui::PointerButton::Primary,
                                    pressed: true,
                                    modifiers: self.modifiers,
                                }),
                                2 => self.raw_input.events.push(egui::Event::PointerButton {
                                    pos: egui::pos2(e.x as f32, e.y as f32),
                                    button: egui::PointerButton::Middle,
                                    pressed: true,
                                    modifiers: self.modifiers,
                                }),
                                3 => self.raw_input.events.push(egui::Event::PointerButton {
                                    pos: egui::pos2(e.x as f32, e.y as f32),
                                    button: egui::PointerButton::Secondary,
                                    pressed: true,
                                    modifiers: self.modifiers,
                                }),
                                4 => self.raw_input.events.push(egui::Event::MouseWheel {
                                    unit: egui::MouseWheelUnit::Line,
                                    delta: egui::vec2(0.0, 1.0),
                                    modifiers: self.modifiers,
                                }),
                                5 => self.raw_input.events.push(egui::Event::MouseWheel {
                                    unit: egui::MouseWheelUnit::Line,
                                    delta: egui::vec2(0.0, -1.0),
                                    modifiers: self.modifiers,
                                }),
                                _ => {}
                            }
                        }
                    }
                    xlib::ButtonRelease => {
                        let e = event.button;
                        if e.window == self.window {
                            let button = match e.button {
                                1 => Some(egui::PointerButton::Primary),
                                2 => Some(egui::PointerButton::Middle),
                                3 => Some(egui::PointerButton::Secondary),
                                _ => None,
                            };
                            if let Some(btn) = button {
                                self.raw_input.events.push(egui::Event::PointerButton {
                                    pos: egui::pos2(e.x as f32, e.y as f32),
                                    button: btn,
                                    pressed: false,
                                    modifiers: self.modifiers,
                                });
                            }
                        }
                    }
                    xlib::MotionNotify => {
                        let e = event.motion;
                        if e.window == self.window {
                            self.raw_input
                                .events
                                .push(egui::Event::PointerMoved(egui::pos2(
                                    e.x as f32, e.y as f32,
                                )));
                        }
                    }
                    xlib::KeyPress | xlib::KeyRelease => {
                        let e = event.key;
                        let pressed = etype == xlib::KeyPress;
                        let keysym = xlib::XLookupKeysym(&mut event.key as *mut _, 0);

                        // Update modifiers
                        self.modifiers = egui::Modifiers {
                            alt: (e.state & xlib::Mod1Mask) != 0,
                            ctrl: (e.state & xlib::ControlMask) != 0,
                            shift: (e.state & xlib::ShiftMask) != 0,
                            mac_cmd: false,
                            command: (e.state & xlib::ControlMask) != 0,
                        };
                        self.raw_input.modifiers = self.modifiers;

                        // Map keysym to egui::Key
                        if let Some(key) = keysym_to_egui_key(keysym) {
                            self.raw_input.events.push(egui::Event::Key {
                                key,
                                physical_key: None,
                                pressed,
                                repeat: false,
                                modifiers: self.modifiers,
                            });
                        }

                        // Also send text input for printable characters on press
                        if pressed {
                            let mut buf = [0u8; 8];
                            let mut keysym_out: c_ulong = 0;
                            let len = xlib::XLookupString(
                                &mut event.key as *mut _,
                                buf.as_mut_ptr() as *mut _,
                                8,
                                &mut keysym_out,
                                ptr::null_mut(),
                            );
                            if len > 0 {
                                if let Ok(s) = std::str::from_utf8(&buf[..len as usize]) {
                                    let ch = s.chars().next().unwrap_or('\0');
                                    if !ch.is_control() {
                                        self.raw_input
                                            .events
                                            .push(egui::Event::Text(s.to_string()));
                                    }
                                }
                            }
                        }
                    }
                    xlib::FocusIn => {
                        self.raw_input.focused = true;
                    }
                    xlib::FocusOut => {
                        self.raw_input.focused = false;
                    }
                    xlib::EnterNotify => {
                        let e = event.crossing;
                        self.raw_input
                            .events
                            .push(egui::Event::PointerMoved(egui::pos2(
                                e.x as f32, e.y as f32,
                            )));
                    }
                    xlib::LeaveNotify => {
                        self.raw_input.events.push(egui::Event::PointerGone);
                    }
                    _ => {}
                }
            }
        }
        app_events
    }

    /// Run egui for one frame. Call the provided closure to build UI.
    /// Returns the full output (for checking wants_repaint, etc).
    /// Run egui UI logic (collect shapes). Call this first.
    pub fn begin_frame<F: FnMut(&egui::Context)>(&mut self, ui_fn: F) {
        self.raw_input.time = Some(self.start_time.elapsed().as_secs_f64());
        let full_output = self.egui_ctx.run(self.raw_input.take(), ui_fn);
        self.pending_output = Some(full_output);
    }

    /// Paint egui overlay and swap buffers. Call after rendering
    /// the background (NVFBC quad). GL context must already be current.
    pub fn end_frame(&mut self, gl: &GlState) {
        let full_output = self.pending_output.take().expect("begin_frame not called");

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
            glx::glXSwapBuffers(gl.display, self.glx_window);
        }
    }

    /// Check if the window is currently mapped (visible).
    /// Tracked via MapNotify/UnmapNotify events — no X11 round-trip.
    pub fn is_mapped(&self) -> bool {
        self.mapped
    }
}

impl Drop for AppWindow {
    fn drop(&mut self) {
        self.egui_painter.destroy();
        unsafe {
            glx::glXDestroyWindow(self.display, self.glx_window);
            xlib::XDestroyWindow(self.display, self.window);
        }
    }
}

// ─── Keysym → egui::Key mapping ─────────────────────────────────────────────

fn keysym_to_egui_key(keysym: c_ulong) -> Option<egui::Key> {
    use x11::keysym::*;
    #[allow(non_upper_case_globals)]
    match keysym as u32 {
        XK_Escape => Some(egui::Key::Escape),
        XK_Tab => Some(egui::Key::Tab),
        XK_Return | XK_KP_Enter => Some(egui::Key::Enter),
        XK_BackSpace => Some(egui::Key::Backspace),
        XK_Delete => Some(egui::Key::Delete),
        XK_Home => Some(egui::Key::Home),
        XK_End => Some(egui::Key::End),
        XK_Page_Up => Some(egui::Key::PageUp),
        XK_Page_Down => Some(egui::Key::PageDown),
        XK_Left => Some(egui::Key::ArrowLeft),
        XK_Right => Some(egui::Key::ArrowRight),
        XK_Up => Some(egui::Key::ArrowUp),
        XK_Down => Some(egui::Key::ArrowDown),
        XK_space => Some(egui::Key::Space),
        XK_F1 => Some(egui::Key::F1),
        XK_F2 => Some(egui::Key::F2),
        XK_F3 => Some(egui::Key::F3),
        XK_F4 => Some(egui::Key::F4),
        XK_F5 => Some(egui::Key::F5),
        XK_plus | XK_KP_Add => Some(egui::Key::Plus),
        XK_minus | XK_KP_Subtract => Some(egui::Key::Minus),
        XK_equal => Some(egui::Key::Equals),
        // Letters
        XK_a | XK_A => Some(egui::Key::A),
        XK_b | XK_B => Some(egui::Key::B),
        XK_c | XK_C => Some(egui::Key::C),
        XK_d | XK_D => Some(egui::Key::D),
        XK_e | XK_E => Some(egui::Key::E),
        XK_f | XK_F => Some(egui::Key::F),
        XK_g | XK_G => Some(egui::Key::G),
        XK_h | XK_H => Some(egui::Key::H),
        XK_r | XK_R => Some(egui::Key::R),
        XK_s | XK_S => Some(egui::Key::S),
        // Numbers
        XK_0 | XK_KP_0 => Some(egui::Key::Num0),
        XK_1 | XK_KP_1 => Some(egui::Key::Num1),
        XK_2 | XK_KP_2 => Some(egui::Key::Num2),
        XK_3 | XK_KP_3 => Some(egui::Key::Num3),
        XK_4 | XK_KP_4 => Some(egui::Key::Num4),
        XK_5 | XK_KP_5 => Some(egui::Key::Num5),
        XK_6 | XK_KP_6 => Some(egui::Key::Num6),
        XK_7 | XK_KP_7 => Some(egui::Key::Num7),
        XK_8 | XK_KP_8 => Some(egui::Key::Num8),
        XK_9 | XK_KP_9 => Some(egui::Key::Num9),
        _ => None,
    }
}
