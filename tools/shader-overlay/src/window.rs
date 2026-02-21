use anyhow::{bail, Result};
use std::os::raw::c_ulong;
use std::ptr;
use std::thread;
use std::time::Duration;
use x11::xlib;

/// Information about a discovered X11 window
pub struct WindowInfo {
    pub id: c_ulong,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// Find a window by target string. Target can be:
/// - A hex window ID (0x1234 or 0X1234)
/// - A decimal window ID (4660)
/// - A window name/title substring match
pub fn find_window(display: *mut xlib::Display, target: &str) -> Result<WindowInfo> {
    // Try parsing as window ID first
    if let Some(id) = parse_window_id(target) {
        return get_window_info(display, id);
    }

    let root = unsafe { xlib::XDefaultRootWindow(display) };
    let pattern_lower = target.to_lowercase();
    let mut results = Vec::new();

    // Method 1: Check _NET_CLIENT_LIST (WM's known top-level windows)
    // This is how xdotool finds windows — more reliable than tree traversal
    if let Some(client_windows) = get_client_list(display, root) {
        for wid in &client_windows {
            if let Some(name) = get_window_name(display, *wid) {
                if name.to_lowercase().contains(&pattern_lower) {
                    if let Ok(info) = get_window_info(display, *wid) {
                        results.push(info);
                    }
                }
            }
        }
    }

    // Method 2: Recursive tree walk as fallback
    if results.is_empty() {
        find_windows_recursive(display, root, &pattern_lower, &mut results)?;
    }

    if results.is_empty() {
        bail!("No window found matching '{}'", target);
    }

    // Sort by area (largest first) — prefer real windows over tiny Qt widgets
    results.sort_by(|a, b| {
        let area_a = (a.width as u64) * (a.height as u64);
        let area_b = (b.width as u64) * (b.height as u64);
        area_b.cmp(&area_a)
    });

    if results.len() > 1 {
        log::info!("Multiple windows match '{}', using largest (0x{:x} {}x{}). All matches:", target, results[0].id, results[0].width, results[0].height);
        for w in &results {
            log::info!("  0x{:x} ({}x{} at {},{})", w.id, w.width, w.height, w.x, w.y);
        }
    }

    Ok(results.into_iter().next().unwrap())
}

/// Wait for a window matching the target to appear, polling every 500ms
pub fn wait_for_window(display: *mut xlib::Display, target: &str, timeout_secs: u32) -> Result<WindowInfo> {
    let attempts = timeout_secs * 2;
    for i in 0..attempts {
        match find_window(display, target) {
            Ok(info) => return Ok(info),
            Err(_) if i < attempts - 1 => {
                thread::sleep(Duration::from_millis(500));
            }
            Err(e) => return Err(e),
        }
    }
    bail!("Timed out after {}s waiting for window '{}'", timeout_secs, target)
}

fn parse_window_id(target: &str) -> Option<c_ulong> {
    let trimmed = target.trim();
    if trimmed.starts_with("0x") || trimmed.starts_with("0X") {
        c_ulong::from_str_radix(&trimmed[2..], 16).ok()
    } else {
        trimmed.parse::<c_ulong>().ok()
    }
}

fn get_window_info(display: *mut xlib::Display, window: c_ulong) -> Result<WindowInfo> {
    unsafe {
        let mut attrs: xlib::XWindowAttributes = std::mem::zeroed();
        if xlib::XGetWindowAttributes(display, window, &mut attrs) == 0 {
            bail!("Failed to get attributes for window 0x{:x}", window);
        }

        // Translate coordinates to root window space
        let mut x: i32 = 0;
        let mut y: i32 = 0;
        let mut child: c_ulong = 0;
        let root = xlib::XDefaultRootWindow(display);
        xlib::XTranslateCoordinates(
            display, window, root, 0, 0, &mut x, &mut y, &mut child,
        );

        Ok(WindowInfo {
            id: window,
            x,
            y,
            width: attrs.width as u32,
            height: attrs.height as u32,
        })
    }
}

/// Get _NET_CLIENT_LIST from root window (list of top-level windows known to the WM)
fn get_client_list(display: *mut xlib::Display, root: c_ulong) -> Option<Vec<c_ulong>> {
    unsafe {
        let atom = xlib::XInternAtom(display, b"_NET_CLIENT_LIST\0".as_ptr() as *const _, 0);

        let mut actual_type: c_ulong = 0;
        let mut actual_format: i32 = 0;
        let mut nitems: c_ulong = 0;
        let mut bytes_after: c_ulong = 0;
        let mut prop: *mut u8 = ptr::null_mut();

        if xlib::XGetWindowProperty(
            display, root, atom, 0, 1024, 0,
            xlib::XA_WINDOW, &mut actual_type, &mut actual_format,
            &mut nitems, &mut bytes_after, &mut prop,
        ) == 0 && !prop.is_null() && nitems > 0 && actual_format == 32 {
            let windows = std::slice::from_raw_parts(prop as *const c_ulong, nitems as usize).to_vec();
            xlib::XFree(prop as *mut std::os::raw::c_void);
            Some(windows)
        } else {
            if !prop.is_null() { xlib::XFree(prop as *mut std::os::raw::c_void); }
            None
        }
    }
}

/// Get window name, trying _NET_WM_NAME (UTF-8) first, then WM_NAME (legacy)
fn get_window_name(display: *mut xlib::Display, window: c_ulong) -> Option<String> {
    unsafe {
        // Try _NET_WM_NAME first (UTF-8, used by modern apps like Qt)
        let utf8_string = xlib::XInternAtom(display, b"UTF8_STRING\0".as_ptr() as *const _, 0);
        let net_wm_name = xlib::XInternAtom(display, b"_NET_WM_NAME\0".as_ptr() as *const _, 0);

        let mut actual_type: c_ulong = 0;
        let mut actual_format: i32 = 0;
        let mut nitems: c_ulong = 0;
        let mut bytes_after: c_ulong = 0;
        let mut prop: *mut u8 = ptr::null_mut();

        if xlib::XGetWindowProperty(
            display, window, net_wm_name, 0, 1024, 0,
            utf8_string, &mut actual_type, &mut actual_format,
            &mut nitems, &mut bytes_after, &mut prop,
        ) == 0 && !prop.is_null() && nitems > 0 {
            let name = String::from_utf8_lossy(std::slice::from_raw_parts(prop, nitems as usize)).to_string();
            xlib::XFree(prop as *mut std::os::raw::c_void);
            return Some(name);
        }
        if !prop.is_null() { xlib::XFree(prop as *mut std::os::raw::c_void); }

        // Fall back to WM_NAME (legacy Latin-1)
        let mut name_ptr: *mut std::os::raw::c_char = ptr::null_mut();
        if xlib::XFetchName(display, window, &mut name_ptr) != 0 && !name_ptr.is_null() {
            let name = std::ffi::CStr::from_ptr(name_ptr as *const _).to_string_lossy().to_string();
            xlib::XFree(name_ptr as *mut std::os::raw::c_void);
            return Some(name);
        }

        None
    }
}

fn find_windows_recursive(display: *mut xlib::Display, window: c_ulong, pattern: &str, results: &mut Vec<WindowInfo>) -> Result<()> {
    unsafe {
        // Check this window's name
        if let Some(name) = get_window_name(display, window) {
            if name.to_lowercase().contains(pattern) {
                let mut attrs: xlib::XWindowAttributes = std::mem::zeroed();
                if xlib::XGetWindowAttributes(display, window, &mut attrs) != 0
                    && attrs.width > 1
                    && attrs.height > 1
                {
                    // Map the window if needed — XComposite requires mapped windows
                    if attrs.map_state != xlib::IsViewable {
                        log::info!("Mapping unmapped window 0x{:x} '{}' ({}x{})", window, name, attrs.width, attrs.height);
                        xlib::XMapWindow(display, window);
                        xlib::XSync(display, 0);
                    }
                    if let Ok(info) = get_window_info(display, window) {
                        results.push(info);
                    }
                }
            }
        }

        // Recurse into children
        let mut root_return: c_ulong = 0;
        let mut parent_return: c_ulong = 0;
        let mut children: *mut c_ulong = ptr::null_mut();
        let mut nchildren: u32 = 0;

        if xlib::XQueryTree(display, window, &mut root_return, &mut parent_return, &mut children, &mut nchildren) != 0 {
            for i in 0..nchildren as isize {
                find_windows_recursive(display, *children.offset(i), pattern, results)?;
            }
            if !children.is_null() {
                xlib::XFree(children as *mut std::os::raw::c_void);
            }
        }
    }
    Ok(())
}
