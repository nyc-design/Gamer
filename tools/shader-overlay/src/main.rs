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
use std::time::Duration;
use x11::xlib;

/// Custom X11 error handler — logs but doesn't abort
unsafe extern "C" fn x11_error_handler(display: *mut xlib::Display, event: *mut xlib::XErrorEvent) -> c_int {
    let _ = display;
    let err = unsafe { &*event };
    log::warn!("X11 error: request={}, error_code={}, minor={}", err.request_code, err.error_code, err.minor_code);
    0
}

#[derive(Parser)]
#[command(name = "shader-overlay", about = "Apply RetroArch .slangp shaders to any X11 window")]
struct Args {
    /// Window spec: <window_id_or_name>:<shader.slangp>
    /// Can be specified multiple times for multiple windows.
    #[arg(long = "window", short = 'w', required = true)]
    windows: Vec<String>,

    /// Timeout in seconds to wait for windows to appear
    #[arg(long, default_value = "60")]
    timeout: u32,
}

struct PipelineEntry {
    capture: capture::WindowCapture,
    shader: shader::ShaderPipeline,
    overlay: overlay::OverlayWindow,
    shader_path: PathBuf,
}

fn parse_window_spec(spec: &str) -> Result<(String, PathBuf)> {
    // Format: <target>:<shader_path>
    // Target can contain colons (window titles), so split from the right
    // But shader paths can also contain colons (unlikely on Linux). Use last colon.
    if let Some(pos) = spec.rfind(':') {
        let target = spec[..pos].to_string();
        let shader_path = PathBuf::from(&spec[pos + 1..]);
        if target.is_empty() {
            bail!("Empty window target in spec: {}", spec);
        }
        if !shader_path.exists() {
            bail!("Shader preset not found: {:?}", shader_path);
        }
        Ok((target, shader_path))
    } else {
        bail!("Invalid window spec '{}'. Expected <window>:<shader.slangp>", spec);
    }
}

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let args = Args::parse();

    if args.windows.is_empty() {
        bail!("At least one --window spec is required");
    }

    // Parse window specs before doing anything else
    let specs: Vec<(String, PathBuf)> = args.windows.iter()
        .map(|s| parse_window_spec(s))
        .collect::<Result<Vec<_>>>()?;

    log::info!("shader-overlay starting with {} window(s)", specs.len());

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

    // Set up pipelines for each window
    let mut pipelines: Vec<PipelineEntry> = Vec::new();

    for (target, shader_path) in &specs {
        log::info!("Waiting for window '{}'...", target);
        let win_info = window::wait_for_window(gl.display, target, args.timeout)?;
        log::info!("Found window 0x{:x} '{}' at ({},{}) {}x{}", win_info.id, target, win_info.x, win_info.y, win_info.width, win_info.height);

        // Make sure GL context is current for off-screen work
        unsafe { gl.make_current_offscreen(); }

        let cap = capture::WindowCapture::new(&gl, win_info.id)?;
        let shd = shader::ShaderPipeline::new(&gl.glow_ctx, shader_path, win_info.width, win_info.height)?;
        let ovl = overlay::OverlayWindow::new(&gl, win_info.x, win_info.y, win_info.width, win_info.height)?;

        // Raise overlay above source
        ovl.raise_above(win_info.id);

        pipelines.push(PipelineEntry {
            capture: cap,
            shader: shd,
            overlay: ovl,
            shader_path: shader_path.clone(),
        });
    }

    log::info!("All pipelines ready. Entering event loop.");

    // Event loop
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
                if let Err(e) = entry.shader.reload(&gl.glow_ctx, &entry.shader_path) {
                    log::error!("Failed to reload {:?}: {}", entry.shader_path, e);
                }
            }
        }

        // Process all pending X events
        let mut got_events = false;
        unsafe {
            while xlib::XPending(gl.display) > 0 {
                xlib::XNextEvent(gl.display, &mut event);
                got_events = true;

                let event_type = event.type_;

                // Check if this is a Damage event
                let mut is_damage = false;
                for entry in &mut pipelines {
                    if event_type == entry.capture.damage_event_base() {
                        // DamageNotify — the damage ID is at a known offset in the event struct
                        // XDamageNotifyEvent: type, serial, send_event, display, drawable, damage, ...
                        let damage_event_ptr = &event as *const xlib::XEvent as *const u8;
                        // drawable is at offset 32 (after type=4, serial=8, send_event=4, display=8, drawable=8)
                        let drawable = *(damage_event_ptr.add(32) as *const c_ulong);

                        if drawable == entry.capture.source_window() {
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
                        if pipelines.iter().any(|p| p.capture.source_window() == e.window) {
                            log::info!("Source window 0x{:x} destroyed, exiting", e.window);
                            break 'main;
                        }
                    }
                    xlib::ConfigureNotify => {
                        let e = event.configure;
                        for entry in &mut pipelines {
                            if entry.capture.source_window() == e.window {
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

        // Render all dirty pipelines
        let mut any_rendered = false;
        for entry in &mut pipelines {
            if entry.capture.is_dirty() {
                // Make context current for offscreen FBO rendering
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

    log::info!("shader-overlay shutting down");
    // Pipelines drop automatically, cleaning up X resources
    drop(pipelines);
    Ok(())
}
