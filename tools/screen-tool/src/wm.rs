//! Window management — find, move, resize, and layout other windows.
//!
//! Platform-agnostic API backed by X11 (Linux) or Win32 (Windows).

use std::time::Instant;

/// Information about a window on the desktop.
#[derive(Debug, Clone)]
pub struct WindowInfo {
    pub id: u64,
    pub title: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub mapped: bool,
}

/// Information about a connected display/monitor.
#[derive(Debug, Clone)]
pub struct DisplayInfo {
    pub name: String, // e.g. "DP-0", "DP-2"
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    #[allow(dead_code)]
    pub is_primary: bool,
}

/// How to position a window on a display.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutMode {
    Fullscreen,
    LeftHalf,
    RightHalf,
}

/// Window manager: discovers windows and displays, moves/resizes windows.
pub struct WindowManager {
    #[cfg(target_os = "linux")]
    display: *mut x11::xlib::Display,
    windows: Vec<WindowInfo>,
    displays: Vec<DisplayInfo>,
    last_window_refresh: Instant,
    last_display_refresh: Instant,
}

impl WindowManager {
    /// Create a new WindowManager using the given X11 display pointer.
    #[cfg(target_os = "linux")]
    pub fn new(display: *mut x11::xlib::Display) -> Self {
        Self {
            display,
            windows: Vec::new(),
            displays: Vec::new(),
            last_window_refresh: Instant::now() - std::time::Duration::from_secs(100),
            last_display_refresh: Instant::now() - std::time::Duration::from_secs(100),
        }
    }

    /// Refresh the list of windows (throttled to avoid X11 spam).
    pub fn refresh_windows(&mut self) {
        #[cfg(target_os = "linux")]
        {
            self.windows = crate::platform::linux::list_windows(self.display);
            self.last_window_refresh = Instant::now();
        }
    }

    /// Refresh the list of displays (throttled).
    pub fn refresh_displays(&mut self) {
        #[cfg(target_os = "linux")]
        {
            self.displays = crate::platform::linux::list_displays(self.display);
            self.last_display_refresh = Instant::now();
        }
    }

    /// Refresh windows if stale (older than the given duration).
    pub fn refresh_windows_if_stale(&mut self, max_age: std::time::Duration) {
        if self.last_window_refresh.elapsed() >= max_age {
            self.refresh_windows();
        }
    }

    /// Refresh displays if stale.
    pub fn refresh_displays_if_stale(&mut self, max_age: std::time::Duration) {
        if self.last_display_refresh.elapsed() >= max_age {
            self.refresh_displays();
        }
    }

    pub fn windows(&self) -> &[WindowInfo] {
        &self.windows
    }

    pub fn displays(&self) -> &[DisplayInfo] {
        &self.displays
    }

    /// Find a display by name (e.g. "DP-0").
    pub fn find_display(&self, name: &str) -> Option<&DisplayInfo> {
        self.displays.iter().find(|d| d.name == name)
    }

    /// Find windows whose title contains the given pattern.
    pub fn find_windows_by_title(&self, pattern: &str) -> Vec<&WindowInfo> {
        self.windows
            .iter()
            .filter(|w| w.title.contains(pattern))
            .collect()
    }

    /// Move a window to fill a display using the given layout mode.
    pub fn move_window_to_display(&self, wid: u64, display_name: &str, mode: LayoutMode) {
        let disp = match self.find_display(display_name) {
            Some(d) => d,
            None => {
                log::warn!("Display '{}' not found", display_name);
                return;
            }
        };

        let (x, y, w, h) = match mode {
            LayoutMode::Fullscreen => (disp.x, disp.y, disp.width, disp.height),
            LayoutMode::LeftHalf => (disp.x, disp.y, disp.width / 2, disp.height),
            LayoutMode::RightHalf => (
                disp.x + disp.width as i32 / 2,
                disp.y,
                disp.width / 2,
                disp.height,
            ),
        };

        self.move_resize(wid, x, y, w, h);
    }

    /// Move and resize a window to exact coordinates.
    pub fn move_resize(&self, wid: u64, x: i32, y: i32, width: u32, height: u32) {
        #[cfg(target_os = "linux")]
        crate::platform::linux::move_resize_window(self.display, wid, x, y, width, height);
    }

    /// Map (show) a window.
    pub fn map_window(&self, wid: u64) {
        #[cfg(target_os = "linux")]
        crate::platform::linux::map_window(self.display, wid);
    }

    /// Unmap (hide) a window.
    pub fn unmap_window(&self, wid: u64) {
        #[cfg(target_os = "linux")]
        crate::platform::linux::unmap_window(self.display, wid);
    }

    /// Raise window to top.
    pub fn raise_window(&self, wid: u64) {
        #[cfg(target_os = "linux")]
        crate::platform::linux::raise_window(self.display, wid);
    }
}
