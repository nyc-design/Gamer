# Remaining Issues — Root Cause Analysis

Status: Dual-screen streaming WORKS (both screens visible). These 3 issues remain.

---

## Issue 1: Bottom Screen Latency (75-80ms vs 30-35ms)

### Symptom
Client A (top screen) has ~30-35ms network latency. Client B (bottom screen) has ~75-80ms — about 45ms extra.

### Root Cause
The bottom screen video goes through an extra hop:

```
Top screen (direct):
  compositor → interpipesink(primary_video) → interpipesrc → cudaupload → encode → RTP

Bottom screen (extra hop):
  compositor → interpipesink(secondary_video) → interpipesrc(in config.toml) → cudaupload → encode → RTP
```

The key differences between the two interpipesrc configs:

| Setting | Top Screen (default) | Bottom Screen (config.toml) |
|---------|---------------------|-----------------------------|
| `stream-sync` | `restart-ts` | `compensate-ts` |
| `max-buffers` | `1` | `2` |
| `leaky-type` | `downstream` | _(not set)_ |
| Extra elements | None | `allow-renegotiation=true accept-events=true` |
| Queue after | None | `queue max-size-buffers=8 leaky=downstream` |

### Fix Plan
1. Change bottom screen `stream-sync` to `restart-ts` (same as top screen — lower latency)
2. Change `max-buffers` to `1` (less buffering = lower latency)
3. Add `leaky-type=downstream` (same as top screen)
4. Reduce `queue max-size-buffers` from `8` to `2` (less buffering)

Edit `/etc/wolf/cfg/config.toml` on the VM, Bottom Screen video source:
```toml
source = 'interpipesrc listen-to=secondary_video is-live=true stream-sync=restart-ts max-bytes=0 max-buffers=1 leaky-type=downstream block=false ! queue max-size-buffers=2 leaky=downstream'
```

---

## Issue 2: Input Not Working (keyboard, mouse, controller)

### Symptom
No keyboard, mouse, or controller input works on either client. ESC key worked once.

### Root Cause (confirmed from code analysis)
Wolf has TWO input paths, and they're in conflict:

**Path A — inputtino (Wolf's standard path):**
Wolf creates virtual input devices on the HOST via inputtino → devices are bind-mounted into the Azahar Docker container → app picks them up via `/dev/input/event*`.

**Path B — compositor commands (gst-wayland-display path):**
Wolf sends `Command::KeyboardInput`, `Command::PointerMotion` etc. to the compositor's calloop → compositor relays via Wayland protocol → Xwayland → X11 app.

**The problem:** Our compositor (gst-wayland-display) runs INSIDE the Wolf container, not inside the Azahar container. Wolf creates inputtino devices and mounts them in the Azahar container. But Azahar connects to our compositor via Xwayland — it expects input from the Wayland/X11 protocol, not from raw `/dev/input/` devices.

Looking at the code:
- `comp_mod.rs:315` — compositor creates a `Libinput::new_from_path()` context
- `comp_mod.rs:456` — `Command::InputDevice(path)` adds devices via `path_add_device`
- Wolf logs show `PlugDeviceEvent` for joypads but NO "Adding input device" from our compositor
- This means Wolf is NOT sending `Command::InputDevice` to our compositor

**Why keyboard partially works:** The `Command::KeyboardInput` handler exists at `comp_mod.rs:790`. Wolf MAY be sending some keyboard events directly (the ESC key working once suggests this). But mouse/controller go through the inputtino path which doesn't reach our compositor.

### Fix Plan
The compositor needs to receive the inputtino device paths. Two approaches:

**Approach A (simpler):** Make the compositor auto-discover inputtino devices. The inputtino devices are created at `/dev/input/event*` on the host. Since Wolf runs with `--privileged` and `--network host`, these devices are accessible. Add a udev/inotify monitor to the compositor that watches `/dev/input/` for new event devices and calls `path_add_device` automatically.

**Approach B (cleaner):** Wolf already sends `PlugDeviceEvent` with device info. The gst-wayland-display plugin needs to intercept these events and convert them to `Command::InputDevice(path)`. This requires modifying Wolf's C++ code or the GStreamer element to listen for device events.

**Approach C (quickest hack):** Since Wolf and the compositor run in the same container, the compositor can simply monitor `/dev/input/` for new devices using inotify. When a new `event*` file appears, add it via `path_add_device`. This doesn't require any Wolf code changes.

---

## Issue 3: Reconnect Fails ("no video from host")

### Symptom
After closing both clients and reconnecting, Client A gets "no video from host" error.

### Root Cause
Orphaned Xwayland processes accumulate across sessions. Each Wolf session spawns a new Xwayland instance. When the session ends, Xwayland may not be killed. The next session's Xwayland gets a different display number (`:1`, `:2`, etc.) but the Azahar config hardcodes `DISPLAY=:0`.

From `wolf-config.toml`:
```toml
env = [
    ...
    'DISPLAY=:0',
    ...
]
```

When Xwayland `:0` is still running from the previous session, the new session's Xwayland gets `:1`. Azahar connects to `:0` (the zombie) instead of `:1` (the live one). Result: Azahar's windows never appear in the compositor → "no video from host".

### Fix Plan
**Option A (best):** Remove `DISPLAY=:0` from config.toml. The compositor already sets `DISPLAY` dynamically in the xhost patch (`comp_mod.rs`):
```rust
let display_str = format!(":{}", display_number);
unsafe { std::env::set_var("DISPLAY", &display_str); }
```
The Azahar container inherits `WAYLAND_DISPLAY` from Wolf. If we remove the hardcoded `DISPLAY=:0`, Azahar will either:
- Use the WAYLAND_DISPLAY directly (if Qt wayland plugin is available — it's NOT for Azahar AppImage)
- Need DISPLAY set dynamically

Since the compositor sets DISPLAY in the Wolf container's env, but the Azahar container has its OWN env, this won't propagate automatically. We need the compositor to communicate the display number to Wolf so Wolf can set it in the container env.

**Option B (pragmatic):** Ensure old Xwayland processes are killed before starting a new session. Add cleanup to Wolf's session start or to the compositor startup code.

**Option C (defensive):** In the compositor's Xwayland startup, if display `:0` is taken, kill the old process first. Or configure Xwayland to always use a specific display number.
