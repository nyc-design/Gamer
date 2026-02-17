# Dual-Screen Streaming Architecture Research

## Goal
Stream Azahar 3DS emulator (which has two screens) to two separate Moonlight clients:
- Client A (iPad): Top screen
- Client B (iPhone): Bottom screen with touch input

## Current Architecture

### Wolf Streaming Server
Wolf is a Moonlight-compatible streaming server that:
1. Runs a headless Wayland compositor (via gst-wayland-display)
2. Captures Wayland surfaces to GStreamer buffers
3. Encodes with NVENC and streams via Moonlight protocol

### gst-wayland-display Multi-Output Fork
Our fork at `github.com/nyc-design/gst-wayland-display` branch `multi-output` adds:
- `waylanddisplaysecondary` GStreamer element
- Secondary output/space in the Smithay compositor
- Window routing: first toplevel → primary space, second toplevel → secondary space

**Key code** (`wayland-display-core/src/wayland/handlers/compositor.rs:127-141`):
```rust
let use_secondary = self.multi_output_enabled
    && self.secondary_output.is_some()
    && primary_count >= 1
    && secondary_count == 0;

if use_secondary {
    self.secondary_space.map_element(window.clone(), loc, true);
} else {
    self.space.map_element(window.clone(), loc, true);
}
```

## The Problem

### Azahar is an X11 Application
Azahar 3DS emulator:
- Built with Qt/XCB (X11)
- Bundled as AppImage with its own Qt libraries
- Cannot run on pure Wayland - needs Xwayland

### Gamescope Provides Xwayland But...
When we run Azahar inside Gamescope:
1. Gamescope provides Xwayland for X11 apps
2. Azahar creates two X11 windows (SeparateWindows mode)
3. **BUT** Gamescope composites ALL X11 windows into ONE Wayland surface
4. Wolf sees only ONE toplevel → secondary output never gets used

### Without Gamescope
Setting `RUN_GAMESCOPE=0`:
- No Xwayland available
- Azahar crashes immediately (exit code 9 SIGKILL)

## Why Client-Side Crop Won't Work
We considered: Gamescope renders both screens stacked → crop at GStreamer level

Problems:
1. Different Moonlight clients request different resolutions
2. Azahar runs at different internal resolution scales (1x, 2x, 3x)
3. Crop dimensions would need to be dynamic based on session negotiation
4. GStreamer crop is applied BEFORE client resolution negotiation

## Potential Solutions

### 1. Qt Wayland Backend (QT_QPA_PLATFORM=wayland)
Check if Azahar's bundled Qt supports Wayland:
```bash
# Inside container
/Applications/azahar.AppImage --appimage-extract
ls squashfs-root/usr/plugins/platforms/
# Look for libqwayland*.so
```

If Wayland plugin exists, try:
```bash
QT_QPA_PLATFORM=wayland /Applications/azahar.AppImage --appimage-extract-and-run
```

**Risk**: AppImage may bundle XCB-only Qt.

### 2. Build Native Wayland Azahar
Fork Azahar, build with Qt6 Wayland support, create custom Docker image.

**Effort**: High - requires maintaining Azahar fork.

### 3. Standalone Xwayland (Without Gamescope)
Run Xwayland directly as a Wayland client:
```bash
Xwayland :1 &
DISPLAY=:1 /Applications/azahar.AppImage
```

**Problem**: Xwayland itself is a SINGLE Wayland surface, same as Gamescope.

### 4. Two Gamescope Instances
Run two separate Gamescope instances, each capturing one Azahar window.

**Problem**: Azahar is one process - can't split its windows across X servers.

### 5. Modify gst-wayland-display to Capture X11 Windows
Extend the fork to:
- Detect when running with Gamescope
- Use X11 APIs to enumerate windows inside Gamescope
- Capture individual X11 windows directly

**Effort**: Very high - essentially writing a new compositor.

### 6. Gamescope Output Splitting
Check if Gamescope has options for:
- Multiple Wayland outputs
- Window-to-output mapping

```bash
gamescope --help | grep -i output
```

### 7. Wolf Video Source Override with Dynamic Crop
Wolf supports `[profiles.apps.video] source = "..."` override.

Could we:
1. Get the negotiated resolution from Moonlight session
2. Calculate crop region based on 3DS aspect ratio
3. Apply crop dynamically in the GStreamer pipeline

**Needs investigation**: How Wolf passes session parameters to GStreamer.

## Wolf Configuration Reference

### Video Source Override
```toml
[profiles.apps.video]
source = "videotestsrc pattern=ball is-live=true ! video/x-raw, framerate={fps}/1"
```

Available placeholders: `{fps}`, `{width}`, `{height}`, `{color_range}`, `{color_space}`

### GStreamer Crop Element
```
videocrop top=0 bottom=480 left=0 right=0
```

## Files to Modify

| File | Purpose |
|------|---------|
| `/workspaces/Gamer/infrastructure/poc-3ds/wolf/config.toml` | Wolf app definitions |
| `/workspaces/Gamer/infrastructure/poc-3ds/azahar/startup-app.sh` | Container startup |
| `/workspaces/Gamer/infrastructure/poc-3ds/azahar/Dockerfile` | Azahar image |
| `/workspaces/Gamer/infrastructure/poc-3ds/wolf/Dockerfile.wolf-dual` | Wolf with multi-output |
| `github.com/nyc-design/gst-wayland-display` | Compositor fork |

## Testing Commands

### SSH to VM
```bash
ssh -i ~/.ssh/id_ed25519 user@206.168.81.17
```

### Check Wolf Logs
```bash
sudo docker logs poc-3ds-wolf-1 --tail 100
```

### Check Azahar Container
```bash
sudo docker ps -a | grep Azahar
sudo docker logs <container_id>
```

### Restart Wolf
```bash
cd /home/gamer && sudo docker compose down && sudo docker compose up -d
```

### Rebuild Azahar Image
```bash
cd /workspaces/Gamer/infrastructure/poc-3ds
docker build --no-cache -t gamer/azahar:poc -f azahar/Dockerfile azahar/
# Then SCP to VM and docker load
```

## Research Findings (2024-02-17)

### Qt Wayland Backend - NOT AVAILABLE
Checked Azahar AppImage plugins:
```
squashfs-root/usr/plugins/platforms/
└── libqxcb.so  (X11 only - no Wayland plugin)
```

### Gamescope Multi-Output - NOT SUPPORTED
Reviewed `gamescope --help` - no options for multiple Wayland surfaces.
Has `--xwayland-count` for multiple X servers but not useful for our case.

### gst-wayland-display Window Routing
The compositor routes windows based on **toplevel count**:
- First wl_surface → primary space → HEADLESS-1
- Second wl_surface → secondary space → HEADLESS-2

This works for **native Wayland apps** that create multiple surfaces.
It does NOT work when app is X11 wrapped by Gamescope (single surface).

## Recommended Solution Path

### Option A: X11 Window Capture Inside Gamescope (Complex)
Modify gst-wayland-display to:
1. Detect Gamescope/Xwayland environment
2. Use X11 APIs (XComposite) to enumerate windows
3. Capture specific X11 windows directly by window ID
4. Route them to separate outputs

**Effort**: Very high - needs X11 compositor expertise

### Option B: Build Native Wayland Azahar (Medium)
1. Fork Azahar repository
2. Build with Qt6 + Wayland support
3. Create custom Docker image without AppImage

**Effort**: Medium - requires maintaining fork, but straightforward

### Option C: Server-Side Crop - ARCHITECTURE LIMITATION
Wolf's GStreamer pipeline is structured as:
```
source ! video_params ! encoder ! sink
```

Where:
- `source`: Raw capture (interpipesrc or waylanddisplaysrc)
- `video_params`: Has `{width}`, `{height}`, `{fps}` parameters
- `encoder`: Codec-specific
- `sink`: RTP output

The `[profiles.apps.video] source` override only replaces the capture part.
The `{width}` and `{height}` are only available in `video_params`.

**Problem**: We'd need to crop BEFORE scaling, but crop params would need to know
the source resolution (Azahar's stacked output) which varies with internal scale.

Even if we could crop, the dimensions would be:
- Source: Azahar at 3x scale = ~1200x1440 (stacked)
- Client requests: 1920x1080
- We'd need to crop top/bottom half of source THEN scale to client resolution

This requires overriding `video_params` with hardcoded crop, breaking dynamic resolution.

### Option D: Two Azahar Processes - NOT POSSIBLE
Azahar doesn't have a "bottom screen only" mode.

### Option E: Build Native Wayland Azahar (RECOMMENDED)
1. Fork Azahar repository
2. Build with Qt6 + Wayland support (`-DCMAKE_PREFIX_PATH` with Qt6 Wayland)
3. Create custom Docker image without AppImage
4. Azahar will create two separate wl_surfaces
5. Our multi-output fork will route them correctly

**Effort**: Medium - requires maintaining fork, but straightforward
**Benefit**: Clean solution that works with existing multi-output architecture

### Option F: Modify gst-wayland-display for X11 Capture
Extend fork to capture X11 windows inside Gamescope directly.

**Effort**: Very high - needs X11/XComposite expertise

## RECOMMENDED SOLUTION: Add Xwayland Support to gst-wayland-display

### Key Discovery (2026-02-17)

**The root cause**: gst-wayland-display does NOT have Xwayland support enabled in its Smithay dependency.

Looking at `wayland-display-core/Cargo.toml`:
```toml
[dependencies.smithay]
git = "https://github.com/games-on-whales/smithay"
features = [
    "backend_drm",
    "backend_egl",
    # ... other features
    "wayland_frontend"
    # NOTE: "xwayland" feature is MISSING!
]
```

Smithay has a built-in `xwayland` feature that provides:
- `X11Wm` - X11 window manager
- `X11Surface` - Represents an X11 window
- `XwmHandler` - Trait to handle X11 window events (map, unmap, etc.)

**When Xwayland is enabled:**
1. Each X11 window becomes an `X11Surface`
2. Each `X11Surface` gets its own wl_surface via xwayland-shell protocol
3. The compositor sees them as separate toplevels
4. Our existing multi-output routing logic applies!

### Implementation Plan

1. **Add Xwayland feature to Smithay dependency**:
   ```toml
   features = [
       # existing features...
       "xwayland"
   ]
   ```

2. **Implement XwmHandler for State** (like Anvil's `shell/x11.rs`):
   ```rust
   impl XwmHandler for State {
       fn map_window_request(&mut self, _xwm: XwmId, window: X11Surface) {
           window.set_mapped(true).unwrap();
           let window = Window::new_x11_window(window);

           // Use SAME routing logic as Wayland toplevels
           let primary_count = self.space.elements().count();
           let secondary_count = self.secondary_space.elements().count();
           let use_secondary = self.multi_output_enabled
               && self.secondary_output.is_some()
               && primary_count >= 1
               && secondary_count == 0;

           if use_secondary {
               self.secondary_space.map_element(window, (0, 0), true);
           } else {
               self.space.map_element(window, (0, 0), true);
           }
       }
       // ... other handlers
   }
   ```

3. **Initialize X11Wm in compositor startup**:
   ```rust
   let xwm = X11Wm::start_wm(handle, dh, display_number)?;
   state.xwm = Some(xwm);
   ```

4. **Start Xwayland when compositor starts**:
   ```rust
   let xwayland = XWayland::new(&dh);
   xwayland.start(handle, data)?;
   ```

### Why This Works for ALL Emulators

This solution doesn't require modifying any emulator. It enables:
- **Azahar (3DS)**: X11 app → 2 X11 windows → 2 X11Surfaces → 2 outputs
- **melonDS (DS)**: Same pattern
- **Any X11 app**: Just works

### Implementation Files

| File | Changes Needed |
|------|----------------|
| `wayland-display-core/Cargo.toml` | Add `xwayland` feature |
| `wayland-display-core/src/comp/mod.rs` | Add `xwm: Option<X11Wm>` field |
| `wayland-display-core/src/wayland/handlers/mod.rs` | Add `x11.rs` module |
| `wayland-display-core/src/wayland/handlers/x11.rs` | NEW: XwmHandler impl |
| `wayland-display-core/src/lib.rs` | Initialize Xwayland on startup |

### Testing

After implementing:
1. Build gst-wayland-display with xwayland feature
2. Build wolf-dual with new compositor
3. Deploy to TensorDock VM
4. Run Azahar in SeparateWindows mode (without Gamescope!)
5. Verify two X11 windows route to separate outputs

### Fallback: Build Native Wayland Emulators

If Xwayland approach has issues, the alternative is building emulators with native Wayland:

1. Clone emulator (e.g., Azahar): `git clone https://github.com/azahar-emu/azahar`
2. Build with Qt6 Wayland:
   ```bash
   cmake -DENABLE_QT=ON -DENABLE_QT_TRANSLATION=OFF \
         -DCMAKE_PREFIX_PATH=/path/to/qt6 ..
   ```
3. Ensure Qt6 has wayland plugin: `apt install qt6-wayland`
4. Test: `QT_QPA_PLATFORM=wayland ./azahar`
5. If two windows appear as separate Wayland surfaces, success!
6. Create Docker image, deploy to VM, test with Wolf multi-output

But this requires maintaining forks of each emulator - Xwayland support in the compositor is cleaner.
