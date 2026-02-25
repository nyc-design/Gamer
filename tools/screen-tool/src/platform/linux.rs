//! Linux/X11 window management and display enumeration.
//!
//! Replaces xdotool/xwininfo/xrandr subprocess calls with direct X11 API usage.

use std::os::raw::c_void;
use x11::xlib;
use x11::xrandr;

use crate::wm::{DisplayInfo, WindowInfo};

/// Enumerate all managed client windows using _NET_CLIENT_LIST (EWMH).
/// Falls back to XQueryTree if _NET_CLIENT_LIST is not available.
/// Reads _NET_WM_NAME (UTF-8) or falls back to XFetchName.
pub fn list_windows(display: *mut xlib::Display) -> Vec<WindowInfo> {
    let mut results = Vec::new();
    unsafe {
        let screen = xlib::XDefaultScreen(display);
        let root = xlib::XRootWindow(display, screen);

        // Try _NET_CLIENT_LIST first — gives actual client windows, not WM frames
        let atom_client_list =
            xlib::XInternAtom(display, b"_NET_CLIENT_LIST\0".as_ptr() as *const _, 0);
        let mut actual_type: xlib::Atom = 0;
        let mut actual_format: i32 = 0;
        let mut nitems: u64 = 0;
        let mut bytes_after: u64 = 0;
        let mut prop: *mut u8 = std::ptr::null_mut();

        let status = xlib::XGetWindowProperty(
            display, root, atom_client_list,
            0, 1024, 0, xlib::AnyPropertyType as u64,
            &mut actual_type, &mut actual_format, &mut nitems, &mut bytes_after, &mut prop,
        );

        if status == 0 && !prop.is_null() && nitems > 0 && actual_format == 32 {
            let window_ids = std::slice::from_raw_parts(prop as *const u64, nitems as usize);
            for &wid in window_ids {
                if let Some(info) = get_window_info(display, wid as xlib::Window) {
                    if !info.title.is_empty() {
                        results.push(info);
                    }
                }
            }
            xlib::XFree(prop as *mut c_void);
            return results;
        }
        if !prop.is_null() {
            xlib::XFree(prop as *mut c_void);
        }

        // Fallback: XQueryTree on root (may return WM frame windows)
        let mut root_ret: xlib::Window = 0;
        let mut parent_ret: xlib::Window = 0;
        let mut children: *mut xlib::Window = std::ptr::null_mut();
        let mut nchildren: u32 = 0;

        if xlib::XQueryTree(
            display, root, &mut root_ret, &mut parent_ret, &mut children, &mut nchildren,
        ) == 0
        {
            return results;
        }

        for i in 0..nchildren as usize {
            let wid = *children.add(i);
            if let Some(info) = get_window_info(display, wid) {
                if !info.title.is_empty() {
                    results.push(info);
                }
            }
        }

        if !children.is_null() {
            xlib::XFree(children as *mut c_void);
        }
    }
    results
}

/// Get info about a single window.
fn get_window_info(display: *mut xlib::Display, wid: xlib::Window) -> Option<WindowInfo> {
    unsafe {
        // Get window attributes
        let mut attrs: xlib::XWindowAttributes = std::mem::zeroed();
        if xlib::XGetWindowAttributes(display, wid, &mut attrs) == 0 {
            return None;
        }

        let title = get_window_title(display, wid);
        let mapped = attrs.map_state == xlib::IsViewable;

        Some(WindowInfo {
            id: wid as u64,
            title,
            x: attrs.x,
            y: attrs.y,
            width: attrs.width as u32,
            height: attrs.height as u32,
            mapped,
        })
    }
}

/// Get window title, preferring _NET_WM_NAME (UTF-8) over XFetchName.
fn get_window_title(display: *mut xlib::Display, wid: xlib::Window) -> String {
    unsafe {
        // Try _NET_WM_NAME first (handles UTF-8 properly, unlike XFetchName)
        let atom_name =
            xlib::XInternAtom(display, b"_NET_WM_NAME\0".as_ptr() as *const _, 0);
        let atom_utf8 =
            xlib::XInternAtom(display, b"UTF8_STRING\0".as_ptr() as *const _, 0);

        let mut actual_type: xlib::Atom = 0;
        let mut actual_format: i32 = 0;
        let mut nitems: u64 = 0;
        let mut bytes_after: u64 = 0;
        let mut prop: *mut u8 = std::ptr::null_mut();

        let status = xlib::XGetWindowProperty(
            display,
            wid,
            atom_name,
            0,
            1024,
            0,
            atom_utf8,
            &mut actual_type,
            &mut actual_format,
            &mut nitems,
            &mut bytes_after,
            &mut prop,
        );

        if status == 0 && !prop.is_null() && actual_type == atom_utf8 && nitems > 0 {
            let slice = std::slice::from_raw_parts(prop, nitems as usize);
            let title = String::from_utf8_lossy(slice).to_string();
            xlib::XFree(prop as *mut c_void);
            return title;
        }
        if !prop.is_null() {
            xlib::XFree(prop as *mut c_void);
        }

        // Fallback to XFetchName
        let mut name_ptr: *mut std::os::raw::c_char = std::ptr::null_mut();
        if xlib::XFetchName(display, wid, &mut name_ptr) != 0 && !name_ptr.is_null() {
            let name = std::ffi::CStr::from_ptr(name_ptr).to_string_lossy().to_string();
            xlib::XFree(name_ptr as *mut c_void);
            return name;
        }

        String::new()
    }
}

/// Enumerate connected displays using XRandR.
pub fn list_displays(display: *mut xlib::Display) -> Vec<DisplayInfo> {
    let mut results = Vec::new();
    unsafe {
        let screen = xlib::XDefaultScreen(display);
        let root = xlib::XRootWindow(display, screen);
        let resources = xrandr::XRRGetScreenResources(display, root);
        if resources.is_null() {
            return results;
        }

        let res = &*resources;
        for i in 0..res.noutput {
            let output_id = *res.outputs.add(i as usize);
            let output_info = xrandr::XRRGetOutputInfo(display, resources, output_id);
            if output_info.is_null() {
                continue;
            }
            let info = &*output_info;

            // Only include connected outputs with an active CRTC
            if info.connection == 0 && info.crtc != 0 {
                let crtc_info = xrandr::XRRGetCrtcInfo(display, resources, info.crtc);
                if !crtc_info.is_null() {
                    let crtc = &*crtc_info;
                    let name = if info.nameLen > 0 {
                        let slice =
                            std::slice::from_raw_parts(info.name as *const u8, info.nameLen as usize);
                        String::from_utf8_lossy(slice).to_string()
                    } else {
                        format!("Output-{}", i)
                    };

                    results.push(DisplayInfo {
                        name,
                        x: crtc.x,
                        y: crtc.y,
                        width: crtc.width,
                        height: crtc.height,
                        is_primary: i == 0, // DP-0 is typically primary
                    });

                    xrandr::XRRFreeCrtcInfo(crtc_info);
                }
            }

            xrandr::XRRFreeOutputInfo(output_info);
        }

        xrandr::XRRFreeScreenResources(resources);
    }
    results
}

/// Move and resize a window.
pub fn move_resize_window(
    display: *mut xlib::Display,
    wid: u64,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
) {
    unsafe {
        xlib::XMoveResizeWindow(display, wid as xlib::Window, x, y, width, height);
        xlib::XFlush(display);
    }
}

/// Map (show) a window.
pub fn map_window(display: *mut xlib::Display, wid: u64) {
    unsafe {
        xlib::XMapWindow(display, wid as xlib::Window);
        xlib::XFlush(display);
    }
}

/// Unmap (hide) a window.
pub fn unmap_window(display: *mut xlib::Display, wid: u64) {
    unsafe {
        xlib::XUnmapWindow(display, wid as xlib::Window);
        xlib::XFlush(display);
    }
}

/// Raise window to top of stacking order.
pub fn raise_window(display: *mut xlib::Display, wid: u64) {
    unsafe {
        xlib::XRaiseWindow(display, wid as xlib::Window);
        xlib::XFlush(display);
    }
}
