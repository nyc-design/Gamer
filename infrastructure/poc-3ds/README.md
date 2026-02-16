# 3DS Streaming PoC — Wolf + Azahar + Moonlight

Proof-of-concept for streaming 3DS games from a cloud GPU VM to an iPhone via the native Moonlight app.

## Architecture

```
iPhone (Moonlight app)
    ↕ Moonlight protocol (RTSP/RTP over internet)
GPU VM
    ├── Wolf (streaming server)
    │   ├── NVENC video encoding (H.265/HEVC)
    │   ├── PulseAudio → Opus audio encoding
    │   ├── inputtino virtual gamepad/touch
    │   └── Spawns Azahar container on connect
    └── Azahar container (GOW base-app)
        ├── Sway Wayland compositor
        ├── Azahar 3DS emulator
        └── ROM + saves mounted from host
```

## What This Validates

- Wolf streaming server works with NVIDIA GPU on a cloud VM
- Azahar 3DS emulator runs inside GOW base-app container under Wolf
- Native Moonlight iOS app can pair with Wolf and stream gameplay
- Gamepad input flows from Moonlight → Wolf → inputtino → Azahar
- Video encoding (NVENC HEVC) delivers playable latency over internet
- Single-screen 3DS layout (top screen) streams correctly

## Prerequisites

### GPU VM Requirements

| Requirement | Specification |
|------------|---------------|
| **GPU** | NVIDIA T4, L4, RTX 3060+ (any GPU with NVENC) |
| **RAM** | 8GB minimum |
| **Storage** | 20GB+ SSD |
| **Network** | 100Mbps+ with public IP |
| **OS** | Ubuntu 22.04 or 24.04 LTS |
| **Ports** | 47984, 47989, 48010 (TCP); 47998-48000 (UDP) |

### Recommended Cloud Providers

- **TensorDock**: Cheapest GPU VMs (~$0.10-0.30/hr for RTX 3060/T4)
- **GCP**: T4 instances in Compute Engine
- **Any provider** with NVIDIA GPU and Docker support

### iPhone Requirements

- Moonlight app from App Store
- iOS 16+ (for HEVC decode)
- MFi controller recommended (or on-screen controls)

## Quick Start

### 1. Provision a GPU VM

Spin up an Ubuntu VM with an NVIDIA GPU from your cloud provider.

### 2. Clone and run setup

```bash
# On the VM
git clone https://github.com/nyc-design/Gamer.git
cd Gamer/infrastructure/poc-3ds

# Run the setup script (installs NVIDIA driver, Docker, NVIDIA Container Toolkit)
sudo ./setup-vm.sh
```

If the NVIDIA driver was just installed, reboot and re-run:
```bash
sudo reboot
# After reboot:
cd Gamer/infrastructure/poc-3ds
sudo ./setup-vm.sh
```

### 3. Add your 3DS ROM

```bash
# Copy your ROM to the roms directory
cp /path/to/your-game.3ds /home/gamer/roms/
```

### 4. Configure the ROM filename

Edit `wolf/config.toml` and set the ROM filename in the env section:

```toml
env = [
    "ROM_FILENAME=your-game.3ds",   # ← Change this
    ...
]
```

### 5. (Optional) Add 3DS firmware

If your game requires system files:

```bash
# Place 3DS system data in the firmware directory
cp -r /path/to/sysdata/* /home/gamer/firmware/3ds/sysdata/
```

### 6. Start Wolf

```bash
docker compose up -d
```

### 7. Pair your iPhone

1. Open **Moonlight** on your iPhone
2. Tap **+** (Add Host)
3. Enter the VM's **public IP address**
4. Check Wolf logs for the pairing PIN:
   ```bash
   docker compose logs wolf | grep -i pin
   ```
5. Enter the PIN in Moonlight
6. Once paired, you'll see **"Azahar 3DS"** in the app list
7. Tap it to start streaming

## File Structure

```
poc-3ds/
├── docker-compose.yml          # Wolf + Azahar build definition
├── setup-vm.sh                 # VM setup script (driver, Docker, dirs)
├── README.md                   # This file
├── wolf/
│   └── config.toml             # Wolf config with Azahar app registration
└── azahar/
    ├── Dockerfile              # Azahar emulator container (GOW base-app)
    ├── startup-app.sh          # Entrypoint script Wolf invokes
    └── azahar-config/
        └── qt-config.ini       # Default Azahar config (streaming-optimized)
```

## Directory Layout on VM

```
/home/gamer/
├── roms/           # Your 3DS ROM files (.3ds, .cia)
├── saves/          # Azahar save files (persistent)
├── config/         # Azahar config overrides (persistent)
└── firmware/       # 3DS firmware and system files
    └── 3ds/
        └── sysdata/
```

## Troubleshooting

### Wolf won't start
```bash
# Check logs
docker compose logs wolf

# Verify NVIDIA driver and container toolkit
nvidia-smi
docker run --rm --gpus all nvidia/cuda:12.0-base nvidia-smi
```

### Can't pair from Moonlight
```bash
# Ensure ports are open
sudo ufw status
# Or check cloud provider's security group/firewall rules

# Verify Wolf is listening
ss -tlnp | grep 47984
```

### Azahar container crashes
```bash
# Check container logs
docker logs GamerAzahar

# Common issues:
# - ROM file not found: check ROM_FILENAME in config.toml matches actual file
# - GPU access: ensure NVIDIA Container Toolkit is configured
# - Wayland: Sway may need --unsupported-gpu flag (already set in GOW base)
```

### Black screen after connecting
```bash
# Azahar may need specific OpenGL settings
# Check if Sway compositor started
docker exec GamerAzahar cat /tmp/sway.log 2>/dev/null

# Try switching to Gamescope compositor in config.toml:
# Change "RUN_SWAY=1" to "RUN_GAMESCOPE=1"
```

### High latency
- Use HEVC encoding (already default) — lower bandwidth than H.264
- In Moonlight settings: reduce resolution to 720p, increase bitrate
- Ensure VM is geographically close to you
- Check `nvidia-smi` for GPU encoding load

## How It Maps to End-State Architecture

This PoC validates the core streaming pipeline from ARCHITECTURE.md:

| PoC Component | End-State Equivalent |
|---------------|---------------------|
| Manual VM setup | Provisioner API + TensorDock/GCP automation |
| Manual ROM copy | R2 rclone mount (zero-egress ROM storage) |
| Manual config edit | Gamer Agent (auto-generates Wolf config from session manifest) |
| Manual pairing | Pre-shared client cert via session manifest |
| Single ROM | Game library with MongoDB metadata |
| Local saves | GCS-backed save slots with versioning |
| No fake time | libfaketime integration (already in Dockerfile, just needs env var) |
| Single screen | Dual-screen via moonlight-web-stream (browser client) |

## Next Steps After PoC

1. **Validate streaming quality**: Test latency, visual quality, input responsiveness
2. **Test gamepad**: Pair an MFi controller, verify all 3DS buttons map correctly
3. **Test multiple games**: Try different 3DS titles for compatibility
4. **Measure cold start time**: How long from `docker compose up` to playing
5. **Test on TensorDock**: Verify the cheapest viable GPU tier for 3DS
