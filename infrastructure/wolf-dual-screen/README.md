# Wolf Dual-Screen Compositor Patches

Working patches for dual-screen 3DS streaming via Wolf (Games on Whales) with the `gst-wayland-display` fork.

## What This Achieves

Two separate Moonlight clients each receive one screen of a 3DS emulator (Azahar) as independent video streams:
- **Client A** → "Azahar 3DS (Dual Screen)" → top screen (launches emulator)
- **Client B** → "Azahar 3DS (Bottom Screen)" → bottom screen (receives secondary video)

## Source Repository

Fork: `nyc-design/gst-wayland-display`, branch: `feature/xwayland-support`
The base image `wolf-dual-local:xwayland-fix` contains initial Xwayland support patches.
Our patches are applied on top via `Dockerfile.fix`.

## Patches Applied (6 total)

### 1. Secondary Pipeline Caps Fix (P0 — Client B video)
**File**: `waylandsecondary_imp.rs` (waylanddisplaysecondary GStreamer element)
**Problem**: The secondary element's caps negotiation defaulted to `1x1@1fps` because interpipesink accepts any caps and GStreamer's default `BaseSrc` fixation picks minimums.
**Fix**: Added `fixate()` override that picks `1920x1080@60fps` as default. The actual resolution comes from Client B's Moonlight negotiation via Wolf's consumer pipeline.

### 2. Secondary Pipeline String (backpressure fix)
**File**: `waylandsrc_imp.rs` (waylanddisplaysrc auto-spawns secondary pipeline)
**Problem**: Original pipeline had `sync=true` on interpipesink, causing backpressure that blocked the compositor's calloop thread.
**Fix**: Changed to `sync=false async=false` with `queue max-size-buffers=4 leaky=downstream` and `drop=true max-buffers=4` on interpipesink.

### 3. Frame Callback Reordering (P1 — 1fps fix)
**File**: `comp_mod.rs` (compositor event loop)
**Problem**: Frame callbacks were sent AFTER `buffer_sender.send()` which is a `sync_channel(0)` blocking call. Xwayland clients block on frame callbacks before rendering next frame → starvation → 1fps.
**Fix**: Moved `window.send_frame()` and presentation feedback BEFORE `buffer_sender.send()` in both primary and secondary render paths.

### 4. Empty Secondary Space Guard (P1 — calloop starvation)
**File**: `comp_mod.rs` (SecondaryBuffer handler)
**Problem**: Secondary pipeline continuously rendered frames even when no windows were mapped to secondary space, wasting GPU time and calloop thread budget.
**Fix**: Added guard that returns `TemporaryFailure` when `secondary_space.elements().count() == 0`, causing the secondary element to retry with a 5ms sleep.

### 5. XWaylandClientData Fix (compositor crash)
**File**: `handlers_compositor.rs`
**Problem**: `client.get_data::<ClientState>().unwrap()` panicked for Xwayland clients because Smithay sets `XWaylandClientData` instead of `ClientState`.
**Fix**: Try `XWaylandClientData` first, fall back to `ClientState`.

### 6. Xhost + DISPLAY Fix (Azahar can't connect)
**File**: `comp_mod.rs` (Xwayland ready handler)
**Problem**: Azahar's AppImage bundles only Qt XCB plugin (not Wayland) so it needs DISPLAY env var and X11 access control disabled.
**Fix**: When Xwayland WM starts, run `xhost +local:` and set `DISPLAY=:<n>`.

## Additional Requirements

### Cargo.toml Fix
**File**: `gst_plugin_Cargo.toml`
The `cuda` feature must forward to `wayland-display-core/cuda` (was `cuda = []`).

### Docker Environment Variables
Wolf container requires:
- `GST_WD_MULTI_OUTPUT=1` — enables multi-output compositor mode
- `GST_WD_SECONDARY_SINK_NAME=secondary_video` — names the secondary interpipesink
- `GST_REGISTRY_FORK=no` — prevents GStreamer plugin scanner from running as separate process (can't resolve CUDA symbols)
- `GST_DEBUG='waylanddisplaysrc:6,waylanddisplaysecondary:6,interpipe*:4'` — useful debug logging

### Docker Image Build
```bash
cd /tmp/gst-wd-xwayland
docker build -f Dockerfile.fix -t wolf-dual-local:fixed .
```

### Wolf Config
See `wolf-config.toml` for the full Wolf configuration including:
- "Azahar 3DS (Dual Screen)" app with `start_virtual_compositor=true`
- "Azahar 3DS (Bottom Screen)" app with `interpipesrc listen-to=secondary_video` video source
- Paired client certificates

### Wolf Run Command
```bash
docker run -d --name wolf-dual --network host --privileged --runtime nvidia \
  -e NVIDIA_VISIBLE_DEVICES=all -e NVIDIA_DRIVER_CAPABILITIES=all \
  -e XDG_RUNTIME_DIR=/tmp/sockets -e HOST_APPS_STATE_FOLDER=/etc/wolf \
  -e WOLF_CFG_FILE=/etc/wolf/cfg/config.toml -e WOLF_LOG_LEVEL=DEBUG \
  -e WOLF_RENDER_NODE=/dev/dri/renderD128 \
  -e GST_WD_MULTI_OUTPUT=1 -e GST_WD_SECONDARY_SINK_NAME=secondary_video \
  -e GST_DEBUG='waylanddisplaysrc:6,waylanddisplaysecondary:6,interpipe*:4' \
  -e GST_REGISTRY_FORK=no \
  -v /etc/wolf/cfg:/etc/wolf/cfg:rw -v /var/run/docker.sock:/var/run/docker.sock:rw \
  -v /home/gamer:/home/gamer:rw -v /tmp/sockets:/tmp/sockets:rw \
  -v /dev/shm:/dev/shm:rw -v /dev/input:/dev/input:rw \
  -v nvidia-driver-vol:/usr/nvidia:rw \
  wolf-dual-local:fixed
```

## Known Remaining Issues

1. **Bottom screen latency** (75-80ms vs 30-35ms top screen) — interpipesrc/interpipesink adds latency
2. **Input not working** — keyboard, mouse, controller all non-functional on both clients
3. **Reconnect fails** — orphaned Xwayland processes accumulate; DISPLAY=:0 hardcoded in config but new session gets different display number

## VM Details

- **IP**: 206.168.81.17 (TensorDock, RTX 4090)
- **Source code**: `/tmp/gst-wd-xwayland/` on VM
- **Wolf config**: `/etc/wolf/cfg/config.toml` on VM
- **Docker images on VM**:
  - `wolf-dual-local:fixed` — current working image with all patches
  - `wolf-dual-local:xwayland-fix` — base image (used as FROM in Dockerfile.fix)
  - `gamer/azahar:poc` — Azahar 3DS emulator container
