//! LD_PRELOAD library that intercepts glXSwapBuffers to apply RetroArch .slangp shaders.
//!
//! Instead of using XComposite (which breaks NVFBC on NVIDIA), this injects directly
//! into the application's GL rendering pipeline. Before each buffer swap:
//! 1. Copy the back buffer to an input texture
//! 2. Run the librashader FilterChain
//! 3. Blit the shader output back to the default framebuffer
//! 4. Call the real glXSwapBuffers
//!
//! Env vars:
//!   SHADER_PRESET         - Path to .slangp preset for primary window
//!   SHADER_PRESET_BOTTOM  - Path to .slangp preset for secondary window (dual-screen)
//!   SHADER_WINDOW         - Substring to match in primary window title
//!   SHADER_WINDOW_BOTTOM  - Substring to match in secondary window title

use glow::HasContext;
use std::collections::HashMap;
use std::ffi::{CStr, CString};
use std::os::raw::{c_int, c_void};
use std::path::PathBuf;
use std::sync::{Arc, Once};
use x11::glx;
use x11::xlib;

use librashader::presets::ShaderFeatures;
use librashader::runtime::gl::{FilterChain, FilterChainOptions, FrameOptions, GLImage};
use librashader::runtime::{Size, Viewport};

// Real glXSwapBuffers function pointer, resolved via dlsym
type GlXSwapBuffersFn = unsafe extern "C" fn(*mut xlib::Display, glx::GLXDrawable);
static mut REAL_SWAP: Option<GlXSwapBuffersFn> = None;

// Per-drawable shader pipeline state
struct DrawablePipeline {
    filter_chain: FilterChain,
    input_texture: glow::Texture,
    output_texture: glow::Texture,
    output_fbo: glow::Framebuffer,
    capture_fbo: glow::Framebuffer,
    width: u32,
    height: u32,
    frame_count: usize,
    glow_ctx: Arc<glow::Context>,
}

// Global state
static INIT: Once = Once::new();
static mut STATE: Option<GlobalState> = None;

struct GlobalState {
    pipelines: HashMap<glx::GLXDrawable, DrawablePipeline>,
    // Window matching config
    primary_preset: Option<PathBuf>,
    primary_match: Option<String>,
    secondary_preset: Option<PathBuf>,
    secondary_match: Option<String>,
    // Drawables we checked but didn't match yet — re-check every N frames
    // Value is the frame count when we last checked
    pending: HashMap<glx::GLXDrawable, usize>,
    global_frame: usize,
}

extern "C" {
    fn dlsym(handle: *mut c_void, symbol: *const i8) -> *mut c_void;
}
const RTLD_NEXT: *mut c_void = -1isize as *mut c_void;

/// Initialize global state from env vars
fn init_global() {
    INIT.call_once(|| {
        // Initialize logging
        let _ = env_logger::Builder::from_env(
            env_logger::Env::default().default_filter_or("info")
        ).try_init();

        // Resolve real glXSwapBuffers
        unsafe {
            let name = CString::new("glXSwapBuffers").unwrap();
            let ptr = dlsym(RTLD_NEXT, name.as_ptr());
            if ptr.is_null() {
                log::error!("[shader-inject] Failed to resolve real glXSwapBuffers!");
                return;
            }
            REAL_SWAP = Some(std::mem::transmute(ptr));
        }

        let primary_preset = std::env::var("SHADER_PRESET").ok().map(PathBuf::from);
        let primary_match = std::env::var("SHADER_WINDOW").ok();
        let secondary_preset = std::env::var("SHADER_PRESET_BOTTOM").ok().map(PathBuf::from);
        let secondary_match = std::env::var("SHADER_WINDOW_BOTTOM").ok();

        log::info!("[shader-inject] Initialized");
        if let Some(ref p) = primary_preset {
            log::info!("[shader-inject] Primary shader: {:?} (match: {:?})", p, primary_match);
        }
        if let Some(ref p) = secondary_preset {
            log::info!("[shader-inject] Secondary shader: {:?} (match: {:?})", p, secondary_match);
        }

        if primary_preset.is_none() {
            log::info!("[shader-inject] No SHADER_PRESET set, passthrough mode");
        }

        unsafe {
            STATE = Some(GlobalState {
                pipelines: HashMap::new(),
                primary_preset,
                primary_match,
                secondary_preset,
                secondary_match,
                pending: HashMap::new(),
                global_frame: 0,
            });
        }
    });
}

/// Read _NET_WM_NAME or WM_NAME for a single window
fn read_window_name(display: *mut xlib::Display, window: xlib::Window) -> Option<String> {
    unsafe {
        let atom_net_wm_name = xlib::XInternAtom(
            display,
            b"_NET_WM_NAME\0".as_ptr() as *const i8,
            1,
        );
        let atom_utf8 = xlib::XInternAtom(
            display,
            b"UTF8_STRING\0".as_ptr() as *const i8,
            1,
        );

        if atom_net_wm_name != 0 && atom_utf8 != 0 {
            let mut actual_type: xlib::Atom = 0;
            let mut actual_format: c_int = 0;
            let mut nitems: u64 = 0;
            let mut bytes_after: u64 = 0;
            let mut prop: *mut u8 = std::ptr::null_mut();

            let status = xlib::XGetWindowProperty(
                display, window, atom_net_wm_name,
                0, 1024, 0, atom_utf8,
                &mut actual_type, &mut actual_format, &mut nitems, &mut bytes_after, &mut prop,
            );

            if status == 0 && !prop.is_null() && nitems > 0 {
                let slice = std::slice::from_raw_parts(prop, nitems as usize);
                let title = String::from_utf8_lossy(slice).to_string();
                xlib::XFree(prop as *mut c_void);
                return Some(title);
            }
            if !prop.is_null() {
                xlib::XFree(prop as *mut c_void);
            }
        }

        let mut name_ptr: *mut i8 = std::ptr::null_mut();
        if xlib::XFetchName(display, window, &mut name_ptr) != 0 && !name_ptr.is_null() {
            let title = CStr::from_ptr(name_ptr).to_string_lossy().to_string();
            xlib::XFree(name_ptr as *mut c_void);
            return Some(title);
        }

        None
    }
}

/// Get the window title for a drawable.
/// The drawable may be a child widget (e.g., GLX rendering area) whose title
/// differs from the top-level window. Walk up through parents to find the
/// real application window title.
fn get_window_title(display: *mut xlib::Display, drawable: glx::GLXDrawable) -> Option<String> {
    unsafe {
        let mut window = drawable as xlib::Window;
        let root = xlib::XDefaultRootWindow(display);

        // Walk up from the drawable to find a window with a meaningful title
        // (top-level windows have the game name; child widgets may just say "Azahar")
        for _ in 0..10 {
            if let Some(title) = read_window_name(display, window) {
                // Check if this looks like the real title (contains game name or "Secondary")
                // If it's just a generic name like "Azahar", keep walking up
                if title.len() > 10 || window == root {
                    return Some(title);
                }
            }

            // Walk up to parent
            let mut parent: xlib::Window = 0;
            let mut root_ret: xlib::Window = 0;
            let mut children: *mut xlib::Window = std::ptr::null_mut();
            let mut nchildren: u32 = 0;
            if xlib::XQueryTree(display, window, &mut root_ret, &mut parent, &mut children, &mut nchildren) == 0 {
                break;
            }
            if !children.is_null() {
                xlib::XFree(children as *mut c_void);
            }
            if parent == 0 || parent == root_ret {
                // Reached the root — return whatever title we have for the original window
                return read_window_name(display, drawable as xlib::Window);
            }
            window = parent;
        }

        read_window_name(display, drawable as xlib::Window)
    }
}

/// Get the window dimensions
fn get_window_size(display: *mut xlib::Display, drawable: glx::GLXDrawable) -> (u32, u32) {
    unsafe {
        let mut attrs: xlib::XWindowAttributes = std::mem::zeroed();
        if xlib::XGetWindowAttributes(display, drawable as xlib::Window, &mut attrs) != 0 {
            (attrs.width as u32, attrs.height as u32)
        } else {
            (1920, 1080) // fallback
        }
    }
}

/// Create a pipeline for a drawable if it matches a configured shader
fn try_create_pipeline(
    display: *mut xlib::Display,
    drawable: glx::GLXDrawable,
    state: &GlobalState,
) -> Option<DrawablePipeline> {
    let title = get_window_title(display, drawable)?;
    log::info!("[shader-inject] Window title for drawable 0x{:x}: '{}'", drawable, title);

    // Match against configured window patterns
    let preset_path = if let Some(ref pattern) = state.primary_match {
        if title.contains(pattern.as_str()) && !title.contains("Secondary") {
            state.primary_preset.as_ref()
        } else if let Some(ref sec_pattern) = state.secondary_match {
            if title.contains(sec_pattern.as_str()) {
                state.secondary_preset.as_ref()
            } else {
                None
            }
        } else {
            None
        }
    } else {
        // No pattern specified — apply primary shader to all windows
        state.primary_preset.as_ref()
    };

    let preset_path = preset_path?;

    if !preset_path.exists() {
        log::error!("[shader-inject] Shader preset not found: {:?}", preset_path);
        return None;
    }

    let (width, height) = get_window_size(display, drawable);
    log::info!(
        "[shader-inject] Creating pipeline for '{}' (0x{:x}) {}x{} with {:?}",
        title, drawable, width, height, preset_path
    );

    // Create glow context from current GL context's function pointers
    let glow_ctx = Arc::new(unsafe {
        glow::Context::from_loader_function_cstr(|name: &CStr| {
            let ptr = glx::glXGetProcAddress(name.as_ptr() as *const u8);
            match ptr {
                Some(f) => f as *const c_void,
                None => std::ptr::null(),
            }
        })
    });

    // Create input texture (to copy the back buffer into)
    let input_texture = unsafe {
        let tex = glow_ctx.create_texture().ok()?;
        glow_ctx.bind_texture(glow::TEXTURE_2D, Some(tex));
        glow_ctx.tex_storage_2d(glow::TEXTURE_2D, 1, glow::RGBA8, width as i32, height as i32);
        glow_ctx.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
        glow_ctx.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
        glow_ctx.bind_texture(glow::TEXTURE_2D, None);
        tex
    };

    // Create capture FBO (to read back buffer via blit)
    let capture_fbo = unsafe {
        let fbo = glow_ctx.create_framebuffer().ok()?;
        glow_ctx.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
        glow_ctx.framebuffer_texture_2d(
            glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0,
            glow::TEXTURE_2D, Some(input_texture), 0
        );
        let status = glow_ctx.check_framebuffer_status(glow::FRAMEBUFFER);
        if status != glow::FRAMEBUFFER_COMPLETE {
            log::error!("[shader-inject] Capture FBO incomplete: 0x{:x}", status);
            glow_ctx.delete_framebuffer(fbo);
            glow_ctx.delete_texture(input_texture);
            return None;
        }
        glow_ctx.bind_framebuffer(glow::FRAMEBUFFER, None);
        fbo
    };

    // Create output texture + FBO for shader output
    let (output_texture, output_fbo) = unsafe {
        let tex = glow_ctx.create_texture().ok()?;
        glow_ctx.bind_texture(glow::TEXTURE_2D, Some(tex));
        glow_ctx.tex_storage_2d(glow::TEXTURE_2D, 1, glow::RGBA8, width as i32, height as i32);
        glow_ctx.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
        glow_ctx.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
        glow_ctx.bind_texture(glow::TEXTURE_2D, None);

        let fbo = glow_ctx.create_framebuffer().ok()?;
        glow_ctx.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
        glow_ctx.framebuffer_texture_2d(
            glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0,
            glow::TEXTURE_2D, Some(tex), 0
        );
        let status = glow_ctx.check_framebuffer_status(glow::FRAMEBUFFER);
        if status != glow::FRAMEBUFFER_COMPLETE {
            log::error!("[shader-inject] Output FBO incomplete: 0x{:x}", status);
            glow_ctx.delete_framebuffer(fbo);
            glow_ctx.delete_texture(tex);
            glow_ctx.delete_framebuffer(capture_fbo);
            glow_ctx.delete_texture(input_texture);
            return None;
        }
        glow_ctx.bind_framebuffer(glow::FRAMEBUFFER, None);
        (tex, fbo)
    };

    // Load the shader filter chain
    let options = FilterChainOptions {
        glsl_version: 330,
        use_dsa: false,
        force_no_mipmaps: false,
        disable_cache: false,
    };

    let filter_chain = unsafe {
        match FilterChain::load_from_path(preset_path, ShaderFeatures::NONE, Arc::clone(&glow_ctx), Some(&options)) {
            Ok(fc) => fc,
            Err(e) => {
                log::error!("[shader-inject] Failed to load shader {:?}: {:?}", preset_path, e);
                glow_ctx.delete_framebuffer(output_fbo);
                glow_ctx.delete_texture(output_texture);
                glow_ctx.delete_framebuffer(capture_fbo);
                glow_ctx.delete_texture(input_texture);
                return None;
            }
        }
    };

    log::info!("[shader-inject] Pipeline created for drawable 0x{:x}", drawable);

    Some(DrawablePipeline {
        filter_chain,
        input_texture,
        output_texture,
        output_fbo,
        capture_fbo,
        width,
        height,
        frame_count: 0,
        glow_ctx,
    })
}

/// Apply shader to the current back buffer before swapping
fn apply_shader(pipeline: &mut DrawablePipeline) {
    unsafe {
        let gl = &pipeline.glow_ctx;

        // Save current viewport
        let mut saved_viewport = [0i32; 4];
        gl.get_parameter_i32_slice(glow::VIEWPORT, &mut saved_viewport);

        // Step 1: Copy back buffer (default FBO) to input texture via blit
        gl.bind_framebuffer(glow::READ_FRAMEBUFFER, None); // read from back buffer
        gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, Some(pipeline.capture_fbo)); // write to input texture
        gl.blit_framebuffer(
            0, 0, pipeline.width as i32, pipeline.height as i32,
            0, 0, pipeline.width as i32, pipeline.height as i32,
            glow::COLOR_BUFFER_BIT,
            glow::NEAREST,
        );

        // Step 2: Run shader chain: input_texture -> output_texture
        let input = GLImage {
            handle: Some(pipeline.input_texture),
            format: glow::RGBA8,
            size: Size { width: pipeline.width, height: pipeline.height },
        };

        let output = GLImage {
            handle: Some(pipeline.output_texture),
            format: glow::RGBA8,
            size: Size { width: pipeline.width, height: pipeline.height },
        };

        let viewport = Viewport {
            x: 0.0,
            y: 0.0,
            mvp: None,
            output: &output,
            size: Size { width: pipeline.width, height: pipeline.height },
        };

        if let Err(e) = pipeline.filter_chain.frame(
            &input, &viewport, pipeline.frame_count, Some(&FrameOptions::default())
        ) {
            log::error!("[shader-inject] Shader frame error: {:?}", e);
            // Restore state and return without modifying the buffer
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            gl.viewport(saved_viewport[0], saved_viewport[1], saved_viewport[2], saved_viewport[3]);
            return;
        }

        // Step 3: Blit shader output back to back buffer (default FBO)
        gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(pipeline.output_fbo)); // read from shader output
        gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, None); // write to back buffer
        gl.blit_framebuffer(
            0, 0, pipeline.width as i32, pipeline.height as i32,
            0, 0, pipeline.width as i32, pipeline.height as i32,
            glow::COLOR_BUFFER_BIT,
            glow::NEAREST,
        );

        // Restore state
        gl.bind_framebuffer(glow::FRAMEBUFFER, None);
        gl.viewport(saved_viewport[0], saved_viewport[1], saved_viewport[2], saved_viewport[3]);

        pipeline.frame_count += 1;
    }
}

/// The intercepted glXSwapBuffers — called by the application
#[no_mangle]
pub unsafe extern "C" fn glXSwapBuffers(display: *mut xlib::Display, drawable: glx::GLXDrawable) {
    init_global();

    let state = match STATE.as_mut() {
        Some(s) => s,
        None => {
            // Fallback: call real swap if state init failed
            if let Some(real) = REAL_SWAP {
                real(display, drawable);
            }
            return;
        }
    };

    state.global_frame += 1;

    // No shader configured — passthrough
    if state.primary_preset.is_none() && state.secondary_preset.is_none() {
        if let Some(real) = REAL_SWAP {
            real(display, drawable);
        }
        return;
    }

    // Check if we already have a pipeline for this drawable
    if !state.pipelines.contains_key(&drawable) {
        // Re-check pending drawables every 60 frames (~1 second)
        // Window titles change after game loads (e.g., "Azahar" -> "Azahar | Pokémon Alpha Sapphire")
        let should_check = match state.pending.get(&drawable) {
            None => true,
            Some(last_check) => state.global_frame - last_check >= 60,
        };

        if should_check {
            if let Some(pipeline) = try_create_pipeline(display, drawable, state) {
                state.pipelines.insert(drawable, pipeline);
                state.pending.remove(&drawable);
            } else {
                state.pending.insert(drawable, state.global_frame);
            }
        }
    }

    // Apply shader if we have a pipeline
    if let Some(pipeline) = state.pipelines.get_mut(&drawable) {
        // Check for window resize
        let (w, h) = get_window_size(display, drawable);
        if w != pipeline.width || h != pipeline.height {
            log::info!("[shader-inject] Window resized to {}x{}, recreating pipeline", w, h);
            state.pipelines.remove(&drawable);
            state.pending.remove(&drawable);
            // Will be recreated on next frame
        } else {
            apply_shader(pipeline);
        }
    }

    // Call the real glXSwapBuffers
    if let Some(real) = REAL_SWAP {
        real(display, drawable);
    }
}
