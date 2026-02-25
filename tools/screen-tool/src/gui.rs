//! egui UI for the screen-tool magnifier.
//!
//! The captured display is rendered as a fullscreen quad using raw OpenGL
//! (bypassing egui's texture pipeline), then egui renders the toolbar
//! overlay on top.

use glow::HasContext;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::gamepad::{GamepadButton, StickAxis};
use crate::nvfbc::NvfbcCapture;
use crate::system_stats::SystemStatsSnapshot;

// ─── Modern Retro Color Palette ────────────────────────────────────────────
#[allow(dead_code)]
mod colors {
    use egui::Color32;

    // Backgrounds
    pub const BG_DEEP: Color32 = Color32::from_rgb(10, 11, 20);         // #0A0B14
    pub const BG_PANEL: Color32 = Color32::from_rgba_premultiplied(18, 21, 42, 230); // glass panel
    pub const BG_PANEL_LIGHT: Color32 = Color32::from_rgba_premultiplied(24, 28, 52, 220);
    pub const BG_CARD: Color32 = Color32::from_rgba_premultiplied(22, 26, 48, 235);
    pub const BG_INPUT: Color32 = Color32::from_rgba_premultiplied(14, 16, 32, 240);

    // Accents
    pub const ACCENT_BLUE: Color32 = Color32::from_rgb(0, 212, 255);     // #00D4FF — primary
    pub const ACCENT_PINK: Color32 = Color32::from_rgb(255, 46, 151);    // #FF2E97 — secondary
    pub const ACCENT_GREEN: Color32 = Color32::from_rgb(0, 255, 136);    // #00FF88 — success/GPU
    pub const ACCENT_AMBER: Color32 = Color32::from_rgb(255, 184, 0);    // #FFB800 — warning
    pub const ACCENT_RED: Color32 = Color32::from_rgb(255, 82, 82);      // #FF5252 — danger

    // Text
    pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(232, 236, 244);   // #E8ECF4
    pub const TEXT_SECONDARY: Color32 = Color32::from_rgb(123, 141, 181); // #7B8DB5
    pub const TEXT_DIM: Color32 = Color32::from_rgb(80, 95, 130);         // dimmer labels

    // Borders
    pub const BORDER_GLOW: Color32 = Color32::from_rgba_premultiplied(0, 212, 255, 40);
    pub const BORDER_SUBTLE: Color32 = Color32::from_rgba_premultiplied(80, 100, 160, 60);

    // Tab bar
    pub const TAB_ACTIVE_BG: Color32 = Color32::from_rgba_premultiplied(0, 212, 255, 35);
    pub const TAB_HOVER_BG: Color32 = Color32::from_rgba_premultiplied(0, 212, 255, 18);

    // Gauge colors
    pub const GAUGE_CPU: Color32 = Color32::from_rgb(255, 120, 92);
    pub const GAUGE_RAM: Color32 = Color32::from_rgb(109, 186, 255);
    pub const GAUGE_GPU: Color32 = Color32::from_rgb(0, 255, 136);

    /// Interpolate between two colors based on t (0..1)
    pub fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
        let t = t.clamp(0.0, 1.0);
        Color32::from_rgba_premultiplied(
            (a.r() as f32 + (b.r() as f32 - a.r() as f32) * t) as u8,
            (a.g() as f32 + (b.g() as f32 - a.g() as f32) * t) as u8,
            (a.b() as f32 + (b.b() as f32 - a.b() as f32) * t) as u8,
            (a.a() as f32 + (b.a() as f32 - a.a() as f32) * t) as u8,
        )
    }

    /// Get gauge color based on percentage (green → amber → red)
    pub fn gauge_heat_color(pct: f32) -> Color32 {
        if pct < 60.0 {
            lerp_color(ACCENT_GREEN, ACCENT_AMBER, pct / 60.0)
        } else {
            lerp_color(ACCENT_AMBER, ACCENT_RED, (pct - 60.0) / 40.0)
        }
    }
}

// ─── Drawing Helpers ───────────────────────────────────────────────────────

/// Draw a frosted glass panel with glow border
fn draw_glass_panel(painter: &egui::Painter, rect: egui::Rect, rounding: f32, glow: bool) {
    // Soft shadow (slightly larger, offset rect)
    let shadow_rect = rect.expand(2.0).translate(egui::vec2(0.0, 1.0));
    painter.rect_filled(
        shadow_rect,
        rounding + 2.0,
        egui::Color32::from_rgba_premultiplied(0, 0, 0, 50),
    );
    // Panel fill
    painter.rect_filled(rect, rounding, colors::BG_PANEL);
    // Border with optional glow
    let border_color = if glow { colors::BORDER_GLOW } else { colors::BORDER_SUBTLE };
    painter.rect_stroke(rect, rounding, egui::Stroke::new(1.0, border_color), egui::StrokeKind::Inside);
}

/// Draw a full-ring gauge with glow effect
fn draw_ring_gauge(
    painter: &egui::Painter,
    center: egui::Pos2,
    radius: f32,
    thickness: f32,
    pct: f32,
    color: egui::Color32,
    label: &str,
    value_text: &str,
    scale: f32,
) {
    let pct = pct.clamp(0.0, 100.0);
    let t = pct / 100.0;

    // Full circle background track
    let steps = 64;
    let start_angle = -std::f32::consts::FRAC_PI_2; // 12 o'clock
    let full_sweep = std::f32::consts::TAU;

    let arc_points = |a0: f32, a1: f32, r: f32| -> Vec<egui::Pos2> {
        let mut pts = Vec::with_capacity(steps + 1);
        for i in 0..=steps {
            let p = i as f32 / steps as f32;
            let a = a0 + (a1 - a0) * p;
            pts.push(egui::pos2(center.x + r * a.cos(), center.y + r * a.sin()));
        }
        pts
    };

    // Background track
    painter.add(egui::Shape::line(
        arc_points(start_angle, start_angle + full_sweep, radius),
        egui::Stroke::new(thickness, egui::Color32::from_rgba_premultiplied(40, 50, 80, 80)),
    ));

    // Glow layer (slightly thicker, more transparent)
    if t > 0.01 {
        let glow_color = egui::Color32::from_rgba_premultiplied(
            color.r(), color.g(), color.b(), 40,
        );
        painter.add(egui::Shape::line(
            arc_points(start_angle, start_angle + full_sweep * t, radius),
            egui::Stroke::new(thickness + 4.0, glow_color),
        ));
    }

    // Active arc
    if t > 0.01 {
        painter.add(egui::Shape::line(
            arc_points(start_angle, start_angle + full_sweep * t, radius),
            egui::Stroke::new(thickness, color),
        ));
    }

    // End dot
    if t > 0.01 {
        let end_angle = start_angle + full_sweep * t;
        let dot = egui::pos2(center.x + radius * end_angle.cos(), center.y + radius * end_angle.sin());
        painter.circle_filled(dot, thickness * 0.6, color);
    }

    // Center percentage text
    painter.text(
        center,
        egui::Align2::CENTER_CENTER,
        value_text,
        egui::FontId::proportional((22.0 * scale).clamp(18.0, 36.0)),
        colors::TEXT_PRIMARY,
    );

    // Label below
    painter.text(
        egui::pos2(center.x, center.y + radius + thickness + 8.0 * scale),
        egui::Align2::CENTER_TOP,
        label,
        egui::FontId::proportional((13.0 * scale).clamp(11.0, 34.0)),
        colors::TEXT_SECONDARY,
    );
}

/// Draw a "keycap" style button background
fn draw_keycap(painter: &egui::Painter, rect: egui::Rect, hovered: bool, scale: f32) {
    let rounding = 8.0 * scale;
    // Bottom shadow (3D effect)
    let shadow_rect = egui::Rect::from_min_max(
        egui::pos2(rect.left(), rect.top() + 2.0 * scale),
        egui::pos2(rect.right(), rect.bottom() + 2.0 * scale),
    );
    painter.rect_filled(
        shadow_rect,
        rounding,
        egui::Color32::from_rgba_premultiplied(0, 0, 0, 80),
    );
    // Key face
    let face_color = if hovered {
        egui::Color32::from_rgba_premultiplied(40, 50, 80, 230)
    } else {
        egui::Color32::from_rgba_premultiplied(30, 36, 62, 220)
    };
    painter.rect_filled(rect, rounding, face_color);
    // Top highlight (subtle 3D effect)
    let highlight_rect = egui::Rect::from_min_max(
        rect.left_top(),
        egui::pos2(rect.right(), rect.top() + 2.0 * scale),
    );
    painter.rect_filled(
        highlight_rect,
        egui::CornerRadius { nw: rounding.round() as u8, ne: rounding.round() as u8, sw: 0, se: 0 },
        egui::Color32::from_rgba_premultiplied(255, 255, 255, 15),
    );
    // Border
    let border = if hovered { colors::ACCENT_BLUE } else { colors::BORDER_SUBTLE };
    painter.rect_stroke(rect, rounding, egui::Stroke::new(1.0, border), egui::StrokeKind::Inside);
}

/// Draw a retro digital clock display
fn draw_digital_clock(painter: &egui::Painter, rect: egui::Rect, time_str: &str, scale: f32) {
    let rounding = 10.0 * scale;
    // LCD panel background
    painter.rect_filled(
        rect,
        rounding,
        egui::Color32::from_rgba_premultiplied(8, 12, 20, 240),
    );
    painter.rect_stroke(
        rect,
        rounding,
        egui::Stroke::new(1.5, colors::ACCENT_GREEN.linear_multiply(0.3)),
        egui::StrokeKind::Inside,
    );
    // Subtle scanline effect (horizontal lines every 3px)
    let step = (3.0 * scale).max(2.0);
    let mut y = rect.top() + step;
    while y < rect.bottom() {
        painter.line_segment(
            [egui::pos2(rect.left() + rounding, y), egui::pos2(rect.right() - rounding, y)],
            egui::Stroke::new(0.5, egui::Color32::from_rgba_premultiplied(0, 255, 136, 8)),
        );
        y += step;
    }
    // Time text
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        time_str,
        egui::FontId::proportional((28.0 * scale).clamp(22.0, 44.0)),
        colors::ACCENT_GREEN,
    );
}

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

/// Display info for the Windows tab (populated by LayoutManager/WindowManager).
#[derive(Clone, Debug)]
pub struct WmDisplayInfo {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub x: i32,
    pub y: i32,
}

/// Window entry for the Windows tab.
#[derive(Clone, Debug)]
pub struct WmWindowEntry {
    pub id: u64,
    pub title: String,
    pub display: String, // which display it's currently on
    pub width: u32,
    pub height: u32,
    pub mapped: bool,
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

fn ellipsize_middle(input: &str, max_chars: usize) -> String {
    if input.chars().count() <= max_chars {
        return input.to_string();
    }
    if max_chars <= 3 {
        return "...".to_string();
    }
    let keep = max_chars - 3;
    let left = keep / 2;
    let right = keep - left;
    let start: String = input.chars().take(left).collect();
    let end: String = input
        .chars()
        .rev()
        .take(right)
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    format!("{start}...{end}")
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

fn query_output_refresh_hz(output_name: &str) -> Option<f32> {
    let out = std::process::Command::new("xrandr")
        .arg("--current")
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let mut in_target = false;
    for raw in text.lines() {
        let line = raw.trim_end();
        let trimmed = line.trim_start();
        // New output section starts when line begins at column 0 and looks like "DP-0 connected..."
        if !raw.starts_with(' ') && !raw.starts_with('\t') {
            in_target = trimmed.starts_with(output_name);
            continue;
        }
        if !in_target {
            continue;
        }

        // Prefer currently selected mode token (has '*', e.g. "60.00*+")
        for tok in trimmed.split_whitespace() {
            if tok.contains('*') {
                let cleaned = tok.trim_end_matches(|c| c == '*' || c == '+');
                if let Ok(v) = cleaned.parse::<f32>() {
                    if (20.0..=360.0).contains(&v) {
                        return Some(v);
                    }
                }
            }
        }
    }
    None
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

fn hotkeys_file_path() -> String {
    std::env::var("SCREEN_TOOL_HOTKEYS_FILE")
        .unwrap_or_else(|_| "/gamer/conf/screen-tool-hotkeys.txt".to_string())
}

fn load_hotkeys_from_file(path: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let content = match std::fs::read_to_string(path) {
        Ok(v) => v,
        Err(_) => return out,
    };
    for raw in content.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((title, keyseq)) = line.split_once('=') else {
            continue;
        };
        let title = title.trim();
        let keyseq = keyseq.trim();
        if title.is_empty() || keyseq.is_empty() {
            continue;
        }
        out.push((title.to_string(), keyseq.to_string()));
    }
    out
}

fn find_emulator_primary_window_id(target_pattern: &str) -> anyhow::Result<String> {
    let search = std::process::Command::new("xdotool")
        .args(["search", "--onlyvisible", "--name", target_pattern])
        .output()?;
    if !search.status.success() {
        anyhow::bail!("xdotool search failed");
    }

    let stdout = String::from_utf8_lossy(&search.stdout);
    let ids: Vec<String> = stdout
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(ToOwned::to_owned)
        .collect();
    if ids.is_empty() {
        anyhow::bail!("No emulator window matching '{}'", target_pattern);
    }

    let mut preferred: Vec<(u64, String)> = Vec::new();
    let mut fallback: Vec<(u64, String)> = Vec::new();

    for wid in ids {
        let title = std::process::Command::new("xdotool")
            .args(["getwindowname", &wid])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default();
        let num = wid.parse::<u64>().unwrap_or(u64::MAX);

        // In dual-window emulators, prefer the primary game window over
        // "Secondary Window" so hotkeys/focus consistently target the first window.
        if title.to_ascii_lowercase().contains("secondary window") {
            fallback.push((num, wid));
        } else {
            preferred.push((num, wid));
        }
    }

    if !preferred.is_empty() {
        preferred.sort_by_key(|(n, _)| *n);
        return Ok(preferred[0].1.clone());
    }
    fallback.sort_by_key(|(n, _)| *n);
    Ok(fallback[0].1.clone())
}

fn send_hotkey_to_emulator(keyseq: &str, target_pattern: &str) -> anyhow::Result<()> {
    let primary_wid = find_emulator_primary_window_id(target_pattern)?;

    // Ensure emulator gets focus first for reliable shortcut handling.
    // Azahar can ignore key events when sent to non-active windows.
    let activated = std::process::Command::new("xdotool")
        .args(["windowactivate", "--sync", &primary_wid])
        .output()?;
    if !activated.status.success() {
        anyhow::bail!("Failed to activate emulator window");
    }

    // Send to active window first (most reliable path for Qt apps).
    let active_send = std::process::Command::new("xdotool")
        .args(["key", "--clearmodifiers", keyseq])
        .output()?;
    if active_send.status.success() {
        return Ok(());
    }

    // Fallback: target explicit window id.
    let send = std::process::Command::new("xdotool")
        .args(["key", "--window", &primary_wid, "--clearmodifiers", keyseq])
        .output()?;
    if !send.status.success() {
        anyhow::bail!("Failed to send key '{}' to emulator window", keyseq);
    }
    Ok(())
}

fn focus_emulator_window(target_pattern: &str) -> anyhow::Result<()> {
    let first_id = find_emulator_primary_window_id(target_pattern)?;
    let activate = std::process::Command::new("xdotool")
        .args(["windowactivate", "--sync", &first_id])
        .output()?;
    if !activate.status.success() {
        anyhow::bail!("Failed to focus emulator window");
    }
    Ok(())
}

fn faketime_timestamp_file_path() -> String {
    std::env::var("FAKETIME_TIMESTAMP_FILE")
        .unwrap_or_else(|_| "/home/gamer/.cache/faketime.timestamp".to_string())
}

fn read_faketime_string(path: &str) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|v| v.lines().next().map(|s| s.trim().to_string()))
        .filter(|v| !v.is_empty())
}

fn read_faketime_from_mtime(path: &str) -> Option<String> {
    let meta = std::fs::metadata(path).ok()?;
    let modified = meta.modified().ok()?;
    let secs = modified
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs()
        .to_string();
    let out = std::process::Command::new("date")
        .args(["-d", &format!("@{secs}"), "+%Y-%m-%d %H:%M:%S"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let parsed = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if parsed.is_empty() { None } else { Some(parsed) }
}

fn write_faketime_follow_file(path: &str, formatted: &str) -> anyhow::Result<()> {
    std::fs::write(path, format!("{formatted}\n"))?;
    let touch = std::process::Command::new("touch")
        .args(["-d", formatted, path])
        .output()?;
    if !touch.status.success() {
        anyhow::bail!("touch -d failed for '{}'", path);
    }
    Ok(())
}

fn parse_faketime_parts(s: &str) -> Option<(i32, u32, u32, u32, u32, u32)> {
    let mut parts = s.split_whitespace();
    let date = parts.next()?;
    let time = parts.next()?;
    let mut d = date.split('-');
    let mut t = time.split(':');
    let year = d.next()?.parse::<i32>().ok()?;
    let month = d.next()?.parse::<u32>().ok()?;
    let day = d.next()?.parse::<u32>().ok()?;
    let hour = t.next()?.parse::<u32>().ok()?;
    let minute = t.next()?.parse::<u32>().ok()?;
    let second = t.next()?.parse::<u32>().ok()?;
    Some((year, month, day, hour, minute, second))
}

fn format_faketime_parts(
    year: i32,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
) -> String {
    format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02}")
}

fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_leap_year(year) {
                29
            } else {
                28
            }
        }
        _ => 30,
    }
}

/// Raw GL resources for rendering a textured quad.
struct QuadRenderer {
    program: glow::NativeProgram,
    vao: glow::NativeVertexArray,
    vbo: glow::NativeBuffer,
    u_uv_offset: glow::UniformLocation,
    u_uv_scale: glow::UniformLocation,
    last_filter_tex_id: u32,
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
                last_filter_tex_id: 0,
            }
        }
    }

    /// Draw a textured quad with the given NVFBC texture and UV parameters.
    fn draw(&mut self, gl: &glow::Context, texture_id: u32, uv_offset: [f32; 2], uv_scale: [f32; 2]) {
        unsafe {
            let tex = glow::NativeTexture(std::num::NonZeroU32::new_unchecked(texture_id));

            gl.use_program(Some(self.program));
            gl.uniform_2_f32(Some(&self.u_uv_offset), uv_offset[0], uv_offset[1]);
            gl.uniform_2_f32(Some(&self.u_uv_scale), uv_scale[0], uv_scale[1]);

            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(tex));

            // Only set filter params when texture changes (they're texture-object state)
            if self.last_filter_tex_id != texture_id {
                gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
                gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
                self.last_filter_tex_id = texture_id;
            }

            gl.bind_vertex_array(Some(self.vao));
            gl.draw_arrays(glow::TRIANGLES, 0, 6);
            // Skip unbinding — egui_glow sets its own state before painting
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
    output_selection_changed: bool,

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
    shader_list_dir: String,
    shader_dirs: Vec<String>,
    shader_files: Vec<String>,
    shader_selected: Option<String>,
    shader_status: Option<String>,
    shader_status_until: Option<Instant>,

    // hotkey pad
    hotkeys_file: String,
    hotkeys: Vec<(String, String)>,
    hotkey_target: String, // window name pattern to send hotkeys to
    hotkeys_status: Option<String>,
    hotkeys_status_until: Option<Instant>,

    // cached refresh rate (avoid shelling out to xrandr every frame)
    cached_refresh_hz: Option<f32>,
    cached_refresh_output: String,
    last_refresh_query: Instant,

    // Window management tab state
    pub wm_displays: Vec<WmDisplayInfo>,
    pub wm_windows: Vec<WmWindowEntry>,
    pub wm_pending_actions: Vec<(u64, String, String)>, // (window_id, display_name, mode)
    wm_status: Option<String>,
    wm_status_until: Option<Instant>,

    // Virtual gamepad state
    pub gamepad_pending_presses: Vec<(GamepadButton, bool)>,
    pub gamepad_pending_axes: Vec<(StickAxis, i32)>,
    gamepad_button_states: [bool; GamepadButton::COUNT],
    gamepad_left_stick: (f32, f32),   // normalized -1..1
    gamepad_right_stick: (f32, f32),  // normalized -1..1

    // live faketime controls
    faketime_file: String,
    faketime_year: i32,
    faketime_month: u32,
    faketime_day: u32,
    faketime_hour: u32,
    faketime_minute: u32,
    faketime_second: u32,
    faketime_status: Option<String>,
    faketime_status_until: Option<Instant>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ToolTab {
    Crop,
    Performance,
    Windows,
    Shaders,
    Hotkeys,
    Faketime,
    Gamepad,
}

impl ScreenToolGui {
    pub fn new(outputs: Vec<OutputInfo>) -> Self {
        let default_ui_scale = std::env::var("SCREEN_TOOL_UI_SCALE")
            .ok()
            .and_then(|v| v.parse::<f32>().ok())
            .unwrap_or(1.0)
            .clamp(0.8, 3.0);
        let hotkeys_file = hotkeys_file_path();
        let faketime_file = faketime_timestamp_file_path();
        let mut gui = Self {
            toggle_only: false,
            active_tab: ToolTab::Crop,
            available_outputs: outputs,
            selected_output_idx: 0,
            output_selection_changed: false,
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
            shader_list_dir: String::new(),
            shader_dirs: Vec::new(),
            shader_files: Vec::new(),
            shader_selected: None,
            shader_status: None,
            shader_status_until: None,
            hotkeys_file: hotkeys_file.clone(),
            hotkeys: load_hotkeys_from_file(&hotkeys_file),
            hotkey_target: std::env::var("SCREEN_TOOL_HOTKEY_TARGET").unwrap_or_else(|_| "Azahar".to_string()),
            hotkeys_status: None,
            hotkeys_status_until: None,
            faketime_file: faketime_file.clone(),
            faketime_year: 2024,
            faketime_month: 1,
            faketime_day: 1,
            faketime_hour: 12,
            faketime_minute: 0,
            faketime_second: 0,
            cached_refresh_hz: None,
            cached_refresh_output: String::new(),
            last_refresh_query: Instant::now() - Duration::from_secs(60),
            faketime_status: None,
            faketime_status_until: None,
            gamepad_pending_presses: Vec::new(),
            gamepad_pending_axes: Vec::new(),
            gamepad_button_states: [false; GamepadButton::COUNT],
            gamepad_left_stick: (0.0, 0.0),
            gamepad_right_stick: (0.0, 0.0),
            wm_displays: Vec::new(),
            wm_windows: Vec::new(),
            wm_pending_actions: Vec::new(),
            wm_status: None,
            wm_status_until: None,
        };
        if let Some(s) =
            read_faketime_string(&faketime_file).or_else(|| read_faketime_from_mtime(&faketime_file))
        {
            if let Some((y, m, d, hh, mm, ss)) = parse_faketime_parts(&s) {
                gui.faketime_year = y;
                gui.faketime_month = m.clamp(1, 12);
                gui.faketime_day = d.max(1);
                gui.faketime_hour = hh.min(23);
                gui.faketime_minute = mm.min(59);
                gui.faketime_second = ss.min(59);
            }
        }
        gui
    }

    pub fn wants_capture(&self) -> bool {
        !self.toggle_only && self.active_tab == ToolTab::Crop
    }

    pub fn take_output_selection_changed(&mut self) -> bool {
        let v = self.output_selection_changed;
        self.output_selection_changed = false;
        v
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
        style.spacing.button_padding = egui::vec2(14.0 * ui_scale, 8.0 * ui_scale);
        style.spacing.window_margin = egui::Margin::same((12.0 * ui_scale) as i8);
        style.text_styles.insert(
            egui::TextStyle::Heading,
            egui::FontId::new(24.0 * ui_scale, egui::FontFamily::Proportional),
        );
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
        visuals.panel_fill = colors::BG_PANEL;
        visuals.window_fill = colors::BG_PANEL;
        visuals.window_corner_radius = egui::CornerRadius::same(12);
        visuals.window_stroke = egui::Stroke::new(1.0, colors::BORDER_SUBTLE);
        visuals.widgets.noninteractive.bg_fill = colors::BG_PANEL_LIGHT;
        visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, colors::TEXT_SECONDARY);
        visuals.widgets.inactive.bg_fill = egui::Color32::from_rgba_premultiplied(28, 34, 60, 210);
        visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, colors::TEXT_PRIMARY);
        visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(8);
        visuals.widgets.hovered.bg_fill = colors::TAB_HOVER_BG;
        visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, colors::ACCENT_BLUE);
        visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(8);
        visuals.widgets.active.bg_fill = colors::TAB_ACTIVE_BG;
        visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0, colors::ACCENT_BLUE);
        visuals.widgets.active.corner_radius = egui::CornerRadius::same(8);
        visuals.selection.bg_fill = colors::ACCENT_BLUE.linear_multiply(0.2);
        visuals.selection.stroke = egui::Stroke::new(1.0, colors::ACCENT_BLUE);
        ctx.set_visuals(visuals);
    }

    fn refresh_shader_listing_if_needed(&mut self) {
        if self.shader_list_dir != self.shader_current_dir {
            let (dirs, files) = list_shader_entries(&self.shader_current_dir);
            self.shader_dirs = dirs;
            self.shader_files = files;
            self.shader_list_dir = self.shader_current_dir.clone();
        }
    }

    fn get_refresh_hz(&mut self, output_name: &str) -> Option<f32> {
        if self.cached_refresh_output == output_name
            && self.last_refresh_query.elapsed() < Duration::from_secs(10)
        {
            return self.cached_refresh_hz;
        }
        let hz = query_output_refresh_hz(output_name);
        self.cached_refresh_hz = hz;
        self.cached_refresh_output = output_name.to_string();
        self.last_refresh_query = Instant::now();
        hz
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
                gl.clear_color(0.04, 0.043, 0.078, 0.95); // BG_DEEP
                gl.clear(glow::COLOR_BUFFER_BIT);
            }
            return;
        }

        if self.active_tab != ToolTab::Crop {
            unsafe {
                gl.viewport(0, 0, viewport_w as i32, viewport_h as i32);
                gl.disable(glow::SCISSOR_TEST);
                gl.disable(glow::BLEND);
                gl.clear_color(0.039, 0.043, 0.078, 1.0); // BG_DEEP
                gl.clear(glow::COLOR_BUFFER_BIT);
            }
            return;
        }

        if self.current_nvfbc_tex_id == 0 {
            // No NVFBC frame yet — clear to background instead of leaving garbage
            unsafe {
                gl.viewport(0, 0, viewport_w as i32, viewport_h as i32);
                gl.disable(glow::SCISSOR_TEST);
                gl.disable(glow::BLEND);
                gl.clear_color(0.039, 0.043, 0.078, 1.0); // BG_DEEP
                gl.clear(glow::COLOR_BUFFER_BIT);
            }
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

        self.quad_renderer.as_mut().unwrap().draw(
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
        // Scale based on geometric mean, using a smaller reference so phone-sized screens
        // get scale ~2.0x (good touch targets). Desktop 1920x1080 gets ~1.5x.
        let w = rect.width();
        let h = rect.height();
        let auto_scale = ((w * h) / (1100.0 * 750.0)).sqrt().clamp(1.0, 3.0);
        let ui_scale = (auto_scale * self.ui_scale_user).clamp(1.0, 4.0);
        self.ensure_style(ctx, ui_scale);

        if self.toggle_only {
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE)
                .show(ctx, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.add_space((ui.available_height() * 0.15).max(2.0));
                        let side = ui.available_width().min(ui.available_height()).max(24.0);
                        if ui
                            .add_sized(
                                [side, side],
                                egui::Button::new(
                                    egui::RichText::new("G")
                                        .size(0.5 * side)
                                        .color(colors::ACCENT_BLUE),
                                )
                                .fill(egui::Color32::from_rgba_premultiplied(0, 212, 255, 18))
                                .stroke(egui::Stroke::new(1.5, colors::ACCENT_BLUE.linear_multiply(0.5)))
                                .corner_radius(side * 0.5),
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
            if i.key_pressed(egui::Key::F6) {
                self.active_tab = ToolTab::Crop;
            }
            if i.key_pressed(egui::Key::F7) {
                self.active_tab = ToolTab::Performance;
            }
            if i.key_pressed(egui::Key::F9) {
                self.active_tab = ToolTab::Shaders;
            }
            if i.key_pressed(egui::Key::F10) {
                self.active_tab = ToolTab::Windows;
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

        // Reuse the unified ui_scale from show()
        let scale = ui_scale;

        // Compact floating toolbar (Crop tab only, auto-hideable)
        if self.show_toolbar && self.active_tab == ToolTab::Crop {
            egui::Area::new(egui::Id::new("crop_toolbar"))
                .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 8.0))
                .order(egui::Order::Foreground)
                .interactable(true)
                .show(ctx, |ui| {
                    egui::Frame::NONE
                        .fill(colors::BG_PANEL)
                        .corner_radius((10.0 * scale) as u8)
                        .stroke(egui::Stroke::new(1.0, colors::BORDER_GLOW))
                        .inner_margin(egui::Margin::symmetric((12.0 * scale) as i8, (6.0 * scale) as i8))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                // Display selector
                                let current_name = self.available_outputs.get(self.selected_output_idx).map(|o| o.name.as_str()).unwrap_or("None");
                                let prev_idx = self.selected_output_idx;
                                ui.label(egui::RichText::new("Display").size(12.0 * scale).color(colors::TEXT_DIM));
                                egui::ComboBox::from_id_salt("display_selector")
                                    .selected_text(egui::RichText::new(current_name).size((12.0 * scale).clamp(12.0, 28.0)))
                                    .width((80.0 * scale).clamp(80.0, 200.0))
                                    .show_ui(ui, |ui| {
                                        for (idx, output) in self.available_outputs.iter().enumerate() {
                                            let label = format!("{} ({}x{})", output.name, output.width, output.height);
                                            let resp = ui.selectable_value(&mut self.selected_output_idx, idx,
                                                egui::RichText::new(label).size((12.0 * scale).clamp(12.0, 28.0)));
                                            // Ensure minimum tap target height
                                            if resp.rect.height() < 32.0 {
                                                ui.add_space(4.0);
                                            }
                                        }
                                    });
                                if self.selected_output_idx != prev_idx {
                                    self.output_selection_changed = true;
                                }

                                ui.add_space(8.0 * scale);
                                ui.label(egui::RichText::new("|").size(12.0 * scale).color(colors::BORDER_SUBTLE));
                                ui.add_space(4.0 * scale);

                                // Quick presets
                                for label in ["Full", "TL", "TR", "BL", "BR"] {
                                    if ui.add(egui::Button::new(egui::RichText::new(label).size(11.0 * scale).color(colors::TEXT_SECONDARY)).fill(egui::Color32::TRANSPARENT).corner_radius(6.0)).clicked() {
                                        self.apply_quick_preset(label);
                                    }
                                }

                                ui.add_space(8.0 * scale);
                                ui.label(egui::RichText::new("|").size(12.0 * scale).color(colors::BORDER_SUBTLE));
                                ui.add_space(4.0 * scale);

                                // Slots
                                for idx in 0..3 {
                                    let slot_label = format!("{}", idx + 1);
                                    let has_slot = self.saved_slots[idx].is_some();
                                    let color = if has_slot { colors::ACCENT_BLUE } else { colors::TEXT_DIM };
                                    if ui.add(egui::Button::new(egui::RichText::new(&slot_label).size(11.0 * scale).color(color)).fill(egui::Color32::TRANSPARENT).corner_radius(6.0)).clicked() {
                                        if let Some((center, zoom)) = self.saved_slots[idx] {
                                            self.set_view(center, zoom);
                                        }
                                    }
                                }
                            });
                        });
                });
        }

        if self.show_stats_panel && self.active_tab == ToolTab::Crop {
            egui::Area::new(egui::Id::new("stats_hud"))
                .anchor(egui::Align2::LEFT_TOP, egui::vec2(12.0, 52.0))
                .order(egui::Order::Foreground)
                .interactable(false)
                .show(ctx, |ui| {
                    egui::Frame::NONE
                        .fill(egui::Color32::from_rgba_premultiplied(10, 12, 24, 180))
                        .corner_radius(8)
                        .stroke(egui::Stroke::new(0.5, colors::BORDER_SUBTLE))
                        .inner_margin(egui::Margin::symmetric(10, 6))
                        .show(ui, |ui| {
                            let s = (11.0 * scale).clamp(10.0, 30.0);
                            ui.label(egui::RichText::new(format!("FPS {:.0} | Capture {:.0}", self.fps, self.capture_fps)).size(s).color(colors::ACCENT_GREEN));
                            if self.capture_size.0 > 0 {
                                ui.label(egui::RichText::new(format!("{}x{} | {:.1}x", self.capture_size.0, self.capture_size.1, self.zoom_level)).size(s).color(colors::TEXT_DIM));
                            }
                        });
                });
        }

        if self.show_help_panel && self.active_tab == ToolTab::Crop {
            egui::Area::new(egui::Id::new("help_hud"))
                .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-12.0, 52.0))
                .order(egui::Order::Foreground)
                .interactable(true)
                .show(ctx, |ui| {
                    egui::Frame::NONE
                        .fill(colors::BG_PANEL)
                        .corner_radius(10)
                        .stroke(egui::Stroke::new(1.0, colors::BORDER_SUBTLE))
                        .inner_margin(egui::Margin::symmetric(12, 8))
                        .show(ui, |ui| {
                            let s = (11.0 * scale).clamp(10.0, 28.0);
                            let shortcut = |ui: &mut egui::Ui, key: &str, desc: &str| {
                                ui.horizontal(|ui| {
                                    ui.label(egui::RichText::new(key).size(s).color(colors::ACCENT_BLUE).strong());
                                    ui.label(egui::RichText::new(desc).size(s).color(colors::TEXT_DIM));
                                });
                            };
                            shortcut(ui, "Scroll", "Zoom");
                            shortcut(ui, "Drag", "Pan");
                            shortcut(ui, "Tab", "Toggle toolbar");
                            shortcut(ui, "F1/F2", "Stats / Help");
                            shortcut(ui, "F5", "Screenshot");
                            shortcut(ui, "F6/F7/F9", "Crop / Perf / Shader");
                            shortcut(ui, "Alt+1..5", "Switch display");
                            shortcut(ui, "1..3", "Load slot");
                            shortcut(ui, "Ctrl+1..3", "Save slot");
                        });
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
                    let gpu = self.system_stats.gpu_usage_pct.unwrap_or(0.0).clamp(0.0, 100.0);
                    let selected_output = self.available_outputs.get(self.selected_output_idx).cloned();
                    let selected_output_name = selected_output.as_ref().map(|o| o.name.clone()).unwrap_or_else(|| "N/A".into());
                    let selected_output_mode = selected_output.as_ref().map(|o| format!("{}x{}", o.width, o.height)).unwrap_or_else(|| "N/A".into());
                    let selected_output_hz = selected_output.as_ref().map(|o| o.name.clone()).and_then(|name| self.get_refresh_hz(&name));

                    let bottom_reserved = (96.0 * scale).clamp(84.0, 260.0);
                    let panel_w = (ui.available_width() - 20.0).max(200.0);
                    let panel_h = (ui.available_height() - bottom_reserved).max(300.0);
                    let card_gap = (8.0 * scale).clamp(4.0, 14.0);

                    // Main glass panel
                    let panel_rect = egui::Rect::from_min_size(
                        ui.cursor().min + egui::vec2(10.0, 4.0),
                        egui::vec2(panel_w, panel_h),
                    );
                    let painter = ui.painter();
                    draw_glass_panel(painter, panel_rect, 14.0 * scale, true);

                    ui.allocate_new_ui(egui::UiBuilder::new().max_rect(panel_rect.shrink(16.0 * scale)), |ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;  // we handle gaps manually
                        ui.vertical_centered(|ui| {
                            // Header with branding
                            ui.add_space(6.0 * scale);
                            ui.label(
                                egui::RichText::new("GAMER")
                                    .size((11.0 * scale).clamp(10.0, 30.0))
                                    .color(colors::ACCENT_BLUE)
                                    .strong(),
                            );
                            ui.label(
                                egui::RichText::new("Performance Monitor")
                                    .size((24.0 * scale).clamp(20.0, 56.0))
                                    .color(colors::TEXT_PRIMARY)
                                    .strong(),
                            );
                            ui.label(
                                egui::RichText::new("Live system telemetry")
                                    .size((12.0 * scale).clamp(11.0, 30.0))
                                    .color(colors::TEXT_DIM),
                            );

                            ui.add_space(12.0 * scale);

                            // Ring gauges row
                            let content_w = ui.available_width();
                            let gauge_area_h = (panel_h * 0.45).clamp(120.0, 450.0);
                            let gauge_card_w = ((content_w - card_gap * 2.0) / 3.0).clamp(60.0, content_w * 0.32);
                            let gauge_radius = (gauge_card_w * 0.3).min(gauge_area_h * 0.32);
                            let gauge_thickness = (6.0 * scale).clamp(4.0, 10.0);

                            ui.horizontal(|ui| {
                                // CPU gauge card
                                let (cpu_rect, _) = ui.allocate_exact_size(egui::vec2(gauge_card_w, gauge_area_h), egui::Sense::hover());
                                let p = ui.painter_at(cpu_rect);
                                draw_glass_panel(&p, cpu_rect, 12.0 * scale, false);
                                let cpu_center = egui::pos2(cpu_rect.center().x, cpu_rect.center().y - 6.0 * scale);
                                draw_ring_gauge(&p, cpu_center, gauge_radius, gauge_thickness, cpu, colors::GAUGE_CPU, "CPU", &format!("{:.0}%", cpu), scale);

                                ui.add_space(card_gap);

                                // RAM gauge card
                                let (ram_rect, _) = ui.allocate_exact_size(egui::vec2(gauge_card_w, gauge_area_h), egui::Sense::hover());
                                let p = ui.painter_at(ram_rect);
                                draw_glass_panel(&p, ram_rect, 12.0 * scale, false);
                                let ram_center = egui::pos2(ram_rect.center().x, ram_rect.center().y - 6.0 * scale);
                                draw_ring_gauge(&p, ram_center, gauge_radius, gauge_thickness, ram, colors::GAUGE_RAM, "RAM", &format!("{:.0}%", ram), scale);

                                ui.add_space(card_gap);

                                // GPU gauge card
                                let (gpu_rect, _) = ui.allocate_exact_size(egui::vec2(gauge_card_w, gauge_area_h), egui::Sense::hover());
                                let p = ui.painter_at(gpu_rect);
                                draw_glass_panel(&p, gpu_rect, 12.0 * scale, false);
                                let gpu_center = egui::pos2(gpu_rect.center().x, gpu_rect.center().y - 6.0 * scale);
                                let gpu_text = self.system_stats.gpu_usage_pct.map(|v| format!("{v:.0}%")).unwrap_or_else(|| "N/A".into());
                                draw_ring_gauge(&p, gpu_center, gauge_radius, gauge_thickness, gpu, colors::GAUGE_GPU, "GPU", &gpu_text, scale);
                            });

                            ui.add_space(12.0 * scale);

                            // Metric cards row
                            let pill_w = ((content_w - card_gap * 4.0) / 5.0).clamp(40.0, content_w * 0.19);
                            let pill_h = (48.0 * scale).clamp(40.0, 130.0);
                            let name_size = (11.0 * scale).clamp(10.0, 30.0);
                            let value_size = (14.0 * scale).clamp(13.0, 38.0);
                            let gpu_mem_text = match (self.system_stats.gpu_mem_used_mib, self.system_stats.gpu_mem_total_mib) {
                                (Some(used), Some(total)) => format!("{used}/{total}M"),
                                _ => "N/A".into(),
                            };

                            let metric_card = |ui: &mut egui::Ui, title: &str, value: &str, accent: egui::Color32| {
                                let (rect, _) = ui.allocate_exact_size(egui::vec2(pill_w, pill_h), egui::Sense::hover());
                                let p = ui.painter_at(rect);
                                draw_glass_panel(&p, rect, 8.0 * scale, false);
                                p.text(
                                    egui::pos2(rect.center().x, rect.top() + 8.0 * scale),
                                    egui::Align2::CENTER_TOP,
                                    title,
                                    egui::FontId::proportional(name_size),
                                    colors::TEXT_DIM,
                                );
                                p.text(
                                    egui::pos2(rect.center().x, rect.bottom() - 8.0 * scale),
                                    egui::Align2::CENTER_BOTTOM,
                                    value,
                                    egui::FontId::proportional(value_size),
                                    accent,
                                );
                            };

                            ui.horizontal(|ui| {
                                metric_card(ui, "Display", &selected_output_name, colors::TEXT_PRIMARY);
                                ui.add_space(card_gap);
                                metric_card(ui, "Resolution", &selected_output_mode, colors::TEXT_PRIMARY);
                                ui.add_space(card_gap);
                                metric_card(ui, "Refresh", &selected_output_hz.map(|v| format!("{v:.0} Hz")).unwrap_or_else(|| "N/A".into()), colors::ACCENT_BLUE);
                                ui.add_space(card_gap);
                                metric_card(ui, "VRAM", &gpu_mem_text, colors::ACCENT_GREEN);
                                ui.add_space(card_gap);
                                metric_card(ui, "RAM", &format!("{:.1}/{:.1}G", self.system_stats.ram_used_gib, self.system_stats.ram_total_gib), colors::GAUGE_RAM);
                            });
                        });
                    });
                });
        }

        // ─── Windows Tab ──────────────────────────────────────────────
        if self.active_tab == ToolTab::Windows {
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE)
                .show(ctx, |ui| {
                    let bottom_reserved = (96.0 * scale).clamp(84.0, 260.0);
                    let panel_w = (ui.available_width() - 20.0).max(440.0);
                    let panel_h = (ui.available_height() - bottom_reserved).max(300.0);
                    let panel_rect = egui::Rect::from_min_size(
                        egui::pos2(
                            (ui.available_width() - panel_w) / 2.0 + ui.min_rect().left(),
                            (ui.available_height() - panel_h - bottom_reserved) / 2.0 + ui.min_rect().top(),
                        ),
                        egui::vec2(panel_w, panel_h),
                    );
                    draw_glass_panel(ui.painter(), panel_rect, 16.0 * scale, true);

                    let mut inner_ui = ui.new_child(
                        egui::UiBuilder::new()
                            .max_rect(panel_rect.shrink(16.0 * scale))
                    );
                    let iu = &mut inner_ui;

                    iu.vertical(|ui| {
                        ui.add_space(4.0 * scale);
                        ui.label(
                            egui::RichText::new("Window Manager")
                                .size(20.0 * scale)
                                .color(colors::TEXT_PRIMARY),
                        );
                        ui.add_space(8.0 * scale);

                        // Displays section
                        ui.label(
                            egui::RichText::new("Displays")
                                .size(14.0 * scale)
                                .color(colors::ACCENT_BLUE),
                        );
                        ui.add_space(4.0 * scale);

                        if self.wm_displays.is_empty() {
                            ui.label(
                                egui::RichText::new("No displays detected")
                                    .size(12.0 * scale)
                                    .color(colors::TEXT_DIM),
                            );
                        } else {
                            for disp in &self.wm_displays {
                                ui.horizontal(|ui| {
                                    let indicator_color = if disp.name.contains("DP-0") || disp.name.contains("primary") {
                                        colors::ACCENT_GREEN
                                    } else {
                                        colors::ACCENT_PINK
                                    };
                                    let (rect, _) = ui.allocate_exact_size(egui::vec2(8.0 * scale, 8.0 * scale), egui::Sense::hover());
                                    ui.painter().circle_filled(rect.center(), 4.0 * scale, indicator_color);
                                    ui.label(
                                        egui::RichText::new(format!(
                                            "{} — {}x{} @ ({},{})",
                                            disp.name, disp.width, disp.height, disp.x, disp.y
                                        ))
                                        .size(12.0 * scale)
                                        .color(colors::TEXT_SECONDARY),
                                    );
                                });
                            }
                        }

                        ui.add_space(12.0 * scale);
                        ui.separator();
                        ui.add_space(8.0 * scale);

                        // Windows section
                        ui.label(
                            egui::RichText::new("Open Windows")
                                .size(14.0 * scale)
                                .color(colors::ACCENT_BLUE),
                        );
                        ui.add_space(4.0 * scale);

                        if self.wm_windows.is_empty() {
                            ui.label(
                                egui::RichText::new("No emulator windows found")
                                    .size(12.0 * scale)
                                    .color(colors::TEXT_DIM),
                            );
                        } else {
                            egui::ScrollArea::vertical()
                                .max_height(panel_h * 0.55)
                                .show(ui, |ui| {
                                    // Collect actions to apply after iteration
                                    let mut actions: Vec<(u64, String, &str)> = Vec::new();

                                    let windows_snapshot: Vec<_> = self.wm_windows.clone();
                                    for win in &windows_snapshot {
                                        ui.horizontal(|ui| {
                                            // Window title and current display
                                            let mapped_label = if win.mapped { "" } else { " [hidden]" };
                                            ui.label(
                                                egui::RichText::new(format!(
                                                    "{}{} ({}x{}) on {}",
                                                    win.title, mapped_label, win.width, win.height, win.display
                                                ))
                                                .size(11.0 * scale)
                                                .color(colors::TEXT_PRIMARY),
                                            );

                                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                                // Move buttons for each display
                                                for disp in &self.wm_displays {
                                                    let btn_label = format!("{}▪", disp.name);
                                                    if ui.add(
                                                        egui::Button::new(
                                                            egui::RichText::new(&btn_label)
                                                                .size(10.0 * scale)
                                                                .color(colors::TEXT_SECONDARY),
                                                        )
                                                        .fill(colors::BG_INPUT)
                                                        .corner_radius(4.0)
                                                    ).on_hover_text(format!("Fullscreen on {}", disp.name))
                                                    .clicked()
                                                    {
                                                        actions.push((win.id, disp.name.clone(), "full"));
                                                    }
                                                }

                                                // Side-by-side buttons (only when 2+ displays)
                                                if self.wm_displays.len() >= 1 {
                                                    let first_disp = &self.wm_displays[0].name;
                                                    if ui.add(
                                                        egui::Button::new(
                                                            egui::RichText::new("L½")
                                                                .size(10.0 * scale)
                                                                .color(colors::TEXT_SECONDARY),
                                                        )
                                                        .fill(colors::BG_INPUT)
                                                        .corner_radius(4.0)
                                                    ).on_hover_text(format!("Left half on {}", first_disp))
                                                    .clicked()
                                                    {
                                                        actions.push((win.id, first_disp.clone(), "left"));
                                                    }
                                                    if ui.add(
                                                        egui::Button::new(
                                                            egui::RichText::new("R½")
                                                                .size(10.0 * scale)
                                                                .color(colors::TEXT_SECONDARY),
                                                        )
                                                        .fill(colors::BG_INPUT)
                                                        .corner_radius(4.0)
                                                    ).on_hover_text(format!("Right half on {}", first_disp))
                                                    .clicked()
                                                    {
                                                        actions.push((win.id, first_disp.clone(), "right"));
                                                    }
                                                }
                                            });
                                        });
                                        ui.add_space(2.0 * scale);
                                    }

                                    // Queue actions for main.rs to execute via WindowManager
                                    for (wid, display, mode) in actions {
                                        self.wm_status = Some(format!("Moving {} → {} ({})", wid, display, mode));
                                        self.wm_status_until = Some(Instant::now() + Duration::from_secs(3));
                                        self.wm_pending_actions.push((wid, display, mode.to_string()));
                                    }
                                });
                        }

                        // Status message
                        if let Some(ref status) = self.wm_status {
                            if self.wm_status_until.map_or(false, |t| Instant::now() < t) {
                                ui.add_space(8.0 * scale);
                                ui.label(
                                    egui::RichText::new(status)
                                        .size(11.0 * scale)
                                        .color(colors::ACCENT_GREEN),
                                );
                            } else {
                                self.wm_status = None;
                            }
                        }
                    });
                });
        }

        if self.active_tab == ToolTab::Shaders {
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE)
                .show(ctx, |ui| {
                    let bottom_reserved = (96.0 * scale).clamp(84.0, 260.0);
                    let panel_w = (ui.available_width() - 20.0).max(440.0);
                    let panel_h = (ui.available_height() - bottom_reserved).max(300.0);

                    let panel_rect = egui::Rect::from_min_size(
                        ui.cursor().min + egui::vec2(10.0, 4.0),
                        egui::vec2(panel_w, panel_h),
                    );
                    let painter = ui.painter();
                    draw_glass_panel(painter, panel_rect, 14.0 * scale, true);

                    ui.allocate_new_ui(egui::UiBuilder::new().max_rect(panel_rect.shrink(12.0 * scale)), |ui| {
                        // Header
                        ui.vertical_centered(|ui| {
                            ui.add_space(4.0 * scale);
                            ui.label(egui::RichText::new("GAMER").size((11.0 * scale).clamp(10.0, 30.0)).color(colors::ACCENT_BLUE).strong());
                            ui.label(egui::RichText::new("Shader Library").size((22.0 * scale).clamp(18.0, 56.0)).color(colors::TEXT_PRIMARY).strong());
                            ui.label(egui::RichText::new("Browse and apply shader presets").size((11.0 * scale).clamp(10.0, 30.0)).color(colors::TEXT_DIM));
                        });
                        ui.add_space(6.0 * scale);

                        // Path bar with navigation
                        let current_display = ellipsize_middle(&self.shader_current_dir, 60);
                        let path_bar_h = (28.0 * scale).clamp(26.0, 70.0);
                        let path_rect = egui::Rect::from_min_size(
                            ui.cursor().min,
                            egui::vec2(ui.available_width(), path_bar_h),
                        );
                        let p = ui.painter();
                        p.rect_filled(path_rect, 6.0 * scale, colors::BG_INPUT);
                        p.rect_stroke(path_rect, 6.0 * scale, egui::Stroke::new(1.0, colors::BORDER_SUBTLE), egui::StrokeKind::Inside);

                        ui.allocate_new_ui(egui::UiBuilder::new().max_rect(path_rect.shrink2(egui::vec2(8.0 * scale, 2.0))), |ui| {
                            ui.horizontal_centered(|ui| {
                                // Up button
                                if ui.add(egui::Button::new(
                                    egui::RichText::new("\u{25C0}").size((12.0 * scale).clamp(11.0, 34.0)).color(colors::ACCENT_BLUE)
                                ).fill(egui::Color32::TRANSPARENT).frame(false)).clicked() {
                                    if let Some(parent) = std::path::Path::new(&self.shader_current_dir).parent() {
                                        let parent = parent.to_string_lossy().to_string();
                                        if parent.starts_with(&self.shader_root_dir) {
                                            self.shader_current_dir = parent;
                                        }
                                    }
                                }
                                ui.label(egui::RichText::new(&current_display).size((11.0 * scale).clamp(10.0, 30.0)).color(colors::TEXT_SECONDARY));
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if ui.add(egui::Button::new(
                                        egui::RichText::new("Refresh").size((10.0 * scale).clamp(10.0, 28.0)).color(colors::ACCENT_BLUE)
                                    ).fill(egui::Color32::TRANSPARENT).frame(false)).clicked() {
                                        self.shader_presets = discover_shader_presets();
                                        self.shader_list_dir.clear();
                                    }
                                });
                            });
                        });
                        ui.add_space(path_bar_h + 6.0 * scale);

                        // Two-pane browser
                        self.refresh_shader_listing_if_needed();
                        let dirs = self.shader_dirs.clone();
                        let files = self.shader_files.clone();
                        let item_text_size = (14.0 * scale).clamp(14.0, 38.0);
                        let row_h = (24.0 * scale).clamp(30.0, 60.0);
                        let col_gap = (6.0 * scale).clamp(6.0, 14.0);
                        // Fixed footer height — no shifting when selection changes
                        let footer_h = (55.0 * scale).clamp(60.0, 160.0);
                        let available = ui.available_height();
                        let list_h = (available - footer_h - 12.0 * scale).max(available * 0.4);

                        ui.allocate_ui_with_layout(
                            egui::vec2(ui.available_width(), list_h),
                            egui::Layout::left_to_right(egui::Align::Min),
                            |ui| {
                                let col_w = ((ui.available_width() - col_gap) / 2.0).max(180.0);

                                // Folders pane
                                let folder_rect = egui::Rect::from_min_size(ui.cursor().min, egui::vec2(col_w, list_h));
                                let p = ui.painter();
                                p.rect_filled(folder_rect, 8.0 * scale, colors::BG_CARD);
                                p.rect_stroke(folder_rect, 8.0 * scale, egui::Stroke::new(1.0, colors::BORDER_SUBTLE), egui::StrokeKind::Inside);

                                ui.allocate_new_ui(egui::UiBuilder::new().max_rect(folder_rect.shrink(6.0 * scale)).layout(egui::Layout::top_down(egui::Align::Min)), |ui| {
                                    ui.label(egui::RichText::new("Folders").size((12.0 * scale).clamp(11.0, 34.0)).color(colors::ACCENT_BLUE).strong());
                                    ui.add_space(3.0 * scale);
                                    egui::ScrollArea::vertical()
                                        .id_salt("shader_dirs_scroll")
                                        .max_height(list_h - 30.0 * scale)
                                        .show(ui, |ui| {
                                            for d in &dirs {
                                                let dir_name = std::path::Path::new(d)
                                                    .file_name()
                                                    .and_then(|s| s.to_str())
                                                    .unwrap_or(d);
                                                let is_current = *d == self.shader_current_dir;
                                                let text_color = if is_current { colors::ACCENT_BLUE } else { colors::TEXT_PRIMARY };
                                                let bg = if is_current { colors::TAB_ACTIVE_BG } else { egui::Color32::TRANSPARENT };
                                                if ui.add_sized(
                                                    [ui.available_width(), row_h],
                                                    egui::Button::new(
                                                        egui::RichText::new(dir_name).size(item_text_size).color(text_color),
                                                    ).fill(bg).corner_radius(egui::CornerRadius::from(4.0 * scale)),
                                                ).clicked() {
                                                    self.shader_current_dir = d.clone();
                                                    self.shader_selected = None;
                                                }
                                            }
                                        });
                                });
                                ui.add_space(col_w + col_gap);

                                // Presets pane
                                let presets_rect = egui::Rect::from_min_size(
                                    egui::pos2(folder_rect.right() + col_gap, folder_rect.top()),
                                    egui::vec2(col_w, list_h),
                                );
                                let p = ui.painter();
                                p.rect_filled(presets_rect, 8.0 * scale, colors::BG_CARD);
                                p.rect_stroke(presets_rect, 8.0 * scale, egui::Stroke::new(1.0, colors::BORDER_SUBTLE), egui::StrokeKind::Inside);

                                ui.allocate_new_ui(egui::UiBuilder::new().max_rect(presets_rect.shrink(6.0 * scale)).layout(egui::Layout::top_down(egui::Align::Min)), |ui| {
                                    ui.label(egui::RichText::new("Presets").size((12.0 * scale).clamp(11.0, 34.0)).color(colors::ACCENT_PINK).strong());
                                    ui.add_space(3.0 * scale);
                                    egui::ScrollArea::vertical()
                                        .id_salt("shader_files_scroll")
                                        .max_height(list_h - 30.0 * scale)
                                        .show(ui, |ui| {
                                            for file in &files {
                                                let selected = self.shader_selected.as_ref().map(|s| s == file).unwrap_or(false);
                                                let label = std::path::Path::new(file)
                                                    .file_name()
                                                    .and_then(|s| s.to_str())
                                                    .unwrap_or(file)
                                                    .to_string();
                                                let text_color = if selected { colors::ACCENT_PINK } else { colors::TEXT_PRIMARY };
                                                let bg = if selected { egui::Color32::from_rgba_premultiplied(255, 46, 151, 30) } else { egui::Color32::TRANSPARENT };
                                                if ui.add_sized(
                                                    [ui.available_width(), row_h],
                                                    egui::Button::new(
                                                        egui::RichText::new(&label).size(item_text_size).color(text_color),
                                                    ).fill(bg).corner_radius(egui::CornerRadius::from(4.0 * scale)),
                                                ).clicked() {
                                                    self.shader_selected = Some(file.clone());
                                                }
                                            }
                                        });
                                });
                            },
                        );

                        ui.add_space(8.0 * scale);

                        // Action footer — proper card below panes
                        let footer_rect = egui::Rect::from_min_size(
                            ui.cursor().min,
                            egui::vec2(ui.available_width(), footer_h),
                        );
                        let p = ui.painter();
                        p.rect_filled(footer_rect, 8.0 * scale, colors::BG_CARD);
                        p.rect_stroke(footer_rect, 8.0 * scale, egui::Stroke::new(1.0, colors::BORDER_SUBTLE), egui::StrokeKind::Inside);

                        ui.allocate_new_ui(egui::UiBuilder::new().max_rect(footer_rect.shrink(8.0 * scale)).layout(egui::Layout::top_down(egui::Align::Center)), |ui| {
                            if let Some(selected) = self.shader_selected.clone() {
                                let selected_name = std::path::Path::new(&selected)
                                    .file_name()
                                    .and_then(|s| s.to_str())
                                    .unwrap_or(&selected)
                                    .to_string();

                                // Selected preset name
                                ui.label(egui::RichText::new(ellipsize_middle(&selected_name, 50))
                                    .size((14.0 * scale).clamp(14.0, 38.0)).color(colors::ACCENT_PINK).strong());

                                // Apply buttons row
                                let btn_h = (24.0 * scale).clamp(26.0, 62.0);
                                ui.horizontal(|ui| {
                                    let btn_w = ((ui.available_width() - 24.0 * scale) / 4.0).max(70.0);
                                    let apply = |target: &str, path: &str, status: &mut Option<String>, until: &mut Option<Instant>| {
                                        match write_shader_file(target, path) {
                                            Ok(()) => {
                                                *status = Some(format!("{} shader applied", target));
                                                *until = Some(Instant::now() + Duration::from_secs(4));
                                            }
                                            Err(e) => {
                                                *status = Some(format!("Failed: {}", e));
                                                *until = Some(Instant::now() + Duration::from_secs(4));
                                            }
                                        }
                                    };
                                    if ui.add_sized([btn_w, btn_h], egui::Button::new(
                                        egui::RichText::new("Top").size((12.0 * scale).clamp(11.0, 34.0)).color(colors::TEXT_PRIMARY)
                                    ).fill(colors::BG_INPUT).corner_radius(egui::CornerRadius::from(6.0 * scale))).clicked() {
                                        apply("primary", &selected, &mut self.shader_status, &mut self.shader_status_until);
                                    }
                                    ui.add_space(6.0 * scale);
                                    if ui.add_sized([btn_w, btn_h], egui::Button::new(
                                        egui::RichText::new("Bottom").size((12.0 * scale).clamp(11.0, 34.0)).color(colors::TEXT_PRIMARY)
                                    ).fill(colors::BG_INPUT).corner_radius(egui::CornerRadius::from(6.0 * scale))).clicked() {
                                        apply("secondary", &selected, &mut self.shader_status, &mut self.shader_status_until);
                                    }
                                    ui.add_space(6.0 * scale);
                                    if ui.add_sized([btn_w, btn_h], egui::Button::new(
                                        egui::RichText::new("Both").size((12.0 * scale).clamp(11.0, 34.0)).color(egui::Color32::from_rgb(10, 11, 20)).strong()
                                    ).fill(colors::ACCENT_BLUE).corner_radius(egui::CornerRadius::from(6.0 * scale))).clicked() {
                                        match write_shader_file("primary", &selected) {
                                            Ok(()) => {
                                                let _ = write_shader_file("secondary", &selected);
                                                self.shader_status = Some("Applied to both screens".into());
                                                self.shader_status_until = Some(Instant::now() + Duration::from_secs(4));
                                            }
                                            Err(e) => {
                                                self.shader_status = Some(format!("Failed: {}", e));
                                                self.shader_status_until = Some(Instant::now() + Duration::from_secs(4));
                                            }
                                        }
                                    }
                                    ui.add_space(6.0 * scale);
                                    if ui.add_sized([btn_w, btn_h], egui::Button::new(
                                        egui::RichText::new("Clear").size((12.0 * scale).clamp(11.0, 34.0)).color(colors::ACCENT_RED)
                                    ).fill(colors::BG_INPUT).corner_radius(egui::CornerRadius::from(6.0 * scale))).clicked() {
                                        let ok1 = write_shader_file("primary", "").is_ok();
                                        let ok2 = write_shader_file("secondary", "").is_ok();
                                        self.shader_status = Some(if ok1 && ok2 { "Cleared all shaders".into() } else { "Failed to clear".into() });
                                        self.shader_status_until = Some(Instant::now() + Duration::from_secs(4));
                                    }
                                });
                            } else {
                                ui.vertical_centered(|ui| {
                                    ui.add_space(10.0 * scale);
                                    ui.label(egui::RichText::new("Select a shader preset above").size((13.0 * scale).clamp(12.0, 36.0)).color(colors::TEXT_DIM));
                                    ui.add_space(8.0 * scale);
                                    let btn_h = (24.0 * scale).clamp(26.0, 62.0);
                                    if ui.add_sized([120.0 * scale, btn_h], egui::Button::new(
                                        egui::RichText::new("Clear All").size((12.0 * scale).clamp(11.0, 34.0)).color(colors::ACCENT_RED)
                                    ).fill(colors::BG_INPUT).corner_radius(egui::CornerRadius::from(6.0 * scale))).clicked() {
                                        let ok1 = write_shader_file("primary", "").is_ok();
                                        let ok2 = write_shader_file("secondary", "").is_ok();
                                        self.shader_status = Some(if ok1 && ok2 { "Cleared all shaders".into() } else { "Failed to clear".into() });
                                        self.shader_status_until = Some(Instant::now() + Duration::from_secs(4));
                                    }
                                });
                            }

                            // Status toast
                            if let Some(until) = self.shader_status_until {
                                if Instant::now() > until {
                                    self.shader_status = None;
                                    self.shader_status_until = None;
                                }
                            }
                            if let Some(status) = &self.shader_status {
                                ui.add_space(4.0 * scale);
                                ui.vertical_centered(|ui| {
                                    ui.label(egui::RichText::new(status).size((11.0 * scale).clamp(10.0, 30.0)).color(colors::ACCENT_GREEN));
                                });
                            }
                        });
                    });
                });
        }

        if self.active_tab == ToolTab::Hotkeys {
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE)
                .show(ctx, |ui| {
                    let bottom_reserved = (96.0 * scale).clamp(84.0, 260.0);
                    let panel_w = (ui.available_width() - 20.0).max(440.0);
                    let panel_h = (ui.available_height() - bottom_reserved).max(300.0);

                    let panel_rect = egui::Rect::from_min_size(
                        ui.cursor().min + egui::vec2(10.0, 4.0),
                        egui::vec2(panel_w, panel_h),
                    );
                    let painter = ui.painter();
                    draw_glass_panel(painter, panel_rect, 14.0 * scale, true);

                    ui.allocate_new_ui(egui::UiBuilder::new().max_rect(panel_rect.shrink(16.0 * scale)), |ui| {
                        ui.vertical_centered(|ui| {
                            ui.add_space(6.0 * scale);
                            ui.label(egui::RichText::new("GAMER").size((11.0 * scale).clamp(10.0, 30.0)).color(colors::ACCENT_BLUE).strong());
                            ui.label(egui::RichText::new("Hotkey Pad").size((22.0 * scale).clamp(18.0, 56.0)).color(colors::TEXT_PRIMARY).strong());
                            ui.label(egui::RichText::new("Tap to send emulator shortcuts").size((11.0 * scale).clamp(10.0, 30.0)).color(colors::TEXT_DIM));
                        });
                        ui.add_space(10.0 * scale);

                        // Target window selector + reload
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("Target:").size((11.0 * scale).clamp(10.0, 30.0)).color(colors::TEXT_DIM));
                            ui.add_space(4.0 * scale);

                            // Dropdown of visible windows from wm_windows
                            let current_label = if self.hotkey_target.is_empty() { "(auto)" } else { &self.hotkey_target };
                            egui::ComboBox::from_id_salt("hotkey_target_combo")
                                .selected_text(egui::RichText::new(ellipsize_middle(current_label, 30)).size((11.0 * scale).clamp(10.0, 30.0)).color(colors::ACCENT_BLUE))
                                .width(panel_w * 0.45)
                                .show_ui(ui, |ui| {
                                    // Auto option (uses default emulator search)
                                    if ui.selectable_label(self.hotkey_target.is_empty(), "(auto)").clicked() {
                                        self.hotkey_target = String::new();
                                    }
                                    ui.separator();
                                    // Actual visible windows from wm_windows
                                    for win in &self.wm_windows {
                                        let selected = self.hotkey_target == win.title;
                                        let label = format!("{} ({}x{})", ellipsize_middle(&win.title, 35), win.width, win.height);
                                        if ui.selectable_label(selected, label).clicked() {
                                            self.hotkey_target = win.title.clone();
                                        }
                                    }
                                    if self.wm_windows.is_empty() {
                                        ui.label(egui::RichText::new("No windows detected").size((10.0 * scale).clamp(9.0, 28.0)).color(colors::TEXT_DIM));
                                    }
                                });

                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.add(egui::Button::new(egui::RichText::new("Reload").size((10.0 * scale).clamp(9.0, 28.0)).color(colors::ACCENT_BLUE)).fill(egui::Color32::TRANSPARENT)).clicked() {
                                    self.hotkeys = load_hotkeys_from_file(&self.hotkeys_file);
                                }
                            });
                        });
                        ui.add_space(4.0 * scale);
                        ui.label(egui::RichText::new(ellipsize_middle(&self.hotkeys_file, 50)).size((9.0 * scale).clamp(8.0, 24.0)).color(colors::TEXT_DIM));
                        ui.add_space(8.0 * scale);

                        // Keycap-style hotkey grid
                        let columns = if panel_w > 1000.0 { 4 } else if panel_w > 700.0 { 3 } else { 2 };
                        let btn_h = (70.0 * scale).clamp(56.0, 180.0);
                        let btn_w = ((ui.available_width() - (columns as f32 - 1.0) * 10.0 * scale) / columns as f32).max(140.0);

                        egui::ScrollArea::vertical().show(ui, |ui| {
                            let mut col_idx = 0usize;
                            egui::Grid::new("hotkeys_grid")
                                .num_columns(columns)
                                .spacing(egui::vec2(10.0 * scale, 10.0 * scale))
                                .show(ui, |ui| {
                                    for (title, keyseq) in &self.hotkeys {
                                        let (rect, response) = ui.allocate_exact_size(egui::vec2(btn_w, btn_h), egui::Sense::click());
                                        let p = ui.painter_at(rect);
                                        draw_keycap(&p, rect, response.hovered(), scale);

                                        // Title
                                        p.text(
                                            egui::pos2(rect.center().x, rect.top() + 12.0 * scale),
                                            egui::Align2::CENTER_TOP,
                                            title,
                                            egui::FontId::proportional((13.0 * scale).clamp(11.0, 34.0)),
                                            colors::TEXT_PRIMARY,
                                        );
                                        // Key sequence in accent color
                                        p.text(
                                            egui::pos2(rect.center().x, rect.bottom() - 12.0 * scale),
                                            egui::Align2::CENTER_BOTTOM,
                                            keyseq,
                                            egui::FontId::proportional((11.0 * scale).clamp(10.0, 30.0)),
                                            colors::ACCENT_BLUE,
                                        );

                                        if response.clicked() {
                                            match send_hotkey_to_emulator(keyseq, &self.hotkey_target) {
                                                Ok(()) => { self.hotkeys_status = Some(format!("Sent: {title}")); }
                                                Err(e) => { self.hotkeys_status = Some(format!("Failed: {e}")); }
                                            }
                                            self.hotkeys_status_until = Some(Instant::now() + Duration::from_secs(3));
                                        }
                                        col_idx += 1;
                                        if col_idx % columns == 0 { ui.end_row(); }
                                    }
                                });
                        });

                        if self.hotkeys.is_empty() {
                            ui.add_space(20.0 * scale);
                            ui.label(egui::RichText::new("No hotkeys configured").size(13.0 * scale).color(colors::TEXT_DIM));
                        }
                        // Status toast
                        if let Some(until) = self.hotkeys_status_until {
                            if Instant::now() > until { self.hotkeys_status = None; self.hotkeys_status_until = None; }
                        }
                        if let Some(status) = &self.hotkeys_status {
                            ui.add_space(6.0 * scale);
                            ui.label(egui::RichText::new(status).size(11.0 * scale).color(colors::ACCENT_GREEN));
                        }
                    });
                });
        }

        if self.active_tab == ToolTab::Faketime {
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE)
                .show(ctx, |ui| {
                    let bottom_reserved = (96.0 * scale).clamp(84.0, 260.0);
                    let panel_w = (ui.available_width() - 20.0).max(440.0);
                    let panel_h = (ui.available_height() - bottom_reserved).max(300.0);

                    let panel_rect = egui::Rect::from_min_size(
                        ui.cursor().min + egui::vec2(10.0, 4.0),
                        egui::vec2(panel_w, panel_h),
                    );
                    let painter = ui.painter();
                    draw_glass_panel(painter, panel_rect, 14.0 * scale, true);

                    ui.allocate_new_ui(egui::UiBuilder::new().max_rect(panel_rect.shrink(16.0 * scale)), |ui| {
                        ui.vertical_centered(|ui| {
                            ui.add_space(6.0 * scale);
                            ui.label(egui::RichText::new("GAMER").size((11.0 * scale).clamp(10.0, 30.0)).color(colors::ACCENT_BLUE).strong());
                            ui.label(egui::RichText::new("Time Warp").size((22.0 * scale).clamp(18.0, 56.0)).color(colors::TEXT_PRIMARY).strong());
                            ui.label(egui::RichText::new("In-game clock control (libfaketime)").size((11.0 * scale).clamp(10.0, 30.0)).color(colors::TEXT_DIM));
                        });
                        ui.add_space(12.0 * scale);

                        // Validate values
                        self.faketime_month = self.faketime_month.clamp(1, 12);
                        let max_days = days_in_month(self.faketime_year, self.faketime_month);
                        self.faketime_day = self.faketime_day.clamp(1, max_days);
                        self.faketime_hour = self.faketime_hour.min(23);
                        self.faketime_minute = self.faketime_minute.min(59);
                        self.faketime_second = self.faketime_second.min(59);

                        let formatted = format_faketime_parts(
                            self.faketime_year, self.faketime_month, self.faketime_day,
                            self.faketime_hour, self.faketime_minute, self.faketime_second,
                        );

                        // Retro digital clock display
                        let clock_h = (64.0 * scale).clamp(48.0, 100.0);
                        let clock_w = (panel_w - 32.0 * scale).min(400.0 * scale);
                        let (clock_rect, _) = ui.allocate_exact_size(egui::vec2(clock_w, clock_h), egui::Sense::hover());
                        draw_digital_clock(&ui.painter_at(clock_rect), clock_rect, &formatted, scale);

                        ui.add_space(16.0 * scale);

                        // Touch-friendly +/- spinner helper
                        let btn_fs = (14.0 * scale).clamp(14.0, 36.0);
                        let val_fs = (16.0 * scale).clamp(15.0, 40.0);
                        let spin_btn_w = (32.0 * scale).clamp(36.0, 80.0);
                        let spin_btn_h = (26.0 * scale).clamp(30.0, 64.0);
                        let spin_gap = (4.0 * scale).clamp(2.0, 10.0);

                        let spinner = |ui: &mut egui::Ui, label: &str, val: &mut i32, min: i32, max: i32, width: f32| {
                            ui.vertical(|ui| {
                                ui.set_width(width);
                                ui.label(egui::RichText::new(label).size((11.0 * scale).clamp(10.0, 28.0)).color(colors::TEXT_DIM));
                                ui.add_space(spin_gap);
                                ui.horizontal(|ui| {
                                    if ui.add_sized(
                                        [spin_btn_w, spin_btn_h],
                                        egui::Button::new(egui::RichText::new("\u{2212}").size(btn_fs).color(colors::TEXT_PRIMARY))
                                            .fill(colors::BG_INPUT).corner_radius(6.0),
                                    ).clicked() {
                                        *val = if *val <= min { max } else { *val - 1 };
                                    }
                                    ui.add_space(spin_gap);
                                    let display = format!("{:02}", *val);
                                    ui.label(egui::RichText::new(display).size(val_fs).color(colors::TEXT_PRIMARY).strong().family(egui::FontFamily::Monospace));
                                    ui.add_space(spin_gap);
                                    if ui.add_sized(
                                        [spin_btn_w, spin_btn_h],
                                        egui::Button::new(egui::RichText::new("+").size(btn_fs).color(colors::TEXT_PRIMARY))
                                            .fill(colors::BG_INPUT).corner_radius(6.0),
                                    ).clicked() {
                                        *val = if *val >= max { min } else { *val + 1 };
                                    }
                                });
                            });
                        };

                        // Date controls — cast u32 fields through i32 temporaries
                        ui.vertical_centered(|ui| {
                            ui.label(egui::RichText::new("Date").size((13.0 * scale).clamp(12.0, 32.0)).color(colors::TEXT_SECONDARY));
                            ui.add_space(4.0 * scale);
                            let spin_col_w = (90.0 * scale).clamp(100.0, 200.0);
                            let mut month_i = self.faketime_month as i32;
                            let mut day_i = self.faketime_day as i32;
                            ui.horizontal(|ui| {
                                ui.add_space((ui.available_width() - spin_col_w * 3.0).max(0.0) * 0.5);
                                spinner(ui, "Year", &mut self.faketime_year, 1970, 2099, spin_col_w);
                                spinner(ui, "Month", &mut month_i, 1, 12, spin_col_w);
                                spinner(ui, "Day", &mut day_i, 1, max_days as i32, spin_col_w);
                            });
                            self.faketime_month = month_i as u32;
                            self.faketime_day = day_i as u32;
                            ui.add_space(10.0 * scale);
                            ui.label(egui::RichText::new("Time").size((13.0 * scale).clamp(12.0, 32.0)).color(colors::TEXT_SECONDARY));
                            ui.add_space(4.0 * scale);
                            let mut hour_i = self.faketime_hour as i32;
                            let mut min_i = self.faketime_minute as i32;
                            let mut sec_i = self.faketime_second as i32;
                            ui.horizontal(|ui| {
                                ui.add_space((ui.available_width() - spin_col_w * 3.0).max(0.0) * 0.5);
                                spinner(ui, "Hour", &mut hour_i, 0, 23, spin_col_w);
                                spinner(ui, "Min", &mut min_i, 0, 59, spin_col_w);
                                spinner(ui, "Sec", &mut sec_i, 0, 59, spin_col_w);
                            });
                            self.faketime_hour = hour_i as u32;
                            self.faketime_minute = min_i as u32;
                            self.faketime_second = sec_i as u32;
                        });

                        ui.add_space(16.0 * scale);

                        // Action buttons
                        let act_btn_h = (36.0 * scale).clamp(40.0, 80.0);
                        let act_btn_w = (100.0 * scale).clamp(110.0, 220.0);
                        ui.horizontal(|ui| {
                            ui.add_space((ui.available_width() - act_btn_w * 2.0 - 16.0 * scale).max(0.0) * 0.5);
                            if ui.add_sized(
                                [act_btn_w, act_btn_h],
                                egui::Button::new(egui::RichText::new("Reload").size((14.0 * scale).clamp(14.0, 34.0)).color(colors::TEXT_SECONDARY))
                                    .fill(colors::BG_CARD)
                                    .corner_radius(8.0),
                            ).clicked() {
                                if let Some(s) = read_faketime_string(&self.faketime_file)
                                    .or_else(|| read_faketime_from_mtime(&self.faketime_file))
                                {
                                    if let Some((y, m, d, hh, mm, ss)) = parse_faketime_parts(&s) {
                                        self.faketime_year = y;
                                        self.faketime_month = m.clamp(1, 12);
                                        self.faketime_day = d.max(1);
                                        self.faketime_hour = hh.min(23);
                                        self.faketime_minute = mm.min(59);
                                        self.faketime_second = ss.min(59);
                                        self.faketime_status = Some("Loaded from file".into());
                                        self.faketime_status_until = Some(Instant::now() + Duration::from_secs(3));
                                    }
                                }
                            }
                            ui.add_space(12.0 * scale);
                            if ui.add_sized(
                                [act_btn_w, act_btn_h],
                                egui::Button::new(egui::RichText::new("Apply").size((15.0 * scale).clamp(15.0, 36.0)).color(colors::BG_DEEP).strong())
                                    .fill(colors::ACCENT_GREEN)
                                    .corner_radius(8.0),
                            ).clicked() {
                                match write_faketime_follow_file(&self.faketime_file, &formatted) {
                                    Ok(()) => { self.faketime_status = Some(format!("Set → {formatted}")); }
                                    Err(e) => { self.faketime_status = Some(format!("Failed: {e}")); }
                                }
                                self.faketime_status_until = Some(Instant::now() + Duration::from_secs(3));
                            }
                        });

                        // Status toast
                        if let Some(until) = self.faketime_status_until {
                            if Instant::now() > until { self.faketime_status = None; self.faketime_status_until = None; }
                        }
                        if let Some(status) = &self.faketime_status {
                            ui.add_space(8.0 * scale);
                            ui.label(egui::RichText::new(status).size(11.0 * scale).color(colors::ACCENT_GREEN));
                        }
                    });
                });
        }

        // ─── Gamepad Tab ─────────────────────────────────────────────────
        if self.active_tab == ToolTab::Gamepad {
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE.fill(colors::BG_DEEP))
                .show(ctx, |ui| {
                    let avail = ui.available_size();
                    let gp_tab_h = (36.0 * ui_scale).clamp(44.0, 120.0);
                    let gp_tab_pad = (8.0 * ui_scale).clamp(8.0, 24.0);
                    let pad_w = avail.x * 0.94;
                    let pad_h = (avail.y - gp_tab_h - gp_tab_pad - 8.0).max(200.0);
                    let origin = ui.cursor().min + egui::vec2((avail.x - pad_w) * 0.5, 4.0);

                    // Bigger buttons for touch — scale with display
                    let unit = (pad_w.min(pad_h) * 0.085).clamp(36.0, 140.0);
                    let btn_sz = unit;
                    let dpad_sz = unit * 0.95;
                    let shoulder_w = unit * 2.2;
                    let shoulder_h = unit * 0.7;
                    let stick_r = unit * 1.1;

                    // Xbox-style asymmetric layout:
                    // Left stick upper-left, d-pad lower-left
                    // Face buttons upper-right, right stick lower-right
                    let left_cx = origin.x + pad_w * 0.22;
                    let right_cx = origin.x + pad_w * 0.78;
                    let center_x = origin.x + pad_w * 0.5;
                    // Shoulder/trigger row at top
                    let trigger_y = origin.y + pad_h * 0.04;
                    let shoulder_y = origin.y + pad_h * 0.14;
                    // Left stick upper-left, face buttons upper-right
                    let lstick_y = origin.y + pad_h * 0.33;
                    let face_y   = origin.y + pad_h * 0.33;
                    // D-pad lower-left, right stick lower-right
                    let dpad_y   = origin.y + pad_h * 0.60;
                    let rstick_y = origin.y + pad_h * 0.62;
                    // L3/R3 below their respective sticks
                    let l3_y     = origin.y + pad_h * 0.50;
                    let r3_y     = origin.y + pad_h * 0.78;
                    // Select/Start in the center
                    let meta_y   = origin.y + pad_h * 0.88;

                    let panel_rect = egui::Rect::from_min_size(
                        origin - egui::vec2(6.0, 4.0),
                        egui::vec2(pad_w + 12.0, pad_h + 8.0),
                    );
                    draw_glass_panel(ui.painter(), panel_rect, 14.0, true);

                    let states = &mut self.gamepad_button_states;
                    let pending = &mut self.gamepad_pending_presses;

                    // Unified gamepad button helper: style = "rect", "round", or "dpad:up/down/left/right"
                    let mut draw_btn = |ui: &mut egui::Ui, center: egui::Pos2, w: f32, h: f32,
                                       label: &str, color: egui::Color32, btn: GamepadButton, style: &str| {
                        let rect = egui::Rect::from_center_size(center, egui::vec2(w, h));
                        let resp = ui.allocate_rect(rect, egui::Sense::click_and_drag());
                        let idx = btn.index();
                        let is_down = resp.is_pointer_button_down_on();
                        if is_down && !states[idx] { states[idx] = true; pending.push((btn, true)); }
                        else if !is_down && states[idx] { states[idx] = false; pending.push((btn, false)); }
                        let fill = if is_down { color.linear_multiply(0.7) } else { colors::BG_CARD };
                        let stroke_c = if is_down { color } else { color.linear_multiply(0.4) };
                        let p = ui.painter();

                        if style == "round" {
                            let r = w * 0.5;
                            p.circle_filled(center, r, fill);
                            p.circle_stroke(center, r, egui::Stroke::new(2.5, stroke_c));
                            if is_down { p.circle_stroke(center, r + 4.0, egui::Stroke::new(1.5, color.linear_multiply(0.3))); }
                            let ts = (r * 0.75).clamp(14.0, 42.0);
                            p.text(center, egui::Align2::CENTER_CENTER, label, egui::FontId::new(ts, egui::FontFamily::Proportional),
                                if is_down { colors::TEXT_PRIMARY } else { color });
                        } else if style.starts_with("dpad:") {
                            let dir = &style[5..];
                            let rnd = w.min(h) * 0.15;
                            p.rect_filled(rect, rnd, fill);
                            p.rect_stroke(rect, rnd, egui::Stroke::new(2.0, stroke_c), egui::StrokeKind::Inside);
                            let a = w.min(h) * 0.3;
                            let ac = if is_down { colors::TEXT_PRIMARY } else { color };
                            let pts = match dir {
                                "up"    => vec![egui::pos2(center.x, center.y - a), egui::pos2(center.x - a, center.y + a * 0.5), egui::pos2(center.x + a, center.y + a * 0.5)],
                                "down"  => vec![egui::pos2(center.x, center.y + a), egui::pos2(center.x - a, center.y - a * 0.5), egui::pos2(center.x + a, center.y - a * 0.5)],
                                "left"  => vec![egui::pos2(center.x - a, center.y), egui::pos2(center.x + a * 0.5, center.y - a), egui::pos2(center.x + a * 0.5, center.y + a)],
                                "right" => vec![egui::pos2(center.x + a, center.y), egui::pos2(center.x - a * 0.5, center.y - a), egui::pos2(center.x - a * 0.5, center.y + a)],
                                _ => vec![],
                            };
                            if !pts.is_empty() { p.add(egui::Shape::convex_polygon(pts, ac, egui::Stroke::NONE)); }
                        } else {
                            // rect style
                            let rnd = (w.min(h) * 0.25).min(16.0);
                            p.rect_filled(rect, rnd, fill);
                            p.rect_stroke(rect, rnd, egui::Stroke::new(2.0, stroke_c), egui::StrokeKind::Inside);
                            if is_down { p.rect_stroke(rect.expand(3.0), rnd + 3.0, egui::Stroke::new(1.5, color.linear_multiply(0.3)), egui::StrokeKind::Outside); }
                            let ts = (w * 0.32).clamp(14.0, 42.0);
                            p.text(center, egui::Align2::CENTER_CENTER, label, egui::FontId::new(ts, egui::FontFamily::Proportional),
                                if is_down { colors::TEXT_PRIMARY } else { color });
                        }
                    };

                    // ── L2/R2 triggers ──
                    draw_btn(ui, egui::pos2(left_cx, trigger_y), shoulder_w, shoulder_h, "L2", colors::TEXT_DIM, GamepadButton::L2, "rect");
                    draw_btn(ui, egui::pos2(right_cx, trigger_y), shoulder_w, shoulder_h, "R2", colors::TEXT_DIM, GamepadButton::R2, "rect");

                    // ── L/R bumpers ──
                    draw_btn(ui, egui::pos2(left_cx, shoulder_y), shoulder_w, shoulder_h, "L", colors::TEXT_SECONDARY, GamepadButton::L, "rect");
                    draw_btn(ui, egui::pos2(right_cx, shoulder_y), shoulder_w, shoulder_h, "R", colors::TEXT_SECONDARY, GamepadButton::R, "rect");

                    // ── Face buttons: upper-right (diamond layout) ──
                    let face_gap = btn_sz * 1.15;
                    let face_d = btn_sz * 1.08;
                    draw_btn(ui, egui::pos2(right_cx, face_y + face_gap), face_d, face_d, "A", colors::ACCENT_GREEN, GamepadButton::A, "round");
                    draw_btn(ui, egui::pos2(right_cx + face_gap, face_y), face_d, face_d, "B", colors::ACCENT_RED, GamepadButton::B, "round");
                    draw_btn(ui, egui::pos2(right_cx - face_gap, face_y), face_d, face_d, "X", colors::ACCENT_BLUE, GamepadButton::X, "round");
                    draw_btn(ui, egui::pos2(right_cx, face_y - face_gap), face_d, face_d, "Y", colors::ACCENT_AMBER, GamepadButton::Y, "round");

                    // ── D-pad: lower-left (cross layout) ──
                    let dpad_gap = dpad_sz * 1.1;
                    draw_btn(ui, egui::pos2(left_cx, dpad_y - dpad_gap), dpad_sz, dpad_sz, "", colors::TEXT_SECONDARY, GamepadButton::DpadUp, "dpad:up");
                    draw_btn(ui, egui::pos2(left_cx, dpad_y + dpad_gap), dpad_sz, dpad_sz, "", colors::TEXT_SECONDARY, GamepadButton::DpadDown, "dpad:down");
                    draw_btn(ui, egui::pos2(left_cx - dpad_gap, dpad_y), dpad_sz, dpad_sz, "", colors::TEXT_SECONDARY, GamepadButton::DpadLeft, "dpad:left");
                    draw_btn(ui, egui::pos2(left_cx + dpad_gap, dpad_y), dpad_sz, dpad_sz, "", colors::TEXT_SECONDARY, GamepadButton::DpadRight, "dpad:right");
                    ui.painter().circle_filled(egui::pos2(left_cx, dpad_y), dpad_sz * 0.3, colors::BG_INPUT);

                    // ── Analog sticks ──
                    let axis_max = crate::gamepad::AXIS_MAX as f32;
                    // Left stick
                    {
                        let sc = egui::pos2(left_cx, lstick_y);
                        let resp = ui.allocate_rect(egui::Rect::from_center_size(sc, egui::vec2(stick_r * 2.2, stick_r * 2.2)), egui::Sense::click_and_drag());
                        let p = ui.painter();
                        p.circle_filled(sc, stick_r * 1.1, colors::BG_INPUT);
                        p.circle_stroke(sc, stick_r * 1.1, egui::Stroke::new(2.0, colors::BORDER_SUBTLE));
                        if resp.dragged() {
                            let d = resp.drag_delta();
                            self.gamepad_left_stick.0 = (self.gamepad_left_stick.0 + d.x / stick_r).clamp(-1.0, 1.0);
                            self.gamepad_left_stick.1 = (self.gamepad_left_stick.1 + d.y / stick_r).clamp(-1.0, 1.0);
                            self.gamepad_pending_axes.push((StickAxis::LeftX, (self.gamepad_left_stick.0 * axis_max) as i32));
                            self.gamepad_pending_axes.push((StickAxis::LeftY, (self.gamepad_left_stick.1 * axis_max) as i32));
                        } else if self.gamepad_left_stick != (0.0, 0.0) {
                            self.gamepad_left_stick = (0.0, 0.0);
                            self.gamepad_pending_axes.push((StickAxis::LeftX, 0));
                            self.gamepad_pending_axes.push((StickAxis::LeftY, 0));
                        }
                        let thumb = sc + egui::vec2(self.gamepad_left_stick.0 * stick_r * 0.6, self.gamepad_left_stick.1 * stick_r * 0.6);
                        let p = ui.painter();
                        p.circle_filled(thumb, stick_r * 0.5, colors::BG_PANEL_LIGHT);
                        p.circle_stroke(thumb, stick_r * 0.5, egui::Stroke::new(2.0, colors::ACCENT_BLUE.linear_multiply(0.5)));
                    }
                    // Right stick
                    {
                        let sc = egui::pos2(right_cx, rstick_y);
                        let resp = ui.allocate_rect(egui::Rect::from_center_size(sc, egui::vec2(stick_r * 2.2, stick_r * 2.2)), egui::Sense::click_and_drag());
                        let p = ui.painter();
                        p.circle_filled(sc, stick_r * 1.1, colors::BG_INPUT);
                        p.circle_stroke(sc, stick_r * 1.1, egui::Stroke::new(2.0, colors::BORDER_SUBTLE));
                        if resp.dragged() {
                            let d = resp.drag_delta();
                            self.gamepad_right_stick.0 = (self.gamepad_right_stick.0 + d.x / stick_r).clamp(-1.0, 1.0);
                            self.gamepad_right_stick.1 = (self.gamepad_right_stick.1 + d.y / stick_r).clamp(-1.0, 1.0);
                            self.gamepad_pending_axes.push((StickAxis::RightX, (self.gamepad_right_stick.0 * axis_max) as i32));
                            self.gamepad_pending_axes.push((StickAxis::RightY, (self.gamepad_right_stick.1 * axis_max) as i32));
                        } else if self.gamepad_right_stick != (0.0, 0.0) {
                            self.gamepad_right_stick = (0.0, 0.0);
                            self.gamepad_pending_axes.push((StickAxis::RightX, 0));
                            self.gamepad_pending_axes.push((StickAxis::RightY, 0));
                        }
                        let thumb = sc + egui::vec2(self.gamepad_right_stick.0 * stick_r * 0.6, self.gamepad_right_stick.1 * stick_r * 0.6);
                        let p = ui.painter();
                        p.circle_filled(thumb, stick_r * 0.5, colors::BG_PANEL_LIGHT);
                        p.circle_stroke(thumb, stick_r * 0.5, egui::Stroke::new(2.0, colors::ACCENT_PINK.linear_multiply(0.5)));
                    }

                    // ── L3/R3 ──
                    let l3r3_sz = btn_sz * 0.6;
                    draw_btn(ui, egui::pos2(left_cx, l3_y), l3r3_sz, l3r3_sz, "L3", colors::TEXT_DIM, GamepadButton::L3, "rect");
                    draw_btn(ui, egui::pos2(right_cx, r3_y), l3r3_sz, l3r3_sz, "R3", colors::TEXT_DIM, GamepadButton::R3, "rect");

                    // ── Start / Select ──
                    let meta_w = btn_sz * 1.6;
                    let meta_h = btn_sz * 0.65;
                    let meta_gap = meta_w * 0.7;
                    draw_btn(ui, egui::pos2(center_x - meta_gap, meta_y), meta_w, meta_h, "SEL", colors::TEXT_DIM, GamepadButton::Select, "rect");
                    draw_btn(ui, egui::pos2(center_x + meta_gap, meta_y), meta_w, meta_h, "START", colors::TEXT_DIM, GamepadButton::Start, "rect");

                    ctx.request_repaint();
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

        // Compact floating zoom controls (Crop tab only)
        if self.active_tab == ToolTab::Crop {
            let btn_sz = (44.0 * scale).clamp(36.0, 110.0);
            egui::Area::new(egui::Id::new("quick_zoom_controls"))
                .anchor(egui::Align2::RIGHT_CENTER, egui::vec2(-16.0, 0.0))
                .order(egui::Order::Foreground)
                .interactable(true)
                .show(ctx, |ui| {
                    egui::Frame::NONE
                        .fill(colors::BG_PANEL)
                        .corner_radius(egui::CornerRadius::from(10.0 * scale))
                        .stroke(egui::Stroke::new(1.0, colors::BORDER_SUBTLE))
                        .inner_margin(egui::Margin::symmetric((6.0 * scale) as i8, (8.0 * scale) as i8))
                        .show(ui, |ui| {
                            ui.vertical_centered(|ui| {
                                let zoom_btn = |ui: &mut egui::Ui, label: &str, size: f32| -> bool {
                                    ui.add_sized(
                                        [btn_sz, btn_sz],
                                        egui::Button::new(egui::RichText::new(label).size(size).color(colors::TEXT_PRIMARY))
                                            .fill(egui::Color32::TRANSPARENT)
                                            .corner_radius(8.0),
                                    ).clicked()
                                };
                                if zoom_btn(ui, "+", 20.0 * scale) {
                                    self.zoom_level = (self.zoom_level * 1.2).clamp(1.0, 8.0);
                                }
                                // Zoom level display
                                ui.label(egui::RichText::new(format!("{:.1}x", self.zoom_level)).size(11.0 * scale).color(colors::ACCENT_BLUE));
                                if zoom_btn(ui, "−", 20.0 * scale) {
                                    self.zoom_level = (self.zoom_level / 1.2).clamp(1.0, 8.0);
                                }
                                ui.add_space(4.0 * scale);
                                if zoom_btn(ui, "1:1", 12.0 * scale) {
                                    self.reset_view();
                                }
                            });
                        });
                });
        }

        // ─── Bottom Navigation Bar ─────────────────────────────────────
        // Bigger pill-shaped tab bar with drawn icons + label
        let tab_h = (36.0 * scale).clamp(44.0, 120.0);
        let tab_w = (44.0 * scale).clamp(52.0, 130.0);
        let tab_gap = (2.0 * scale).clamp(2.0, 8.0);
        let tab_pad = (8.0 * scale).clamp(8.0, 24.0);

        egui::Area::new(egui::Id::new("feature_tab_controls"))
            .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -tab_pad))
            .order(egui::Order::Foreground)
            .interactable(true)
            .show(ctx, |ui| {
                egui::Frame::NONE
                    .fill(colors::BG_PANEL)
                    .corner_radius(egui::CornerRadius::from(14.0 * scale))
                    .stroke(egui::Stroke::new(1.0, colors::BORDER_GLOW))
                    .inner_margin(egui::Margin::symmetric((8.0 * scale) as i8, (4.0 * scale) as i8))
                    .show(ui, |ui| {
                        ui.horizontal_centered(|ui| {
                            let icon_sz = (18.0 * scale).clamp(16.0, 48.0);
                            let label_size = (9.0 * scale).clamp(8.0, 22.0);
                            let stroke_w = (1.5 * scale).clamp(1.5, 4.0);

                            // Draw icons using painter primitives
                            let draw_icon = |p: &egui::Painter, cx: f32, cy: f32, s: f32, which: &str, color: egui::Color32| {
                                let sw = stroke_w;
                                match which {
                                    "crop" => {
                                        // Viewfinder: 4 corner brackets
                                        let h = s * 0.4;
                                        let a = s * 0.15;
                                        // top-left
                                        p.line_segment([egui::pos2(cx - h, cy - h + a), egui::pos2(cx - h, cy - h)], egui::Stroke::new(sw, color));
                                        p.line_segment([egui::pos2(cx - h, cy - h), egui::pos2(cx - h + a, cy - h)], egui::Stroke::new(sw, color));
                                        // top-right
                                        p.line_segment([egui::pos2(cx + h - a, cy - h), egui::pos2(cx + h, cy - h)], egui::Stroke::new(sw, color));
                                        p.line_segment([egui::pos2(cx + h, cy - h), egui::pos2(cx + h, cy - h + a)], egui::Stroke::new(sw, color));
                                        // bottom-left
                                        p.line_segment([egui::pos2(cx - h, cy + h - a), egui::pos2(cx - h, cy + h)], egui::Stroke::new(sw, color));
                                        p.line_segment([egui::pos2(cx - h, cy + h), egui::pos2(cx - h + a, cy + h)], egui::Stroke::new(sw, color));
                                        // bottom-right
                                        p.line_segment([egui::pos2(cx + h - a, cy + h), egui::pos2(cx + h, cy + h)], egui::Stroke::new(sw, color));
                                        p.line_segment([egui::pos2(cx + h, cy + h), egui::pos2(cx + h, cy + h - a)], egui::Stroke::new(sw, color));
                                        // center cross
                                        let c = s * 0.1;
                                        p.line_segment([egui::pos2(cx - c, cy), egui::pos2(cx + c, cy)], egui::Stroke::new(sw * 0.7, color));
                                        p.line_segment([egui::pos2(cx, cy - c), egui::pos2(cx, cy + c)], egui::Stroke::new(sw * 0.7, color));
                                    }
                                    "perf" => {
                                        // Bar chart: 3 bars
                                        let bw = s * 0.18;
                                        let gap = s * 0.08;
                                        let base = cy + s * 0.35;
                                        for (i, h) in [0.3, 0.55, 0.7].iter().enumerate() {
                                            let x = cx - bw * 1.5 - gap + (bw + gap) * i as f32;
                                            let top = base - s * h;
                                            p.rect_filled(egui::Rect::from_min_max(egui::pos2(x, top), egui::pos2(x + bw, base)), 2.0, color);
                                        }
                                    }
                                    "win" => {
                                        // Two overlapping windows
                                        let w = s * 0.3;
                                        let h = s * 0.25;
                                        p.rect_stroke(egui::Rect::from_min_size(egui::pos2(cx - w, cy - h), egui::vec2(w * 1.3, h * 1.4)),
                                            2.0, egui::Stroke::new(sw, color), egui::StrokeKind::Inside);
                                        p.rect_stroke(egui::Rect::from_min_size(egui::pos2(cx - w * 0.3, cy - h * 0.3), egui::vec2(w * 1.3, h * 1.4)),
                                            2.0, egui::Stroke::new(sw, color), egui::StrokeKind::Inside);
                                    }
                                    "shader" => {
                                        // Sparkle: 4-pointed star
                                        let a = s * 0.38;
                                        let b = s * 0.12;
                                        p.line_segment([egui::pos2(cx, cy - a), egui::pos2(cx, cy + a)], egui::Stroke::new(sw, color));
                                        p.line_segment([egui::pos2(cx - a, cy), egui::pos2(cx + a, cy)], egui::Stroke::new(sw, color));
                                        p.line_segment([egui::pos2(cx - b, cy - b), egui::pos2(cx + b, cy + b)], egui::Stroke::new(sw * 0.7, color));
                                        p.line_segment([egui::pos2(cx + b, cy - b), egui::pos2(cx - b, cy + b)], egui::Stroke::new(sw * 0.7, color));
                                    }
                                    "keys" => {
                                        // Keyboard outline
                                        let w = s * 0.38;
                                        let h = s * 0.25;
                                        let r = egui::Rect::from_center_size(egui::pos2(cx, cy), egui::vec2(w * 2.0, h * 2.0));
                                        p.rect_stroke(r, 3.0, egui::Stroke::new(sw, color), egui::StrokeKind::Inside);
                                        // Key rows
                                        p.line_segment([egui::pos2(r.left() + 3.0, cy - h * 0.3), egui::pos2(r.right() - 3.0, cy - h * 0.3)], egui::Stroke::new(sw * 0.5, color.linear_multiply(0.5)));
                                        p.line_segment([egui::pos2(r.left() + 3.0, cy + h * 0.3), egui::pos2(r.right() - 3.0, cy + h * 0.3)], egui::Stroke::new(sw * 0.5, color.linear_multiply(0.5)));
                                    }
                                    "time" => {
                                        // Clock: circle + hands
                                        let r = s * 0.35;
                                        p.circle_stroke(egui::pos2(cx, cy), r, egui::Stroke::new(sw, color));
                                        p.line_segment([egui::pos2(cx, cy), egui::pos2(cx, cy - r * 0.65)], egui::Stroke::new(sw, color));
                                        p.line_segment([egui::pos2(cx, cy), egui::pos2(cx + r * 0.45, cy + r * 0.1)], egui::Stroke::new(sw * 0.7, color));
                                    }
                                    "pad" => {
                                        // Gamepad: rectangle with bumps + 2 dots
                                        let w = s * 0.35;
                                        let h = s * 0.22;
                                        let r = egui::Rect::from_center_size(egui::pos2(cx, cy), egui::vec2(w * 2.0, h * 2.0));
                                        p.rect_stroke(r, 5.0, egui::Stroke::new(sw, color), egui::StrokeKind::Inside);
                                        // D-pad hint (left)
                                        p.line_segment([egui::pos2(cx - w * 0.5, cy), egui::pos2(cx - w * 0.2, cy)], egui::Stroke::new(sw, color));
                                        p.line_segment([egui::pos2(cx - w * 0.35, cy - h * 0.4), egui::pos2(cx - w * 0.35, cy + h * 0.4)], egui::Stroke::new(sw, color));
                                        // Face hint (right)
                                        p.circle_filled(egui::pos2(cx + w * 0.35, cy - h * 0.25), sw, color);
                                        p.circle_filled(egui::pos2(cx + w * 0.55, cy + h * 0.1), sw, color);
                                    }
                                    _ => {}
                                }
                            };

                            let tab_button = |ui: &mut egui::Ui, icon_name: &str, label: &str, active: bool| -> bool {
                                let fill = if active { colors::TAB_ACTIVE_BG } else { egui::Color32::TRANSPARENT };
                                let accent = if active { colors::ACCENT_BLUE } else { colors::TEXT_SECONDARY };
                                let (rect, response) = ui.allocate_exact_size(egui::vec2(tab_w, tab_h), egui::Sense::click());

                                if response.hovered() && !active {
                                    ui.painter().rect_filled(rect, 10.0 * scale, colors::TAB_HOVER_BG);
                                } else {
                                    ui.painter().rect_filled(rect, 10.0 * scale, fill);
                                }

                                // Draw icon
                                let icon_y = rect.top() + tab_h * 0.32;
                                draw_icon(ui.painter(), rect.center().x, icon_y, icon_sz, icon_name, accent);

                                // Label
                                let label_y = rect.top() + tab_h * 0.78;
                                ui.painter().text(
                                    egui::pos2(rect.center().x, label_y),
                                    egui::Align2::CENTER_CENTER,
                                    label,
                                    egui::FontId::new(label_size, egui::FontFamily::Proportional),
                                    if active { colors::TEXT_PRIMARY } else { colors::TEXT_DIM },
                                );

                                if active {
                                    let uy = rect.bottom() - 2.0;
                                    let hw = rect.width() * 0.3;
                                    ui.painter().line_segment(
                                        [egui::pos2(rect.center().x - hw, uy), egui::pos2(rect.center().x + hw, uy)],
                                        egui::Stroke::new(2.5, colors::ACCENT_BLUE),
                                    );
                                }
                                response.clicked()
                            };

                            if tab_button(ui, "crop", "Crop", self.active_tab == ToolTab::Crop) { self.active_tab = ToolTab::Crop; }
                            ui.add_space(tab_gap);
                            if tab_button(ui, "perf", "Perf", self.active_tab == ToolTab::Performance) { self.active_tab = ToolTab::Performance; }
                            ui.add_space(tab_gap);
                            if tab_button(ui, "win", "Win", self.active_tab == ToolTab::Windows) { self.active_tab = ToolTab::Windows; }
                            ui.add_space(tab_gap);
                            if tab_button(ui, "shader", "Shdr", self.active_tab == ToolTab::Shaders) { self.active_tab = ToolTab::Shaders; }
                            ui.add_space(tab_gap);
                            if tab_button(ui, "keys", "Keys", self.active_tab == ToolTab::Hotkeys) { self.active_tab = ToolTab::Hotkeys; }
                            ui.add_space(tab_gap);
                            if tab_button(ui, "time", "Time", self.active_tab == ToolTab::Faketime) { self.active_tab = ToolTab::Faketime; }
                            ui.add_space(tab_gap);
                            if tab_button(ui, "pad", "Pad", self.active_tab == ToolTab::Gamepad) { self.active_tab = ToolTab::Gamepad; }

                            // Separator + Focus Game button
                            ui.add_space(8.0 * scale);
                            ui.label(egui::RichText::new("|").size(16.0 * scale).color(colors::BORDER_SUBTLE));
                            ui.add_space(4.0 * scale);

                            let focus_resp = ui.add_sized(
                                [tab_w, tab_h],
                                egui::Button::new(
                                    egui::RichText::new("Focus\nGame")
                                        .size(label_size)
                                        .color(colors::ACCENT_PINK),
                                )
                                .fill(egui::Color32::from_rgba_premultiplied(255, 46, 151, 15))
                                .corner_radius(10.0 * scale),
                            );
                            if focus_resp.clicked() {
                                match focus_emulator_window(&self.hotkey_target) {
                                    Ok(_) => {
                                        self.hotkeys_status = Some("Focused emulator".into());
                                        self.hotkeys_status_until = Some(Instant::now() + Duration::from_secs(2));
                                    }
                                    Err(e) => {
                                        self.hotkeys_status = Some(format!("Focus failed: {}", e));
                                        self.hotkeys_status_until = Some(Instant::now() + Duration::from_secs(3));
                                    }
                                }
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
