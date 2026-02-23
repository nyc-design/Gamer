//! egui UI for the screen-tool magnifier.
//!
//! The captured display is rendered as a fullscreen quad using raw OpenGL
//! (bypassing egui's texture pipeline), then egui renders the toolbar
//! overlay on top.

use glow::HasContext;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::nvfbc::NvfbcCapture;
use crate::system_stats::SystemStatsSnapshot;

/// Save the current GL framebuffer as a PPM image file.
/// PPM is a simple uncompressed format that needs no extra dependencies.
fn save_framebuffer_ppm(gl: &glow::Context, width: u32, height: u32, path: &str) {
    unsafe {
        let size = (width * height * 3) as usize;
        let mut pixels = vec![0u8; size];
        gl.read_pixels(
            0,
            0,
            width as i32,
            height as i32,
            glow::RGB,
            glow::UNSIGNED_BYTE,
            glow::PixelPackData::Slice(Some(&mut pixels)),
        );
        // GL reads bottom-to-top, flip vertically
        let row_bytes = (width * 3) as usize;
        for y in 0..height as usize / 2 {
            let top = y * row_bytes;
            let bot = (height as usize - 1 - y) * row_bytes;
            for x in 0..row_bytes {
                pixels.swap(top + x, bot + x);
            }
        }
        // Write PPM
        let header = format!("P6\n{} {}\n255\n", width, height);
        if let Ok(mut f) = std::fs::File::create(path) {
            use std::io::Write;
            let _ = f.write_all(header.as_bytes());
            let _ = f.write_all(&pixels);
            log::info!("Screenshot saved: {} ({}x{})", path, width, height);
        } else {
            log::warn!("Failed to create screenshot file: {}", path);
        }
    }
}

/// Info about an available NVFBC output.
#[derive(Clone)]
pub struct OutputInfo {
    pub id: u32,
    pub name: String,
    pub width: u32,
    pub height: u32,
}

fn discover_shader_presets() -> Vec<String> {
    fn walk(dir: &std::path::Path, out: &mut Vec<String>) {
        let entries = match std::fs::read_dir(dir) {
            Ok(v) => v,
            Err(_) => return,
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, out);
                continue;
            }
            if p.extension()
                .and_then(|s| s.to_str())
                .map(|s| s.eq_ignore_ascii_case("slangp"))
                .unwrap_or(false)
            {
                out.push(p.to_string_lossy().to_string());
            }
        }
    }

    let mut presets = Vec::new();
    for root in ["/gamer/shaders", "/gamer"] {
        walk(std::path::Path::new(root), &mut presets);
    }
    presets.sort();
    presets.dedup();
    presets
}

fn initial_shader_root() -> String {
    for root in ["/gamer/shaders", "/gamer"] {
        if std::path::Path::new(root).exists() {
            return root.to_string();
        }
    }
    "/gamer".to_string()
}

fn list_shader_entries(dir: &str) -> (Vec<String>, Vec<String>) {
    let mut dirs = Vec::new();
    let mut files = Vec::new();
    let entries = match std::fs::read_dir(dir) {
        Ok(v) => v,
        Err(_) => return (dirs, files),
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            dirs.push(p.to_string_lossy().to_string());
        } else if p
            .extension()
            .and_then(|s| s.to_str())
            .map(|s| s.eq_ignore_ascii_case("slangp"))
            .unwrap_or(false)
        {
            files.push(p.to_string_lossy().to_string());
        }
    }
    dirs.sort();
    files.sort();
    (dirs, files)
}

fn write_shader_file(target: &str, preset_path: &str) -> anyhow::Result<()> {
    let file = match target {
        "primary" => std::env::var("SHADER_PRESET_FILE")
            .unwrap_or_else(|_| "/tmp/shader_preset_primary.path".to_string()),
        "secondary" => std::env::var("SHADER_PRESET_BOTTOM_FILE")
            .unwrap_or_else(|_| "/tmp/shader_preset_secondary.path".to_string()),
        _ => return Ok(()),
    };
    std::fs::write(&file, format!("{preset_path}\n"))?;
    Ok(())
}

fn toggle_tool_mode_file() -> anyhow::Result<String> {
    let mode_file = std::env::var("SCREEN_TOOL_MODE_FILE")
        .unwrap_or_else(|_| "/home/gamer/.cache/screen-tool.mode".into());
    let current = std::fs::read_to_string(&mode_file)
        .unwrap_or_else(|_| "auto".into())
        .trim()
        .to_string();
    let next = if current == "force_show" {
        "force_hide"
    } else {
        "force_show"
    };
    std::fs::write(&mode_file, format!("{next}\n"))?;
    Ok(next.to_string())
}

/// Raw GL resources for rendering a textured quad.
struct QuadRenderer {
    program: glow::NativeProgram,
    vao: glow::NativeVertexArray,
    vbo: glow::NativeBuffer,
    u_uv_offset: glow::UniformLocation,
    u_uv_scale: glow::UniformLocation,
}

impl QuadRenderer {
    fn new(gl: &glow::Context) -> Self {
        unsafe {
            let vs_src = r#"#version 330 core
layout(location = 0) in vec2 a_pos;
layout(location = 1) in vec2 a_uv;
out vec2 v_uv;
uniform vec2 u_uv_offset;
uniform vec2 u_uv_scale;
void main() {
    gl_Position = vec4(a_pos, 0.0, 1.0);
    v_uv = u_uv_offset + a_uv * u_uv_scale;
}
"#;
            let fs_src = r#"#version 330 core
in vec2 v_uv;
out vec4 color;
uniform sampler2D u_texture;
void main() {
    // NVFBC ToGL with BGRA source can appear with swapped R/B on some stacks.
    // Swizzle back here so gameplay colors remain correct.
    vec4 c = texture(u_texture, v_uv);
    color = vec4(c.b, c.g, c.r, c.a);
}
"#;

            let vs = gl.create_shader(glow::VERTEX_SHADER).unwrap();
            gl.shader_source(vs, vs_src);
            gl.compile_shader(vs);
            if !gl.get_shader_compile_status(vs) {
                panic!("VS: {}", gl.get_shader_info_log(vs));
            }

            let fs = gl.create_shader(glow::FRAGMENT_SHADER).unwrap();
            gl.shader_source(fs, fs_src);
            gl.compile_shader(fs);
            if !gl.get_shader_compile_status(fs) {
                panic!("FS: {}", gl.get_shader_info_log(fs));
            }

            let program = gl.create_program().unwrap();
            gl.attach_shader(program, vs);
            gl.attach_shader(program, fs);
            gl.link_program(program);
            if !gl.get_program_link_status(program) {
                panic!("Link: {}", gl.get_program_info_log(program));
            }
            gl.delete_shader(vs);
            gl.delete_shader(fs);

            let u_uv_offset = gl.get_uniform_location(program, "u_uv_offset").unwrap();
            let u_uv_scale = gl.get_uniform_location(program, "u_uv_scale").unwrap();

            // Fullscreen quad: two triangles covering [-1,1] in clip space
            // Positions (x,y) and UVs (u,v) interleaved
            #[rustfmt::skip]
            let vertices: [f32; 24] = [
                // pos        uv
                -1.0, -1.0,   0.0, 1.0,  // bottom-left  (uv flipped for GL)
                 1.0, -1.0,   1.0, 1.0,  // bottom-right
                 1.0,  1.0,   1.0, 0.0,  // top-right
                -1.0, -1.0,   0.0, 1.0,  // bottom-left
                 1.0,  1.0,   1.0, 0.0,  // top-right
                -1.0,  1.0,   0.0, 0.0,  // top-left
            ];

            let vao = gl.create_vertex_array().unwrap();
            gl.bind_vertex_array(Some(vao));

            let vbo = gl.create_buffer().unwrap();
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
            let byte_slice = std::slice::from_raw_parts(
                vertices.as_ptr() as *const u8,
                vertices.len() * std::mem::size_of::<f32>(),
            );
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, byte_slice, glow::STATIC_DRAW);

            // a_pos: location 0, 2 floats, stride 16 bytes, offset 0
            gl.enable_vertex_attrib_array(0);
            gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, 16, 0);
            // a_uv: location 1, 2 floats, stride 16 bytes, offset 8
            gl.enable_vertex_attrib_array(1);
            gl.vertex_attrib_pointer_f32(1, 2, glow::FLOAT, false, 16, 8);

            gl.bind_vertex_array(None);

            Self {
                program,
                vao,
                vbo,
                u_uv_offset,
                u_uv_scale,
            }
        }
    }

    /// Draw a textured quad with the given NVFBC texture and UV parameters.
    fn draw(&self, gl: &glow::Context, texture_id: u32, uv_offset: [f32; 2], uv_scale: [f32; 2]) {
        unsafe {
            let tex = glow::NativeTexture(std::num::NonZeroU32::new_unchecked(texture_id));

            gl.use_program(Some(self.program));
            gl.uniform_2_f32(Some(&self.u_uv_offset), uv_offset[0], uv_offset[1]);
            gl.uniform_2_f32(Some(&self.u_uv_scale), uv_scale[0], uv_scale[1]);

            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(tex));
            // Ensure proper sampling for NVFBC texture
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MIN_FILTER,
                glow::LINEAR as i32,
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MAG_FILTER,
                glow::LINEAR as i32,
            );

            gl.bind_vertex_array(Some(self.vao));
            gl.draw_arrays(glow::TRIANGLES, 0, 6);
            gl.bind_vertex_array(None);

            gl.bind_texture(glow::TEXTURE_2D, None);
            gl.use_program(None);
        }
    }

    fn destroy(&self, gl: &glow::Context) {
        unsafe {
            gl.delete_program(self.program);
            gl.delete_vertex_array(self.vao);
            gl.delete_buffer(self.vbo);
        }
    }
}

pub struct ScreenToolGui {
    pub toggle_only: bool,
    // Main feature tabs
    active_tab: ToolTab,

    // Source selection
    pub available_outputs: Vec<OutputInfo>,
    pub selected_output_idx: usize,

    // Viewport
    pub zoom_level: f32,
    pub pan_center: (f32, f32), // normalized 0..1

    // Raw GL quad renderer (bypasses egui for texture rendering)
    quad_renderer: Option<QuadRenderer>,
    current_nvfbc_tex_id: u32,
    capture_size: (u32, u32),
    texture_uv_scale: (f32, f32), // accounts for NVFBC allocating larger textures

    // UI state
    pub show_toolbar: bool,
    pub screenshot_requested: bool,
    pub show_stats_panel: bool,
    pub show_help_panel: bool,

    // FPS tracking
    frame_count: u32,
    fps: f32,
    last_fps_time: std::time::Instant,

    // Capture freshness tracking
    new_frame_count: u32,
    capture_fps: f32,
    last_capture_fps_time: std::time::Instant,
    last_capture_diag_time: std::time::Instant,
    last_capture_diag_tex_id: u32,
    last_capture_diag_size: (u32, u32),

    // Saved zoom/pan slots for quick recall (minimap-style workflows)
    saved_slots: [Option<((f32, f32), f32)>; 3],

    // One-time style init for a cleaner, modern overlay look
    style_initialized: bool,
    applied_style_scale: f32,
    pub ui_scale_user: f32,

    // machine stats (sampled in background thread)
    system_stats: SystemStatsSnapshot,

    // shader preset control
    shader_presets: Vec<String>,
    shader_root_dir: String,
    shader_current_dir: String,
    shader_selected: Option<String>,
    shader_status: Option<String>,
    shader_status_until: Option<Instant>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ToolTab {
    Crop,
    Performance,
    Shaders,
}

impl ScreenToolGui {
    pub fn new(outputs: Vec<OutputInfo>) -> Self {
        let default_ui_scale = std::env::var("SCREEN_TOOL_UI_SCALE")
            .ok()
            .and_then(|v| v.parse::<f32>().ok())
            .unwrap_or(1.0)
            .clamp(0.8, 3.0);
        Self {
            toggle_only: false,
            active_tab: ToolTab::Performance,
            available_outputs: outputs,
            selected_output_idx: 0,
            zoom_level: 1.0,
            pan_center: (0.5, 0.5),
            quad_renderer: None,
            current_nvfbc_tex_id: 0,
            capture_size: (0, 0),
            texture_uv_scale: (1.0, 1.0),
            show_toolbar: true,
            screenshot_requested: false,
            show_stats_panel: true,
            show_help_panel: false,
            frame_count: 0,
            fps: 0.0,
            last_fps_time: std::time::Instant::now(),
            new_frame_count: 0,
            capture_fps: 0.0,
            last_capture_fps_time: std::time::Instant::now(),
            last_capture_diag_time: std::time::Instant::now(),
            last_capture_diag_tex_id: 0,
            last_capture_diag_size: (0, 0),
            saved_slots: [None, None, None],
            style_initialized: false,
            applied_style_scale: 0.0,
            ui_scale_user: default_ui_scale,
            system_stats: SystemStatsSnapshot::default(),
            shader_presets: discover_shader_presets(),
            shader_root_dir: initial_shader_root(),
            shader_current_dir: initial_shader_root(),
            shader_selected: None,
            shader_status: None,
            shader_status_until: None,
        }
    }

    pub fn wants_capture(&self) -> bool {
        !self.toggle_only && self.active_tab == ToolTab::Crop
    }

    pub fn update_system_stats(&mut self, stats: SystemStatsSnapshot) {
        self.system_stats = stats;
    }

    fn ensure_style(&mut self, ctx: &egui::Context, ui_scale: f32) {
        if self.style_initialized && (self.applied_style_scale - ui_scale).abs() < 0.05 {
            return;
        }
        self.style_initialized = true;
        self.applied_style_scale = ui_scale;

        let mut style = (*ctx.style()).clone();
        style.spacing.item_spacing = egui::vec2(8.0 * ui_scale, 6.0 * ui_scale);
        style.spacing.button_padding = egui::vec2(12.0 * ui_scale, 8.0 * ui_scale);
        style.text_styles.insert(
            egui::TextStyle::Body,
            egui::FontId::new(15.0 * ui_scale, egui::FontFamily::Proportional),
        );
        style.text_styles.insert(
            egui::TextStyle::Button,
            egui::FontId::new(15.0 * ui_scale, egui::FontFamily::Proportional),
        );
        style.text_styles.insert(
            egui::TextStyle::Small,
            egui::FontId::new(13.0 * ui_scale, egui::FontFamily::Proportional),
        );
        ctx.set_style(style);

        let mut visuals = egui::Visuals::dark();
        visuals.panel_fill = egui::Color32::from_rgba_premultiplied(18, 20, 28, 215);
        visuals.window_fill = egui::Color32::from_rgba_premultiplied(18, 20, 28, 225);
        visuals.widgets.noninteractive.bg_fill =
            egui::Color32::from_rgba_premultiplied(35, 38, 52, 210);
        visuals.widgets.inactive.bg_fill = egui::Color32::from_rgba_premultiplied(46, 50, 70, 200);
        visuals.widgets.hovered.bg_fill = egui::Color32::from_rgba_premultiplied(65, 74, 110, 220);
        visuals.widgets.active.bg_fill = egui::Color32::from_rgba_premultiplied(77, 94, 142, 240);
        ctx.set_visuals(visuals);
    }

    pub fn reset_view(&mut self) {
        self.zoom_level = 1.0;
        self.pan_center = (0.5, 0.5);
    }

    fn clamp_pan_center(&mut self) {
        self.pan_center.0 = self.pan_center.0.clamp(0.0, 1.0);
        self.pan_center.1 = self.pan_center.1.clamp(0.0, 1.0);
    }

    fn set_view(&mut self, center: (f32, f32), zoom: f32) {
        self.pan_center = center;
        self.zoom_level = zoom.clamp(1.0, 8.0);
        self.clamp_pan_center();
    }

    fn apply_quick_preset(&mut self, name: &str) {
        match name {
            "Full" => self.reset_view(),
            "TL" => self.set_view((0.25, 0.25), 3.0),
            "TR" => self.set_view((0.75, 0.25), 3.0),
            "BL" => self.set_view((0.25, 0.75), 3.0),
            "BR" => self.set_view((0.75, 0.75), 3.0),
            "Center" => self.set_view((0.5, 0.5), 3.0),
            _ => {}
        }
    }

    /// Store the NVFBC texture ID. With output tracking, NVFBC returns a texture
    /// sized exactly to the tracked output (not the full screen), so UV scale is always (1,1).
    pub fn update_capture_texture(
        &mut self,
        _gl: &Arc<glow::Context>,
        gl_texture_id: u32,
        width: u32,
        height: u32,
        screen_width: u32,
        screen_height: u32,
        is_new_frame: bool,
    ) {
        let prev_tex = self.current_nvfbc_tex_id;
        self.current_nvfbc_tex_id = gl_texture_id;
        self.capture_size = (width, height);
        self.texture_uv_scale = (1.0, 1.0);
        if is_new_frame {
            self.new_frame_count += 1;
        }
        let capture_elapsed = self.last_capture_fps_time.elapsed().as_secs_f32();
        if capture_elapsed >= 1.0 {
            self.capture_fps = self.new_frame_count as f32 / capture_elapsed;
            self.new_frame_count = 0;
            self.last_capture_fps_time = std::time::Instant::now();
        }
        // Avoid per-frame log spam (textures alternate in the ring buffer).
        // Emit diagnostics only when dimensions change or every ~10 seconds.
        let dims_changed = self.last_capture_diag_size != (width, height);
        let tex_changed = self.last_capture_diag_tex_id != gl_texture_id && prev_tex == 0;
        let periodic = self.last_capture_diag_time.elapsed().as_secs() >= 10;
        if dims_changed || tex_changed || periodic {
            log::debug!(
                "NVFBC tex {}: capture={}x{}, screen={}x{}, uv_scale=(1.0, 1.0), new_frame={}",
                gl_texture_id,
                width,
                height,
                screen_width,
                screen_height,
                is_new_frame
            );
            self.last_capture_diag_time = std::time::Instant::now();
            self.last_capture_diag_tex_id = gl_texture_id;
            self.last_capture_diag_size = (width, height);
        }
    }

    /// Compute UV offset and scale for current zoom/pan state.
    /// Accounts for NVFBC textures being larger than the capture area.
    fn uv_params(&self) -> ([f32; 2], [f32; 2]) {
        let (tex_su, tex_sv) = self.texture_uv_scale;
        if self.zoom_level <= 1.0 {
            // No zoom — map to just the captured region within the texture
            ([0.0, 0.0], [tex_su, tex_sv])
        } else {
            let view_w = tex_su / self.zoom_level;
            let view_h = tex_sv / self.zoom_level;
            let cx = self.pan_center.0 * tex_su;
            let cy = self.pan_center.1 * tex_sv;
            let cx = cx.clamp(view_w / 2.0, tex_su - view_w / 2.0);
            let cy = cy.clamp(view_h / 2.0, tex_sv - view_h / 2.0);
            ([cx - view_w / 2.0, cy - view_h / 2.0], [view_w, view_h])
        }
    }

    /// Render the NVFBC texture as a fullscreen quad using raw GL.
    /// Call this BEFORE egui rendering so the toolbar overlays on top.
    pub fn render_capture(&mut self, gl: &Arc<glow::Context>, viewport_w: u32, viewport_h: u32) {
        if self.toggle_only {
            unsafe {
                gl.viewport(0, 0, viewport_w as i32, viewport_h as i32);
                gl.disable(glow::SCISSOR_TEST);
                gl.disable(glow::BLEND);
                gl.clear_color(0.07, 0.09, 0.13, 0.95);
                gl.clear(glow::COLOR_BUFFER_BIT);
            }
            return;
        }

        if self.active_tab != ToolTab::Crop {
            unsafe {
                gl.viewport(0, 0, viewport_w as i32, viewport_h as i32);
                gl.disable(glow::SCISSOR_TEST);
                gl.disable(glow::BLEND);
                gl.clear_color(0.04, 0.045, 0.06, 1.0);
                gl.clear(glow::COLOR_BUFFER_BIT);
            }
            return;
        }

        if self.current_nvfbc_tex_id == 0 {
            return;
        }

        if self.quad_renderer.is_none() {
            self.quad_renderer = Some(QuadRenderer::new(gl));
        }

        let (uv_offset, uv_scale) = self.uv_params();

        // "1x" semantics: show the full captured output, preserving aspect ratio.
        // We draw into a fitted viewport (letter/pillar-box if needed) instead of
        // stretching or cropping to the app window dimensions.
        let (src_w, src_h) = self.capture_size;
        let (draw_x, draw_y, draw_w, draw_h) = if src_w > 0 && src_h > 0 {
            let src_aspect = src_w as f32 / src_h as f32;
            let dst_aspect = viewport_w as f32 / viewport_h.max(1) as f32;
            if dst_aspect > src_aspect {
                // Window wider than source => pillarbox
                let h = viewport_h as i32;
                let w = (h as f32 * src_aspect).round() as i32;
                ((viewport_w as i32 - w) / 2, 0, w.max(1), h.max(1))
            } else {
                // Window taller than source => letterbox
                let w = viewport_w as i32;
                let h = (w as f32 / src_aspect).round() as i32;
                (0, (viewport_h as i32 - h) / 2, w.max(1), h.max(1))
            }
        } else {
            (0, 0, viewport_w as i32, viewport_h as i32)
        };

        unsafe {
            gl.viewport(0, 0, viewport_w as i32, viewport_h as i32);
            gl.disable(glow::SCISSOR_TEST);
            gl.disable(glow::BLEND);
            gl.clear_color(0.0, 0.0, 0.0, 1.0);
            gl.clear(glow::COLOR_BUFFER_BIT);
            gl.viewport(draw_x, draw_y, draw_w, draw_h);
        }

        self.quad_renderer.as_ref().unwrap().draw(
            gl,
            self.current_nvfbc_tex_id,
            uv_offset,
            uv_scale,
        );

        // Save screenshot after quad is drawn (before egui overlay)
        if self.screenshot_requested {
            self.screenshot_requested = false;
            save_framebuffer_ppm(
                gl,
                viewport_w,
                viewport_h,
                "/tmp/screen-tool-screenshot.ppm",
            );
        }
    }

    /// Main UI function — called inside window.frame() for the egui overlay.
    /// Only renders the toolbar and handles input — the texture is rendered separately.
    pub fn show(&mut self, ctx: &egui::Context, _capture: &mut Option<&mut NvfbcCapture>) {
        let rect = ctx.screen_rect();
        let auto_scale = ((rect.width() * rect.height()) / (1920.0 * 1080.0))
            .sqrt()
            .clamp(0.95, 3.2);
        let ui_scale = (auto_scale * self.ui_scale_user).clamp(0.85, 4.0);
        self.ensure_style(ctx, ui_scale);

        if self.toggle_only {
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE)
                .show(ctx, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.add_space((ui.available_height() * 0.2).max(2.0));
                        let side = ui.available_width().min(ui.available_height()).max(24.0);
                        if ui
                            .add_sized(
                                [side, side],
                                egui::Button::new(egui::RichText::new("◎").size(0.62 * side))
                                    .fill(egui::Color32::from_rgb(48, 67, 120)),
                            )
                            .clicked()
                        {
                            let _ = toggle_tool_mode_file();
                        }
                    });
                });
            return;
        }

        // Track FPS
        self.frame_count += 1;
        let elapsed = self.last_fps_time.elapsed().as_secs_f32();
        if elapsed >= 1.0 {
            self.fps = self.frame_count as f32 / elapsed;
            self.frame_count = 0;
            self.last_fps_time = std::time::Instant::now();
        }

        // Handle keyboard shortcuts
        ctx.input(|i| {
            if i.key_pressed(egui::Key::Tab) || i.key_pressed(egui::Key::H) {
                self.show_toolbar = !self.show_toolbar;
            }
            if i.key_pressed(egui::Key::F1) {
                self.show_stats_panel = !self.show_stats_panel;
            }
            if i.key_pressed(egui::Key::F2) {
                self.show_help_panel = !self.show_help_panel;
            }
            if i.key_pressed(egui::Key::Escape) || i.key_pressed(egui::Key::R) {
                self.reset_view();
            }
            if i.key_pressed(egui::Key::Plus) || i.key_pressed(egui::Key::Equals) {
                self.zoom_level = (self.zoom_level * 1.5).min(8.0);
            }
            if i.key_pressed(egui::Key::Minus) {
                self.zoom_level = (self.zoom_level / 1.5).max(1.0);
            }
            if i.key_pressed(egui::Key::F5) {
                self.screenshot_requested = true;
            }
            if i.key_pressed(egui::Key::F8) {
                match toggle_tool_mode_file() {
                    Ok(mode) => {
                        self.shader_status = Some(format!("Overlay mode: {}", mode));
                        self.shader_status_until = Some(Instant::now() + Duration::from_secs(3));
                    }
                    Err(e) => {
                        self.shader_status = Some(format!("Toggle failed: {}", e));
                        self.shader_status_until = Some(Instant::now() + Duration::from_secs(3));
                    }
                }
            }
            // Arrow key panning
            let pan_step = 0.05 / self.zoom_level;
            if i.key_pressed(egui::Key::ArrowLeft) {
                self.pan_center.0 -= pan_step;
            }
            if i.key_pressed(egui::Key::ArrowRight) {
                self.pan_center.0 += pan_step;
            }
            if i.key_pressed(egui::Key::ArrowUp) {
                self.pan_center.1 -= pan_step;
            }
            if i.key_pressed(egui::Key::ArrowDown) {
                self.pan_center.1 += pan_step;
            }
            // Alt+Number keys to select output (reserve plain 1..3 for crop slots)
            if i.modifiers.alt {
                for (idx, key) in [
                    egui::Key::Num1,
                    egui::Key::Num2,
                    egui::Key::Num3,
                    egui::Key::Num4,
                    egui::Key::Num5,
                ]
                .iter()
                .enumerate()
                {
                    if i.key_pressed(*key) && idx < self.available_outputs.len() {
                        self.selected_output_idx = idx;
                    }
                }
            }

            // Ctrl+1/2/3 save current crop slot. Plain 1/2/3 loads slot if it exists.
            if i.modifiers.ctrl {
                if i.key_pressed(egui::Key::Num1) {
                    self.saved_slots[0] = Some((self.pan_center, self.zoom_level));
                }
                if i.key_pressed(egui::Key::Num2) {
                    self.saved_slots[1] = Some((self.pan_center, self.zoom_level));
                }
                if i.key_pressed(egui::Key::Num3) {
                    self.saved_slots[2] = Some((self.pan_center, self.zoom_level));
                }
            } else {
                if i.key_pressed(egui::Key::Num1) {
                    if let Some((center, zoom)) = self.saved_slots[0] {
                        self.set_view(center, zoom);
                    }
                }
                if i.key_pressed(egui::Key::Num2) {
                    if let Some((center, zoom)) = self.saved_slots[1] {
                        self.set_view(center, zoom);
                    }
                }
                if i.key_pressed(egui::Key::Num3) {
                    if let Some((center, zoom)) = self.saved_slots[2] {
                        self.set_view(center, zoom);
                    }
                }
            }
        });
        self.clamp_pan_center();

        // Scale controls continuously with available screen real estate.
        let scale =
            ((rect.width().min(rect.height()) / 620.0).clamp(1.0, 3.8)) * self.ui_scale_user;

        // Toolbar
        if self.show_toolbar {
            egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Display:");
                    let current_name = self
                        .available_outputs
                        .get(self.selected_output_idx)
                        .map(|o| o.name.as_str())
                        .unwrap_or("None");
                    egui::ComboBox::from_id_salt("display_selector")
                        .selected_text(current_name)
                        .show_ui(ui, |ui| {
                            for (idx, output) in self.available_outputs.iter().enumerate() {
                                let label =
                                    format!("{} ({}x{})", output.name, output.width, output.height);
                                ui.selectable_value(&mut self.selected_output_idx, idx, label);
                            }
                        });

                    ui.separator();

                    ui.label("Zoom:");
                    ui.add(
                        egui::Slider::new(&mut self.zoom_level, 1.0..=8.0)
                            .logarithmic(true)
                            .text("x"),
                    );

                    if ui.button("Reset").clicked() {
                        self.reset_view();
                    }

                    ui.separator();

                    ui.label("Quick:");
                    for label in ["Full", "TL", "TR", "BL", "BR", "Center"] {
                        if ui.small_button(label).clicked() {
                            self.apply_quick_preset(label);
                        }
                    }

                    ui.separator();

                    ui.label("Slots:");
                    for idx in 0..3 {
                        let slot_text = format!("S{}", idx + 1);
                        if ui.small_button(slot_text).clicked() {
                            if let Some((center, zoom)) = self.saved_slots[idx] {
                                self.set_view(center, zoom);
                            }
                        }
                        if ui.small_button(format!("Save {}", idx + 1)).clicked() {
                            self.saved_slots[idx] = Some((self.pan_center, self.zoom_level));
                        }
                    }

                    ui.separator();
                    ui.checkbox(&mut self.show_stats_panel, "Stats");
                    ui.checkbox(&mut self.show_help_panel, "Help");
                    ui.separator();
                    ui.label("UI Scale");
                    ui.add(
                        egui::Slider::new(&mut self.ui_scale_user, 0.8..=3.0)
                            .logarithmic(true)
                            .show_value(true),
                    );
                });
            });
        }

        if self.show_stats_panel && self.active_tab == ToolTab::Crop {
            egui::Window::new("Performance")
                .default_pos(egui::pos2(10.0, 56.0))
                .resizable(false)
                .collapsible(true)
                .show(ctx, |ui| {
                    ui.label(format!("Render FPS: {:.0}", self.fps));
                    ui.label(format!("Capture FPS (new): {:.0}", self.capture_fps));
                    if self.capture_size.0 > 0 {
                        ui.label(format!(
                            "Capture: {}x{}",
                            self.capture_size.0, self.capture_size.1
                        ));
                    }
                    ui.label(format!("Zoom: {:.2}x", self.zoom_level));
                    ui.label(format!(
                        "Pan: {:.3}, {:.3}",
                        self.pan_center.0, self.pan_center.1
                    ));
                });
        }

        if self.show_help_panel && self.active_tab == ToolTab::Crop {
            egui::Window::new("Controls")
                .default_pos(egui::pos2(280.0, 56.0))
                .resizable(false)
                .collapsible(true)
                .show(ctx, |ui| {
                    ui.label("Scroll: Zoom");
                    ui.label("Drag / Arrows: Pan");
                    ui.label("Tab/H: Toggle toolbar");
                    ui.label("F1: Toggle stats");
                    ui.label("F2: Toggle help");
                    ui.label("Alt+1..5: Switch display output");
                    ui.label("Ctrl+1..3: Save crop slots");
                    ui.label("1..3: Recall crop slots");
                    ui.label("F5: Save screenshot");
                    ui.label("Ctrl+Shift+F8 or Ctrl+Shift+K: Toggle tool overlay");
                });
        }

        if self.active_tab == ToolTab::Performance {
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE)
                .show(ctx, |ui| {
                    let cpu = self.system_stats.cpu_usage_pct.clamp(0.0, 100.0);
                    let ram = if self.system_stats.ram_total_gib > 0.0 {
                        (self.system_stats.ram_used_gib / self.system_stats.ram_total_gib * 100.0)
                            .clamp(0.0, 100.0)
                    } else {
                        0.0
                    };
                    let gpu = self
                        .system_stats
                        .gpu_usage_pct
                        .unwrap_or(0.0)
                        .clamp(0.0, 100.0);

                    let bottom_reserved = 110.0 * scale;
                    let panel_w = (ui.available_width() - 20.0).max(380.0);
                    let panel_h = (ui.available_height() - bottom_reserved).max(240.0);
                    ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
                        ui.add_space(6.0 * scale);
                        egui::Frame::window(ui.style())
                            .inner_margin(egui::Margin::same((16.0 * scale) as i8))
                            .show(ui, |ui| {
                                ui.set_min_width(panel_w);
                                ui.set_max_width(panel_w);
                                egui::ScrollArea::vertical()
                                    .id_salt("perf_scroll")
                                    .max_height(panel_h)
                                    .show(ui, |ui| {
                                        ui.vertical_centered(|ui| {
                                            ui.heading(
                                                egui::RichText::new("Performance Monitor")
                                                    .size(28.0 * scale),
                                            );
                                        });
                                        ui.add_space(8.0 * scale);

                                        let bar_h = 16.0 * scale;
                                        let name_size = 16.0 * scale;
                                        let value_size = 20.0 * scale;

                                        let draw_metric = |ui: &mut egui::Ui,
                                                       name: &str,
                                                       pct: f32,
                                                       text: String,
                                                       color: egui::Color32| {
                                        ui.horizontal(|ui| {
                                            ui.label(egui::RichText::new(name).size(name_size));
                                            ui.add_space(6.0 * scale);
                                            let bar = egui::ProgressBar::new(pct / 100.0)
                                                .desired_width(panel_w * 0.50)
                                                .fill(color)
                                                .show_percentage();
                                            ui.add_sized([panel_w * 0.50, bar_h], bar);
                                            ui.add_space(6.0 * scale);
                                            ui.label(
                                                egui::RichText::new(text)
                                                    .size(value_size)
                                                    .strong(),
                                            );
                                        });
                                    };

                                        draw_metric(
                                            ui,
                                            "CPU",
                                            cpu,
                                            format!("{:.1}%", cpu),
                                            egui::Color32::from_rgb(242, 135, 68),
                                        );
                                        draw_metric(
                                            ui,
                                            "RAM",
                                            ram,
                                            format!(
                                                "{:.1}/{:.1} GiB",
                                                self.system_stats.ram_used_gib,
                                                self.system_stats.ram_total_gib
                                            ),
                                            egui::Color32::from_rgb(108, 176, 255),
                                        );
                                        draw_metric(
                                            ui,
                                            "GPU",
                                            gpu,
                                            self.system_stats
                                                .gpu_usage_pct
                                                .map(|v| format!("{:.0}%", v))
                                                .unwrap_or_else(|| "N/A".to_string()),
                                            egui::Color32::from_rgb(109, 212, 128),
                                        );

                                        ui.separator();
                                        egui::Grid::new("perf_grid_bottom")
                                            .num_columns(2)
                                            .spacing([18.0 * scale, 8.0 * scale])
                                            .show(ui, |ui| {
                                                ui.label(
                                                    egui::RichText::new("Top-screen FPS")
                                                        .size(name_size),
                                                );
                                                ui.label(
                                                    egui::RichText::new(format!(
                                                        "{:.0}",
                                                        self.capture_fps
                                                    ))
                                                    .size(value_size)
                                                    .strong(),
                                                );
                                                ui.end_row();
                                                ui.label(
                                                    egui::RichText::new("Render FPS")
                                                        .size(name_size),
                                                );
                                                ui.label(
                                                    egui::RichText::new(format!("{:.0}", self.fps))
                                                        .size(value_size)
                                                        .strong(),
                                                );
                                                ui.end_row();
                                                ui.label(
                                                    egui::RichText::new("GPU memory")
                                                        .size(name_size),
                                                );
                                                let gpu_mem_text = match (
                                                    self.system_stats.gpu_mem_used_mib,
                                                    self.system_stats.gpu_mem_total_mib,
                                                ) {
                                                    (Some(used), Some(total)) => {
                                                        format!("{} / {} MiB", used, total)
                                                    }
                                                    _ => "N/A".to_string(),
                                                };
                                                ui.label(
                                                    egui::RichText::new(gpu_mem_text)
                                                        .size(value_size)
                                                        .strong(),
                                                );
                                                ui.end_row();
                                                ui.label(
                                                    egui::RichText::new("Capture size")
                                                        .size(name_size),
                                                );
                                                ui.label(
                                                    egui::RichText::new(format!(
                                                        "{} x {}",
                                                        self.capture_size.0, self.capture_size.1
                                                    ))
                                                    .size(value_size)
                                                    .strong(),
                                                );
                                                ui.end_row();
                                            });
                                    });
                            });
                    });
                });
        }

        if self.active_tab == ToolTab::Shaders {
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE)
                .show(ctx, |ui| {
                    let bottom_reserved = 110.0 * scale;
                    let panel_w = (ui.available_width() - 20.0).max(540.0);
                    let panel_h = (ui.available_height() - bottom_reserved).max(320.0);
                    ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
                        ui.add_space(6.0 * scale);
                        egui::Frame::window(ui.style())
                            .inner_margin(egui::Margin::same((14.0 * scale) as i8))
                            .show(ui, |ui| {
                                ui.set_min_width(panel_w);
                                ui.set_max_width(panel_w);
                                egui::ScrollArea::vertical()
                                    .id_salt("shader_outer_scroll")
                                    .max_height(panel_h)
                                    .show(ui, |ui| {
                                    ui.vertical_centered(|ui| {
                                        ui.heading(
                                            egui::RichText::new("Shader Presets")
                                                .size(30.0 * scale),
                                        );
                                        ui.label("Choose a preset and apply to Top / Bottom / Both");
                                    });
                                    ui.add_space(8.0 * scale);

                                    let current_display = if self.shader_current_dir.len() > 80 {
                                        format!("…{}", &self.shader_current_dir[self.shader_current_dir.len() - 80..])
                                    } else {
                                        self.shader_current_dir.clone()
                                    };
                                    ui.horizontal(|ui| {
                                        ui.label("Current:");
                                        ui.monospace(current_display);
                                        if ui.button("↑").clicked() {
                                            if let Some(parent) =
                                                std::path::Path::new(&self.shader_current_dir).parent()
                                            {
                                                let parent = parent.to_string_lossy().to_string();
                                                if parent.starts_with(&self.shader_root_dir) {
                                                    self.shader_current_dir = parent;
                                                }
                                            }
                                        }
                                        if ui.button("Refresh").clicked() {
                                            self.shader_presets = discover_shader_presets();
                                        }
                                    });

                                    ui.separator();
                                    ui.columns(2, |cols| {
                                        cols[0].heading("Folders");
                                        cols[1].heading("Presets");

                                        let (dirs, files) = list_shader_entries(&self.shader_current_dir);
                                        let list_h = (panel_h * 0.55).max(160.0);
                                        egui::ScrollArea::vertical()
                                            .id_salt("shader_dirs_scroll")
                                            .max_height(list_h)
                                            .show(&mut cols[0], |ui| {
                                                for d in dirs {
                                                    let label = format!(
                                                        "📁 {}",
                                                        std::path::Path::new(&d)
                                                            .file_name()
                                                            .and_then(|s| s.to_str())
                                                            .unwrap_or(&d)
                                                    );
                                                    if ui.button(label).clicked() {
                                                        self.shader_current_dir = d;
                                                        self.shader_selected = None;
                                                    }
                                                }
                                            });
                                        egui::ScrollArea::vertical()
                                            .id_salt("shader_files_scroll")
                                            .max_height(list_h)
                                            .show(&mut cols[1], |ui| {
                                                for file in files {
                                                    let selected = self
                                                        .shader_selected
                                                        .as_ref()
                                                        .map(|s| s == &file)
                                                        .unwrap_or(false);
                                                    let label = std::path::Path::new(&file)
                                                        .file_name()
                                                        .and_then(|s| s.to_str())
                                                        .unwrap_or(&file)
                                                        .to_string();
                                                    if ui.selectable_label(selected, label).clicked() {
                                                        self.shader_selected = Some(file);
                                                    }
                                                }
                                            });
                                    });

                                    ui.separator();
                                    if let Some(selected) = self.shader_selected.clone() {
                                        ui.horizontal_wrapped(|ui| {
                                            ui.label("Selected:");
                                            ui.monospace(&selected);
                                        });
                                        ui.add_space(6.0 * scale);
                                        ui.horizontal(|ui| {
                                            let apply = |target: &str,
                                                         path: &str,
                                                         status: &mut Option<String>,
                                                         until: &mut Option<Instant>| {
                                                match write_shader_file(target, path) {
                                                    Ok(()) => {
                                                        *status = Some(format!("{} shader → {}", target, path));
                                                        *until = Some(Instant::now() + Duration::from_secs(4));
                                                    }
                                                    Err(e) => {
                                                        *status = Some(format!("Failed ({}): {}", target, e));
                                                        *until = Some(Instant::now() + Duration::from_secs(4));
                                                    }
                                                }
                                            };
                                            if ui.button("Apply to Top").clicked() {
                                                apply(
                                                    "primary",
                                                    &selected,
                                                    &mut self.shader_status,
                                                    &mut self.shader_status_until,
                                                );
                                            }
                                            if ui.button("Apply to Bottom").clicked() {
                                                apply(
                                                    "secondary",
                                                    &selected,
                                                    &mut self.shader_status,
                                                    &mut self.shader_status_until,
                                                );
                                            }
                                            if ui.button("Apply to Both").clicked() {
                                                match write_shader_file("primary", &selected) {
                                                    Ok(()) => {
                                                        let _ = write_shader_file("secondary", &selected);
                                                        self.shader_status = Some(format!("Both shaders → {}", selected));
                                                        self.shader_status_until =
                                                            Some(Instant::now() + Duration::from_secs(4));
                                                    }
                                                    Err(e) => {
                                                        self.shader_status = Some(format!("Failed (both): {}", e));
                                                        self.shader_status_until =
                                                            Some(Instant::now() + Duration::from_secs(4));
                                                    }
                                                }
                                            }
                                            if ui.button("Clear Shaders").clicked() {
                                                let clear_primary = write_shader_file("primary", "");
                                                let clear_secondary = write_shader_file("secondary", "");
                                                if clear_primary.is_ok() && clear_secondary.is_ok() {
                                                    self.shader_status = Some("Cleared shaders (top+bottom)".to_string());
                                                } else {
                                                    self.shader_status = Some("Failed to clear one or more shader targets".to_string());
                                                }
                                                self.shader_status_until =
                                                    Some(Instant::now() + Duration::from_secs(4));
                                            }
                                        });
                                    } else {
                                        ui.label("Select a shader preset file to enable apply buttons.");
                                        if ui.button("Clear Shaders").clicked() {
                                            let clear_primary = write_shader_file("primary", "");
                                            let clear_secondary = write_shader_file("secondary", "");
                                            if clear_primary.is_ok() && clear_secondary.is_ok() {
                                                self.shader_status = Some("Cleared shaders (top+bottom)".to_string());
                                            } else {
                                                self.shader_status = Some("Failed to clear one or more shader targets".to_string());
                                            }
                                            self.shader_status_until =
                                                Some(Instant::now() + Duration::from_secs(4));
                                        }
                                    }

                                    if let Some(until) = self.shader_status_until {
                                        if Instant::now() > until {
                                            self.shader_status = None;
                                            self.shader_status_until = None;
                                        }
                                    }
                                    if let Some(status) = &self.shader_status {
                                        ui.add_space(8.0 * scale);
                                        ui.label(status);
                                    }
                                    });
                            });
                    });
                });
        }

        // Crop-only interaction layer.
        if self.active_tab == ToolTab::Crop {
            // Transparent central panel — only for capturing scroll/drag input
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE)
                .show(ctx, |ui| {
                    let available_size = ui.available_size();
                    let rect = egui::Rect::from_min_size(ui.cursor().min, available_size);
                    let response = ui.allocate_rect(rect, egui::Sense::click_and_drag());

                    // Scroll wheel zoom
                    if response.hovered() {
                        let scroll = ui.input(|i| i.raw_scroll_delta.y);
                        if scroll != 0.0 {
                            let factor = if scroll > 0.0 { 1.2 } else { 1.0 / 1.2 };
                            self.zoom_level = (self.zoom_level * factor).clamp(1.0, 8.0);
                        }
                    }

                    // Drag to pan
                    if response.dragged_by(egui::PointerButton::Primary) {
                        let delta = ui.input(|i| i.pointer.delta());
                        if available_size.x > 1.0 && available_size.y > 1.0 {
                            let scale = 1.0 / self.zoom_level;
                            self.pan_center.0 -= delta.x / available_size.x * scale;
                            self.pan_center.1 -= delta.y / available_size.y * scale;
                        }
                    }
                });
        }

        // Always-visible, touch-friendly zoom controls for high-res displays.
        if self.active_tab == ToolTab::Crop {
            let btn_w = 60.0 * scale;
            let btn_h = 48.0 * scale;
            let reset_h = 34.0 * scale;
            let pad = 24.0;
            egui::Area::new(egui::Id::new("quick_zoom_controls"))
                .anchor(egui::Align2::RIGHT_CENTER, egui::vec2(-pad, 0.0))
                .order(egui::Order::Foreground)
                .interactable(true)
                .show(ctx, |ui| {
                    egui::Frame::window(ui.style())
                        .inner_margin(egui::Margin::same((8.0 * scale) as i8))
                        .show(ui, |ui| {
                            ui.vertical_centered(|ui| {
                                if ui
                                    .add_sized(
                                        [btn_w, btn_h],
                                        egui::Button::new(
                                            egui::RichText::new("+").size(22.0 * scale),
                                        ),
                                    )
                                    .clicked()
                                {
                                    self.zoom_level = (self.zoom_level * 1.2).clamp(1.0, 8.0);
                                }
                                ui.add_space(8.0 * scale);
                                if ui
                                    .add_sized(
                                        [btn_w, btn_h],
                                        egui::Button::new(
                                            egui::RichText::new("−").size(22.0 * scale),
                                        ),
                                    )
                                    .clicked()
                                {
                                    self.zoom_level = (self.zoom_level / 1.2).clamp(1.0, 8.0);
                                }
                                ui.add_space(8.0 * scale);
                                if ui
                                    .add_sized(
                                        [btn_w, reset_h],
                                        egui::Button::new(
                                            egui::RichText::new("1x").size(16.0 * scale),
                                        ),
                                    )
                                    .clicked()
                                {
                                    self.reset_view();
                                }
                            });
                        });
                });
        }

        // Left-side vertical feature tabs (icon buttons) on top of all content.
        let tab_btn = 60.0 * scale;
        let tab_pad = 24.0;
        egui::Area::new(egui::Id::new("feature_tab_controls"))
            .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -tab_pad))
            .order(egui::Order::Foreground)
            .interactable(true)
            .show(ctx, |ui| {
                egui::Frame::window(ui.style())
                    .inner_margin(egui::Margin::same((8.0 * scale) as i8))
                    .show(ui, |ui| {
                        ui.horizontal_centered(|ui| {
                            if ui
                                .add_sized(
                                    [tab_btn, tab_btn],
                                    egui::Button::new(egui::RichText::new("🔍").size(24.0 * scale))
                                        .selected(self.active_tab == ToolTab::Crop),
                                )
                                .on_hover_text("Screen Crop Tool")
                                .clicked()
                            {
                                self.active_tab = ToolTab::Crop;
                            }
                            ui.add_space(10.0 * scale);
                            if ui
                                .add_sized(
                                    [tab_btn, tab_btn],
                                    egui::Button::new(egui::RichText::new("📈").size(24.0 * scale))
                                        .selected(self.active_tab == ToolTab::Performance),
                                )
                                .on_hover_text("Performance Monitor")
                                .clicked()
                            {
                                self.active_tab = ToolTab::Performance;
                            }
                            ui.add_space(10.0 * scale);
                            if ui
                                .add_sized(
                                    [tab_btn, tab_btn],
                                    egui::Button::new(egui::RichText::new("🎨").size(24.0 * scale))
                                        .selected(self.active_tab == ToolTab::Shaders),
                                )
                                .on_hover_text("Shader Presets")
                                .clicked()
                            {
                                self.active_tab = ToolTab::Shaders;
                            }
                        });
                    });
            });
        self.clamp_pan_center();
    }

    pub fn destroy(&mut self, gl: &Arc<glow::Context>) {
        if let Some(renderer) = self.quad_renderer.take() {
            renderer.destroy(gl);
        }
    }
}
