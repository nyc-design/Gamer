# Gamer Streaming Architecture — v3

## Summary

Cloud-streamed emulator and PC gaming platform. User picks a game in the web app, we spin up a GPU VM, launch the emulator/Steam in a Docker container via Wolf (Games on Whales), and stream video to the user's browser via moonlight-web-stream (v1) or MoQ (v2). Dual-screen emulators (DS/3DS) stream to two separate browser clients with client-side crop. ROMs live in Cloudflare R2 (zero egress fees); saves, configs, firmware, and Steam files live in GCS. A FastAPI agent on each VM orchestrates setup, and a main server handles metadata, session lifecycle, and VM management.

---

## Key Architectural Decisions

1. **Wolf replaces Sunshine + custom compositor.** Wolf includes a Smithay Wayland micro-compositor, Moonlight protocol, GStreamer+NVENC encoding, Docker app spawning, and virtual input via inputtino. No custom streaming infrastructure needed.
2. **Dual-screen solved at the web client layer.** Wolf streams a combined frame (both DS screens in one image). Two browser clients decode the full frame with WebCodecs, each crops to their assigned screen. No server-side modifications.
3. **moonlight-web-stream (Helix fork) is the v1 web client.** WebSocket transport, WebCodecs decode, binary input protocol. Production-proven by Helix. MoQ is the v2 transport — its GStreamer plugin eliminates the middleman and its pub/sub model is ideal for dual-screen fan-out.
4. **Split storage: Cloudflare R2 for ROMs, GCS for everything else.** ROMs are large (DS 16-512MB, GC 1.4GB, Wii 4.7GB, Switch 16-32GB), read-heavy, and pulled to VMs on non-GCP providers (TensorDock). R2 has zero egress fees, S3-compatible API, and no rate limits — perfect for this. Saves, configs, firmware, and Steam files are small, write-heavy, and benefit from GCS object versioning. rclone mounts both into VMs. No custom file sync, no inotify-based save management (with one exception: save event timestamps for game clock tracking).
5. **MongoDB for metadata only.** Games collection and saves collection. The web app reads MongoDB; files live in R2/GCS.
6. **One Docker image per emulator.** Shared GOW base, separate images for melonDS, Dolphin, PPSSPP, Azahar, Ryujinx, Steam. Each has baked default configs with user overrides layered on.
7. **Gamer Agent (FastAPI) on each VM** receives a session manifest from the main server, sets up rclone mounts, configures Wolf, reports session events. It does not manage files or make shutdown decisions.
8. **Main server owns VM lifecycle.** Agent pushes events (started, idle, save timestamps). Server makes all stop/destroy decisions. Server polls agents as safety net for stuck VMs.

---

## Wolf Architecture

### Components

| Component | What It Does | Source |
|-----------|-------------|--------|
| **Wolf** (C++) | Moonlight protocol server, session management, event bus, Docker orchestration | `games-on-whales/wolf` |
| **gst-wayland-display** (Rust) | Smithay-based headless Wayland micro-compositor, exposes framebuffer to GStreamer | `games-on-whales/gst-wayland-display` |
| **inputtino** (C++) | Virtual input library — mouse, keyboard, gamepad (gyro/accel/touchpad via uhid), touch | `games-on-whales/inputtino` |
| **GStreamer pipeline** | Video/audio encoding (NVENC, VAAPI, QuickSync), RTP packetization, FEC | Configurable via `config.toml` |

### Session Lifecycle

1. Browser/Moonlight client connects to Wolf
2. Wolf creates: virtual Wayland compositor, PulseAudio sink, virtual input devices
3. Wolf spawns emulator Docker container with access to compositor, audio, and input
4. GStreamer encoding pipeline starts (NVENC)
5. Video/audio streams to client, input flows back
6. On disconnect: container stopped, resources cleaned up

### What Wolf Handles (We Don't Build)

- Moonlight protocol (pairing, RTSP, RTP streaming, ENET input)
- Virtual display creation (headless Wayland, any resolution/FPS)
- Video encoding (NVENC H.264/HEVC/AV1, zero-copy CUDA pipeline)
- Audio (PulseAudio virtual sinks, Opus encoding)
- Input injection (keyboard, mouse, gamepad with gyro, touch — all via uinput/uhid)
- Docker orchestration (spawns/stops app containers, GPU passthrough)
- Multi-session isolation (each session: own compositor, audio sink, input devices)
- Hotplug (gamepad connect/disconnect via fake-udev)

---

## Storage Architecture

### Why Split Storage?

GCS charges ~$0.12/GiB for internet egress (data leaving Google's network). This is free when accessed from GCP VMs in the same region, but TensorDock and other non-GCP providers cross the public internet. ROMs are large and read-heavy — a moderate usage pattern of 500GB/month in ROM downloads to TensorDock would cost ~$60/month in GCS egress alone.

Cloudflare R2 has zero egress fees to any destination, ever. It's S3-compatible, works natively with rclone, has no hard rate limits on its S3 API, and storage is $0.015/GB/month. The tradeoff: R2 doesn't have GCS-style object versioning or the same write-path integration with GCP services.

The split: R2 for ROMs (large, read-heavy, zero egress matters), GCS for everything else (small, write-heavy, versioning matters).

### Cloudflare R2 — ROMs

```
r2://gamer-roms/{user_id}/
    ├── pokemon-black.nds
    ├── smash-melee.iso
    └── tears-of-the-kingdom.xci
```

- User uploads ROMs via web app → stored in R2
- Read-only from VM perspective
- Zero egress regardless of VM provider (GCP, TensorDock, etc.)
- rclone mount with `--read-only` and aggressive VFS cache

### GCS — Saves, Configs, Firmware, Steam

```
gs://gamer-data/{user_id}/
    ├── saves/                             ← Save files, organized by game + slot
    │   ├── pokemon-black/
    │   │   ├── trainer-1/
    │   │   │   └── pokemon-black.sav
    │   │   ├── shiny-hunt/
    │   │   │   └── pokemon-black.sav
    │   │   └── nuzlocke/
    │   │       └── pokemon-black.sav
    │   └── smash-melee/
    │       └── default/
    │           └── smash-melee.gci
    ├── configs/                           ← User config overrides (persisted between sessions)
    │   ├── melonds/
    │   │   └── melonDS.ini
    │   └── dolphin/
    │       └── GFX.ini
    ├── firmware/                          ← BIOS, firmware, keys
    │   ├── ds/
    │   │   ├── bios7.bin
    │   │   ├── bios9.bin
    │   │   └── firmware.bin
    │   ├── 3ds/
    │   │   └── sysdata/
    │   └── switch/
    │       ├── prod.keys
    │       └── firmware/
    └── steam/                             ← Steam game installs + config
        ├── steamapps/
        │   └── common/
        │       └── KINGDOM HEARTS/
        └── config/                        ← Steam login tokens, settings
```

GCS object versioning enabled on `saves/` for rollback safety.

### Cost Comparison (500GB ROM downloads/month to TensorDock)

```
GCS only:       $60/month egress (500GB × $0.12)
R2 + GCS split: $0 egress (R2 ROMs) + ~$1 (GCS for small saves/configs) = ~$1/month
Savings:        ~$59/month
```

On GCP VMs (same region): GCS egress is free, so the split only matters for TensorDock usage. But R2 is still cheaper storage ($0.015 vs $0.020/GB/mo) and avoids any risk if we add more non-GCP providers later.

### rclone Mount Strategy

Two rclone mounts on the host VM: one to R2 (ROMs), one to GCS (everything else). Wolf bind-mounts host directories into emulator containers.

```
R2 (ROMs)                        Host VM                    Container
─────────                        ───────                    ─────────
r2://.../roms/       ──rclone──→ /mnt/roms/     ──bind──→ /home/retro/roms/      (ro)

GCS (saves, configs, firmware, steam)
────────────────────────────────
gs://.../saves/      ──rclone──→ /mnt/saves/    (agent copies selected slot)
gs://.../configs/    ──rclone──→ /mnt/configs/  ──bind──→ /home/retro/config/    (rw)
gs://.../firmware/   ──rclone──→ /mnt/firmware/ ──bind──→ /home/retro/firmware/  (ro)
gs://.../steam/      ──rclone──→ /mnt/steam/    ──bind──→ /home/retro/.steam/    (rw)
```

**Save files get special handling.** The emulator expects its save at a fixed path (e.g., melonDS always writes `game.sav` to its save dir). But we have multiple save slots. So:

1. Gamer Agent copies the selected save slot from `/mnt/saves/{game}/{slot}/game.sav` to `/mnt/emusaves/game.sav`
2. Wolf bind-mounts `/mnt/emusaves/` into the container as `/home/retro/saves/`
3. Emulator reads/writes `/home/retro/saves/game.sav`
4. On session end, Agent copies `/mnt/emusaves/game.sav` back to `/mnt/saves/{game}/{slot}/game.sav`
5. rclone flushes to GCS

```bash
# R2 mount — ROMs (read-only, aggressive cache)
rclone mount r2:gamer-roms/{user_id}/ /mnt/roms/ \
  --vfs-cache-mode full --vfs-cache-max-size 50G --read-only --daemon

# GCS mounts — saves, configs, firmware
rclone mount gcs:gamer-data/{user_id}/saves/ /mnt/saves/ \
  --vfs-cache-mode full --vfs-cache-max-size 5G --daemon

rclone mount gcs:gamer-data/{user_id}/configs/ /mnt/configs/ \
  --vfs-cache-mode full --vfs-cache-max-size 1G --daemon

rclone mount gcs:gamer-data/{user_id}/firmware/ /mnt/firmware/ \
  --vfs-cache-mode full --vfs-cache-max-size 5G --read-only --daemon
```

### Steam Storage

Steam is different — it manages its own game downloads and saves.

```bash
# Steam mounts (read-write, larger cache)
rclone mount gcs:gamer-data/{user_id}/steam/ /mnt/steam/ \
  --vfs-cache-mode full --vfs-cache-max-size 100G \
  --vfs-read-ahead 128M --vfs-write-back 5s --daemon
```

- **Game files:** Steam downloads via its own CDN, writes through rclone to GCS. Subsequent sessions: rclone cache fills from GCS, Steam verifies, launches.
- **Login tokens:** Stored in `/mnt/steam/config/`. First session: user logs in through the stream. Subsequent sessions: auto-login from cached tokens.
- **Saves:** Steam Cloud handles sync for supported games. Local saves also backed up via GCS (the `.steam/` mount covers everything).

---

## Data Model (MongoDB)

### `games` Collection

```json
{
  "_id": "pokemon-black",
  "title": "Pokemon Black",
  "emulator": "melonds",
  "r2_rom_path": "roms/pokemon-black.nds",
  "emulator_image": "ghcr.io/gamer/melonds:latest",
  "default_config": {
    "screen_layout": 1,
    "screen_sizing": 0
  },
  "controller_override": null,
  "requires_firmware": true,
  "firmware_dir": "ds"
}
```

```json
{
  "_id": "kingdom-hearts",
  "title": "Kingdom Hearts",
  "emulator": "steam",
  "steam_app_id": 2552430,
  "emulator_image": "ghcr.io/gamer/steam:latest",
  "default_config": {},
  "controller_override": null,
  "requires_firmware": false
}
```

### `saves` Collection

```json
{
  "_id": "trainer-1-pokemon-black",
  "game_id": "pokemon-black",
  "user_id": "me",
  "name": "Trainer 1",
  "gcs_save_path": "saves/pokemon-black/trainer-1/pokemon-black.sav",
  "save_filename": "pokemon-black.sav",
  "fake_time": {
    "epoch": "2011-03-06T08:00:00",
    "accumulated_seconds": 14400
  },
  "play_time_total_seconds": 14400,
  "last_played": "2026-02-05T22:30:00Z"
}
```

```json
{
  "_id": "default-kingdom-hearts",
  "game_id": "kingdom-hearts",
  "user_id": "me",
  "name": "Default",
  "gcs_save_path": null,
  "save_filename": null,
  "fake_time": null,
  "play_time_total_seconds": 7200,
  "last_played": "2026-02-04T18:00:00Z"
}
```

Notes:
- `fake_time` is optional. Only set for games that need time spoofing (Pokemon).
- `gcs_save_path` is null for Steam games (Steam Cloud handles saves).
- `play_time_total_seconds` tracks total playtime for display in the web app.

---

## Session Manifest

When the user hits Play, the main server builds a manifest and makes it available to the Gamer Agent:

```json
{
  "session_id": "abc123",
  "user_id": "me",
  "vm_token": "xyz789",

  "emulator_image": "ghcr.io/gamer/melonds:latest",
  "container_network_mode": "host",

  "rom_path": "roms/pokemon-black.nds",
  "save_path": "saves/pokemon-black/trainer-1/pokemon-black.sav",
  "save_filename": "pokemon-black.sav",
  "firmware_dir": "ds",

  "fake_time": "2011-03-06T12:00:00",

  "emulator_config": {
    "screen_layout": 1,
    "screen_sizing": 0
  },
  "controller_override": null,

  "resolution": "1920x1080",
  "fps": 60,
  "codec": "hevc",

  "client_cert": "-----BEGIN CERTIFICATE-----\n...",

  "dual_screen": {
    "enabled": true,
    "top": {"x": 0, "y": 0, "w": 1024, "h": 768},
    "bottom": {"x": 0, "y": 768, "w": 1024, "h": 768}
  }
}
```

Steam session manifest is simpler (no rom_path, save_path, firmware_dir, fake_time, dual_screen):

```json
{
  "session_id": "def456",
  "user_id": "me",
  "vm_token": "xyz789",
  "emulator_image": "ghcr.io/gamer/steam:latest",
  "container_network_mode": "host",
  "steam_app_id": 2552430,
  "resolution": "1920x1080",
  "fps": 60,
  "codec": "hevc",
  "client_cert": "-----BEGIN CERTIFICATE-----\n..."
}
```

---

## Emulator Container Images

### Image Hierarchy

```
ghcr.io/games-on-whales/base-app:edge     ← GOW base (Wayland, PulseAudio, GPU libs)
    ├── ghcr.io/gamer/base:latest          ← Our base (+ libfaketime, common utils)
    │   ├── ghcr.io/gamer/melonds:latest   ← DS
    │   ├── ghcr.io/gamer/dolphin:latest   ← GameCube/Wii
    │   ├── ghcr.io/gamer/ppsspp:latest    ← PSP
    │   ├── ghcr.io/gamer/azahar:latest    ← 3DS
    │   ├── ghcr.io/gamer/ryujinx:latest   ← Switch
    │   └── ghcr.io/gamer/steam:latest     ← Steam (Proton for Windows titles)
```

### Our Base Image

```dockerfile
FROM ghcr.io/games-on-whales/base-app:edge
RUN apt-get update && apt-get install -y libfaketime
```

libfaketime is in the base — it's only activated when the `FAKETIME` env var is set.

### Example: melonDS Image

```dockerfile
FROM ghcr.io/gamer/base:latest
RUN apt-get update && apt-get install -y melonds
COPY melonds-config/ /defaults/config/
COPY entrypoint.sh /entrypoint.sh
ENTRYPOINT ["/entrypoint.sh"]
```

### Entrypoint Script Pattern

```bash
#!/bin/bash

# libfaketime (optional — only if FAKETIME env var is set by Gamer Agent)
if [ -n "$FAKETIME" ]; then
    export LD_PRELOAD=/usr/lib/faketime/libfaketime.so
    export FAKETIME_NO_CACHE=1  # time advances normally from fake start
fi

# Copy baked defaults on first run (user overrides persist in /home/retro/config/)
if [ ! -f /home/retro/config/melonDS.ini ]; then
    cp -r /defaults/config/* /home/retro/config/
fi

exec melonDS --config-dir /home/retro/config/ "/home/retro/roms/${ROM_FILENAME}"
```

### Steam Image

```dockerfile
FROM ghcr.io/gamer/base:latest
RUN curl -s https://steamcdn-a.akamaihd.net/client/installer/steamcmd_linux.tar.gz | tar xz -C /usr/local/bin
COPY entrypoint.sh /entrypoint.sh
ENTRYPOINT ["/entrypoint.sh"]
```

```bash
#!/bin/bash
if [ -n "$FAKETIME" ]; then
    export LD_PRELOAD=/usr/lib/faketime/libfaketime.so
    export FAKETIME_NO_CACHE=1
fi

# Launch Steam in Big Picture mode
# -steamdeck flag enables Proton by default and gives controller-friendly UI
exec steam -bigpicture -steamdeck
```

First session: user logs in through the stream. Credentials cached in `/home/retro/.steam/` (rclone-mounted to GCS). Subsequent sessions: auto-login.

For faster launches, the Gamer Agent can run `steamcmd +login {user} +app_update {app_id} +quit` headlessly before starting Wolf, using cached credentials. Game downloads while Wolf starts up; by the time the user connects, the game may already be installed.

### Container Networking

All containers use `network_mode: host`. Required for:
- Online multiplayer in emulated games
- Steam login, game downloads, Steam Cloud sync
- Future non-emulator use cases

Single-user ephemeral VM means no isolation concerns.

### Config Strategy

- **Baked defaults** in `/defaults/config/` inside each image — sane out-of-box experience
- **User overrides** persist in `/home/retro/config/` via rclone mount to `gs://.../configs/{emulator}/`
- **Per-session overrides** via env vars (resolution, layout) set by Gamer Agent from the session manifest
- **Non-negotiable settings** (renderer backend, GPU selection) forced in the entrypoint script regardless of config file contents

### Firmware/BIOS

Required by certain emulators:
- **melonDS:** DS BIOS (`bios7.bin`, `bios9.bin`) + firmware (`firmware.bin`)
- **Azahar:** 3DS system archives
- **Ryujinx:** Switch `prod.keys` + firmware dumps

Stored in `gs://.../firmware/{platform}/`, rclone-mounted read-only to `/home/retro/firmware/`.

---

## libfaketime — In-Game Clock Spoofing

### Why

Some games (Pokemon) use real-time clock for gameplay mechanics (seasons, day/night, berry growth). When running on a cloud VM with real 2026 time, the game world is wrong. libfaketime intercepts all userspace time calls and returns a spoofed value.

### How It Works

```
Server stores per save slot:
{
  "fake_time": {
    "epoch": "2011-03-06T08:00:00",      ← game world start date
    "accumulated_seconds": 14400           ← total play time so far
  }
}

On session start, server computes:
  fake_current = epoch + accumulated_seconds
  → "2011-03-06T12:00:00"

Gamer Agent passes to container:
  FAKETIME="2011-03-06 12:00:00"
  LD_PRELOAD=/usr/lib/faketime/libfaketime.so
  FAKETIME_NO_CACHE=1

Result: emulator sees time starting at "2011-03-06 12:00:00" and advancing normally.
If user plays for 2 hours, game clock advances to "2011-03-06 14:00:00".
```

### Time Tracking

The game clock only advances while the player is actively playing and saving. Tracking uses inotify on the save file — the one exception to "no inotify":

```python
# Gamer Agent watches the emulator save directory
watcher = inotify.adapters.Inotify()
watcher.add_watch("/mnt/emusaves/")

for event in watcher.event_gen():
    if event and "IN_CLOSE_WRITE" in event[1]:
        requests.post(f"{API_URL}/api/session/{session_id}/save_event", json={
            "wall_clock": datetime.utcnow().isoformat()
        })
```

Server on each save event:

```python
elapsed = save_event.wall_clock - session.started_at
save.accumulated_seconds = session.base_accumulated + elapsed  # replace, not increment
save.last_played = save_event.wall_clock
save.save()
```

The replace (not increment) makes it idempotent — saves at 1h, 2h, 3h set accumulated time to 3h, not 1+2+3.

If the VM dies or session never ends cleanly, accumulated time is accurate up to the last in-game save.

**Note:** libfaketime doesn't affect filesystem mtime. mtime is set by the kernel, not by libc time calls. So inotify + mtime work correctly even with libfaketime active.

**Games that don't need fake time:** The Gamer Agent simply doesn't set the `FAKETIME` env var. No `LD_PRELOAD`, no interception, real system time.

---

## Gamer Agent (FastAPI on VM)

### Responsibilities

- Receive session manifest from main server
- Set up rclone mounts (R2 for ROMs, GCS for saves/configs/firmware/steam)
- Copy selected save slot to emulator working directory
- Write Wolf `config.toml`
- Start Wolf container
- Watch save directory with inotify (only for games with `fake_time`)
- Report session events to main server (started, save events, idle)
- Expose `/health` endpoint for server polling
- Copy save back to slot on session end

### Boot Sequence

```
1. VM boots → Gamer Agent starts (systemd)
2. GET /api/session/{vm_token} → receives manifest
3. Set up rclone mounts (roms, saves, configs, firmware, steam)
4. Copy selected save slot to /mnt/emusaves/
5. Write Wolf config.toml:
   - App image from manifest
   - FAKETIME env var (if present)
   - ROM_FILENAME, container network mode
   - Client pairing cert
   - GStreamer pipeline config (codec, resolution)
6. Start Wolf container (docker compose up)
7. POST /api/session/{session_id}/started
   { "started_at": "2026-02-06T14:00:00Z" }
8. Start inotify watcher on /mnt/emusaves/ (if fake_time game)
9. Monitor Wolf for client disconnects
10. If no clients for 10 minutes:
    POST /api/session/{session_id}/idle
    { "last_client_disconnect": "2026-02-06T15:30:00Z" }
11. On shutdown signal from main server:
    - Copy save back from /mnt/emusaves/ to /mnt/saves/{game}/{slot}/
    - Wait for rclone flush
    - POST /api/session/{session_id}/ended
      { "ended_at": "2026-02-06T15:32:00Z" }
```

### Health Endpoint

```python
@app.get("/health")
def health():
    return {
        "status": "ok",
        "connected_clients": get_wolf_active_sessions(),
        "idle_since": last_disconnect_time,
        "session_duration": time.time() - session_started_at,
        "gpu_utilization": get_nvidia_smi_util()
    }
```

The main server polls this as a safety net.

---

## VM Lifecycle

### Provisioning

User hits Play → Main server provisions GPU VM via cloud API (TensorDock or GCP). VM has a pre-baked image with:
- Ubuntu 22.04 + NVIDIA driver
- Docker + NVIDIA Container Toolkit
- Gamer Agent (Python + dependencies)
- Wolf Docker image (pre-pulled)
- Emulator Docker images (pre-pulled)
- rclone

### Lifecycle Management: Agent Push + Server Poll

```
Normal flow (agent push — handles 99% of cases):
  Agent → POST /session/{id}/started         ← server marks VM active
  Agent → POST /session/{id}/save_event      ← server updates game time
  Agent → POST /session/{id}/idle            ← server stops VM immediately

Safety net (server poll — catches edge cases):
  Every 15 min: for each VM marked "running" in DB
    GET http://{vm_ip}:8000/health
    - No response → stop VM (crashed agent)
    - idle_since > 10 min → stop VM (agent failed to report idle)
    - session_duration > 8h → stop VM (hard cap safety)

  Every 24h: for each VM marked "stopped" in DB
    - stopped > 48h → destroy VM
```

### Why Agent Doesn't Self-Shutdown

The agent runs on the VM it would be stopping. If it crashes, hangs, or the shutdown call fails, the VM runs forever burning money. The main server is the only entity that calls the cloud provider API to stop/destroy VMs.

### Reconnection

If the browser tab closes or WiFi drops, the VM keeps running. The web app shows a "Reconnect" button with the same session URL if the VM is still alive. The 10-minute idle timeout gives the user time to reconnect.

### GPU Tier Selection

Different emulators need different GPU power. The main server selects VM spec based on the emulator:

```
melonDS, PPSSPP       → T4 (cheap, DS/PSP are lightweight)
Dolphin, Azahar       → T4 or L4
Ryujinx               → L4 or A10G (Switch emulation is heavy)
Steam                  → L4 or A10G (depends on game)
```

The `games` collection can have a `gpu_tier` field to override per game.

---

## Dual-Screen Streaming

### The Constraint

Wolf's compositor (`gst-wayland-display`) is single-output: one framebuffer, one GStreamer pipeline, one video stream per session. The Moonlight protocol is also single-stream. Neither supports multiple outputs natively.

### Solution: Client-Side Crop via moonlight-web-stream

Wolf streams a combined frame (both DS screens in one image). Two browser clients each decode the full frame with WebCodecs, then crop:

```
melonDS (top+bottom in one window, e.g., 1024x1536)
    ↓ single Wayland surface
Wolf compositor → single framebuffer → NVENC encode
    ↓ Moonlight RTP
moonlight-web-stream (Rust server on VM)
    ↓ H264 NAL units via WebSocket (to both clients)
┌────────────────────┐    ┌────────────────────┐
│ Browser A (iPad)   │    │ Browser B (iPhone)  │
│ decode full frame  │    │ decode full frame    │
│ crop: top 768px    │    │ crop: bottom 768px   │
│ D-pad/button input │    │ Touch input          │
└────────────────────┘    └────────────────────┘
```

Crop config comes from the session manifest. The JavaScript is trivial:

```javascript
ctx.drawImage(frame,
  cropX, cropY, cropW, cropH,  // source rect (which screen)
  0, 0, canvas.width, canvas.height)  // dest (full canvas)
```

### Audio Routing

Audio plays on one device only (top-screen iPad). Bottom-screen iPhone is muted. The web client config specifies `"audio": true/false` per client.

### Native Moonlight Dual-Screen (Future)

Moonlight apps don't support client-side crop. For native dual-screen, the path is server-side crop in the GStreamer pipeline — two Wolf instances, each with `videocrop`:

```
Wolf A: waylanddisplaysrc → videocrop top=0 bottom=768 → nvh265enc → session A
Wolf B: waylanddisplaysrc → videocrop top=768 bottom=0 → nvh265enc → session B
```

Not v1. Evaluate if there's demand.

---

## Web Client: moonlight-web-stream (Helix Fork)

### Architecture

```
Wolf (NVENC H264 encode)
    ↓ Moonlight protocol (RTSP + RTP)
moonlight-web-stream (Rust server on same VM)
    ↓ Extracts H264 NAL units from RTP stream
    ↓ Binary WebSocket frames
Browser (TypeScript client)
    ↓ WebCodecs API (hardware-accelerated H264 decode)
    ↓ Render to <canvas> (with optional crop for dual-screen)
    ↑ Input events (keyboard, mouse, touch, gamepad)
    ↑ Binary WebSocket frames back to server
    ↑ Server translates to Moonlight input protocol → Wolf → inputtino
```

### Why Helix's Fork

Helix (helix.ml) built and battle-tested Wolf + browser streaming for their AI coding agent platform:
1. Started with WebRTC (original moonlight-web-stream) — TURN server issues, 80% success rate
2. Replaced WebRTC with WebSockets — H264 NAL units as binary frames, 100% success rate
3. Added JPEG fallback for extremely constrained networks

### What We Fork + Add

| Component | Source | Our Additions |
|-----------|--------|--------------|
| Rust WebSocket server | `helixml/moonlight-web-stream` | Multi-client per session, crop config per client, input coordinate translation |
| TypeScript browser client | `helixml/moonlight-web-stream` | Canvas crop rendering, touch-to-DS-bottom mapping, dual-device session join UI, audio routing |

### Browser Compatibility

WebCodecs API required for hardware-accelerated H264 decode:
- Chrome 94+ ✅
- Safari 16.4+ ✅ (iOS 16.4+)
- Firefox 130+ ✅

### Native Moonlight (Secondary)

Native Moonlight apps can still connect directly to Wolf for single-screen use cases. Useful for Apple TV, Android TV, or users who prefer native performance. Requires pre-shared certificate pairing (no PIN exchange on ephemeral VMs).

---

## Future Transport: Media over QUIC (MoQ)

### Why MoQ

MoQ has a **GStreamer plugin** (`moq-dev/gstreamer`) that can plug directly into Wolf's encoding pipeline as a `moqsink` element, **eliminating the moonlight-web-stream middleman**:

```
v1: Wolf → Moonlight RTP → moonlight-web-stream → WebSocket → Browser
v2: Wolf → GStreamer moqsink → MoQ relay → WebTransport/WebSocket → Browser
```

Key advantages:

| Feature | moonlight-web-stream | MoQ |
|---------|---------------------|-----|
| Server-side process | Required (bridges Moonlight → WebSocket) | None (GStreamer plugin publishes directly) |
| Multi-client fan-out | Custom code (we fork + add) | Native pub/sub (built into protocol) |
| CDN | Self-hosted WebSocket server only | Cloudflare global CDN or self-hosted `moq-relay` |
| Congestion handling | None (TCP buffers → latency spikes) | Drops stale frames, stays at live edge |

### Why MoQ's Congestion Model Is Right For Gaming

The Helix blog received a comment noting that WebSocket (TCP) is actually better for AI agent streaming because you never want to drop output. This is correct for that use case — you need every character the AI types.

**Game streaming is the opposite.** At 60fps, if the network congests:
- **WebSocket (TCP):** Buffers unsent frames, delivers in order. User sees a freeze, then fast-forward through stale frames. Feels like input lag — the worst possible gaming experience.
- **MoQ (QUIC with group dropping):** Drops stale frames, skips to newest. User sees a brief visual glitch but stays at the live edge. Input remains responsive.

MoQ's "aggressive dropping" is a feature for game streaming, not a bug.

### How It Would Work

```
Wolf GStreamer pipeline:
  waylanddisplaysrc → nvh265enc → moqsink url=https://relay.example.com

Input back-channel (separate — MoQ is media-delivery only):
  Browser → WebSocket → VM input server → Wolf inputtino → virtual devices
```

Dual-screen is trivial with MoQ's pub/sub: one Wolf publisher, two browser subscribers, each crops independently. No custom fan-out code.

### Browser Compatibility

- **WebTransport** (optimal): Chrome 97+ ✅, Firefox 114+ ✅, Safari ❌ (experimental flag only)
- **WebSocket fallback**: All browsers ✅ (MoQ provides `web-transport-ws` polyfill)
- **WebCodecs**: Chrome 94+ ✅, Safari 16.4+ ✅, Firefox 130+ ✅

### Migration Plan

- **v1 (now):** Ship with moonlight-web-stream. Proven, fast to ship.
- **v2 (3-6 months):** Proof-of-concept: Wolf + moqsink → moq-relay → browser. Measure latency.
- **v3 (future):** MoQ for all browser clients. Moonlight protocol retained only for native apps (Apple TV, Android TV).

### References

- MoQ: https://moq.dev
- MoQ GStreamer plugin: https://github.com/moq-dev/gstreamer
- Cloudflare MoQ relay: https://blog.cloudflare.com/moq/
- moq-relay (self-hosted): https://github.com/moq-dev/moq/tree/main/rs/moq-relay

---

## Input Handling

### Touch → DS Bottom Screen

```
User touches iPad screen
  → moonlight-web-stream translates touch coords based on crop config
  → sends as Moonlight touch event to Wolf
  → inputtino creates virtual touchscreen device (/dev/uinput)
  → emulator container receives touch via SDL2/libinput
  → melonDS maps to DS bottom screen based on layout config
```

### Gamepad → Emulator

```
User presses button on MFi controller
  → Moonlight sends gamepad state (Xbox 360 layout)
  → Wolf → inputtino creates virtual Xbox/PS gamepad (uhid)
  → supports gyro, acceleration, touchpad (DualSense)
  → emulator receives via evdev/SDL2
  → emulator maps to console controls per its config
```

### Zero Per-Emulator Input Code

inputtino creates standard Linux input devices (evdev). Emulators read via standard Linux APIs (libinput, SDL2). No per-emulator integration needed. Controller mapping is handled by each emulator's existing config.

Controller overrides per game can be set in the `games` MongoDB collection and passed through the session manifest.

---

## Wolf Configuration

### config.toml (Generated by Gamer Agent)

```toml
hostname = "gamer-vm"
support_hevc = true
config_version = 2

[[paired_clients]]
client_cert = "-----BEGIN CERTIFICATE-----\n..."

[paired_clients.settings]
controllers_override = ["XBOX"]

[[profiles]]
id = "session-profile"

[[profiles.apps]]
title = "Pokemon Black"
start_virtual_compositor = true
start_audio_server = true

[profiles.apps.runner]
type = "docker"
name = "GamerMelonDS"
image = "ghcr.io/gamer/melonds:latest"
mounts = [
    "/mnt/roms:/home/retro/roms:ro",
    "/mnt/emusaves:/home/retro/saves:rw",
    "/mnt/configs/melonds:/home/retro/config:rw",
    "/mnt/firmware/ds:/home/retro/firmware:ro"
]
env = [
    "GOW_REQUIRED_DEVICES=/dev/input/event* /dev/dri/* /dev/nvidia*",
    "ROM_FILENAME=pokemon-black.nds",
    "FAKETIME=2011-03-06 12:00:00",
    "NVIDIA_DRIVER_CAPABILITIES=all",
    "NVIDIA_VISIBLE_DEVICES=all"
]
devices = []
ports = []
base_create_json = """
{
    "HostConfig": {
        "IpcMode": "host",
        "NetworkMode": "host",
        "CapAdd": ["NET_RAW", "MKNOD", "NET_ADMIN", "SYS_NICE"],
        "DeviceCgroupRules": ["c 13:* rmw", "c 244:* rmw"]
    }
}
"""
```

---

## Deployment

### VM Image (Pre-baked)

```
Ubuntu 22.04 LTS
+ NVIDIA driver (matched to cloud provider GPU)
+ Docker + NVIDIA Container Toolkit
+ rclone
+ Gamer Agent (Python + FastAPI + dependencies)
+ Wolf Docker image (pre-pulled)
+ All emulator Docker images (pre-pulled)
+ moonlight-web-stream Docker image (pre-pulled)
```

### Docker Compose (on VM)

```yaml
version: "3"
services:
  wolf:
    image: ghcr.io/games-on-whales/wolf:stable
    environment:
      - NVIDIA_DRIVER_CAPABILITIES=all
      - NVIDIA_VISIBLE_DEVICES=all
      - WOLF_RENDER_NODE=/dev/dri/renderD128
    volumes:
      - /etc/wolf/:/etc/wolf
      - /var/run/docker.sock:/var/run/docker.sock:rw
      - /dev/:/dev/:rw
      - /run/udev:/run/udev:rw
    device_cgroup_rules:
      - 'c 13:* rmw'
    devices:
      - /dev/dri
      - /dev/uinput
      - /dev/uhid
    deploy:
      resources:
        reservations:
          devices:
            - driver: nvidia
              count: all
              capabilities: [gpu]
    network_mode: host
    restart: unless-stopped

  moonlight-web-stream:
    image: ghcr.io/gamer/moonlight-web-stream:latest
    network_mode: host
    environment:
      - WOLF_HOST=127.0.0.1
    restart: unless-stopped
```

### Cloud Provider Requirements

| Requirement | TensorDock | GCP |
|------------|-----------|-----|
| GPU | RTX 3060+ (NVENC) | T4, L4, or A10G |
| RAM | 8-16GB | 8-16GB |
| Storage | 50-100GB SSD | 50-100GB SSD |
| Network | 100Mbps+ | 100Mbps+ |
| OS | Ubuntu 22.04 | Ubuntu 22.04 |
| Ports | 443 (HTTPS/WSS for browser). Wolf ports (47984-48200) on localhost for moonlight-web-stream ↔ Wolf. Optional: open Wolf ports for native Moonlight fallback. | Same |

---

## Risk Register

| Risk | Severity | Mitigation |
|------|----------|------------|
| Wolf stability on cloud GPUs | Medium | Test per cloud provider. Pre-bake validated driver versions. Helix runs Wolf in production — reference their configs. |
| NVENC session limits | Low | Single-user ephemeral VMs. One session per VM. Not a concern. |
| WebCodecs browser compatibility | Low | Safari 16.4+, Chrome 94+, Firefox 130+. Covers >95% of targets. |
| Dual-screen touch accuracy | Medium | Touch coord translation depends on crop alignment with emulator layout. Build debug overlay. Per-emulator calibration. |
| WebSocket latency vs native Moonlight | Medium | ~5-10ms added. Acceptable for emulated games. MoQ migration in v2 addresses this. Native Moonlight available as fallback. |
| Emulator compatibility with headless Wayland | Medium | Test each emulator in Wolf's compositor. Some may need Sway intermediary (`RUN_SWAY=1`). |
| Save file corruption | Low | GCS object versioning for automatic rollback. Save-then-copy pattern prevents partial writes. |
| VM cost runaway | Medium | Agent reports idle → server stops VM. Server polls every 15min as safety net. 8h hard cap. 48h stopped VM cleanup. |
| rclone mount performance (Steam small files) | Medium | `--vfs-cache-mode full` with aggressive caching. If insufficient, fall back to tar archive approach for game installs. |
| Steam login token expiry | Low | Tokens cached in GCS. If expired, user re-authenticates through the stream. Infrequent. |
| Emulator version breaking config/saves | Medium | Pin emulator versions in Docker tags. `games` collection can specify image version. Test before rolling forward. |
| Switch emulation GPU requirements | Medium | Ryujinx needs L4/A10G. Tiered GPU selection prevents over/under-provisioning. |
| Firmware/BIOS legal gray area | Medium | User provides their own. Stored in their GCS space. We never distribute. |
| VM cold start time | Medium | 2+ minutes (provision + boot + mount + Wolf). Good loading UX in web app. Pre-warm VM pool as future optimization. |
| GCS egress on non-GCP VMs | Low | Only saves/configs/firmware cross internet to TensorDock — all small files (<1MB typically). ROMs served from R2 with zero egress. |

---

## Implementation Timeline

### Phase 1: Core Pipeline (Weeks 1-3)
- **Week 1:** Build base + melonDS + Dolphin Docker images. Test with Wolf locally on GPU machine.
- **Week 2:** Fork Helix's moonlight-web-stream. Get single-client WebSocket streaming working end-to-end.
- **Week 3:** Build Gamer Agent (rclone mounts, Wolf config gen, session manifest, health endpoint).

### Phase 2: Dual-Screen + More Emulators (Weeks 4-6)
- **Week 4:** Multi-client moonlight-web-stream. Two browsers, same Wolf session, client-side crop.
- **Week 5:** Touch coordinate translation for DS bottom screen. PPSSPP + Azahar images.
- **Week 6:** Ryujinx (Switch) image. libfaketime integration. Save slot copy logic.

### Phase 3: Product Layer (Weeks 7-9)
- **Week 7:** Main server (FastAPI): MongoDB models, session manifest API, VM provisioning.
- **Week 8:** Web app: game library, save slot management, ROM upload, session launcher.
- **Week 9:** Integration: Web App → API → cloud provisioning → Agent → Wolf → browser.

### Phase 4: Steam + Hardening (Weeks 10-12)
- **Week 10:** Steam image. rclone mount for Steam library. Login flow. `steamcmd` pre-download.
- **Week 11:** VM lifecycle (idle detection, server polling, cost controls). Reconnection handling.
- **Week 12:** MoQ proof-of-concept. Multi-region testing. Polish.

---

## What We Build vs. What We Use

### Must Build

| Component | Description |
|-----------|-------------|
| **Gamer Agent** | FastAPI on VM: session manifest, rclone mounts, Wolf config, health endpoint, save slot management |
| **Main Server** | FastAPI: MongoDB CRUD, session lifecycle, cloud VM provisioning, VM health polling, billing |
| **Web App** | Game library, save slots, ROM upload, session launcher, embedded streaming player |
| **Emulator Docker Images** | Per-emulator images with baked configs and entrypoint scripts |
| **moonlight-web-stream fork** | Multi-client, crop config, touch translation, audio routing |

### Use As-Is

| Component | Source |
|-----------|--------|
| **Wolf** | `ghcr.io/games-on-whales/wolf:stable` |
| **Moonlight** | iOS/platform native apps (secondary client path) |
| **rclone** | R2 + GCS mounting |
| **libfaketime** | Time spoofing |
| **Cloudflare R2** | ROM storage (zero egress) |
| **GCS** | Saves, configs, firmware, Steam storage |
| **MongoDB** | Metadata |

---

## References

### Core Components
- Wolf: https://github.com/games-on-whales/wolf
- Wolf Docs: https://games-on-whales.github.io/wolf/stable/
- gst-wayland-display: https://github.com/games-on-whales/gst-wayland-display
- inputtino: https://github.com/games-on-whales/inputtino

### Web Streaming
- Helix moonlight-web-stream fork: https://github.com/helixml/moonlight-web-stream
- Helix blog — "We Killed WebRTC": https://blog.helix.ml/p/we-killed-webrtc-and-nobody-noticed
- Original moonlight-web-stream: https://github.com/MrCreativ3001/moonlight-web-stream

### Media over QUIC (MoQ)
- MoQ: https://moq.dev
- MoQ GStreamer plugin: https://github.com/moq-dev/gstreamer
- Cloudflare MoQ relay: https://blog.cloudflare.com/moq/

### Emulators
- melonDS 1.0 RC: https://gbatemp.net/threads/melonds-emulator-version-1-0-rc-released-adds-multi-window-support-and-more.663502/
- Azahar (3DS): https://github.com/azahar-emu/azahar

### Storage
- Cloudflare R2 docs: https://developers.cloudflare.com/r2/
- R2 pricing: https://developers.cloudflare.com/r2/pricing/
- R2 rclone setup: https://developers.cloudflare.com/r2/examples/rclone/
- GCS pricing: https://cloud.google.com/storage/pricing

### Moonlight Protocol
- Moonlight FAQ: https://github.com/moonlight-stream/moonlight-docs/wiki/Frequently-Asked-Questions
- moonlight-common-c: https://github.com/moonlight-stream/moonlight-common-c