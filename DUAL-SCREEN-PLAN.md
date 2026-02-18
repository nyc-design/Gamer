# Dual-Screen Fix Plan — Bulletproof Version

## The Two Problems (in priority order)

### P0: Client B Never Gets Video
Client B connects to "Bottom Screen" app → connection error / blank screen.

### P1: 1fps When Any Window Goes Fullscreen
Compositor renders at 1fps when Azahar window is fullscreen. Music continues (emulation isn't slow, just the video capture). Happens even on single-screen app with vanilla Wolf+Sway.

---

## P0 Diagnosis: Why Client B Fails

### What the Logs Show

From the 05:03 session:
```
05:03:21  Creating CUDA context for device /dev/dri/renderD128
05:03:21  basesink warning: Pipeline construction is invalid, please add queues (on 16697641588654413202_video)
05:03:21  Wayland display ready, listening on: wayland-1
05:03:22  Starting container: /GamerAzaharDual_16697641588654413202
05:03:47  Mounting nvidia driver nvidia-driver-vol:/usr/nvidia  ← Client B connecting
05:03:48  GLib-GObject-CRITICAL: value "-1280" of type 'gint' is invalid or out of range for property 'dest-height'
05:03:48  interpipesrc0 segment format mismatched
05:03:58  Pipeline reached End Of Stream  ← Client B's pipeline dies
```

### The Failure Chain

1. **Client A connects** → Wolf creates Dual Screen session → waylanddisplaysrc starts → compositor creates `wayland-1`
2. **Multi-output SHOULD activate** via `GST_WD_MULTI_OUTPUT=1` env var → secondary pipeline with `interpipesink name=secondary_video` SHOULD spawn
3. **Client B connects** (26 seconds later at 05:03:47) → Wolf creates Bottom Screen session → `interpipesrc listen-to=secondary_video` tries to connect
4. **`dest-height=-1280` error** → The interpipesrc IS connecting to something, but the caps/video are garbage
5. **Pipeline EOS** → Client B's pipeline dies

### Root Cause Hypothesis

The `-1280` error happens because:
- The secondary compositor output (HEADLESS-2) hasn't been initialized with proper video info yet
- The `SecondaryVideoInfo` command hasn't been sent to the compositor
- So `waylanddisplaysecondary` produces frames with uninitialized dimensions
- These garbage caps propagate through interpipe to `cudaconvertscale` which computes `dest-height=-1280`

**WHY hasn't HEADLESS-2 been initialized?** Because `SecondaryVideoInfo` is only sent when `waylanddisplaysecondary.set_caps()` is called during GStreamer caps negotiation. But if the secondary pipeline uses `video/x-raw` (RAW format) while the primary uses CUDA/DMA-BUF, caps negotiation might fail or produce wrong dimensions.

Looking at the secondary pipeline string from the binary:
```
waylanddisplaysecondary compositor-name=wayland-1 ! queue max-size-buffers=4 leaky=downstream ! interpipesink sync=false async=false name=secondary_video max-buffers=4 drop=true
```

There's **no capsfilter** between waylanddisplaysecondary and the queue. The caps are negotiated entirely by downstream demand. Since interpipesink doesn't care about caps (it passes through anything), the caps negotiation may pick an arbitrary format that doesn't match what the Bottom Screen encoder expects.

### What Needs to Happen for Client B to Work

1. Multi-output must activate (env vars → enable_multi_output → register compositor)
2. Secondary pipeline must start (waylanddisplaysecondary → interpipesink name=secondary_video)
3. Azahar must create TWO X11 windows (LAYOUT_OPTION=4, separated windows)
4. Second window must be routed to secondary space (XwmHandler routing logic)
5. HEADLESS-2 output must be created with valid video info
6. waylanddisplaysecondary must produce frames from secondary space
7. These frames flow through interpipesink → interpipesrc → encoder → Client B

**Every single link must work.** Any failure is silent.

---

## P0 Fix: Step-by-Step

### Fix 1: Add Explicit Caps to Secondary Pipeline

In `gst-plugin-wayland-display/src/waylandsrc/imp.rs`, the secondary pipeline needs explicit caps to ensure proper negotiation:

**Current** (from binary strings):
```
waylanddisplaysecondary compositor-name={socket} ! queue max-size-buffers=4 leaky=downstream ! interpipesink sync=false async=false name={sink} max-buffers=4 drop=true
```

**Fix**:
```
waylanddisplaysecondary compositor-name={socket} ! video/x-raw,format=RGBX ! queue max-size-buffers=4 leaky=downstream ! interpipesink sync=false async=false name={sink} max-buffers=4 drop=true
```

Adding `video/x-raw,format=RGBX` forces caps negotiation to use the RAW format, which is what the secondary compositor output uses by default. This prevents the garbage caps issue.

### Fix 2: Handle Missing Secondary Output Gracefully

In `waylanddisplaysecondary/imp.rs`, the `create()` method loops forever waiting for secondary output:
```rust
loop {
    let (buffer_tx, buffer_rx) = std::sync::mpsc::sync_channel(0);
    tx.send(Command::SecondaryBuffer(buffer_tx, None));
    match buffer_rx.recv() {
        Ok(Ok(buffer)) => return Ok(CreateSuccess::NewBuffer(buffer)),
        Ok(Err(err)) => {
            tracing::debug!("Secondary frame not ready yet; retrying");
            std::thread::sleep(Duration::from_millis(5));
        }
        Err(err) => return Err(gst::FlowError::Error),
    }
}
```

This loop blocks the secondary pipeline thread while waiting for the compositor's secondary output to be ready. If the compositor never creates HEADLESS-2 (because Azahar hasn't created its second window yet, or because multi-output wasn't enabled), this loop runs forever producing no output.

**This is correct behavior** — it should wait. But the interpipesink might start accepting connections from interpipesrc before waylanddisplaysecondary produces any frames, causing the Bottom Screen pipeline to get empty/garbage data.

### Fix 3: Ensure Bottom Screen Pipeline Waits for Valid Data

The Bottom Screen config should be more resilient:

**Current** (in config.toml):
```toml
[profiles.apps.video]
source = 'interpipesrc listen-to=secondary_video is-live=true stream-sync=restart-ts max-bytes=0 max-buffers=1 block=false ! queue max-size-buffers=8 leaky=downstream'
```

**Fix**: Add `allow-renegotiation=true` and `accept-events=true` to interpipesrc so it properly handles caps changes when the secondary output initializes:
```toml
[profiles.apps.video]
source = 'interpipesrc listen-to=secondary_video is-live=true stream-sync=compensate-ts max-bytes=0 max-buffers=2 block=false allow-renegotiation=true accept-events=true ! queue max-size-buffers=8 leaky=downstream'
```

Also change `stream-sync=restart-ts` to `stream-sync=compensate-ts` — restart-ts can cause timestamp discontinuities that confuse the encoder.

### Fix 4: Verify HEADLESS-2 Creation Timing

The compositor creates HEADLESS-2 output when `Command::SecondaryVideoInfo` is received. This only happens when `waylanddisplaysecondary.set_caps()` is called. But set_caps is only called AFTER GStreamer caps negotiation completes on the secondary pipeline.

If the secondary pipeline's caps negotiation fails (because there's no downstream demand to drive negotiation), HEADLESS-2 is never created.

**Potential issue**: The interpipesink may not drive caps negotiation upstream. It might accept any caps. So the secondary pipeline might go to PLAYING without ever calling `set_caps()`.

**Fix**: Force caps by adding a capsfilter as shown in Fix 1.

---

## P1 Fix: 1fps Fullscreen

### The Problem
When Azahar goes fullscreen (or any X11 window in the compositor), the stream drops to 1fps. The emulation continues normally (audio plays, game advances). This is a compositor rendering issue, not an emulator issue.

### Root Cause
Frame callback starvation. In the compositor's render loop (`comp/mod.rs`):

```rust
// Command::Buffer handler (lines 482-551)
match state.create_frame() {
    Ok((buf, render_result)) => {
        render_result.sync.wait();           // GPU sync (can block!)
        let res = buffer_sender.send(Ok(buf)); // Send to GStreamer (sync_channel, blocks!)
        // Frame callbacks sent HERE — AFTER blocking operations
        for window in state.space.elements() {
            window.send_frame(output, ...);
        }
    }
}
```

The Xwayland client (Azahar in fullscreen) calls `glXSwapBuffers()` which blocks until the compositor sends a `wl_surface.frame` callback. But the compositor only sends callbacks after:
1. GPU sync (potential block)
2. Sending buffer to GStreamer via sync_channel (blocks until GStreamer consumes)

This creates a death spiral:
- Compositor waits on GStreamer → client waits on compositor → no new frames → 1fps

### Fix
Reorder frame callback delivery in `comp/mod.rs` to happen BEFORE the GStreamer push:

```rust
match state.create_frame() {
    Ok((buf, render_result)) => {
        render_result.sync.wait();  // GPU sync (still needed)

        // Send frame callbacks FIRST
        if let Some(output) = state.output.as_ref() {
            for window in state.space.elements() {
                window.send_frame(output, state.clock.now(), Some(Duration::ZERO), |_, _| Some(output.clone()));
            }
            // presentation feedback...
        }

        // THEN push to GStreamer (if this blocks, client isn't affected)
        let res = buffer_sender.send(Ok(buf));
    }
}
```

Same change for `Command::SecondaryBuffer` handler.

**Confidence: 90%** — This is standard Wayland compositor practice. All major compositors (niri, cosmic, Mutter) send frame callbacks before handing off to display/encoder.

---

## Build & Test Plan

### Step 1: Edit Source on VM
SSH to VM, edit files in `/tmp/gst-wd-xwayland/`:

1. `gst-plugin-wayland-display/src/waylandsrc/imp.rs`:
   - Add capsfilter to secondary pipeline string (Fix 1)

2. `wayland-display-core/src/comp/mod.rs`:
   - Reorder frame callbacks before GStreamer push (Fix 4/P1)
   - In BOTH Command::Buffer AND Command::SecondaryBuffer handlers

### Step 2: Compile on VM
```bash
cd /tmp/gst-wd-xwayland
# Install Rust 1.88+ if not already
rustup update stable
cargo build --release -p gst-plugin-wayland-display
cargo build --release -p wayland-display-core
```
The compiled `.so` will be at `target/release/libgstwaylanddisplaysrc.so`

### Step 3: Build New Docker Image
```bash
# Copy compiled plugin into new image
cat > /tmp/Dockerfile.fix << 'EOF'
FROM wolf-dual-local:xwayland-fix
COPY target/release/libgstwaylanddisplaysrc.so /usr/local/lib/x86_64-linux-gnu/gstreamer-1.0/libgstwaylanddisplaysrc.so
EOF
cd /tmp/gst-wd-xwayland
sudo docker build -f /tmp/Dockerfile.fix -t wolf-dual-local:fixed .
```

### Step 4: Update Wolf Config
Edit `/etc/wolf/cfg/config.toml`:
- Bottom Screen video source: add `allow-renegotiation=true accept-events=true`, change `stream-sync=compensate-ts` (Fix 3)

### Step 5: Test
1. Start Wolf with new image + env vars
2. Connect Client A to "Dual Screen" → verify video appears
3. Check GST_DEBUG logs for "Multi-output enabled" and "Secondary pipeline started"
4. Connect Client B to "Bottom Screen" → verify video appears
5. Test fullscreen in Azahar → verify no 1fps drop

### Fallback if Compile Fails on VM
Previous attempt failed with a compilation error in wayland-display-core. If this happens:
1. Try `cargo build --release -p gst-plugin-wayland-display` only (just the GStreamer plugin, not the core lib)
2. If the core lib needs changes too, use Docker multi-stage build with the GStreamer base image

---

## Known Risks

1. **Rust 1.88 not available on VM** — Need to install via rustup
2. **Compilation error** — Previous attempt hit an error, not debugged yet
3. **CUDA caps mismatch** — The primary uses CUDA zero-copy, secondary might need to too
4. **Interpipe timing** — Client B connecting before secondary pipeline is ready
5. **XDG_RUNTIME_DIR warning** — Might affect PulseAudio but shouldn't affect video

## What NOT to Do
- Do NOT switch to Sunshine
- Do NOT use client-side cropping
- Do NOT use Gamescope (composites all windows into one)
- Do NOT interfere with the codex agent working on the same GitHub repo
