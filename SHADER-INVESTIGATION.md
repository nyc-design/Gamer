# Shader Overlay Investigation

## Status: TWO ISSUES FOUND (2026-02-21)

Branch: `dual-screen-fixes`

## Issue 1 (FIXED): NVIDIA Virtual Display Offset Bug

**Status: Fixed and deployed.** fix-display-positions.sh, setup-screen-mode.sh,
and start-azahar.sh all updated.

## Issue 2 (INVESTIGATING): Shader Window Not Visible to NVFBC/X

**The shader overlay renders correctly in GL (confirmed by pixel analysis of
XComposite backing store and internal frame stats), but the output window appears
pure black when captured via XGetImage or viewed through NVFBC streaming.**

This is the actual shader-specific black screen issue. Even with the display offset
bug fixed, the shader output window is all-black from the X compositor's perspective.

### Key findings for Issue 2:
- Shader window visual (0x27) differs from default visual (0x21) — both depth 24 TrueColor, only difference is stencilSize (0 vs 8)
- GLX double-buffered rendering with glXSwapBuffers IS being called (no GL errors)
- No compositor (picom/compton) is running — X has no compositing manager
- `import -window <shader_wid>` shows pure black; `import -window <azahar_wid>` shows game content
- Azahar uses visual 0x21 (default), shader uses 0x27 (selected by GLX FBConfig matching)
- Without a compositor, XGetImage on double-buffered GLX windows may not see the back buffer
- **Unclear if NVFBC can see GLX content** — NVFBC captures from GPU framebuffer directly, which should include GL-rendered windows, but needs live testing with a connected client

### Possible causes:
1. NVFBC captures composited screen but GLX-rendered windows need to be explicitly flushed to the front buffer — `glXSwapBuffers` should handle this but may have NVIDIA quirks
2. The non-default visual (0x27) causes the window to not be composited into NVFBC's capture
3. `glXSwapBuffers` is called on `glx_window` (GLXDrawable) not the actual X window — but this should be equivalent for GLX windows

### Next step:
- Have user connect via Moonlight to test if NVFBC actually sees shader content
- If still black via NVFBC: try using the default visual (0x21) for shader output windows
- If still black: try adding an explicit compositing manager (picom --backend=nvidia)

---

## Issue 1 Detail: NVIDIA Virtual Display Offset Bug

**The NVIDIA driver ignores MetaModes position offsets (+0+0, +0+1080) for virtual
displays and instead centers them in the virtual desktop.** Meanwhile, the emulator
startup script hardcodes window positions at (0,0) and (0,1080). This mismatch means
NVFBC captures empty display areas while the game renders elsewhere.

### The Bug Chain

1. `xorg.conf` specifies: `DP-0: 1920x1080 +0+0, DP-2: 1920x1080 +0+1080`
2. NVIDIA driver starts displays at **+960+840** and **+960+1920** instead (centered in 3840x3840 virtual desktop)
3. `start-azahar.sh` places windows at **(0,0)** and **(0,1080)** — hardcoded, not reading display offsets
4. Windows are NOT on the displays — they're 960px left and 840px above DP-0
5. When client connects at 1920x1080, `setup-screen-mode.sh` sees resolution already matches → **early exits** without repositioning
6. NVFBC captures display areas (960,840) and (960,1920) → sees empty root window background → **BLACK SCREEN**

### Evidence

| Test | Expected | Actual |
|------|----------|--------|
| xrandr DP-0 position after boot | +0+0 (per MetaModes) | **+960+840** |
| xrandr DP-2 position after boot | +0+1080 (per MetaModes) | **+960+1920** |
| Azahar primary window position | On DP-0 | **(0,0)** — not on DP-0 |
| Azahar secondary window position | On DP-2 | **(0,1080)** — not on DP-2 |
| Screenshot of DP-0 area (960,840) | Game content | **#090808 (nearly black)** |
| Screenshot of origin area (0,0) | Empty | **#65615E (game content!)** |
| After manual `xrandr --output DP-0 --pos 0x0` | Game visible | **Game content on DP-0** |
| Xorg log "Setting mode" | +0+0 | Confirms +0+0, but driver overrides |
| Sunshine log "Offset" | 0x0 | **960x840** — Sunshine sees the wrong offset |
| setup-screen-mode.sh at 1920x1080 | Repositions displays | **Early exit** — resolution already matches |
| Fresh container (reproduced) | Same bug | **Confirmed: always +960+840** |

### Why It Worked on the Checkpoint Branch

It likely **didn't always work** on the checkpoint either, or the user connected with a
non-1920x1080 resolution. When the client requests a different resolution (e.g., 3024x1964
from VoidLink), `setup-screen-mode.sh` proceeds past the early exit, applies the mode change
via `xrandr --output DP-0 --mode <new>`, and then repositions DP-2 with
`xrandr --output DP-2 --pos "0x${TOP_HEIGHT}"`. The mode change on DP-0 may implicitly
reset its position to (0,0) as a side effect, which would fix the bug. But at 1920x1080,
the early exit triggers and nothing gets fixed.

## Proposed Fix

Add explicit display repositioning in `setup-screen-mode.sh` BEFORE the early exit check,
or add a startup script that forces correct display positions after Xorg starts.

**Option A: Fix `setup-screen-mode.sh`** — Move the DP-2 repositioning OUTSIDE the
early-exit path, and add explicit DP-0 positioning:

```bash
# Always ensure correct display positioning, regardless of resolution match
xrandr --output DP-0 --pos 0x0 2>/dev/null || true
TOP_HEIGHT=$(xrandr --current | grep "^DP-0" | ...)
xrandr --output DP-2 --pos "0x${TOP_HEIGHT}" 2>/dev/null || true

# THEN check if resolution change is needed
if [ "$CURRENT" = "${WIDTH}x${HEIGHT}" ]; then
    # Still reposition windows even if resolution matches
    "$SCRIPT_DIR/reposition-windows.sh" &
    exit 0
fi
```

**Option B: Fix `start-azahar.sh`** — Read actual display offsets from xrandr instead
of hardcoding (0,0):

```bash
TOP_X=$(xrandr --current | grep "^DP-0" | ... | cut -d+ -f2)
TOP_Y=$(xrandr --current | grep "^DP-0" | ... | cut -d+ -f3)
xdotool windowmove $PRIMARY $TOP_X $TOP_Y
```

**Option C (Recommended): Fix BOTH + add startup display init** — Belt and suspenders:

1. Add a `fix-display-positions.sh` called at supervisor startup (after Xorg, before
   Sunshine) that forces `xrandr --output DP-0 --pos 0x0` and
   `xrandr --output DP-2 --pos 0x1080`
2. Fix `setup-screen-mode.sh` to always reposition even on early exit
3. Fix `start-azahar.sh` to read actual offsets from xrandr

This ensures correct positions regardless of when/if a client connects.

---

## What We're Building

A zero-copy shader overlay system using XComposite + GLX `texture_from_pixmap` + librashader.
The Rust binary (`tools/shader-overlay/`) captures emulator windows, applies RetroArch `.slangp`
shader presets, and renders to output windows that NVFBC captures for streaming.

## Architecture (How It Should Work)

```
Azahar window (emulator) -> XComposite backing pixmap
  -> GLX texture_from_pixmap (zero-copy GPU texture)
  -> librashader FilterChain (applies .slangp shader)
  -> Output FBO -> glBlitFramebuffer to overlay window
  -> Overlay window positioned on same display, stacked ABOVE emulator
  -> NVFBC captures composited display output (whatever is on top)
  -> Sunshine streams to VoidLink client
```

## Key Facts About NVFBC Capture

- NVFBC captures the **entire composited display output** from the GPU framebuffer
- NOT per-window -- it captures whatever is visible on that output
- Two Sunshine instances: top (port 47989, NVFBC output 0 = DP-0) and bottom (port 48089, NVFBC output 1 = DP-2)
- Therefore the shader overlay window MUST be the topmost window on the correct display for NVFBC to capture it

## Tests Performed & Results

### 1. Container Startup (PASS)
- All supervisor services running: xserver, openbox, sunshine-top, sunshine-bottom, azahar, input-remap, shader-overlay
- dbus/udev fail but non-critical (same as checkpoint which works)

### 2. Shader Overlay Logs (PASS)
- 2 pipelines active (primary "Alpha Sapphire" + secondary "Secondary Window")
- ~40fps rendering, damage events flowing
- Zero-copy capture via GLX texture_from_pixmap confirmed
- lcd3x shader loaded for both pipelines
- No errors in shader logs

### 3. Window Positions (PASS after client connect + reposition)
```
reposition-windows.sh output:
  DP-0=3024x1964+0+0  DP-2=1920x1080+0+1964
  primary 18874379 -> 0,0 3024x1964          (Azahar on DP-0)
  secondary 18874443 -> 0,1964 1920x1080     (Azahar on DP-2)
  shader-primary 6291464 -> 0,0 3024x1964    (Shader on DP-0)
  shader-secondary 6291472 -> 0,1964 1920x1080 (Shader on DP-2)
```

### 4. xdotool Regex Fix (PASS)
- Fixed `\|` -> `|` in reposition-windows.sh
- reposition-windows.sh now correctly finds all emulator AND shader windows
- Confirmed via screen-mode.log showing all 4 windows repositioned

### 5. Window Stacking Order (PASS)
```
_NET_CLIENT_LIST_STACKING (bottom to top):
  0x120000b: Azahar Primary (bottom)
  0x120004b: Azahar Secondary
  0x600010: Shader: Secondary Window
  0x600008: Shader: Alpha Sapphire (top)
```
Shader windows ARE on top of emulator windows. Correct.

### 6. Pixel Data Analysis (PASS -- shader works)
- **Azahar composite backing pixmap** (raw emulator render): Real colorful game content
- **Shader output window** (XGetImage): lcd3x pattern -- alternating black/bright pixels
- **Root window** (what the X server composites): Same lcd3x pattern as shader window
- **Conclusion**: Shader IS capturing and processing Azahar content. lcd3x IS producing output.

### 7. Shader Window Moved Off-Screen (CRITICAL FINDING)
- Moved shader primary window to (5000, 5000) -- completely off DP-0
- **User still sees black screen**
- This means **the Azahar window itself is not visible through NVFBC either**
- Rules out the shader being the problem

### 8. Display Position Mismatch (ROOT CAUSE)
- **Before fix**: DP-0 at +960+840, windows at (0,0) -- mismatch
- **After `xrandr --output DP-0 --pos 0x0`**: DP-0 at +0+0, windows at (0,0) -- match!
- Screenshot of DP-0 area BEFORE fix: nearly black (#090808)
- Screenshot of origin area (where windows actually are): colorful game content (#65615E)
- Screenshot of DP-0 area AFTER fix: colorful game content (#65615E)

### 9. setup-screen-mode.sh Early Exit (ROOT CAUSE)
- Client requests 1920x1080, DP-0 already at 1920x1080
- Script checks `$CURRENT == $REQUESTED` -> TRUE -> exits at line 24
- Never reaches DP-2 repositioning at line 70
- Never calls reposition-windows.sh

### 10. Fresh Container Reproduction (CONFIRMED)
- Started brand new container `azahar-fresh`
- Immediately checked xrandr: DP-0 at +960+840, DP-2 at +960+1920
- Bug is 100% reproducible on every container start

### 11. Sunshine NVFBC Logs (CONFIRMS MISMATCH)
```
Found [2] outputs
Virtual Desktop: 3840x3840
-- Output --
  ID: 445, Name: DP-0
  Resolution: 1920x1080, Offset: 960x840    <-- WRONG, should be 0x0
-- Output --
  ID: 611, Name: DP-2
  Resolution: 1920x1080, Offset: 960x1920   <-- WRONG, should be 0x1080
```

## Hypotheses

### CONFIRMED ROOT CAUSE
- **NVIDIA driver ignores MetaModes +0+0 for virtual displays** and centers them in the virtual desktop
- **setup-screen-mode.sh early exit** skips all repositioning when resolution matches
- **start-azahar.sh hardcodes (0,0)** for window positions instead of reading display offsets

### RULED OUT
1. ~~xdotool regex preventing window positioning~~ -- Fixed, reposition-windows.sh works
2. ~~Shader overlay not running~~ -- 2 pipelines active, rendering at 40fps
3. ~~Shader producing black output~~ -- Pixel analysis shows lcd3x output with real content
4. ~~Shader window behind emulator~~ -- Stacking order confirmed correct
5. ~~Shader window wrong size/position~~ -- Matches DP-0 exactly
6. ~~Shader window occluding with black~~ -- Moving shader off-screen didn't fix it
7. ~~NVFBC output index mismatch~~ -- Indices are correct (0=DP-0, 1=DP-2)
8. ~~Docker image build issue~~ -- Same bug with/without shaders

## Files Modified on This Branch (dual-screen-fixes)

### Shell Scripts
- `images/base-sunshine/overlay/gamer/bin/reposition-windows.sh`
  - Fixed `\|` -> `|` in xdotool regex (line 45)
  - Added `xdotool windowraise` for shader windows (stacking order)
  - Added dot cursor integration at end of script
- `images/base-sunshine/overlay/gamer/bin/set-dot-cursor.py` -- NEW, copied from checkpoint

### Rust Shader Overlay
- `tools/shader-overlay/src/overlay.rs`
  - Added `XRaiseWindow` on window creation (line 89)
  - Added `XRaiseWindow` in `reposition()` method (line 138)
- `tools/shader-overlay/src/capture.rs` -- Has visual-matched FBConfig + RGB format (from previous agent)
- `tools/shader-overlay/src/gl_context.rs` -- Has screen-depth output FBConfig (from previous agent)

## Docker Run Command (Working)

```bash
docker run -d --name azahar-shader-test \
  --runtime=nvidia --gpus all --privileged \
  -e NVIDIA_DRIVER_CAPABILITIES=all \
  -e NVIDIA_DRIVER_VERSION=570.211.01 \
  -e DUAL_SCREEN=1 \
  -e LAYOUT_OPTION=4 \
  -e AZAHAR_FULLSCREEN=0 \
  -e SUNSHINE_CAPTURE=nvfbc \
  -e SHADER_PRESET=/gamer/shaders/handheld/lcd3x.slangp \
  -e SHADER_PRESET_BOTTOM=/gamer/shaders/handheld/lcd3x.slangp \
  -e SHADER_WINDOW="Alpha Sapphire" \
  -e SHADER_WINDOW_BOTTOM="Secondary Window" \
  -v /home/gamer/roms:/home/gamer/roms \
  -v /home/gamer/firmware:/home/gamer/firmware \
  -v /home/gamer/saves:/home/gamer/saves \
  -v /home/gamer/config:/home/gamer/config \
  -v /usr/lib/x86_64-linux-gnu/nvidia:/usr/lib/x86_64-linux-gnu/nvidia:ro \
  -v /usr/bin/nvidia-smi:/usr/bin/nvidia-smi:ro \
  -v /dev/input:/dev/input \
  -v /run/udev:/run/udev:ro \
  --network host \
  gamer/azahar-sunshine:latest
```

## Checkpoint Reference

The `linux-dual-screen-checkpoint` branch at commit `3504bc8` has the working
(no-shader) baseline with deployment docs. That image was built with image-dir
context (not repo-root), has no shader-overlay binary, and uses `COPY overlay/`
instead of `COPY images/base-sunshine/overlay/`. The Dockerfile on the shader branch
(`dual-screen-fixes`) uses repo-root context because it needs `tools/shader-overlay/`.
