//! Layout state machine — automatic window positioning and screen-tool visibility.
//!
//! Replaces screen-tool-visibility.sh (408 lines) and reposition-windows.sh (126 lines)
//! with a Rust state machine that runs inside the winit event loop.
//!
//! Rules:
//! - Primary emulator window → fullscreen on top display (always)
//! - Secondary emulator window → fullscreen on bottom display (when bottom stream connected)
//! - Two emulator windows + no bottom stream → side-by-side on top display
//! - Screen-tool → overlay on bottom display (if connected), else top display
//! - Screen-tool never tiles alongside emulator windows
//! - Toggle button → right-center edge of whichever display screen-tool is on

use std::time::{Duration, Instant};

use crate::stream::StreamDetector;
use crate::wm::{DisplayInfo, LayoutMode, WindowManager};

/// Debounce thresholds (matching the bash script's defaults).
const POLL_INTERVAL: Duration = Duration::from_millis(250);
const BOTTOM_DEBOUNCE_POLLS: u32 = 3;   // 750ms for stream state changes
const VISIBILITY_DEBOUNCE_POLLS: u32 = 2; // 500ms for secondary window visibility

/// Patterns used to identify emulator windows.
const SECONDARY_PATTERNS: &[&str] = &[
    "Secondary Window",
    "Bottom Screen",
    "Subscreen",
    "Screen 2",
];

const EMULATOR_PATTERNS: &[&str] = &[
    "Azahar",
    "melonDS",
    "Dolphin",
    "PPSSPP",
    "Ryujinx",
    "Steam",
];

const SHADER_PREFIX: &str = "Shader: ";

/// Visibility mode for the screen-tool overlay (replaces mode file).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum VisibilityMode {
    /// Hidden until user toggles on.
    ForceHide,
    /// Visible, user explicitly toggled on.
    ForceShow,
    /// Auto: hide when secondary window occupies bottom display, show otherwise.
    Auto,
}

/// Which display the screen-tool should appear on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenToolDisplay {
    Top,
    Bottom,
}

/// The layout state machine.
pub struct LayoutManager {
    stream_detector: StreamDetector,

    // Debounced stream state
    bottom_connected: bool,
    bottom_on_streak: u32,
    bottom_off_streak: u32,

    // Debounced secondary window visibility
    secondary_visible: bool,
    secondary_active_streak: u32,
    secondary_inactive_streak: u32,

    // Screen-tool visibility
    visibility_mode: VisibilityMode,
    screen_tool_hidden: bool,
    screen_tool_display: ScreenToolDisplay,

    // Capture output hint (which display NVFBC should capture)
    pub capture_output_hint: Option<String>,
    manual_override_until: Instant,

    // Timing
    last_poll: Instant,
    last_reposition: Instant,

    // Display names
    top_display: String,
    bottom_display: String,
}

impl LayoutManager {
    pub fn new() -> Self {
        Self {
            stream_detector: StreamDetector::new(),
            bottom_connected: false,
            bottom_on_streak: 0,
            bottom_off_streak: 0,
            secondary_visible: false,
            secondary_active_streak: 0,
            secondary_inactive_streak: 0,
            visibility_mode: VisibilityMode::ForceHide,
            screen_tool_hidden: true,
            screen_tool_display: ScreenToolDisplay::Top,
            capture_output_hint: None,
            manual_override_until: Instant::now(),
            last_poll: Instant::now() - Duration::from_secs(100),
            last_reposition: Instant::now() - Duration::from_secs(100),
            top_display: "DP-0".to_string(),
            bottom_display: "DP-2".to_string(),
        }
    }

    /// Set display names (if different from default DP-0/DP-2).
    pub fn set_display_names(&mut self, top: &str, bottom: &str) {
        self.top_display = top.to_string();
        self.bottom_display = bottom.to_string();
    }

    pub fn visibility_mode(&self) -> VisibilityMode {
        self.visibility_mode
    }

    #[allow(dead_code)]
    pub fn is_bottom_connected(&self) -> bool {
        self.bottom_connected
    }

    #[allow(dead_code)]
    pub fn screen_tool_display(&self) -> ScreenToolDisplay {
        self.screen_tool_display
    }

    #[allow(dead_code)]
    pub fn is_screen_tool_hidden(&self) -> bool {
        self.screen_tool_hidden
    }

    /// Toggle visibility (called from toggle button or SIGUSR2).
    pub fn toggle_visibility(&mut self) {
        match self.visibility_mode {
            VisibilityMode::ForceHide | VisibilityMode::Auto => {
                self.visibility_mode = VisibilityMode::ForceShow;
                // Immediate bottom stream check on explicit show
                self.refresh_bottom_immediate();
            }
            VisibilityMode::ForceShow => {
                self.visibility_mode = VisibilityMode::ForceHide;
            }
        }
    }

    /// Set manual capture output override (from GUI output selector).
    pub fn set_manual_override(&mut self, duration: Duration) {
        self.manual_override_until = Instant::now() + duration;
    }

    /// Tick the state machine. Call from the main event loop at frame rate.
    /// Returns true if layout changes were applied (caller may want to redraw).
    pub fn tick(&mut self, wm: &mut WindowManager) -> bool {
        if self.last_poll.elapsed() < POLL_INTERVAL {
            return false;
        }
        self.last_poll = Instant::now();

        // Refresh display and window info
        wm.refresh_displays_if_stale(Duration::from_secs(2));
        wm.refresh_windows_if_stale(Duration::from_millis(500));

        let mut changed = false;

        // Step 1: Debounced bottom stream detection
        changed |= self.poll_bottom_stream();

        // Step 2: Check secondary window state
        changed |= self.poll_secondary_windows(wm);

        // Step 3: Determine screen-tool target display
        let new_display = if self.bottom_connected {
            ScreenToolDisplay::Bottom
        } else {
            ScreenToolDisplay::Top
        };
        if new_display != self.screen_tool_display {
            self.screen_tool_display = new_display;
            changed = true;
        }

        // Step 4: Apply visibility rules
        changed |= self.apply_visibility_rules(wm);

        // Step 5: Position emulator windows
        changed |= self.position_emulator_windows(wm);

        // Step 6: Update capture output hint
        // Capture the display the screen-tool is NOT on.
        // When bottom is disconnected and tool is on top, default to capturing top
        // (there's nothing useful on the disconnected bottom display).
        if Instant::now() >= self.manual_override_until {
            let hint = if !self.bottom_connected {
                // No bottom stream — always capture top (primary emulator)
                &self.top_display
            } else {
                match self.screen_tool_display {
                    // Tool on bottom → capture top emulator
                    ScreenToolDisplay::Bottom => &self.top_display,
                    // Tool on top → capture bottom emulator
                    ScreenToolDisplay::Top => &self.bottom_display,
                }
            };
            let hint = hint.clone();
            if self.capture_output_hint.as_deref() != Some(&hint) {
                self.capture_output_hint = Some(hint);
                changed = true;
            }
        }

        changed
    }

    /// Poll bottom stream connection with debounce.
    fn poll_bottom_stream(&mut self) -> bool {
        if self.stream_detector.is_bottom_stream_connected() {
            self.bottom_on_streak += 1;
            self.bottom_off_streak = 0;
        } else {
            self.bottom_off_streak += 1;
            self.bottom_on_streak = 0;
        }

        let prev = self.bottom_connected;
        if self.bottom_on_streak >= BOTTOM_DEBOUNCE_POLLS {
            self.bottom_connected = true;
        } else if self.bottom_off_streak >= BOTTOM_DEBOUNCE_POLLS {
            self.bottom_connected = false;
        }
        self.bottom_connected != prev
    }

    /// Immediate bottom stream check (for toggle show events).
    fn refresh_bottom_immediate(&mut self) {
        if self.stream_detector.is_bottom_stream_connected() {
            self.bottom_connected = true;
            self.bottom_on_streak = BOTTOM_DEBOUNCE_POLLS;
            self.bottom_off_streak = 0;
        } else if !self.bottom_connected {
            self.bottom_connected = false;
            self.bottom_off_streak = BOTTOM_DEBOUNCE_POLLS;
            self.bottom_on_streak = 0;
        }
    }

    /// Check if a secondary emulator window is actively occupying the bottom display.
    fn poll_secondary_windows(&mut self, wm: &WindowManager) -> bool {
        let mut secondary_active = false;

        if self.bottom_connected {
            let bottom_display = wm.find_display(&self.bottom_display);
            for win in wm.windows() {
                if is_secondary_window(&win.title) && win.mapped {
                    if let Some(disp) = bottom_display {
                        if is_window_on_display(win, disp) {
                            secondary_active = true;
                            break;
                        }
                    }
                }
            }
        }

        if secondary_active {
            self.secondary_active_streak += 1;
            self.secondary_inactive_streak = 0;
        } else {
            self.secondary_inactive_streak += 1;
            self.secondary_active_streak = 0;
        }

        let prev = self.secondary_visible;
        if self.secondary_active_streak >= VISIBILITY_DEBOUNCE_POLLS {
            self.secondary_visible = true;
        } else if self.secondary_inactive_streak >= VISIBILITY_DEBOUNCE_POLLS {
            self.secondary_visible = false;
        }
        self.secondary_visible != prev
    }

    /// Apply visibility rules to the screen-tool window.
    fn apply_visibility_rules(&mut self, wm: &WindowManager) -> bool {
        let should_hide = match self.visibility_mode {
            VisibilityMode::ForceHide => true,
            VisibilityMode::ForceShow => false,
            VisibilityMode::Auto => {
                // In auto mode, hide when secondary is active on bottom display
                self.secondary_visible && self.bottom_connected
            }
        };

        if should_hide != self.screen_tool_hidden {
            // Find screen-tool window and map/unmap it
            let st_windows = wm.find_windows_by_title("ScreenTool");
            for win in &st_windows {
                // Skip the toggle button
                if win.title == "ScreenToolToggle" {
                    continue;
                }
                if should_hide {
                    wm.unmap_window(win.id);
                } else {
                    wm.map_window(win.id);
                    wm.raise_window(win.id);
                }
            }
            self.screen_tool_hidden = should_hide;
            return true;
        }
        false
    }

    /// Position emulator windows on their correct displays.
    fn position_emulator_windows(&mut self, wm: &WindowManager) -> bool {
        // Only reposition every 2 seconds to avoid fighting with window managers
        if self.last_reposition.elapsed() < Duration::from_secs(2) {
            return false;
        }
        self.last_reposition = Instant::now();

        let top_display = wm.find_display(&self.top_display).cloned();
        let bottom_display = wm.find_display(&self.bottom_display).cloned();

        // Classify windows
        let mut primary_wids: Vec<u64> = Vec::new();
        let mut secondary_wids: Vec<u64> = Vec::new();
        let mut shader_primary_wids: Vec<u64> = Vec::new();
        let mut shader_secondary_wids: Vec<u64> = Vec::new();

        for win in wm.windows() {
            if win.title.starts_with(SHADER_PREFIX) {
                if is_secondary_window(&win.title) {
                    shader_secondary_wids.push(win.id);
                } else {
                    shader_primary_wids.push(win.id);
                }
            } else if is_secondary_window(&win.title) {
                secondary_wids.push(win.id);
            } else if is_emulator_window(&win.title) {
                primary_wids.push(win.id);
            }
        }

        let mut changed = false;

        if let Some(ref _top) = top_display {
            // Primary emulator → always fullscreen on top
            for &wid in &primary_wids {
                if self.bottom_connected || secondary_wids.is_empty() {
                    // Normal: primary fullscreen on top
                    wm.move_window_to_display(wid, &self.top_display, LayoutMode::Fullscreen);
                } else {
                    // Two windows, no bottom stream → side by side on top
                    wm.move_window_to_display(wid, &self.top_display, LayoutMode::LeftHalf);
                }
                changed = true;
            }

            // Shader primary → on top, above emulator
            for &wid in &shader_primary_wids {
                if self.bottom_connected || secondary_wids.is_empty() {
                    wm.move_window_to_display(wid, &self.top_display, LayoutMode::Fullscreen);
                } else {
                    wm.move_window_to_display(wid, &self.top_display, LayoutMode::LeftHalf);
                }
                wm.raise_window(wid);
                changed = true;
            }

            // Secondary with no bottom stream → right half on top
            if !self.bottom_connected && !secondary_wids.is_empty() {
                for &wid in &secondary_wids {
                    wm.move_window_to_display(wid, &self.top_display, LayoutMode::RightHalf);
                    changed = true;
                }
                for &wid in &shader_secondary_wids {
                    wm.move_window_to_display(wid, &self.top_display, LayoutMode::RightHalf);
                    wm.raise_window(wid);
                    changed = true;
                }
            }
        }

        if let Some(ref _bot) = bottom_display {
            // Secondary with bottom stream → fullscreen on bottom
            if self.bottom_connected {
                for &wid in &secondary_wids {
                    wm.move_window_to_display(wid, &self.bottom_display, LayoutMode::Fullscreen);
                    changed = true;
                }
                for &wid in &shader_secondary_wids {
                    wm.move_window_to_display(wid, &self.bottom_display, LayoutMode::Fullscreen);
                    wm.raise_window(wid);
                    changed = true;
                }
            }
        }

        changed
    }

    /// Compute where the screen-tool window should be placed (full display).
    /// Returns (x, y, width, height) or None if display not found.
    #[allow(dead_code)]
    pub fn screen_tool_geometry(&self, wm: &WindowManager) -> Option<(i32, i32, u32, u32)> {
        let display_name = match self.screen_tool_display {
            ScreenToolDisplay::Top => &self.top_display,
            ScreenToolDisplay::Bottom => &self.bottom_display,
        };
        wm.find_display(display_name)
            .map(|d| (d.x, d.y, d.width, d.height))
    }

    /// Compute where the toggle button should be placed.
    /// Returns (x, y, size, size) — right-center edge of the target display.
    #[allow(dead_code)]
    pub fn toggle_button_geometry(&self, wm: &WindowManager) -> Option<(i32, i32, u32, u32)> {
        let display_name = match self.screen_tool_display {
            ScreenToolDisplay::Top => &self.top_display,
            ScreenToolDisplay::Bottom => &self.bottom_display,
        };
        wm.find_display(display_name).map(|d| {
            let btn_size: u32 = 52;
            let margin: i32 = 12;
            let x = d.x + d.width as i32 - btn_size as i32 - margin;
            let y = d.y + (d.height as i32 / 2) - (btn_size as i32 / 2);
            (x, y, btn_size, btn_size)
        })
    }
}

/// Check if a window title matches a secondary/bottom-screen pattern.
fn is_secondary_window(title: &str) -> bool {
    SECONDARY_PATTERNS.iter().any(|p| title.contains(p))
}

/// Check if a window title matches an emulator pattern.
fn is_emulator_window(title: &str) -> bool {
    EMULATOR_PATTERNS.iter().any(|p| title.contains(p))
}

/// Check if a window is roughly occupying a display (>60% area, within 140px offset).
fn is_window_on_display(win: &crate::wm::WindowInfo, disp: &DisplayInfo) -> bool {
    let min_w = disp.width * 60 / 100;
    let min_h = disp.height * 60 / 100;
    let dx = (win.x - disp.x).unsigned_abs();
    let dy = (win.y - disp.y).unsigned_abs();

    win.width >= min_w && win.height >= min_h && dx <= 140 && dy <= 140
}
