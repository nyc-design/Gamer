# screen-tool

Gamer companion overlay app for cloud-streamed emulator sessions. Runs inside the Docker container alongside the emulator and Sunshine, providing a touch-friendly egui GUI streamed to the user's device via NVFBC capture.

## Overview

screen-tool is an X11 overlay window rendered with egui + glow (OpenGL). It captures its designated display via NVIDIA's NVFBC API (zero-copy GPU capture) and provides tabs for zoom/crop viewing, performance monitoring, window management, shader selection, hotkeys, time warping, and virtual gamepad input.

Two instances run simultaneously (managed by supervisor):
- **Main instance**: Full UI with NVFBC capture, positioned on the bottom display (DP-2)
- **Toggle instance**: Tiny 52x52 cyan button (`--toggle-only`) on the top display, used to show/hide the main overlay

## Architecture

```
winit EventLoop
  └── ScreenToolApp (ApplicationHandler)
        ├── AppWindow (winit Window + raw GLX context)
        ├── GlState (glow + egui_glow rendering)
        ├── NvfbcCapture (NVFBC frame grab → GL texture)
        ├── ScreenToolGui (all egui UI logic, ~2800 lines)
        ├── LayoutManager (display geometry state machine)
        ├── WindowManager (X11 window enumeration)
        ├── SystemStatsSampler (/proc + nvidia-smi polling)
        └── Gamepad (evdev uinput virtual controller)
```

## Source Files

### `main.rs` (~670 lines)
Entry point and winit event loop. Handles:
- **`Args`** — clap CLI arguments: `--capture-output`, `--x/y/width/height`, `--max-fps`, `--toggle-only`, `--top-display`, `--bottom-display`
- **`ScreenToolApp`** — implements `winit::ApplicationHandler`. On resume: creates GLX window, initializes NVFBC, sets up GUI/layout/window managers. Each frame: grabs NVFBC frame, uploads to GL texture, renders egui, handles display switching and dynamic window resize.
- **Display switching** — when GUI's `selected_output_idx` changes, calls `NvfbcCapture::switch_output_by_id()`. 30-second manual override lockout prevents layout manager from overwriting user's selection.
- **Dynamic window resize** — queries `LayoutManager::screen_tool_geometry()` each frame; if display resolution changes (Sunshine client connect/disconnect), resizes the winit window to match.

### `gui.rs` (~2800 lines)
All egui UI rendering. Key types and sections:
- **`ScreenToolGui`** — main state struct holding all tab data, zoom/pan state, gamepad state, faketime values, shader selections.
- **`ToolTab` enum** — `Crop`, `Performance`, `Windows`, `Shaders`, `Hotkeys`, `Faketime`, `Gamepad`
- **Tab bar** — bottom-anchored `TopBottomPanel` with icon buttons for each tab + hide button
- **Crop tab** — floating toolbar (display selector ComboBox, quick presets Full/TL/TR/BL/BR, save slots), zoom slider, pan via drag, scroll wheel zoom. NVFBC texture rendered as background.
- **Performance tab** — three ring gauges (CPU/RAM/GPU) + five metric pills (Display, Resolution, Refresh, VRAM, RAM). Uses `draw_ring_gauge()` helper.
- **Windows tab** — lists X11 windows from WindowManager, shows display assignments
- **Shaders tab** — two-pane file browser (folders left, presets right) reading `/gamer/shaders/`. Footer shows current selection with "Apply" and "Clear All" buttons. Writes selection to `/tmp/shader-*.txt` files.
- **Hotkeys tab** — touch buttons for common emulator hotkeys (F1-F12, etc.) sent via xdotool
- **Faketime tab** — retro digital clock display + touch-friendly +/- spinners for year/month/day/hour/min/sec. "Reload" reads current faketime, "Apply" writes to follow-file.
- **Gamepad tab** — Xbox-style asymmetric layout: left stick upper-left, d-pad lower-left, face buttons (ABXY diamond) upper-right, right stick lower-right, triggers/bumpers at top, SEL/START centered at bottom. All buttons send events via uinput virtual gamepad.
- **Auto-scaling** — `auto_scale = sqrt((w*h)/(1100*750))` clamped to [1.0, 3.0]. All sizes use `(base * scale).clamp(min, max)` for responsive layout from 1080p to phone resolutions.

### `app_window.rs` (~276 lines)
Winit window wrapper with raw GLX context creation. Handles:
- Creating X11 window via winit with specific position/size
- GLX context setup (visual selection, context creation, makecurrent)
- Window resize and position updates

### `gl_state.rs` (~213 lines)
OpenGL state management via glow:
- egui_glow painter initialization
- NVFBC texture creation and update (GL_TEXTURE_2D from captured frames)
- Frame rendering: clear → draw NVFBC texture as fullscreen quad with zoom/pan → paint egui UI on top

### `layout.rs` (~460 lines)
Display layout state machine for dual-screen setups:
- Monitors xrandr output geometry changes (debounced 250ms ticks)
- Detects which display the emulator windows are on
- Provides `capture_output_hint` for automatic NVFBC output selection
- `screen_tool_geometry()` returns where the screen-tool window should be positioned/sized based on current display layout

### `wm.rs` (~130 lines)
X11 window manager integration:
- Enumerates all X11 windows with `XQueryTree`
- Reads window names via `_NET_WM_NAME` (handles UTF-8, e.g. Pokémon)
- Provides display assignment info for the Windows tab

### `nvfbc.rs` (~600 lines)
NVIDIA Frame Buffer Capture integration:
- Dynamic loading of `libnvidia-fbc.so.1` via libloading
- Session creation, output enumeration, frame grab to system memory
- `switch_output_by_id()` for changing captured display
- `list_outputs()` returns available displays with resolution info

### `gamepad.rs` (~150 lines)
Virtual gamepad via Linux evdev/uinput:
- Creates `/dev/uinput` device with Xbox controller layout
- `GamepadButton` enum: A/B/X/Y, DpadUp/Down/Left/Right, L/R/L2/R2/L3/R3, Start/Select
- `StickAxis` enum: LeftX/LeftY/RightX/RightY
- Sends button press/release and axis events through uinput

### `system_stats.rs` (~200 lines)
System telemetry polling:
- CPU usage from `/proc/stat`
- RAM usage from `/proc/meminfo`
- GPU utilization and VRAM from `nvidia-smi --query-gpu`
- Runs in background thread, polled every ~2 seconds

### `platform/` (linux.rs, windows.rs, mod.rs)
Platform abstraction layer. Currently only Linux is functional:
- `linux.rs`: X11/GLX display and context helpers
- `windows.rs`: stub for future Windows support

### `stream.rs` (~100 lines)
NVFBC stream configuration helpers. Manages capture parameters and output selection state.

## CLI Usage

```bash
# Main overlay (captures DP-0, positioned on bottom display)
screen-tool \
  --capture-output DP-0 \
  --x 960 --y 1920 \
  --width 1920 --height 1080 \
  --max-fps 30

# Toggle button (no capture, tiny window on top display)
screen-tool \
  --toggle-only \
  --title ScreenToolToggle \
  --x 10 --y 10 \
  --width 52 --height 52 \
  --max-fps 30
```

## Build & Deploy

**Must build on the VM** (x86_64 target). The workspace is aarch64 and cross-compilation is not set up.

```bash
# From workspace: sync source to VM
rsync -az --delete --exclude='target/' \
  tools/screen-tool/ user@VM_IP:/home/user/screen-tool-build/ \
  -e "ssh -i ~/.ssh/id_ed25519"

# On VM: build
cd /home/user/screen-tool-build
source ~/.cargo/env
cargo build --release

# Deploy: binary is bind-mounted into container
sudo docker stop azahar-screen-tool
sudo cp target/release/screen-tool /home/user/screen-tool-new
sudo docker start azahar-screen-tool
```

The binary at `/home/user/screen-tool-new` is bind-mounted as `/gamer/bin/screen-tool:ro` in the container.

## Supervisor Integration

Managed by three supervisor programs in the container:

| Program | Description |
|---------|-------------|
| `screen-tool` | Main overlay instance (priority 51) |
| `screen-tool-toggle` | Toggle button instance (priority 53) |
| `screen-tool-visibility` | Script that hides/shows the overlay via xdotool (priority 52) |

## Environment Variables

| Variable | Used By | Description |
|----------|---------|-------------|
| `SCREEN_TOOL_ENABLED` | start scripts | Set to `1` to enable screen-tool |
| `SCREEN_TOOL_WINDOW` | visibility script | Window name pattern for secondary detection |
| `DISPLAY` | all | X11 display (`:0`) |
| `DUAL_SCREEN` | layout manager | `1` for dual-display mode |
| `RUST_LOG` | env_logger | Log level (e.g. `info`) |

## Known Limitations

- **xdotool synthetic clicks don't work with winit** — cannot automate tab switching for testing. Must change default `active_tab` in code, rebuild, and deploy to verify each tab visually.
- **Build requires VM** — aarch64 workspace cannot cross-compile for x86_64 VM target.
- **Scale testing** — UI is designed for 1920x1080 base with dynamic scaling up to ~2624x1206 (iPhone via Sunshine). Some elements may need tuning at extreme resolutions.
