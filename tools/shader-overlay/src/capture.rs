use anyhow::{bail, Result};
use glow::HasContext;
use std::os::raw::{c_int, c_ulong};
use x11::glx;
use x11::xlib;

use crate::gl_context::*;

/// Captures an X11 window via XComposite + GLX texture_from_pixmap (zero-copy).
/// The FBConfig is matched to the source window's visual ID — this is critical
/// for NVIDIA where mismatched configs cause BadPixmap errors.
pub struct WindowCapture {
    display: *mut xlib::Display,
    source_window: c_ulong,
    x_pixmap: c_ulong,
    glx_pixmap: glx::GLXPixmap,
    texture: glow::Texture,
    width: u32,
    height: u32,
    dirty: bool,
    /// FBConfig matching the source window's visual (for resize/recreate)
    capture_fb_config: glx::GLXFBConfig,
    /// Texture format: RGB for depth<=24, RGBA for depth 32
    texture_format: c_int,
    /// Whether the texture is y-inverted (common on NVIDIA)
    pub y_inverted: bool,
    // X Damage tracking
    damage: c_ulong,
    damage_event_base: c_int,
}

// XComposite and XDamage FFI
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

const COMPOSITE_REDIRECT_MANUAL: c_int = 0;
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

            // Get window attributes — we need visual ID and depth
            let mut attrs: xlib::XWindowAttributes = std::mem::zeroed();
            if xlib::XGetWindowAttributes(display, source_window, &mut attrs) == 0 {
                bail!("Failed to get window attributes for 0x{:x}", source_window);
            }
            let width = attrs.width as u32;
            let height = attrs.height as u32;
            let visual_id = (*attrs.visual).visualid;
            let depth = attrs.depth;

            log::info!(
                "Source window 0x{:x}: {}x{}, visual=0x{:x}, depth={}",
                source_window, width, height, visual_id, depth
            );

            // Find FBConfig matching this window's visual ID
            let capture_fb_config = gl.find_fbconfig_for_visual(visual_id)?;

            // Determine texture format based on depth
            let texture_format = if depth >= 32 {
                GLX_TEXTURE_FORMAT_RGBA_EXT
            } else {
                GLX_TEXTURE_FORMAT_RGB_EXT
            };
            log::info!(
                "Using texture format: {} (depth={})",
                if texture_format == GLX_TEXTURE_FORMAT_RGB_EXT { "RGB" } else { "RGBA" },
                depth
            );

            // Check if texture is y-inverted
            let mut y_inv: c_int = 0;
            glx::glXGetFBConfigAttrib(display, capture_fb_config, GLX_Y_INVERTED_EXT, &mut y_inv);
            let y_inverted = y_inv != 0;
            log::info!("Y-inverted: {}", y_inverted);

            // Subscribe to structure events on source window
            xlib::XSelectInput(display, source_window, xlib::StructureNotifyMask);

            // Redirect window for XComposite backing pixmap
            XCompositeRedirectWindow(display, source_window, COMPOSITE_REDIRECT_MANUAL);
            xlib::XSync(display, 0);

            // Get the backing pixmap
            let x_pixmap = XCompositeNameWindowPixmap(display, source_window);
            xlib::XSync(display, 0);
            if x_pixmap == 0 {
                bail!("Failed to get composite pixmap for window 0x{:x}", source_window);
            }
            log::info!("Composite pixmap 0x{:x} for window 0x{:x}", x_pixmap, source_window);

            // Create GLX pixmap with texture_from_pixmap attributes
            let pixmap_attribs: [c_int; 5] = [
                GLX_TEXTURE_TARGET_EXT, GLX_TEXTURE_2D_EXT,
                GLX_TEXTURE_FORMAT_EXT, texture_format,
                0, // terminator
            ];
            let glx_pixmap = glx::glXCreatePixmap(display, capture_fb_config, x_pixmap, pixmap_attribs.as_ptr());
            xlib::XSync(display, 0);
            if glx_pixmap == 0 {
                bail!("glXCreatePixmap failed for window 0x{:x}", source_window);
            }
            log::info!("GLX pixmap 0x{:x} created", glx_pixmap);

            // Create GL texture
            let texture = gl.glow_ctx.create_texture().map_err(|e| anyhow::anyhow!("{}", e))?;
            gl.glow_ctx.bind_texture(glow::TEXTURE_2D, Some(texture));
            gl.glow_ctx.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
            gl.glow_ctx.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);

            // Bind the GLX pixmap to the texture (zero-copy!)
            (gl.glx_ext.bind_tex_image)(display, glx_pixmap, GLX_FRONT_LEFT_EXT, std::ptr::null());
            xlib::XSync(display, 0);

            gl.glow_ctx.bind_texture(glow::TEXTURE_2D, None);

            // Create damage tracker
            let damage = XDamageCreate(display, source_window, XDAMAGE_REPORT_NON_EMPTY);
            if damage == 0 {
                bail!("Failed to create damage tracker for window 0x{:x}", source_window);
            }

            log::info!(
                "Capturing window 0x{:x} ({}x{}) via GLX texture_from_pixmap (zero-copy)",
                source_window, width, height
            );

            Ok(Self {
                display,
                source_window,
                x_pixmap,
                glx_pixmap,
                texture,
                width,
                height,
                dirty: true,
                capture_fb_config,
                texture_format,
                y_inverted,
                damage,
                damage_event_base,
            })
        }
    }

    /// On damage, release and re-bind the texture to pick up new content.
    /// glXWaitX ensures X rendering is complete before GL reads the texture.
    pub fn update_if_dirty(&mut self, gl: &GlState) {
        if !self.dirty {
            return;
        }
        unsafe {
            glx::glXWaitX();
            // Release and re-bind to get latest content
            (gl.glx_ext.release_tex_image)(self.display, self.glx_pixmap, GLX_FRONT_LEFT_EXT);
            gl.glow_ctx.bind_texture(glow::TEXTURE_2D, Some(self.texture));
            (gl.glx_ext.bind_tex_image)(self.display, self.glx_pixmap, GLX_FRONT_LEFT_EXT, std::ptr::null());
            gl.glow_ctx.bind_texture(glow::TEXTURE_2D, None);
            self.dirty = false;
        }
    }

    /// Handle window resize — recreate pixmap and GLX pixmap
    pub fn handle_resize(&mut self, gl: &GlState, new_width: u32, new_height: u32) -> Result<()> {
        if new_width == self.width && new_height == self.height {
            return Ok(());
        }
        log::info!("Window 0x{:x} resized to {}x{}", self.source_window, new_width, new_height);

        unsafe {
            // Release old GLX pixmap binding
            (gl.glx_ext.release_tex_image)(self.display, self.glx_pixmap, GLX_FRONT_LEFT_EXT);
            glx::glXDestroyPixmap(self.display, self.glx_pixmap);
            xlib::XFreePixmap(self.display, self.x_pixmap);
            gl.glow_ctx.delete_texture(self.texture);

            // Recreate composite pixmap
            let x_pixmap = XCompositeNameWindowPixmap(self.display, self.source_window);
            if x_pixmap == 0 {
                bail!("Failed to get composite pixmap after resize");
            }

            // Recreate GLX pixmap
            let pixmap_attribs: [c_int; 5] = [
                GLX_TEXTURE_TARGET_EXT, GLX_TEXTURE_2D_EXT,
                GLX_TEXTURE_FORMAT_EXT, self.texture_format,
                0,
            ];
            let glx_pixmap = glx::glXCreatePixmap(self.display, self.capture_fb_config, x_pixmap, pixmap_attribs.as_ptr());
            if glx_pixmap == 0 {
                bail!("glXCreatePixmap failed after resize");
            }

            // Recreate texture and bind
            let texture = gl.glow_ctx.create_texture().map_err(|e| anyhow::anyhow!("{}", e))?;
            gl.glow_ctx.bind_texture(glow::TEXTURE_2D, Some(texture));
            gl.glow_ctx.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
            gl.glow_ctx.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
            (gl.glx_ext.bind_tex_image)(self.display, glx_pixmap, GLX_FRONT_LEFT_EXT, std::ptr::null());
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
            XCompositeUnredirectWindow(self.display, self.source_window, COMPOSITE_REDIRECT_MANUAL);
        }
    }
}
