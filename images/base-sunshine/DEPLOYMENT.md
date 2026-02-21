# Sunshine Dual-Screen Deployment Guide

Working checkpoint for Linux dual-screen 3DS streaming via Sunshine + NVFBC.

## Architecture

Two Sunshine instances capture separate NVIDIA virtual displays via NVFBC:
- **Top screen**: Sunshine on port 47989, captures DP-0
- **Bottom screen**: Sunshine on port 48089, captures DP-2

Xorg runs with two virtual displays stacked vertically. Openbox WM manages
window placement. Azahar runs in separated-windows mode (layout_option=4),
placing one window per display.

## Prerequisites

- NVIDIA GPU with driver 570+ installed on host
- Docker with `nvidia-container-toolkit` (provides `--runtime=nvidia`)
- `/dev/uinput` accessible (for Sunshine virtual input devices)
- Host udevd running (creates `/dev/input/event*` nodes for uinput devices)

## Building

Build context is each image's own directory:

```bash
# Base image (Sunshine + Xorg + supervisor + openbox)
cd images/base-sunshine
docker build -t gamer/base-sunshine:latest .

# Azahar emulator image
cd images/azahar-sunshine
docker build -t gamer/azahar-sunshine:latest .
```

## Running

```bash
docker run -d --name azahar-test \
  --runtime=nvidia --gpus all --privileged \
  -e NVIDIA_DRIVER_CAPABILITIES=all \
  -e NVIDIA_DRIVER_VERSION=<host-driver-version> \
  -e DUAL_SCREEN=1 \
  -e LAYOUT_OPTION=4 \
  -e AZAHAR_FULLSCREEN=0 \
  -e SUNSHINE_CAPTURE=nvfbc \
  -e ROM_FILENAME=<rom-file.3ds> \
  -v /home/gamer/roms:/home/gamer/roms \
  -v /usr/lib/x86_64-linux-gnu/nvidia:/usr/lib/x86_64-linux-gnu/nvidia:ro \
  -v /usr/bin/nvidia-smi:/usr/bin/nvidia-smi:ro \
  -v /dev/input:/dev/input \
  -v /run/udev:/run/udev:ro \
  --network host \
  gamer/azahar-sunshine:latest
```

### Critical Volume Mounts

| Mount | Purpose |
|-------|---------|
| `/dev/input:/dev/input` | **REQUIRED for input.** Sunshine creates uinput devices at the kernel level. The host's udevd creates `/dev/input/event*` nodes for these devices, but the container's devtmpfs doesn't see them. Bind-mounting `/dev/input` makes the host-created device nodes visible inside the container so Xorg's libinput can open them. |
| `/run/udev:/run/udev:ro` | **REQUIRED for input.** Xorg's libinput needs udev device metadata to initialize input devices. Without this, libinput fails with "Failed to create a device" even if the device nodes exist. |
| NVIDIA libs + nvidia-smi | Driver userspace libraries for GPU access inside container. |
| ROMs directory | Game ROM files. |

### Why /dev/input is Needed (Not a Sunshine Bug)

Sunshine's nightly release (v2026.220.22826) fixed *touch coordinate handling* on the
protocol/client side. The `/dev/input` mount requirement is a separate, lower-level
Linux container issue:

1. Sunshine calls `uinput_create()` to make virtual input devices
2. The kernel creates the input device (visible in `/proc/bus/input/devices`)
3. The **host's** udevd creates `/dev/input/eventN` device nodes
4. But the container has its own `/dev` (devtmpfs) that doesn't see host udev events
5. Without the bind mount, Xorg inside the container can't find the device nodes
6. Result: Sunshine receives input from client, writes to uinput, but Xorg never sees it

This is standard Docker behavior for any application that creates uinput devices.
The `--privileged` flag alone is not sufficient.

## Connecting

Use VoidLink (Moonlight fork) or stock Moonlight:
- Top screen: connect to `<vm-ip>:47989`
- Bottom screen: connect to `<vm-ip>:48089`

### Pairing

Each Sunshine instance needs separate pairing. Submit PIN via API:

```bash
# Top screen (web UI on port 47990)
curl -sk -u admin:admin 'https://localhost:47990/api/pin' \
  -d '{"pin":"<PIN>"}' -H 'Content-Type: application/json'

# Bottom screen (web UI on port 48090)
curl -sk -u admin:admin 'https://localhost:48090/api/pin' \
  -d '{"pin":"<PIN>"}' -H 'Content-Type: application/json'
```

## Bottom Screen Touch Input

Touch works via VoidLink's "single point click" mode. The `input-remap-proxy.py`
handles coordinate remapping for the bottom display:

- Sunshine's absolute input is scaled to the stream resolution only
- On a dual-monitor setup, the bottom display is offset at y=TOP_HEIGHT
- The proxy grabs the bottom Sunshine instance's absolute/touch devices
- Remaps Y coordinates by adding the display offset
- Emits corrected events via a proxy uinput device

This runs at the evdev level with zero additional latency (direct event forwarding,
no buffering, no processing beyond simple integer addition on coordinates).

## Sunshine Version

v2026.220.22826 (pre-release) — includes touch fixes from PRs #4594, #4607, #4665.

## Environment Variables

| Variable | Default | Purpose |
|----------|---------|---------|
| `DUAL_SCREEN` | `0` | Set to `1` for dual-display mode |
| `LAYOUT_OPTION` | `0` | Azahar screen layout (4 = separated windows) |
| `AZAHAR_FULLSCREEN` | `1` | Set to `0` for windowed mode (required for dual) |
| `SUNSHINE_CAPTURE` | `nvfbc` | Capture method (nvfbc for NVIDIA) |
| `ROM_FILENAME` | - | ROM file name in the roms volume |
| `NVIDIA_DRIVER_VERSION` | - | Must match host driver version exactly |

## Ports

| Port | Service |
|------|---------|
| 47989 | Sunshine top (Moonlight protocol) |
| 47990 | Sunshine top (web UI / API) |
| 48089 | Sunshine bottom (Moonlight protocol) |
| 48090 | Sunshine bottom (web UI / API) |
