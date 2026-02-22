# Gamer Dual-Screen + Shader Deployment Guide

Proven working deployment of Azahar 3DS emulator with dual-screen streaming and RetroArch shader injection (lcd3x) on TensorDock RTX 4090 VM.

**Checkpoint branch**: `checkpoint-dual-screen-shader` at this commit.

## Architecture Overview

```
                     VoidLink (Moonlight fork)
                     ┌─────────────────────┐
                     │  Top Screen Client   │──── port 47989 ───┐
                     └─────────────────────┘                    │
                     ┌─────────────────────┐                    │
                     │ Bottom Screen Client │──── port 48089 ───┤
                     └─────────────────────┘                    │
                                                                │
┌───────────────────────────────────────────────────────────────┤
│  Docker Container (azahar-sunshine)           TensorDock VM   │
│                                                               │
│  supervisord                                                  │
│  ├── Xorg :0 (dual virtual displays via nvidia-xconfig)       │
│  │   ├── DP-0 (top)  ── positioned at 0,0                    │
│  │   └── DP-2 (bottom) ── positioned at 0,{top_height}       │
│  ├── openbox (window manager, positions emulator windows)     │
│  ├── pulseaudio                                               │
│  ├── Sunshine top  (NVFBC capture DP-0, port 47989)           │
│  ├── Sunshine bottom (NVFBC capture DP-2, port 48089)         │
│  ├── input-remap-proxy.py (bottom screen coord remapping)     │
│  ├── Azahar 3DS emulator (layout_option=4 = separated windows)│
│  │   ├── Main window → DP-0 (top screen)                     │
│  │   └── Secondary Window → DP-2 (bottom screen)             │
│  └── shader-inject (LD_PRELOAD, intercepts glXSwapBuffers)    │
│       ├── Primary: lcd3x on main window (source: 400x240)    │
│       └── Secondary: lcd3x on secondary (source: 320x240)    │
└───────────────────────────────────────────────────────────────┘
```

## Prerequisites

### VM Setup (TensorDock)

The VM needs:
- Ubuntu 22.04/24.04
- NVIDIA GPU (RTX 4090 proven, T4/L4 should work)
- NVIDIA driver 570+ with display mode (`nvidia-xconfig --enable-all-gpus`)
- Docker + NVIDIA Container Toolkit
- Firewall: open ports 47989-48100 (UDP+TCP) for Sunshine streaming

Run `infrastructure/poc-3ds/setup-vm.sh` for full host bootstrap.

### Host Directories

```bash
sudo mkdir -p /home/gamer/roms /home/gamer/saves /home/gamer/firmware/3ds/sysdata
sudo chown -R 1001:1001 /home/gamer
```

Copy your ROM and 3DS firmware:
```bash
scp "Pokemon Alpha Sapphire.3ds" user@VM_IP:/home/gamer/roms/
scp -r sysdata/ user@VM_IP:/home/gamer/firmware/3ds/sysdata/
```

## Building Docker Images

Images must be built on the VM (or a machine with matching GPU arch). Build context is the repo root.

```bash
# Sync code to VM
rsync -avz --exclude='.git' --exclude='node_modules' --exclude='target' \
  /workspaces/Gamer/ user@VM_IP:~/Gamer/

# SSH to VM
ssh user@VM_IP

# Build base image (includes Sunshine, Xorg, shader-inject, shader-overlay, shaders)
cd ~/Gamer
sudo docker build -t gamer/base-sunshine:latest -f images/base-sunshine/Dockerfile .

# Build Azahar emulator image (extends base)
sudo docker build -t gamer/azahar-sunshine:latest -f images/azahar-sunshine/Dockerfile .
```

Build takes ~5-10 minutes. The Rust shader-inject library compiles in a multi-stage build.

## Running the Container

### Dual-Screen with lcd3x Shader (Proven Config)

```bash
sudo docker run -d \
  --name azahar-lcd3x \
  --runtime nvidia \
  --privileged \
  --network host \
  --shm-size 64m \
  -e DUAL_SCREEN=1 \
  -e LAYOUT_OPTION=4 \
  -e AZAHAR_FULLSCREEN=0 \
  -e SUNSHINE_CAPTURE=nvfbc \
  -e SUNSHINE_USERNAME=admin \
  -e SUNSHINE_PASSWORD_BASE64=YWRtaW4= \
  -e NVIDIA_ENABLE=true \
  -e NVIDIA_DRIVER_TYPE=display \
  -e SHADER_PRESET=/gamer/shaders/handheld/lcd3x.slangp \
  -e SHADER_PRESET_BOTTOM=/gamer/shaders/handheld/lcd3x.slangp \
  -e SHADER_WINDOW=Azahar \
  -e "SHADER_WINDOW_BOTTOM=Secondary Window" \
  -e SHADER_SOURCE_SIZE=400x240 \
  -e SHADER_SOURCE_SIZE_BOTTOM=320x240 \
  -e RUST_LOG=info \
  -v /home/gamer/roms:/home/gamer/roms:ro \
  -v /home/gamer/firmware:/home/gamer/firmware:ro \
  -v /home/gamer/saves:/home/gamer/saves \
  -v /dev/input:/dev/input \
  -v /run/udev:/run/udev:ro \
  gamer/azahar-sunshine:latest
```

### Without Shaders

Remove or leave empty the `SHADER_*` env vars:

```bash
sudo docker run -d \
  --name azahar-noshader \
  --runtime nvidia --privileged --network host --shm-size 64m \
  -e DUAL_SCREEN=1 -e LAYOUT_OPTION=4 -e AZAHAR_FULLSCREEN=0 \
  -e SUNSHINE_CAPTURE=nvfbc \
  -e SUNSHINE_USERNAME=admin -e SUNSHINE_PASSWORD_BASE64=YWRtaW4= \
  -e NVIDIA_ENABLE=true -e NVIDIA_DRIVER_TYPE=display \
  -v /home/gamer/roms:/home/gamer/roms:ro \
  -v /home/gamer/firmware:/home/gamer/firmware:ro \
  -v /home/gamer/saves:/home/gamer/saves \
  -v /dev/input:/dev/input -v /run/udev:/run/udev:ro \
  gamer/azahar-sunshine:latest
```

### Single Screen Mode

Set `DUAL_SCREEN=0` — only the top Sunshine instance starts:

```bash
sudo docker run -d \
  --name azahar-single \
  --runtime nvidia --privileged --network host --shm-size 64m \
  -e DUAL_SCREEN=0 -e LAYOUT_OPTION=0 -e AZAHAR_FULLSCREEN=1 \
  -e SUNSHINE_CAPTURE=nvfbc \
  -e SUNSHINE_USERNAME=admin -e SUNSHINE_PASSWORD_BASE64=YWRtaW4= \
  -e NVIDIA_ENABLE=true -e NVIDIA_DRIVER_TYPE=display \
  -v /home/gamer/roms:/home/gamer/roms:ro \
  -v /home/gamer/firmware:/home/gamer/firmware:ro \
  -v /home/gamer/saves:/home/gamer/saves \
  -v /dev/input:/dev/input -v /run/udev:/run/udev:ro \
  gamer/azahar-sunshine:latest
```

### With libfaketime (Pokemon time spoofing)

Add `FAKETIME` env var for in-game clock spoofing:

```bash
  -e FAKETIME="2011-03-06 12:00:00" \
```

## Connecting Clients (Pairing)

### Client Software

Use **VoidLink** (Moonlight fork) for connecting. Standard Moonlight also works but VoidLink supports single-point-click for bottom screen touch.

### Pairing Process

Sunshine uses PIN-based pairing. Each screen has its own Sunshine instance with its own pairing state.

**Step 1: Add the VM in your client**

- Top screen: Add host `VM_IP` (default port 47989)
- Bottom screen: Add host `VM_IP:48089`

**Step 2: Client initiates pairing and shows a 4-digit PIN**

**Step 3: Submit the PIN via Sunshine's HTTPS API**

Top screen (port 47990 = web API, which is protocol port 47989 + 1):
```bash
curl -s -k \
  -H 'Authorization: Basic YWRtaW46YWRtaW4=' \
  -H 'Content-Type: application/json' \
  -X POST \
  -d '{"pin": "XXXX"}' \
  'https://VM_IP:47990/api/pin'
```

Bottom screen (port 48090 = protocol port 48089 + 1):
```bash
curl -s -k \
  -H 'Authorization: Basic YWRtaW46YWRtaW4=' \
  -H 'Content-Type: application/json' \
  -X POST \
  -d '{"pin": "XXXX"}' \
  'https://VM_IP:48090/api/pin'
```

**Expected response**: `{"status":true}`

**Step 4: After pairing, select "Desktop" app in VoidLink/Moonlight to start streaming**

### Pairing Troubleshooting

| Issue | Fix |
|-------|-----|
| `curl: (7) Failed to connect` | Container not running or port not open. Check `sudo docker ps` and firewall. |
| `SSL_ERROR_SYSCALL` | Wrong port. Web API is protocol_port + 1 (47990, not 47989). |
| `Content type mismatch` | Missing `-H 'Content-Type: application/json'` header. |
| `{"status":false}` | PIN expired or wrong. Re-initiate pairing from client. |
| Connection works but no video | Sunshine may not have an app running. Check `sudo docker logs <container>`. |

### CRITICAL: Port Mapping

Sunshine uses several ports per instance. With `--network host`, all are directly on the host:

| Port | Instance | Purpose |
|------|----------|---------|
| 47989 | Top | Moonlight protocol (RTSP/control) |
| 47990 | Top | HTTPS Web API (pairing, config) |
| 48010 | Top | RTSP streaming |
| 48089 | Bottom | Moonlight protocol (RTSP/control) |
| 48090 | Bottom | HTTPS Web API (pairing, config) |
| ~48100+ | Bottom | RTSP streaming |

## Environment Variable Reference

### Core

| Variable | Default | Description |
|----------|---------|-------------|
| `DUAL_SCREEN` | `1` | `1` = dual Sunshine instances, `0` = single |
| `LAYOUT_OPTION` | `4` | Azahar layout: `0`=default, `1`=single, `4`=separated windows |
| `AZAHAR_FULLSCREEN` | `0` | `0`=windowed (required for dual), `1`=fullscreen |
| `SUNSHINE_CAPTURE` | `nvfbc` | `nvfbc` (zero-copy GPU) or `x11` |
| `SUNSHINE_USERNAME` | `admin` | Sunshine web API username |
| `SUNSHINE_PASSWORD_BASE64` | `YWRtaW4=` | Base64-encoded Sunshine password |
| `ROM_FILENAME` | (empty) | ROM filename in `/home/gamer/roms/` (auto-detects if empty) |

### Shader System

| Variable | Default | Description |
|----------|---------|-------------|
| `SHADER_PRESET` | (empty) | Path to .slangp shader preset for primary window |
| `SHADER_PRESET_BOTTOM` | (empty) | Path to .slangp shader preset for secondary window |
| `SHADER_WINDOW` | (empty) | Window title substring to match for primary shader |
| `SHADER_WINDOW_BOTTOM` | (empty) | Window title substring to match for secondary shader |
| `SHADER_SOURCE_SIZE` | (empty) | Native game resolution for primary (e.g., `400x240`) |
| `SHADER_SOURCE_SIZE_BOTTOM` | (empty) | Native game resolution for secondary (e.g., `320x240`) |
| `SHADER_PASSTHROUGH` | (empty) | Set to `1` for blit-only mode (no shader, for debugging) |
| `RUST_LOG` | (empty) | Set to `info` or `debug` for shader-inject logging |

### Time Spoofing

| Variable | Default | Description |
|----------|---------|-------------|
| `FAKETIME` | (empty) | Fake time string for libfaketime (e.g., `"2011-03-06 12:00:00"`) |

### Available Shader Presets (bundled)

Located at `/gamer/shaders/` inside the container:

- `handheld/lcd3x.slangp` — LCD subpixel simulation (proven working)
- `handheld/dot.slangp` — Dot matrix effect
- `handheld/lcd-grid-v2.slangp` — LCD grid overlay
- `handheld/zfast_lcd.slangp` — Fast LCD shader
- `crt/` — CRT shaders (scanlines, curvature, etc.)
- `interpolation/` — Upscaling/smoothing shaders
- `misc/` — Miscellaneous effects

### Source Sizes by Platform

| Platform | Top/Primary | Bottom/Secondary |
|----------|-------------|------------------|
| 3DS (Azahar) | `400x240` | `320x240` |
| DS (melonDS) | `256x192` | `256x192` |
| GBA | `240x160` | N/A |
| PSP (PPSSPP) | `480x272` | N/A |
| GameCube/Wii (Dolphin) | `640x480` | N/A |

## Docker Volume Mounts

| Host Path | Container Path | Mode | Purpose |
|-----------|---------------|------|---------|
| `/home/gamer/roms` | `/home/gamer/roms` | `ro` | Game ROMs |
| `/home/gamer/firmware` | `/home/gamer/firmware` | `ro` | BIOS/firmware files |
| `/home/gamer/saves` | `/home/gamer/saves` | `rw` | Save files |
| `/dev/input` | `/dev/input` | `rw` | **Required** for input (Sunshine uinput) |
| `/run/udev` | `/run/udev` | `ro` | **Required** for input device discovery |

**CRITICAL**: The `/dev/input` and `/run/udev` mounts are mandatory. Without them, Sunshine creates uinput devices that Xorg cannot see, and all input (keyboard, mouse, controller) will be broken.

## Monitoring & Debugging

### Check container health
```bash
sudo docker ps                           # Status should be "healthy"
sudo docker logs azahar-lcd3x --tail 50  # Recent logs
```

### Check individual service logs inside container
```bash
sudo docker exec azahar-lcd3x cat /gamer/log/sunshine-top.log
sudo docker exec azahar-lcd3x cat /gamer/log/sunshine-bottom.log
sudo docker exec azahar-lcd3x cat /gamer/log/azahar.log
sudo docker exec azahar-lcd3x cat /gamer/log/xserver.log
```

### Check shader-inject is loaded
```bash
# Check if LD_PRELOAD is set in the Azahar process
sudo docker exec azahar-lcd3x bash -c 'cat /proc/$(pgrep -f AppRun.wrapped)/environ | tr "\0" "\n" | grep LD_PRELOAD'
```

Expected output should include `/gamer/lib/libshader_inject.so`.

### Check display layout
```bash
sudo docker exec -e DISPLAY=:0 azahar-lcd3x xrandr --current
```

### Restart individual services
```bash
sudo docker exec azahar-lcd3x supervisorctl restart azahar
sudo docker exec azahar-lcd3x supervisorctl restart sunshine-top
```

## Stopping & Cleanup

```bash
sudo docker stop azahar-lcd3x
sudo docker rm azahar-lcd3x
```

## How the Shader System Works

The shader system uses an LD_PRELOAD approach (`libshader_inject.so`) that intercepts `glXSwapBuffers` in the emulator process:

1. **Emulator renders frame** at its internal resolution into the GL back buffer
2. **glXSwapBuffers is intercepted** by shader-inject before the buffer swap
3. **Back buffer is downscaled** to the source resolution (e.g., 400x240) via `glBlitFramebuffer`
4. **librashader processes** the downscaled input through the shader chain, outputting to a texture at window resolution
5. **Output is blitted back** to the default framebuffer
6. **Real glXSwapBuffers** is called, presenting the shader-processed frame
7. **NVFBC captures** the final composited output from the display

This approach is NVFBC-compatible because the shader processing happens inside the emulator's GL context before the frame reaches the display — NVFBC captures the already-processed result.

The `SHADER_SOURCE_SIZE` env var tells the shader what the original game resolution is, so shaders like lcd3x can compute correct subpixel patterns based on the source-to-output scaling ratio.
