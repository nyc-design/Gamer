//! screen-tool — General-purpose X11 magnifier with egui GUI.
//!
//! Captures a display output via NVFBC (zero-copy GPU capture) and renders
//! a zoomed/pannable view inside an egui-powered X11 window.
//!
//! Controls:
//!   Scroll wheel      — Zoom in/out (1x to 8x, continuous)
//!   Click-drag        — Pan the zoomed view
//!   Arrow keys        — Pan the zoomed view
//!   +/- keys          — Zoom in/out
//!   Escape / R        — Reset to 1x full view
//!   Tab / H           — Toggle toolbar visibility
//!   Alt+1-5           — Switch display output
//!   F5 / SIGUSR1      — Save framebuffer screenshot to /tmp/
//!   F1 / F2           — Toggle stats/help panels

mod gl_state;
mod gui;
mod nvfbc;
mod system_stats;
mod window;

use anyhow::Result;
use clap::Parser;
use signal_hook::flag;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use gl_state::GlState;
use gui::{OutputInfo, ScreenToolGui};
use nvfbc::NvfbcCapture;
use system_stats::SystemStatsSampler;
use window::{AppEvent, AppWindow};

#[derive(Parser)]
#[command(
    name = "screen-tool",
    about = "X11 magnifier with NVFBC capture and egui GUI"
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
}

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let args = Args::parse();

    // Signal handling for clean shutdown and screenshots
    let shutdown = Arc::new(AtomicBool::new(false));
    let screenshot_signal = Arc::new(AtomicBool::new(false));
    flag::register(signal_hook::consts::SIGTERM, Arc::clone(&shutdown))?;
    flag::register(signal_hook::consts::SIGINT, Arc::clone(&shutdown))?;
    flag::register(signal_hook::consts::SIGUSR1, Arc::clone(&screenshot_signal))?;

    // Set X11 error handler (non-fatal logging)
    unsafe {
        x11::xlib::XSetErrorHandler(Some(gl_state::x11_error_handler));
    }

    // Initialize OpenGL context
    let gl = GlState::new()?;

    // Create the application window BEFORE NVFBC init.
    // NVFBC's NvFBCCreateHandle can corrupt the X display connection's internal state,
    // so we must create all X windows first while the connection is clean.
    let mut window = AppWindow::new(&gl, &args.title, args.x, args.y, args.width, args.height)?;

    // Initialize NVFBC capture (after window creation)
    unsafe {
        gl.make_current_offscreen();
    }
    let mut capture = NvfbcCapture::new(&gl, &args.capture_output)?;

    // Enumerate available outputs for the GUI dropdown
    let outputs = capture
        .list_outputs()
        .iter()
        .map(|(id, name, w, h)| OutputInfo {
            id: *id,
            name: name.clone(),
            width: *w,
            height: *h,
        })
        .collect::<Vec<_>>();
    let selected_idx = outputs
        .iter()
        .position(|o| o.name == args.capture_output)
        .unwrap_or(0);

    // Create the GUI
    let mut gui = ScreenToolGui::new(outputs);
    gui.selected_output_idx = selected_idx;
    let system_sampler = SystemStatsSampler::start();

    // Track which output the NVFBC session is currently capturing
    let mut current_output_idx = selected_idx;
    let mut last_output_refresh = Instant::now();
    let mut current_capture_size = (
        gui.available_outputs[current_output_idx].width,
        gui.available_outputs[current_output_idx].height,
    );
    let target_frame_time =
        Duration::from_micros((1_000_000u64 / args.max_fps.max(1) as u64).max(1));

    log::info!(
        "screen-tool running. Tab=toolbar, scroll=zoom, drag=pan, Esc=reset, F5=screenshot."
    );

    loop {
        let frame_start = Instant::now();

        if shutdown.load(Ordering::Relaxed) {
            log::info!("Shutdown signal received");
            break;
        }

        // Process X11 events → egui input
        let app_events = window.process_events();
        for event in app_events {
            match event {
                AppEvent::CloseRequested => {
                    log::info!("Window close requested");
                    gui.destroy(&gl.glow_ctx);
                    return Ok(());
                }
            }
        }

        // If window is unmapped (hidden by external script), sleep and skip rendering
        if !window.is_mapped() {
            thread::sleep(Duration::from_millis(100));
            continue;
        }

        // Check if GUI wants to switch to a different output
        if gui.selected_output_idx != current_output_idx {
            let new_output = &gui.available_outputs[gui.selected_output_idx];
            log::info!(
                "Switching NVFBC to output '{}' (id={})",
                new_output.name,
                new_output.id
            );
            match capture.switch_output_by_id(new_output.id) {
                Ok(()) => {
                    current_output_idx = gui.selected_output_idx;
                    gui.reset_view();
                }
                Err(e) => {
                    log::error!("Failed to switch NVFBC output '{}': {}", new_output.name, e);
                    gui.selected_output_idx = current_output_idx;
                }
            }
        }

        // Periodically refresh output geometry so 1x tracks resolution changes
        // (e.g., client connects mid-session and DP-0 mode changes).
        if last_output_refresh.elapsed() >= Duration::from_secs(1) {
            let latest_outputs = capture
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
                // Preserve selected index if possible by output id.
                let selected_id = gui.available_outputs[gui.selected_output_idx].id;
                gui.available_outputs = latest_outputs;
                if let Some(new_idx) = gui
                    .available_outputs
                    .iter()
                    .position(|o| o.id == selected_id)
                {
                    gui.selected_output_idx = new_idx;
                    current_output_idx = new_idx;
                } else {
                    gui.selected_output_idx = 0;
                    current_output_idx = 0;
                }

                let now_size = (
                    gui.available_outputs[current_output_idx].width,
                    gui.available_outputs[current_output_idx].height,
                );
                if now_size != current_capture_size {
                    let out = &gui.available_outputs[current_output_idx];
                    log::info!(
                        "Output '{}' mode changed: {}x{} -> {}x{}, recreating capture session",
                        out.name,
                        current_capture_size.0,
                        current_capture_size.1,
                        now_size.0,
                        now_size.1
                    );
                    if let Err(e) = capture.switch_output_by_id(out.id) {
                        log::warn!("Failed to refresh capture session after mode change: {}", e);
                    } else {
                        current_capture_size = now_size;
                        gui.reset_view();
                    }
                }
            }
            last_output_refresh = Instant::now();
        }

        // Grab frame from NVFBC only when crop view is active.
        if gui.wants_capture() {
            unsafe {
                gl.make_current_offscreen();
            }
            let grab_result = capture.grab_frame();

            // Store the texture for raw GL rendering
            if let Ok((tex_id, src_w, src_h, is_new)) = grab_result {
                gui.update_capture_texture(
                    &gl.glow_ctx,
                    tex_id,
                    src_w,
                    src_h,
                    capture.screen_width,
                    capture.screen_height,
                    is_new,
                );
            } else if let Err(e) = grab_result {
                log::warn!("NVFBC grab failed: {}", e);
            }
        }

        // Check for SIGUSR1 screenshot signal
        if screenshot_signal.swap(false, Ordering::Relaxed) {
            gui.screenshot_requested = true;
        }

        // Render frame:
        //   1. Run egui UI logic (collect shapes, no painting yet)
        //   2. Draw NVFBC texture as fullscreen background quad (raw GL)
        //   3. Paint egui overlay on top and swap buffers
        window.begin_frame(|ctx| {
            gui.update_system_stats(system_sampler.snapshot());
            gui.show(ctx, &mut Some(&mut capture));
        });

        unsafe {
            gl.make_current(window.glx_window);
        }
        gui.render_capture(&gl.glow_ctx, window.width, window.height);

        window.end_frame(&gl);

        // Frame cap for low overhead / minimal contention with game + Sunshine.
        let elapsed = frame_start.elapsed();
        if elapsed < target_frame_time {
            thread::sleep(target_frame_time - elapsed);
        }
    }

    gui.destroy(&gl.glow_ctx);
    Ok(())
}
