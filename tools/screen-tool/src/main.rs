//! screen-tool — General-purpose X11 magnifier with egui GUI.
//!
//! Captures a display output via NVFBC (zero-copy GPU capture) and renders
//! a zoomed/pannable view inside an egui-powered X11 window.

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

    /// Minimal toggle button mode (no NVFBC capture; tiny control surface only).
    #[arg(long, default_value_t = false)]
    toggle_only: bool,
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

    // Create window BEFORE NVFBC init to avoid NvFBCCreateHandle/Xlib side effects.
    let mut window = AppWindow::new(&gl, &args.title, args.x, args.y, args.width, args.height)?;

    let mut capture = if args.toggle_only {
        None
    } else {
        unsafe {
            gl.make_current_offscreen();
        }
        Some(NvfbcCapture::new(&gl, &args.capture_output)?)
    };

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
        .position(|o| o.name == args.capture_output)
        .unwrap_or(0);

    // Create GUI
    let mut gui = ScreenToolGui::new(outputs);
    gui.toggle_only = args.toggle_only;
    if !args.toggle_only && !gui.available_outputs.is_empty() {
        gui.selected_output_idx = selected_idx;
    }
    let system_sampler = SystemStatsSampler::start();

    // Track which output NVFBC is currently capturing
    let mut current_output_idx = selected_idx;
    let mut last_output_refresh = Instant::now();
    let mut last_desired_output_refresh = Instant::now();
    let mut manual_output_override_until = Instant::now();
    let mut current_capture_size = if !args.toggle_only && !gui.available_outputs.is_empty() {
        (
            gui.available_outputs[current_output_idx].width,
            gui.available_outputs[current_output_idx].height,
        )
    } else {
        (0, 0)
    };
    let mut last_desired_output: Option<String> = None;

    let target_frame_time =
        Duration::from_micros((1_000_000u64 / args.max_fps.max(1) as u64).max(1));

    log::info!(
        "screen-tool running (toggle_only={}).",
        if args.toggle_only { "true" } else { "false" }
    );

    loop {
        let frame_start = Instant::now();

        if shutdown.load(Ordering::Relaxed) {
            log::info!("Shutdown signal received");
            break;
        }

        for event in window.process_events() {
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
            thread::sleep(Duration::from_millis(60));
            continue;
        }

        if !args.toggle_only {
            let cap = capture
                .as_mut()
                .expect("capture should exist in non-toggle-only mode");

            // Optional external hint for which output should be cropped.
            // Used by visibility helper so when the tool is on DP-0, crop defaults
            // to DP-2 (and vice-versa) to avoid self-referential capture.
            if Instant::now() >= manual_output_override_until
                && last_desired_output_refresh.elapsed() >= Duration::from_millis(500)
            {
                let desired_file =
                    std::env::var("SCREEN_TOOL_CAPTURE_OUTPUT_FILE").unwrap_or_else(|_| {
                        "/home/gamer/.cache/screen-tool.capture-output".to_string()
                    });
                if let Ok(raw) = std::fs::read_to_string(desired_file) {
                    let desired = raw.trim();
                    if !desired.is_empty()
                        && last_desired_output.as_deref() != Some(desired)
                        && !gui.available_outputs.is_empty()
                    {
                        if let Some(idx) =
                            gui.available_outputs.iter().position(|o| o.name == desired)
                        {
                            if gui.selected_output_idx != idx {
                                gui.selected_output_idx = idx;
                            }
                            last_desired_output = Some(desired.to_string());
                        }
                    }
                }
                last_desired_output_refresh = Instant::now();
            }

            // Output switching from GUI
            if gui.selected_output_idx != current_output_idx {
                let new_output = &gui.available_outputs[gui.selected_output_idx];
                log::info!(
                    "Switching NVFBC to output '{}' (id={})",
                    new_output.name,
                    new_output.id
                );
                match cap.switch_output_by_id(new_output.id) {
                    Ok(()) => {
                        current_output_idx = gui.selected_output_idx;
                        current_capture_size = (new_output.width, new_output.height);
                        gui.reset_view();
                        manual_output_override_until = Instant::now() + Duration::from_secs(3);
                    }
                    Err(e) => {
                        log::error!("Failed to switch NVFBC output '{}': {}", new_output.name, e);
                        gui.selected_output_idx = current_output_idx;
                    }
                }
            }

            // Periodically refresh output geometry so 1x tracks resolution changes
            if last_output_refresh.elapsed() >= Duration::from_secs(1) {
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
                        if let Err(e) = cap.switch_output_by_id(out.id) {
                            log::warn!(
                                "Failed to refresh capture session after mode change: {}",
                                e
                            );
                        } else {
                            current_capture_size = now_size;
                            gui.reset_view();
                        }
                    }
                }
                last_output_refresh = Instant::now();
            }

            // Grab frame only when crop tab is active.
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

        if screenshot_signal.swap(false, Ordering::Relaxed) {
            gui.screenshot_requested = true;
        }

        window.begin_frame(|ctx| {
            gui.update_system_stats(system_sampler.snapshot());
            let mut capture_ref = capture.as_mut();
            gui.show(ctx, &mut capture_ref);
        });

        // If user manually changed output selector, keep that choice stable briefly
        // and reflect it to the shared capture-output hint file.
        if !args.toggle_only
            && gui.take_output_selection_changed()
            && !gui.available_outputs.is_empty()
        {
            let desired_file = std::env::var("SCREEN_TOOL_CAPTURE_OUTPUT_FILE")
                .unwrap_or_else(|_| "/home/gamer/.cache/screen-tool.capture-output".to_string());
            let selected_name = gui.available_outputs[gui.selected_output_idx].name.clone();
            let _ = std::fs::write(&desired_file, format!("{selected_name}\n"));
            manual_output_override_until = Instant::now() + Duration::from_secs(3);
        }

        unsafe {
            gl.make_current(window.glx_window);
        }
        gui.render_capture(&gl.glow_ctx, window.width, window.height);
        window.end_frame(&gl);

        let elapsed = frame_start.elapsed();
        if elapsed < target_frame_time {
            thread::sleep(target_frame_time - elapsed);
        }
    }

    gui.destroy(&gl.glow_ctx);
    Ok(())
}
