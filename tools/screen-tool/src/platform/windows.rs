//! Windows platform stubs for window management and display enumeration.
//! Phase 7 (future) will implement these using Win32 APIs.

use crate::wm::{DisplayInfo, WindowInfo};

pub fn list_windows(_display_handle: *mut std::os::raw::c_void) -> Vec<WindowInfo> {
    log::warn!("Window enumeration not yet implemented on Windows");
    Vec::new()
}

pub fn list_displays(_display_handle: *mut std::os::raw::c_void) -> Vec<DisplayInfo> {
    log::warn!("Display enumeration not yet implemented on Windows");
    Vec::new()
}

pub fn move_resize_window(
    _display_handle: *mut std::os::raw::c_void,
    _wid: u64,
    _x: i32,
    _y: i32,
    _width: u32,
    _height: u32,
) {
    log::warn!("Window move/resize not yet implemented on Windows");
}

pub fn map_window(_display_handle: *mut std::os::raw::c_void, _wid: u64) {
    log::warn!("Window map not yet implemented on Windows");
}

pub fn unmap_window(_display_handle: *mut std::os::raw::c_void, _wid: u64) {
    log::warn!("Window unmap not yet implemented on Windows");
}

pub fn raise_window(_display_handle: *mut std::os::raw::c_void, _wid: u64) {
    log::warn!("Window raise not yet implemented on Windows");
}
