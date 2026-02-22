//! egui UI for the screen-tool magnifier.
//!
//! The captured display is rendered as a fullscreen quad using raw OpenGL
//! (bypassing egui's texture pipeline), then egui renders the toolbar
//! overlay on top.

use glow::HasContext;
use std::sync::Arc;

use crate::nvfbc::NvfbcCapture;

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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ToolTab {
    Crop,
    Performance,
}

impl ScreenToolGui {
    pub fn new(outputs: Vec<OutputInfo>) -> Self {
        Self {
            active_tab: ToolTab::Crop,
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
        }
    }

    pub fn wants_capture(&self) -> bool {
        self.active_tab == ToolTab::Crop
    }

    fn ensure_style(&mut self, ctx: &egui::Context) {
        if self.style_initialized {
            return;
        }
        self.style_initialized = true;

        let mut style = (*ctx.style()).clone();
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        style.spacing.button_padding = egui::vec2(12.0, 8.0);
        style.text_styles.insert(
            egui::TextStyle::Body,
            egui::FontId::new(15.0, egui::FontFamily::Proportional),
        );
        style.text_styles.insert(
            egui::TextStyle::Button,
            egui::FontId::new(15.0, egui::FontFamily::Proportional),
        );
        style.text_styles.insert(
            egui::TextStyle::Small,
            egui::FontId::new(13.0, egui::FontFamily::Proportional),
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
        if self.active_tab == ToolTab::Performance {
            unsafe {
                gl.viewport(0, 0, viewport_w as i32, viewport_h as i32);
                gl.disable(glow::SCISSOR_TEST);
                gl.disable(glow::BLEND);
                gl.clear_color(0.02, 0.02, 0.03, 1.0);
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
        self.ensure_style(ctx);

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

        // Large top tabs for current and future feature modules.
        egui::TopBottomPanel::top("feature_tabs").show(ctx, |ui| {
            ui.add_space(4.0);
            ui.horizontal_centered(|ui| {
                let crop_selected = self.active_tab == ToolTab::Crop;
                let perf_selected = self.active_tab == ToolTab::Performance;
                if ui
                    .add_sized(
                        [220.0, 42.0],
                        egui::Button::new(egui::RichText::new("Screen Crop Tool").size(18.0))
                            .selected(crop_selected),
                    )
                    .clicked()
                {
                    self.active_tab = ToolTab::Crop;
                }
                ui.add_space(10.0);
                if ui
                    .add_sized(
                        [220.0, 42.0],
                        egui::Button::new(egui::RichText::new("Performance Monitor").size(18.0))
                            .selected(perf_selected),
                    )
                    .clicked()
                {
                    self.active_tab = ToolTab::Performance;
                }
            });
            ui.add_space(4.0);
        });

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
                    ui.label(match self.active_tab {
                        ToolTab::Crop => "Mode: Crop",
                        ToolTab::Performance => "Mode: Perf",
                    });
                });
            });
        }

        if self.show_stats_panel {
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

        if self.show_help_panel {
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
                });
        }

        if self.active_tab == ToolTab::Performance {
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE)
                .show(ctx, |ui| {
                    ui.add_space(24.0);
                    ui.vertical_centered(|ui| {
                        ui.heading("Performance Monitor");
                        ui.add_space(18.0);
                        egui::Grid::new("perf_grid")
                            .num_columns(2)
                            .spacing([24.0, 10.0])
                            .show(ui, |ui| {
                                ui.label("Render FPS");
                                ui.label(format!("{:.0}", self.fps));
                                ui.end_row();
                                ui.label("Capture FPS (new)");
                                ui.label(format!("{:.0}", self.capture_fps));
                                ui.end_row();
                                ui.label("Capture size");
                                ui.label(format!("{}x{}", self.capture_size.0, self.capture_size.1));
                                ui.end_row();
                                ui.label("Zoom");
                                ui.label(format!("{:.2}x", self.zoom_level));
                                ui.end_row();
                                ui.label("Pan");
                                ui.label(format!("{:.3}, {:.3}", self.pan_center.0, self.pan_center.1));
                                ui.end_row();
                            });
                    });
                });
            return;
        }

        // Always-visible, touch-friendly zoom controls for high-res displays.
        let screen_h = ctx.screen_rect().height();
        let scale = if screen_h >= 1400.0 {
            1.45
        } else if screen_h >= 1080.0 {
            1.25
        } else {
            1.0
        };
        let btn_w = 60.0 * scale;
        let btn_h = 48.0 * scale;
        let reset_h = 34.0 * scale;
        let pad = 24.0;
        egui::Area::new(egui::Id::new("quick_zoom_controls"))
            .anchor(egui::Align2::RIGHT_CENTER, egui::vec2(-pad, 0.0))
            .interactable(true)
            .show(ctx, |ui| {
                egui::Frame::window(ui.style())
                    .inner_margin(egui::Margin::same((8.0 * scale) as i8))
                    .show(ui, |ui| {
                        ui.vertical_centered(|ui| {
                            if ui
                                .add_sized([btn_w, btn_h], egui::Button::new(egui::RichText::new("+").size(22.0 * scale)))
                                .clicked()
                            {
                                self.zoom_level = (self.zoom_level * 1.2).clamp(1.0, 8.0);
                            }
                            ui.add_space(8.0 * scale);
                            if ui
                                .add_sized([btn_w, btn_h], egui::Button::new(egui::RichText::new("−").size(22.0 * scale)))
                                .clicked()
                            {
                                self.zoom_level = (self.zoom_level / 1.2).clamp(1.0, 8.0);
                            }
                            ui.add_space(8.0 * scale);
                            if ui
                                .add_sized([btn_w, reset_h], egui::Button::new(egui::RichText::new("1x").size(16.0 * scale)))
                                .clicked()
                            {
                                self.reset_view();
                            }
                        });
                    });
            });

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
        self.clamp_pan_center();
    }

    pub fn destroy(&mut self, gl: &Arc<glow::Context>) {
        if let Some(renderer) = self.quad_renderer.take() {
            renderer.destroy(gl);
        }
    }
}
