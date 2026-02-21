use anyhow::{bail, Result};
use glow::HasContext;
use std::os::raw::{c_int, c_ulong};
use std::ptr;
use x11::glx;
use x11::xlib;

use crate::gl_context::*;

/// Captures an X11 window via XComposite + GLX_EXT_texture_from_pixmap (zero-copy)
pub struct WindowCapture {
    display: *mut xlib::Display,
    source_window: c_ulong,
    x_pixmap: c_ulong,
    glx_pixmap: glx::GLXPixmap,
    texture: glow::Texture,
    fb_config: glx::GLXFBConfig,
    width: u32,
    height: u32,
    dirty: bool,
    nvidia_driver: bool,
    // X Damage tracking
    damage: c_ulong,
    damage_event_base: c_int,
}

// XComposite and XDamage FFI — these aren't in the x11 crate
extern "C" {
    fn XCompositeRedirectWindow(display: *mut xlib::Display, window: c_ulong, update: c_int);
    fn XCompositeUnredirectWindow(display: *mut xlib::Display, window: c_ulong, update: c_int);
    fn XCompositeNameWindowPixmap(display: *mut xlib::Display, window: c_ulong) -> c_ulong;
    fn XCompositeQueryExtension(display: *mut xlib::Display, event_base: *mut c_int, error_base: *mut c_int) -> c_int;
    fn XDamageCreate(display: *mut xlib::Display, drawable: c_ulong, level: c_int) -> c_ulong;
    fn XDamageDestroy(display: *mut xlib::Display, damage: c_ulong);
    fn XDamageSubtract(display: *mut xlib::Display, damage: c_ulong, repair: c_ulong, parts: c_ulong);
    fn XDamageQueryExtension(display: *mut xlib::Display, event_base: *mut c_int, error_base: *mut c_int) -> c_int;
}

const COMPOSITE_REDIRECT_AUTOMATIC: c_int = 1;
const XDAMAGE_REPORT_NON_EMPTY: c_int = 1;

impl WindowCapture {
    pub fn new(gl: &GlState, source_window: c_ulong) -> Result<Self> {
        unsafe {
            let display = gl.display;

            // Verify extensions
            let mut comp_event = 0;
            let mut comp_error = 0;
            if XCompositeQueryExtension(display, &mut comp_event, &mut comp_error) == 0 {
                bail!("XComposite extension not available");
            }

            let mut damage_event_base = 0;
            let mut damage_error = 0;
            if XDamageQueryExtension(display, &mut damage_event_base, &mut damage_error) == 0 {
                bail!("XDamage extension not available");
            }

            // Get window geometry
            let mut attrs: xlib::XWindowAttributes = std::mem::zeroed();
            if xlib::XGetWindowAttributes(display, source_window, &mut attrs) == 0 {
                bail!("Failed to get window attributes for 0x{:x}", source_window);
            }
            let width = attrs.width as u32;
            let height = attrs.height as u32;

            // Subscribe to structure events on source window
            xlib::XSelectInput(display, source_window, xlib::StructureNotifyMask);

            // Redirect window to offscreen composite buffer
            XCompositeRedirectWindow(display, source_window, COMPOSITE_REDIRECT_AUTOMATIC);

            // Get the backing pixmap
            let x_pixmap = XCompositeNameWindowPixmap(display, source_window);
            if x_pixmap == 0 {
                bail!("Failed to get composite pixmap for window 0x{:x}", source_window);
            }

            // Create GLX pixmap from X pixmap
            let pixmap_attribs: Vec<c_int> = vec![
                GLX_TEXTURE_TARGET_EXT, GLX_TEXTURE_2D_EXT,
                GLX_TEXTURE_FORMAT_EXT, GLX_TEXTURE_FORMAT_RGBA_EXT,
                0,
            ];
            let glx_pixmap = glx::glXCreatePixmap(display, gl.fb_config, x_pixmap, pixmap_attribs.as_ptr());
            if glx_pixmap == 0 {
                bail!("Failed to create GLX pixmap");
            }

            // Create GL texture and bind to pixmap (zero-copy)
            let texture = gl.glow_ctx.create_texture().map_err(|e| anyhow::anyhow!("{}", e))?;
            gl.glow_ctx.bind_texture(glow::TEXTURE_2D, Some(texture));
            gl.glow_ctx.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
            gl.glow_ctx.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);

            (gl.glx_ext.bind_tex_image)(display, glx_pixmap, GLX_FRONT_EXT, ptr::null());
            gl.glow_ctx.bind_texture(glow::TEXTURE_2D, None);

            // Create damage tracker
            let damage = XDamageCreate(display, source_window, XDAMAGE_REPORT_NON_EMPTY);
            if damage == 0 {
                bail!("Failed to create damage tracker for window 0x{:x}", source_window);
            }

            // Detect NVIDIA driver — texture_from_pixmap behaves differently
            let vendor = gl.glow_ctx.get_parameter_string(glow::VENDOR);
            let nvidia_driver = vendor.to_lowercase().contains("nvidia");
            log::info!("Capturing window 0x{:x} ({}x{}) via XComposite (vendor={}, nvidia={})", source_window, width, height, vendor, nvidia_driver);

            Ok(Self {
                display,
                source_window,
                x_pixmap,
                glx_pixmap,
                texture,
                fb_config: gl.fb_config,
                width,
                height,
                dirty: true, // render first frame immediately
                nvidia_driver,
                damage,
                damage_event_base,
            })
        }
    }

    /// Update the captured texture. With NVIDIA, the initial bind_tex_image creates
    /// a live link to the window's backing store — no rebind needed, just sync.
    /// On Mesa, we do the release/rebind cycle.
    pub fn update_if_dirty(&mut self, gl: &GlState) {
        if !self.dirty {
            return;
        }
        unsafe {
            if self.nvidia_driver {
                // NVIDIA: texture_from_pixmap is a live binding, just sync
                glx::glXWaitX();
            } else {
                // Mesa/Intel: must release and rebind
                gl.glow_ctx.bind_texture(glow::TEXTURE_2D, Some(self.texture));
                (gl.glx_ext.release_tex_image)(self.display, self.glx_pixmap, GLX_FRONT_EXT);
                (gl.glx_ext.bind_tex_image)(self.display, self.glx_pixmap, GLX_FRONT_EXT, ptr::null());
                gl.glow_ctx.bind_texture(glow::TEXTURE_2D, None);
            }
        }
        self.dirty = false;
    }

    /// Handle window resize — recreate pixmap and texture
    pub fn handle_resize(&mut self, gl: &GlState, new_width: u32, new_height: u32) -> Result<()> {
        if new_width == self.width && new_height == self.height {
            return Ok(());
        }
        log::info!("Window 0x{:x} resized to {}x{}", self.source_window, new_width, new_height);

        unsafe {
            // Release old resources
            gl.glow_ctx.bind_texture(glow::TEXTURE_2D, Some(self.texture));
            (gl.glx_ext.release_tex_image)(self.display, self.glx_pixmap, GLX_FRONT_EXT);
            gl.glow_ctx.bind_texture(glow::TEXTURE_2D, None);
            glx::glXDestroyPixmap(self.display, self.glx_pixmap);
            xlib::XFreePixmap(self.display, self.x_pixmap);
            gl.glow_ctx.delete_texture(self.texture);

            // Recreate
            let x_pixmap = XCompositeNameWindowPixmap(self.display, self.source_window);
            if x_pixmap == 0 {
                bail!("Failed to get composite pixmap after resize");
            }

            let pixmap_attribs: Vec<c_int> = vec![
                GLX_TEXTURE_TARGET_EXT, GLX_TEXTURE_2D_EXT,
                GLX_TEXTURE_FORMAT_EXT, GLX_TEXTURE_FORMAT_RGBA_EXT,
                0,
            ];
            let glx_pixmap = glx::glXCreatePixmap(self.display, self.fb_config, x_pixmap, pixmap_attribs.as_ptr());
            if glx_pixmap == 0 {
                bail!("Failed to create GLX pixmap after resize");
            }

            let texture = gl.glow_ctx.create_texture().map_err(|e| anyhow::anyhow!("{}", e))?;
            gl.glow_ctx.bind_texture(glow::TEXTURE_2D, Some(texture));
            gl.glow_ctx.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
            gl.glow_ctx.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
            (gl.glx_ext.bind_tex_image)(self.display, glx_pixmap, GLX_FRONT_EXT, ptr::null());
            gl.glow_ctx.bind_texture(glow::TEXTURE_2D, None);

            self.x_pixmap = x_pixmap;
            self.glx_pixmap = glx_pixmap;
            self.texture = texture;
            self.width = new_width;
            self.height = new_height;
            self.dirty = true;
        }
        Ok(())
    }

    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    pub fn acknowledge_damage(&self) {
        unsafe {
            XDamageSubtract(self.display, self.damage, 0, 0);
        }
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn texture(&self) -> glow::Texture {
        self.texture
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn source_window(&self) -> c_ulong {
        self.source_window
    }

    pub fn damage_event_base(&self) -> c_int {
        self.damage_event_base
    }

    pub fn damage_id(&self) -> c_ulong {
        self.damage
    }
}

impl Drop for WindowCapture {
    fn drop(&mut self) {
        unsafe {
            XDamageDestroy(self.display, self.damage);
            glx::glXDestroyPixmap(self.display, self.glx_pixmap);
            xlib::XFreePixmap(self.display, self.x_pixmap);
            XCompositeUnredirectWindow(self.display, self.source_window, COMPOSITE_REDIRECT_AUTOMATIC);
        }
    }
}
