# Shader Presets

RetroArch-compatible `.slangp` shader presets for the gst-wayland-display compositor.

Shaders are applied at the compositor level (inside Wolf's GStreamer pipeline), so they work with **any** emulator or application — completely emulator-agnostic.

## Usage

1. Place `.slangp` preset files (and their referenced `.slang` shaders) in this directory
2. Set environment variables on the Wolf container:

```bash
# Primary output shader
GST_WD_SHADER_PRESET=/etc/wolf/shaders/crt-mattias.slangp
GST_WD_SHADER_PARAMS="CURVATURE=0.0;SCANLINE_WEIGHT=0.3"

# Secondary output shader (dual-screen mode)
GST_WD_SECONDARY_SHADER_PRESET=/etc/wolf/shaders/lcd-grid-v2.slangp
GST_WD_SECONDARY_SHADER_PARAMS="GRID_STRENGTH=0.08"
```

## Getting Shader Presets

The full RetroArch shader collection (libretro/slang-shaders) can be cloned:

```bash
git clone https://github.com/libretro/slang-shaders.git /home/gamer/shaders
```

Popular presets for emulators:
- `crt/crt-royale.slangp` — CRT simulation (good for retro consoles)
- `crt/crt-mattias.slangp` — Lightweight CRT effect
- `handheld/lcd-grid-v2.slangp` — LCD grid overlay (good for DS/3DS/PSP)
- `handheld/dot.slangp` — Dot matrix effect (good for Game Boy)
- `interpolation/sharp-bilinear-2x-prescale.slangp` — Sharp scaling

## Directory Structure

On the VM, this directory is mounted into Wolf at `/etc/wolf/shaders/`:
```
/home/gamer/shaders/     (host)
    └── mounted at /etc/wolf/shaders/ (Wolf container)
```

The deploy script creates `/home/gamer/shaders/` on the host during setup.

## Parameter Overrides

Shader parameters can be overridden via the `GST_WD_SHADER_PARAMS` env var.
Format: `PARAM1=value;PARAM2=value`

To see available parameters for a preset, check the `.slangp` file for
`parameters = "..."` lines, or look at the `.slang` source files for
`#pragma parameter` declarations.
