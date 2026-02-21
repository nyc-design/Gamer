mod capture;
mod gl_context;
mod overlay;
mod shader;
mod window;

use anyhow::{bail, Result};
use clap::Parser;
use signal_hook::flag;
use std::os::raw::{c_int, c_ulong};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use x11::xlib;

/// Custom X11 error handler — logs but doesn't abort.
/// Demotes known-benign NVIDIA GLX/Damage errors to debug level.
unsafe extern "C" fn x11_error_handler(display: *mut xlib::Display, event: *mut xlib::XErrorEvent) -> c_int {
    let _ = display;
    let err = unsafe { &*event };
    // GLX texture_from_pixmap errors (request 152/156) are benign on NVIDIA
    if (err.request_code == 152 || err.request_code == 156) && (err.error_code == 9 || err.error_code == 161) {
        log::debug!("X11 error (benign): request={}, error_code={}, minor={}", err.request_code, err.error_code, err.minor_code);
    } else {
        log::warn!("X11 error: request={}, error_code={}, minor={}", err.request_code, err.error_code, err.minor_code);
    }
    0
}

#[derive(Parser)]
#[command(name = "shader-overlay", about = "Apply RetroArch .slangp shaders to any X11 window")]
struct Args {
    /// Window spec: <window_id_or_name>:<shader.slangp>
    /// Can be specified multiple times for multiple windows.
    /// Windows that don't exist yet will be attached dynamically when they appear.
    #[arg(long = "window", short = 'w', required = true)]
    windows: Vec<String>,

    /// Timeout in seconds to wait for the FIRST window to appear.
    /// After at least one pipeline is active, other windows are polled dynamically.
    #[arg(long, default_value = "60")]
    timeout: u32,

    /// How often (in seconds) to poll for missing windows
    #[arg(long, default_value = "2")]
    poll_interval: u32,
}

/// A parsed window spec — the target name/ID and shader path
struct WindowSpec {
    target: String,
    shader_path: PathBuf,
}

/// An active pipeline: capturing a window, processing through a shader, displaying on an overlay
struct ActivePipeline {
    spec_index: usize,
    source_window: c_ulong,
    capture: capture::WindowCapture,
    shader: shader::ShaderPipeline,
    overlay: overlay::OverlayWindow,
}

fn parse_window_spec(spec: &str) -> Result<WindowSpec> {
    // Format: <target>:<shader_path>
    // Split from the right (shader paths won't have colons on Linux)
    if let Some(pos) = spec.rfind(':') {
        let target = spec[..pos].to_string();
        let shader_path = PathBuf::from(&spec[pos + 1..]);
        if target.is_empty() {
            bail!("Empty window target in spec: {}", spec);
        }
        if !shader_path.exists() {
            bail!("Shader preset not found: {:?}", shader_path);
        }
        Ok(WindowSpec { target, shader_path })
    } else {
        bail!("Invalid window spec '{}'. Expected <window>:<shader.slangp>", spec);
    }
}

/// Try to create a pipeline for a window spec. Returns None if the window doesn't exist yet.
fn try_attach_pipeline(
    gl: &gl_context::GlState,
    spec: &WindowSpec,
    spec_index: usize,
) -> Option<ActivePipeline> {
    let win_info = match window::find_window(gl.display, &spec.target) {
        Ok(info) => info,
        Err(_) => return None,
    };

    log::info!(
        "Attaching pipeline for '{}': window 0x{:x} at ({},{}) {}x{}",
        spec.target, win_info.id, win_info.x, win_info.y, win_info.width, win_info.height
    );

    unsafe { gl.make_current_offscreen(); }

    let cap = match capture::WindowCapture::new(gl, win_info.id) {
        Ok(c) => c,
        Err(e) => {
            log::error!("Failed to capture window 0x{:x}: {}", win_info.id, e);
            return None;
        }
    };

    let shd = match shader::ShaderPipeline::new(&gl.glow_ctx, &spec.shader_path, win_info.width, win_info.height) {
        Ok(s) => s,
        Err(e) => {
            log::error!("Failed to load shader {:?}: {}", spec.shader_path, e);
            return None;
        }
    };

    let ovl = match overlay::OverlayWindow::new(gl, win_info.x, win_info.y, win_info.width, win_info.height) {
        Ok(o) => o,
        Err(e) => {
            log::error!("Failed to create overlay: {}", e);
            return None;
        }
    };

    ovl.raise_above(win_info.id);

    Some(ActivePipeline {
        spec_index,
        source_window: win_info.id,
        capture: cap,
        shader: shd,
        overlay: ovl,
    })
}

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let args = Args::parse();

    if args.windows.is_empty() {
        bail!("At least one --window spec is required");
    }

    // Parse all window specs upfront
    let specs: Vec<WindowSpec> = args.windows.iter()
        .map(|s| parse_window_spec(s))
        .collect::<Result<Vec<_>>>()?;

    log::info!("shader-overlay starting with {} window spec(s)", specs.len());

    // Install X11 error handler to prevent fatal crashes on non-critical errors
    unsafe { xlib::XSetErrorHandler(Some(x11_error_handler)); }

    // Set up signal handlers
    let shutdown = Arc::new(AtomicBool::new(false));
    let reload = Arc::new(AtomicBool::new(false));
    flag::register(signal_hook::consts::SIGTERM, Arc::clone(&shutdown))?;
    flag::register(signal_hook::consts::SIGINT, Arc::clone(&shutdown))?;
    flag::register(signal_hook::consts::SIGHUP, Arc::clone(&reload))?;

    // Initialize GL
    let gl = gl_context::GlState::new()?;

    // Wait for at least one window to appear before entering the event loop
    log::info!("Waiting for at least one window to appear...");
    let deadline = Instant::now() + Duration::from_secs(args.timeout as u64);
    let mut pipelines: Vec<ActivePipeline> = Vec::new();

    while pipelines.is_empty() {
        if shutdown.load(Ordering::Relaxed) {
            log::info!("Shutdown before any window found");
            return Ok(());
        }
        if Instant::now() > deadline {
            bail!("Timed out after {}s waiting for any window", args.timeout);
        }

        for (i, spec) in specs.iter().enumerate() {
            if let Some(pipeline) = try_attach_pipeline(&gl, spec, i) {
                pipelines.push(pipeline);
            }
        }

        if pipelines.is_empty() {
            thread::sleep(Duration::from_millis(500));
        }
    }

    log::info!("{} pipeline(s) active. Entering event loop.", pipelines.len());

    // Track which spec indices have active pipelines
    let poll_interval = Duration::from_secs(args.poll_interval as u64);
    let mut last_poll = Instant::now();

    let mut frame_count: usize = 0;
    let mut event: xlib::XEvent = unsafe { std::mem::zeroed() };

    'main: loop {
        if shutdown.load(Ordering::Relaxed) {
            log::info!("Shutdown signal received");
            break;
        }

        // Hot-reload shaders on SIGHUP
        if reload.swap(false, Ordering::Relaxed) {
            log::info!("SIGHUP received, reloading shaders...");
            unsafe { gl.make_current_offscreen(); }
            for entry in &mut pipelines {
                let shader_path = &specs[entry.spec_index].shader_path;
                if let Err(e) = entry.shader.reload(&gl.glow_ctx, shader_path) {
                    log::error!("Failed to reload {:?}: {}", shader_path, e);
                }
            }
        }

        // Periodically poll for missing windows
        if last_poll.elapsed() >= poll_interval {
            last_poll = Instant::now();
            for (i, spec) in specs.iter().enumerate() {
                // Skip if this spec already has an active pipeline
                if pipelines.iter().any(|p| p.spec_index == i) {
                    continue;
                }
                if let Some(pipeline) = try_attach_pipeline(&gl, spec, i) {
                    log::info!("Dynamically attached pipeline for '{}'", spec.target);
                    pipelines.push(pipeline);
                }
            }
        }

        // Process all pending X events
        let mut got_events = false;
        let mut destroyed_windows: Vec<c_ulong> = Vec::new();

        unsafe {
            while xlib::XPending(gl.display) > 0 {
                xlib::XNextEvent(gl.display, &mut event);
                got_events = true;

                let event_type = event.type_;

                // Check if this is a Damage event
                let mut is_damage = false;
                for entry in &mut pipelines {
                    if event_type == entry.capture.damage_event_base() {
                        let damage_event_ptr = &event as *const xlib::XEvent as *const u8;
                        let drawable = *(damage_event_ptr.add(32) as *const c_ulong);

                        if drawable == entry.source_window {
                            entry.capture.mark_dirty();
                            entry.capture.acknowledge_damage();
                            is_damage = true;
                            break;
                        }
                    }
                }

                if is_damage {
                    continue;
                }

                match event_type {
                    xlib::DestroyNotify => {
                        let e = event.destroy_window;
                        if pipelines.iter().any(|p| p.source_window == e.window) {
                            log::info!("Source window 0x{:x} destroyed, tearing down pipeline", e.window);
                            destroyed_windows.push(e.window);
                        }
                    }
                    xlib::UnmapNotify => {
                        let e = event.unmap;
                        if pipelines.iter().any(|p| p.source_window == e.window) {
                            log::info!("Source window 0x{:x} unmapped, tearing down pipeline", e.window);
                            destroyed_windows.push(e.window);
                        }
                    }
                    xlib::ConfigureNotify => {
                        let e = event.configure;
                        for entry in &mut pipelines {
                            if entry.source_window == e.window {
                                let new_w = e.width as u32;
                                let new_h = e.height as u32;
                                gl.make_current_offscreen();
                                if let Err(err) = entry.capture.handle_resize(&gl, new_w, new_h) {
                                    log::error!("Resize error: {}", err);
                                }
                                if let Err(err) = entry.shader.resize_output(&gl.glow_ctx, new_w, new_h) {
                                    log::error!("Shader resize error: {}", err);
                                }
                                entry.overlay.reposition(e.x as i32, e.y as i32, new_w, new_h);
                                entry.overlay.raise_above(e.window);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        // Remove pipelines for destroyed/unmapped windows (Drop cleans up resources)
        if !destroyed_windows.is_empty() {
            for wid in &destroyed_windows {
                if let Some(pos) = pipelines.iter().position(|p| p.source_window == *wid) {
                    let removed = pipelines.remove(pos);
                    log::info!(
                        "Pipeline for '{}' (window 0x{:x}) removed. {} pipeline(s) remaining.",
                        specs[removed.spec_index].target, wid, pipelines.len()
                    );
                }
            }
            // Trigger an immediate poll so the window can re-attach quickly
            last_poll = Instant::now() - poll_interval;
        }

        // Render all dirty pipelines
        let mut any_rendered = false;
        for entry in &mut pipelines {
            if entry.capture.is_dirty() {
                unsafe { gl.make_current_offscreen(); }

                entry.capture.update_if_dirty(&gl);

                if let Err(e) = entry.shader.process(
                    &gl.glow_ctx,
                    entry.capture.texture(),
                    entry.capture.width(),
                    entry.capture.height(),
                    frame_count,
                ) {
                    log::error!("Shader process error: {}", e);
                    continue;
                }

                let out_size = entry.shader.output_size();
                entry.overlay.present(&gl, entry.shader.output_fbo(), out_size.width, out_size.height);
                any_rendered = true;
            }
        }

        if any_rendered {
            frame_count += 1;
        }

        // Sleep briefly if no events and nothing to render
        if !got_events && !any_rendered {
            thread::sleep(Duration::from_millis(1));
        }
    }

    log::info!("shader-overlay shutting down ({} pipeline(s) active)", pipelines.len());
    drop(pipelines);
    Ok(())
}
