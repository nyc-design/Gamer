//! screen-tool — Gamer companion app with NVFBC capture, window management, and egui GUI.
//!
//! Uses winit for cross-platform windowing, with GLX context management for
//! NVFBC zero-copy GPU capture on Linux.

mod app_window;
mod gamepad;
mod gl_state;
mod gui;
mod layout;
mod nvfbc;
mod platform;
mod stream;
mod system_stats;
mod wm;
#[allow(dead_code)]
mod window; // kept for reference during migration, will be removed

use anyhow::Result;
use clap::Parser;
use signal_hook::flag;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::raw_window_handle::{HasDisplayHandle, RawDisplayHandle};
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::window::WindowId;

use app_window::AppWindow;
use gl_state::GlState;
use gui::{OutputInfo, ScreenToolGui, WmDisplayInfo, WmWindowEntry};
use layout::LayoutManager;
use nvfbc::NvfbcCapture;
use system_stats::SystemStatsSampler;
use wm::WindowManager;

#[derive(Parser)]
#[command(
    name = "screen-tool",
    about = "Gamer companion app: zoom/crop viewer, window manager, and system monitor"
)]
struct Args {
    /// NVFBC output name to capture (e.g. "DP-0")
    #[arg(long, default_value = "DP-0")]
    capture_output: String,

    /// Window title
    #[arg(long, default_value = "ScreenTool")]
    title: String,

    /// Initial window X position
    #[arg(long, visible_alias = "output-x", default_value = "0")]
    x: i32,

    /// Initial window Y position
    #[arg(long, visible_alias = "output-y", default_value = "0")]
    y: i32,

    /// Initial window width
    #[arg(long, visible_alias = "output-width", default_value = "640")]
    width: u32,

    /// Initial window height
    #[arg(long, visible_alias = "output-height", default_value = "480")]
    height: u32,

    /// Maximum render/update rate to keep overhead bounded.
    #[arg(long, default_value = "60")]
    max_fps: u32,

    /// Minimal toggle button mode (no NVFBC capture; tiny control surface only).
    #[arg(long, default_value_t = false)]
    toggle_only: bool,

    /// Top display name for layout management
    #[arg(long, default_value = "DP-0")]
    top_display: String,

    /// Bottom display name for layout management
    #[arg(long, default_value = "DP-2")]
    bottom_display: String,
}

/// Application state managed by the winit event loop.
struct ScreenToolApp {
    args: Args,
    gl: Option<GlState>,
    main_window: Option<AppWindow>,
    capture: Option<NvfbcCapture>,
    gui: Option<ScreenToolGui>,
    system_sampler: Option<SystemStatsSampler>,

    // Window management and layout
    window_manager: Option<WindowManager>,
    layout_manager: Option<LayoutManager>,

    // NVFBC output tracking
    current_output_idx: usize,
    last_output_refresh: Instant,
    last_desired_output_refresh: Instant,
    manual_output_override_until: Instant,
    current_capture_size: (u32, u32),
    last_desired_output: Option<String>,

    // Frame pacing
    target_frame_time: Duration,
    last_frame: Instant,

    // Virtual gamepad
    virtual_gamepad: Option<gamepad::VirtualGamepad>,

    // Signals
    shutdown: Arc<AtomicBool>,
    screenshot_signal: Arc<AtomicBool>,
    toggle_signal: Arc<AtomicBool>,
}

impl ScreenToolApp {
    fn new(args: Args) -> Result<Self> {
        let shutdown = Arc::new(AtomicBool::new(false));
        let screenshot_signal = Arc::new(AtomicBool::new(false));
        let toggle_signal = Arc::new(AtomicBool::new(false));
        flag::register(signal_hook::consts::SIGTERM, Arc::clone(&shutdown))?;
        flag::register(signal_hook::consts::SIGINT, Arc::clone(&shutdown))?;
        flag::register(signal_hook::consts::SIGUSR1, Arc::clone(&screenshot_signal))?;
        flag::register(signal_hook::consts::SIGUSR2, Arc::clone(&toggle_signal))?;

        let target_frame_time =
            Duration::from_micros((1_000_000u64 / args.max_fps.max(1) as u64).max(1));

        Ok(Self {
            args,
            gl: None,
            main_window: None,
            capture: None,
            gui: None,
            system_sampler: None,
            window_manager: None,
            layout_manager: None,
            current_output_idx: 0,
            last_output_refresh: Instant::now(),
            last_desired_output_refresh: Instant::now(),
            manual_output_override_until: Instant::now(),
            current_capture_size: (0, 0),
            last_desired_output: None,
            virtual_gamepad: None,
            target_frame_time,
            last_frame: Instant::now(),
            shutdown,
            screenshot_signal,
            toggle_signal,
        })
    }

    /// Initialize GL context, NVFBC, window, GUI, and window manager.
    /// Called once when the event loop starts (in `resumed`).
    fn initialize(&mut self, event_loop: &ActiveEventLoop) -> Result<()> {
        // Set X11 error handler
        unsafe {
            x11::xlib::XSetErrorHandler(Some(gl_state::x11_error_handler));
        }

        // Get winit's X11 display to share the connection
        let raw_display = event_loop
            .display_handle()
            .map_err(|e| anyhow::anyhow!("No display handle: {}", e))?
            .as_raw();
        let x_display = match raw_display {
            RawDisplayHandle::Xlib(handle) => handle.display.unwrap().as_ptr() as *mut x11::xlib::Display,
            _ => anyhow::bail!("Expected Xlib display handle from winit"),
        };

        // Create GL context on winit's X11 display (shared connection)
        let gl = unsafe { GlState::new_with_display(x_display)? };

        // Create main window via winit with matching visual
        let main_window = AppWindow::new(
            event_loop,
            &gl,
            &self.args.title,
            self.args.x,
            self.args.y,
            self.args.width,
            self.args.height,
        )?;

        // Initialize NVFBC capture (skip in toggle-only mode)
        let capture = if self.args.toggle_only {
            None
        } else {
            unsafe {
                gl.make_current_offscreen();
            }
            Some(NvfbcCapture::new(&gl, &self.args.capture_output)?)
        };

        // Get available outputs
        let outputs = if let Some(c) = capture.as_ref() {
            c.list_outputs()
                .iter()
                .map(|(id, name, w, h)| OutputInfo {
                    id: *id,
                    name: name.clone(),
                    width: *w,
                    height: *h,
                })
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        let selected_idx = outputs
            .iter()
            .position(|o| o.name == self.args.capture_output)
            .unwrap_or(0);

        // Create GUI
        let mut gui = ScreenToolGui::new(outputs);
        gui.toggle_only = self.args.toggle_only;
        if !self.args.toggle_only && !gui.available_outputs.is_empty() {
            gui.selected_output_idx = selected_idx;
        }

        let system_sampler = if self.args.toggle_only {
            None
        } else {
            Some(SystemStatsSampler::start())
        };

        self.current_output_idx = selected_idx;
        self.current_capture_size = if !self.args.toggle_only && !gui.available_outputs.is_empty() {
            (
                gui.available_outputs[selected_idx].width,
                gui.available_outputs[selected_idx].height,
            )
        } else {
            (0, 0)
        };

        // Initialize window manager and layout manager
        let wm = WindowManager::new(x_display);
        let mut lm = LayoutManager::new();
        lm.set_display_names(&self.args.top_display, &self.args.bottom_display);

        self.gl = Some(gl);
        self.main_window = Some(main_window);
        self.capture = capture;
        self.gui = Some(gui);
        self.system_sampler = system_sampler;
        self.window_manager = Some(wm);
        self.layout_manager = Some(lm);
        self.virtual_gamepad = gamepad::VirtualGamepad::new()
            .map(|gp| { log::info!("Virtual gamepad ready"); gp })
            .or_else(|| { log::warn!("Virtual gamepad unavailable (no /dev/uinput?)"); None });

        log::info!(
            "screen-tool initialized (toggle_only={}, displays={}/{})",
            if self.args.toggle_only { "true" } else { "false" },
            self.args.top_display,
            self.args.bottom_display,
        );

        Ok(())
    }

    /// Per-frame logic: NVFBC capture, output switching, layout management, GUI render.
    fn frame_tick(&mut self) {
        let gl = match self.gl.as_ref() {
            Some(g) => g,
            None => return,
        };
        let gui = match self.gui.as_mut() {
            Some(g) => g,
            None => return,
        };
        let window = match self.main_window.as_mut() {
            Some(w) => w,
            None => return,
        };

        // Check shutdown signal
        if self.shutdown.load(Ordering::Relaxed) {
            log::info!("Shutdown signal received");
            std::process::exit(0);
        }

        // Handle SIGUSR2 toggle signal
        if self.toggle_signal.swap(false, Ordering::Relaxed) {
            if let Some(ref mut lm) = self.layout_manager {
                lm.toggle_visibility();
                log::info!("Toggle visibility: {:?}", lm.visibility_mode());
            }
        }

        // Tick layout manager (debounced, runs every 250ms internally)
        if let (Some(ref mut lm), Some(ref mut wm)) =
            (&mut self.layout_manager, &mut self.window_manager)
        {
            let layout_changed = lm.tick(wm);

            // Update capture output hint from layout manager
            // Skip if the user just manually changed the display via the GUI dropdown
            let user_changed = gui.take_output_selection_changed();
            if user_changed {
                self.manual_output_override_until =
                    Instant::now() + Duration::from_secs(30);
            }
            if layout_changed && !user_changed {
                if let Some(ref hint) = lm.capture_output_hint {
                    if Instant::now() >= self.manual_output_override_until
                        && self.last_desired_output.as_deref() != Some(hint)
                        && !gui.available_outputs.is_empty()
                    {
                        if let Some(idx) =
                            gui.available_outputs.iter().position(|o| o.name == *hint)
                        {
                            if gui.selected_output_idx != idx {
                                gui.selected_output_idx = idx;
                                self.last_desired_output = Some(hint.clone());
                            }
                        }
                    }
                }
            }

            // Update GUI's window manager data for the Windows tab
            gui.wm_displays = wm
                .displays()
                .iter()
                .map(|d| WmDisplayInfo {
                    name: d.name.clone(),
                    width: d.width,
                    height: d.height,
                    x: d.x,
                    y: d.y,
                })
                .collect();

            gui.wm_windows = wm
                .windows()
                .iter()
                .filter(|w| {
                    // Skip untitled / unmapped / tiny windows
                    if w.title.is_empty() || !w.mapped || (w.width < 100 && w.height < 100) {
                        return false;
                    }
                    // Skip our own windows and known system windows
                    let t = w.title.to_lowercase();
                    let skip_patterns = [
                        "screentool", "openbox", "sunshine", "qt selection owner",
                        "xdg_runtime", "supervisor", "pulseaudio",
                    ];
                    !skip_patterns.iter().any(|p| t.contains(p))
                })
                .map(|w| {
                    let display = determine_window_display(w, wm.displays());
                    WmWindowEntry {
                        id: w.id,
                        title: w.title.clone(),
                        display,
                        width: w.width,
                        height: w.height,
                        mapped: w.mapped,
                    }
                })
                .collect();

            // Execute pending window management actions from GUI
            for (wid, display_name, mode_str) in gui.wm_pending_actions.drain(..) {
                let mode = match mode_str.as_str() {
                    "left" => wm::LayoutMode::LeftHalf,
                    "right" => wm::LayoutMode::RightHalf,
                    _ => wm::LayoutMode::Fullscreen,
                };
                log::info!("WM action: window {} → {} ({:?})", wid, display_name, mode);
                wm.move_window_to_display(wid, &display_name, mode);
            }
        }

        // Process virtual gamepad events from GUI
        if let Some(ref mut gp) = self.virtual_gamepad {
            for (button, pressed) in gui.gamepad_pending_presses.drain(..) {
                gp.set_button(button, pressed);
            }
            for (axis, value) in gui.gamepad_pending_axes.drain(..) {
                gp.set_axis(axis, value);
            }
        } else {
            gui.gamepad_pending_presses.clear();
            gui.gamepad_pending_axes.clear();
        }

        // Dynamic window resize — match the display the screen-tool is on.
        // When a Sunshine client connects, display resolution changes via xrandr.
        // We detect this and resize our window to fill the display.
        if !self.args.toggle_only {
            if let (Some(ref lm), Some(ref wm)) =
                (&self.layout_manager, &self.window_manager)
            {
                if let Some((dx, dy, dw, dh)) = lm.screen_tool_geometry(wm) {
                    if dw > 0 && dh > 0 && (window.width != dw || window.height != dh) {
                        log::info!(
                            "Display resize: {}x{} → {}x{} at ({},{})",
                            window.width, window.height, dw, dh, dx, dy
                        );
                        let _ = window
                            .winit_window
                            .request_inner_size(PhysicalSize::new(dw, dh));
                        window
                            .winit_window
                            .set_outer_position(PhysicalPosition::new(dx, dy));
                    }
                }
            }
        }

        // Skip rendering when unmapped
        if !window.is_mapped() {
            return;
        }

        if !self.args.toggle_only {
            let cap = self
                .capture
                .as_mut()
                .expect("capture should exist in non-toggle-only mode");

            // External hint for which output to crop (file-based, legacy support)
            // Only use file-based hint when layout manager is NOT providing a hint
            let layout_has_hint = self
                .layout_manager
                .as_ref()
                .map(|lm| lm.capture_output_hint.is_some())
                .unwrap_or(false);
            if !layout_has_hint
                && Instant::now() >= self.manual_output_override_until
                && self.last_desired_output_refresh.elapsed() >= Duration::from_millis(500)
            {
                let desired_file =
                    std::env::var("SCREEN_TOOL_CAPTURE_OUTPUT_FILE").unwrap_or_else(|_| {
                        "/home/gamer/.cache/screen-tool.capture-output".to_string()
                    });
                if let Ok(raw) = std::fs::read_to_string(desired_file) {
                    let desired = raw.trim();
                    if !desired.is_empty()
                        && self.last_desired_output.as_deref() != Some(desired)
                        && !gui.available_outputs.is_empty()
                    {
                        if let Some(idx) =
                            gui.available_outputs.iter().position(|o| o.name == desired)
                        {
                            if gui.selected_output_idx != idx {
                                gui.selected_output_idx = idx;
                            }
                            self.last_desired_output = Some(desired.to_string());
                        }
                    }
                }
                self.last_desired_output_refresh = Instant::now();
            }

            // Output switching from GUI
            if gui.selected_output_idx != self.current_output_idx {
                // Set manual override BEFORE attempting the switch so the layout manager
                // cannot overwrite our selection within the same frame
                self.manual_output_override_until =
                    Instant::now() + Duration::from_secs(30);
                let new_output = &gui.available_outputs[gui.selected_output_idx];
                log::info!(
                    "Switching NVFBC to output '{}' (id={})",
                    new_output.name,
                    new_output.id
                );
                match cap.switch_output_by_id(new_output.id) {
                    Ok(()) => {
                        self.current_output_idx = gui.selected_output_idx;
                        self.current_capture_size = (new_output.width, new_output.height);
                        gui.reset_view();
                    }
                    Err(e) => {
                        log::error!(
                            "Failed to switch NVFBC output '{}': {}",
                            new_output.name,
                            e
                        );
                        gui.selected_output_idx = self.current_output_idx;
                    }
                }
            }

            // Periodically refresh output geometry
            if self.last_output_refresh.elapsed() >= Duration::from_secs(5) {
                let latest_outputs = cap
                    .list_outputs()
                    .iter()
                    .map(|(id, name, w, h)| OutputInfo {
                        id: *id,
                        name: name.clone(),
                        width: *w,
                        height: *h,
                    })
                    .collect::<Vec<_>>();
                if !latest_outputs.is_empty() {
                    let selected_id = gui.available_outputs[gui.selected_output_idx].id;
                    gui.available_outputs = latest_outputs;
                    if let Some(new_idx) = gui
                        .available_outputs
                        .iter()
                        .position(|o| o.id == selected_id)
                    {
                        gui.selected_output_idx = new_idx;
                        self.current_output_idx = new_idx;
                    } else {
                        gui.selected_output_idx = 0;
                        self.current_output_idx = 0;
                    }

                    let now_size = (
                        gui.available_outputs[self.current_output_idx].width,
                        gui.available_outputs[self.current_output_idx].height,
                    );
                    if now_size != self.current_capture_size {
                        let out = &gui.available_outputs[self.current_output_idx];
                        log::info!(
                            "Output '{}' mode changed: {}x{} -> {}x{}, recreating capture session",
                            out.name,
                            self.current_capture_size.0,
                            self.current_capture_size.1,
                            now_size.0,
                            now_size.1
                        );
                        if let Err(e) = cap.switch_output_by_id(out.id) {
                            log::warn!(
                                "Failed to refresh capture session after mode change: {}",
                                e
                            );
                        } else {
                            self.current_capture_size = now_size;
                            gui.reset_view();
                        }
                    }
                }
                self.last_output_refresh = Instant::now();
            }

            // Grab frame when crop tab is active
            if gui.wants_capture() {
                unsafe {
                    gl.make_current_offscreen();
                }
                match cap.grab_frame() {
                    Ok((tex_id, src_w, src_h, is_new)) => {
                        gui.update_capture_texture(
                            &gl.glow_ctx,
                            tex_id,
                            src_w,
                            src_h,
                            cap.screen_width,
                            cap.screen_height,
                            is_new,
                        );
                    }
                    Err(e) => log::warn!("NVFBC grab failed: {}", e),
                }
            }
        }

        // Handle screenshot signal
        if self.screenshot_signal.swap(false, Ordering::Relaxed) {
            gui.screenshot_requested = true;
        }

        // egui frame
        window.begin_frame(|ctx| {
            let stats = self
                .system_sampler
                .as_ref()
                .map(|s| s.snapshot())
                .unwrap_or_default();
            gui.update_system_stats(stats);
            let mut capture_ref = self.capture.as_mut();
            gui.show(ctx, &mut capture_ref);
        });

        // Write capture-output hint file on manual change
        if !self.args.toggle_only
            && gui.take_output_selection_changed()
            && !gui.available_outputs.is_empty()
        {
            let desired_file = std::env::var("SCREEN_TOOL_CAPTURE_OUTPUT_FILE")
                .unwrap_or_else(|_| "/home/gamer/.cache/screen-tool.capture-output".to_string());
            let manual_until_file = std::env::var("SCREEN_TOOL_CAPTURE_OUTPUT_MANUAL_UNTIL_FILE")
                .unwrap_or_else(|_| {
                    "/home/gamer/.cache/screen-tool.capture-output.manual-until".to_string()
                });
            let selected_name = gui.available_outputs[gui.selected_output_idx].name.clone();
            let _ = std::fs::write(&desired_file, format!("{selected_name}\n"));
            let _ = std::fs::write(
                &manual_until_file,
                format!(
                    "{}\n",
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs() + 8)
                        .unwrap_or(0)
                ),
            );
            self.manual_output_override_until = Instant::now() + Duration::from_secs(30);

            // Also update the layout manager's manual override
            if let Some(ref mut lm) = self.layout_manager {
                lm.set_manual_override(Duration::from_secs(30));
            }
        }

        // Render: make GL current on the app window, draw capture quad, then egui overlay
        unsafe {
            gl.make_current(window.glx_window);
        }
        gui.render_capture(&gl.glow_ctx, window.width, window.height);
        window.end_frame(gl);

        self.last_frame = Instant::now();
    }
}

/// Determine which display a window is on based on its position.
fn determine_window_display(
    win: &wm::WindowInfo,
    displays: &[wm::DisplayInfo],
) -> String {
    for disp in displays {
        let dx = (win.x - disp.x).unsigned_abs();
        let dy = (win.y - disp.y).unsigned_abs();
        if dx < disp.width && dy < disp.height {
            return disp.name.clone();
        }
    }
    "unknown".to_string()
}

impl ApplicationHandler for ScreenToolApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.gl.is_none() {
            if let Err(e) = self.initialize(event_loop) {
                log::error!("Failed to initialize: {}", e);
                event_loop.exit();
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        // Route events to the correct window
        if let Some(ref mut main_window) = self.main_window {
            if window_id == main_window.id() {
                // Handle RedrawRequested specially — this is where we render
                if matches!(event, WindowEvent::RedrawRequested) {
                    self.frame_tick();
                    return;
                }

                let events = main_window.handle_window_event(&event);
                for app_event in events {
                    match app_event {
                        app_window::AppEvent::CloseRequested => {
                            log::info!("Window close requested");
                            if let Some(ref mut w) = self.main_window {
                                if let Some(ref gl) = self.gl {
                                    if let Some(ref mut gui) = self.gui {
                                        gui.destroy(&gl.glow_ctx);
                                    }
                                    w.destroy();
                                }
                            }
                            event_loop.exit();
                        }
                    }
                }
            }
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // Frame pacing: request redraw at target FPS, sleep until next frame
        let elapsed = self.last_frame.elapsed();
        if elapsed >= self.target_frame_time {
            if let Some(ref window) = self.main_window {
                window.request_redraw();
            }
        } else {
            // Sleep until next frame instead of spinning at 100% CPU
            let remaining = self.target_frame_time - elapsed;
            event_loop.set_control_flow(ControlFlow::wait_duration(remaining));
        }
    }
}

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let args = Args::parse();

    let mut app = ScreenToolApp::new(args)?;

    let event_loop = EventLoop::new()
        .map_err(|e| anyhow::anyhow!("Failed to create event loop: {}", e))?;

    event_loop
        .run_app(&mut app)
        .map_err(|e| anyhow::anyhow!("Event loop error: {}", e))?;

    Ok(())
}
