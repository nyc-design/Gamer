use anyhow::Result;
use glow::HasContext;
use std::path::Path;
use std::sync::Arc;

use librashader::presets::ShaderFeatures;
use librashader::runtime::gl::{FilterChain, FilterChainOptions, FrameOptions, GLImage};
use librashader::runtime::{Size, Viewport};

/// Wraps a librashader FilterChain for applying .slangp shader presets
pub struct ShaderPipeline {
    filter_chain: FilterChain,
    output_texture: glow::Texture,
    output_fbo: glow::Framebuffer,
    output_size: Size<u32>,
}

impl ShaderPipeline {
    pub fn new(gl: &Arc<glow::Context>, preset_path: &Path, width: u32, height: u32) -> Result<Self> {
        let (output_texture, output_fbo) = Self::create_output(gl, width, height)?;
        let filter_chain = Self::load_chain(gl, preset_path)?;

        log::info!("Loaded shader preset: {:?} (output {}x{})", preset_path, width, height);

        Ok(Self {
            filter_chain,
            output_texture,
            output_fbo,
            output_size: Size { width, height },
        })
    }

    /// Process one frame: input texture -> shader chain -> output texture
    pub fn process(&mut self, _gl: &glow::Context, input_texture: glow::Texture, input_width: u32, input_height: u32, frame_count: usize) -> Result<()> {
        let input = GLImage {
            handle: Some(input_texture),
            format: glow::RGBA8,
            size: Size { width: input_width, height: input_height },
        };

        let output = GLImage {
            handle: Some(self.output_texture),
            format: glow::RGBA8,
            size: self.output_size,
        };

        let viewport = Viewport {
            x: 0.0,
            y: 0.0,
            mvp: None,
            output: &output,
            size: self.output_size,
        };

        unsafe {
            self.filter_chain.frame(&input, &viewport, frame_count, Some(&FrameOptions::default()))
                .map_err(|e| anyhow::anyhow!("Shader frame error: {:?}", e))?;
        }

        Ok(())
    }

    /// Resize the output framebuffer
    pub fn resize_output(&mut self, gl: &Arc<glow::Context>, width: u32, height: u32) -> Result<()> {
        if width == self.output_size.width && height == self.output_size.height {
            return Ok(());
        }

        unsafe {
            gl.delete_framebuffer(self.output_fbo);
            gl.delete_texture(self.output_texture);
        }

        let (texture, fbo) = Self::create_output(gl, width, height)?;
        self.output_texture = texture;
        self.output_fbo = fbo;
        self.output_size = Size { width, height };
        Ok(())
    }

    /// Reload the shader preset (hot-reload)
    pub fn reload(&mut self, gl: &Arc<glow::Context>, preset_path: &Path) -> Result<()> {
        self.filter_chain = Self::load_chain(gl, preset_path)?;
        log::info!("Reloaded shader preset: {:?}", preset_path);
        Ok(())
    }

    pub fn output_fbo(&self) -> glow::Framebuffer {
        self.output_fbo
    }

    pub fn output_size(&self) -> Size<u32> {
        self.output_size
    }

    fn load_chain(gl: &Arc<glow::Context>, preset_path: &Path) -> Result<FilterChain> {
        let options = FilterChainOptions {
            glsl_version: 330,
            use_dsa: false,
            force_no_mipmaps: false,
            disable_cache: false,
        };

        unsafe {
            FilterChain::load_from_path(preset_path, ShaderFeatures::NONE, Arc::clone(gl), Some(&options))
                .map_err(|e| anyhow::anyhow!("Failed to load shader preset {:?}: {:?}", preset_path, e))
        }
    }

    fn create_output(gl: &glow::Context, width: u32, height: u32) -> Result<(glow::Texture, glow::Framebuffer)> {
        unsafe {
            let texture = gl.create_texture().map_err(|e| anyhow::anyhow!("{}", e))?;
            gl.bind_texture(glow::TEXTURE_2D, Some(texture));
            gl.tex_storage_2d(glow::TEXTURE_2D, 1, glow::RGBA8, width as i32, height as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
            gl.bind_texture(glow::TEXTURE_2D, None);

            let fbo = gl.create_framebuffer().map_err(|e| anyhow::anyhow!("{}", e))?;
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
            gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(texture), 0);

            let status = gl.check_framebuffer_status(glow::FRAMEBUFFER);
            if status != glow::FRAMEBUFFER_COMPLETE {
                anyhow::bail!("Framebuffer incomplete: 0x{:x}", status);
            }
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);

            Ok((texture, fbo))
        }
    }
}
