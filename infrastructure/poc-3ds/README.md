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
| **Ports** | 47984, 47989, 47999, 48010 (TCP); 47998-48010, 48100, 48200 (UDP) |

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

Spin up an Ubuntu VM with an NVIDIA GPU. For GCP:

```bash
gcloud compute instances create gamer-poc-3ds \
    --project=YOUR_PROJECT \
    --zone=us-central1-a \
    --machine-type=g2-standard-4 \
    --accelerator=count=1,type=nvidia-l4 \
    --boot-disk-size=50GB \
    --image-family=ubuntu-2204-lts \
    --image-project=ubuntu-os-cloud \
    --maintenance-policy=TERMINATE

# Open Moonlight ports (GCP firewall)
gcloud compute firewall-rules create allow-moonlight \
    --project=YOUR_PROJECT \
    --allow=tcp:47984,tcp:47989,tcp:47999,tcp:48010,udp:47998-48010,udp:48100,udp:48200 \
    --target-tags=allow-moonlight
gcloud compute instances add-tags gamer-poc-3ds --tags=allow-moonlight --zone=us-central1-a
```

### 2. Clone and run setup

```bash
# On the VM
git clone https://github.com/nyc-design/Gamer.git
cd Gamer/infrastructure/poc-3ds

# Run the setup script (installs NVIDIA driver, Docker, NVIDIA Container Toolkit)
sudo ./setup-vm.sh
```

### 2.5 Configure storage remotes (R2 + GCS via rclone)

After base VM setup, configure `rclone` remotes used by the Gamer architecture:

```bash
cd Gamer/infrastructure/poc-3ds

sudo env \
  R2_ACCOUNT_ID=... \
  R2_ACCESS_KEY_ID=... \
  R2_SECRET_ACCESS_KEY=... \
  R2_ENDPOINT=https://<account_id>.r2.cloudflarestorage.com \
  R2_BUCKET_NAME=gamer-roms \
  GCS_BUCKET_NAME=gamer-data \
  GCS_SERVICE_ACCOUNT_JSON_B64="$(cat gcs-service-account.json | base64 -w0)" \
  ./setup-rclone.sh
```

This writes `/etc/rclone/rclone.conf` with remotes:
- `r2` (Cloudflare R2 for ROMs)
- `gcs` (Google Cloud Storage for saves/configs/firmware/steam)

If the NVIDIA driver was just installed, reboot and re-run:
```bash
sudo reboot
# After reboot:
cd Gamer/infrastructure/poc-3ds
sudo ./setup-vm.sh
```

### 3. Create NVIDIA driver volume (critical)

Wolf uses the "driver volume" approach to inject NVIDIA libraries into spawned containers. This must be done once after the NVIDIA driver is installed:

```bash
# Get your driver version
NV_VERSION=$(nvidia-smi --query-gpu=driver_version --format=csv,noheader | head -1)
echo "Driver version: $NV_VERSION"

# Build the driver extraction image and create volume
curl -s "https://raw.githubusercontent.com/games-on-whales/wolf/stable/scripts/nvidia-driver-container/Dockerfile" | \
    docker build -t gow/nvidia-driver:latest --build-arg NV_VERSION=$NV_VERSION -

# Populate the volume
docker volume create nvidia-driver-vol
docker create --rm --mount source=nvidia-driver-vol,destination=/usr/nvidia gow/nvidia-driver:latest sh

# Verify it worked
docker run --rm -v nvidia-driver-vol:/usr/nvidia:ro alpine ls /usr/nvidia/lib/
# Should list: libnvidia-allocator.so, libnvidia-egl-wayland.so, etc.
```

> **Why this matters**: Without this volume, spawned app containers won't have GPU access. The driver volume approach is the Wolf-recommended method for NVIDIA — it avoids conflicts between host and container NVIDIA library versions.

### 4. Enable nvidia_drm.modeset (for zero-copy pipeline)

```bash
# Check current status
cat /sys/module/nvidia_drm/parameters/modeset
# If it shows "N":
echo 'options nvidia-drm modeset=1' | sudo tee /etc/modprobe.d/nvidia-drm.conf
sudo update-initramfs -u
sudo reboot
```

> Zero-copy pipeline (GStreamer CUDA DMA buffers) requires DRM/KMS via GBM. Without `modeset=1`, Wolf falls back to a slower copy-based pipeline.

### 5. Add your 3DS ROM

```bash
# Copy your ROM to the roms directory (use a simple filename — avoid special chars)
cp /path/to/your-game.3ds /home/gamer/roms/pokemon-alpha-sapphire.3ds
```

### 6. Configure the ROM filename

Edit `wolf/config.toml` and set the ROM filename in the env section:

```toml
env = [
    "ROM_FILENAME=pokemon-alpha-sapphire.3ds",   # ← Change this
    ...
]
```

### 7. (Optional) Add 3DS firmware

If your game requires system files:

```bash
# Place 3DS system data in the firmware directory
cp -r /path/to/sysdata/* /home/gamer/firmware/3ds/sysdata/
```

### 8. Copy config and start Wolf

```bash
# Wolf reads config from /etc/wolf/cfg/ — copy the repo config there
sudo mkdir -p /etc/wolf/cfg
sudo cp wolf/config.toml /etc/wolf/cfg/

# Build the Azahar image and start Wolf
docker compose build azahar
docker compose up -d
```

### 9. Pair your iPhone

1. Open **Moonlight** on your iPhone
2. Tap **+** (Add Host)
3. Enter the VM's **public IP address**
4. Moonlight will prompt for a PIN
5. Check Wolf logs for the pairing PIN and secret:
   ```bash
   docker compose logs wolf 2>&1 | grep -i 'pin'
   ```
   Look for a line like: `Please insert pin: 1234`
6. Enter the PIN in Moonlight
7. Once paired, you'll see **"Azahar 3DS"** and **"Azahar 3DS (Settings)"** in the app list
8. Tap **"Azahar 3DS"** to start streaming

## Azahar App Modes (recommended workflow)

This PoC now defines two Azahar app entries:

- **Azahar 3DS** — gameplay mode (starts fullscreen with ROM)
- **Azahar 3DS (Settings)** — windowed settings mode (launches Azahar menu without ROM)

Use **Settings** mode when you need to:
- adjust controller bindings
- change graphics/audio/UI options
- use Azahar menu actions safely

Then switch back to **Azahar 3DS** for gameplay.

> Why: toggling Azahar fullscreen on/off *during* a stream can leave stale/frozen regions
> in the captured output in Sway/Wolf. Using dedicated Settings vs Gameplay app modes
> avoids this capture-resize artifact.

## File Structure

```
poc-3ds/
├── docker-compose.yml              # Wolf + Azahar build definition
├── setup-vm.sh                     # VM setup script (driver, Docker, dirs)
├── README.md                       # This file
├── wolf/
│   └── config.toml                 # Wolf config with Azahar app registration
└── azahar/
    ├── Dockerfile                  # Azahar emulator container (GOW base-app)
    ├── startup-app.sh              # Entrypoint script Wolf invokes
    ├── 30-nvidia-readonly-safe.sh  # Read-only-safe nvidia init (replaces base-app's)
    └── azahar-config/
        └── qt-config.ini           # Default Azahar config (streaming-optimized)
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

If Wolf logs show:
- `error 400 - {"message":"Duplicate mount point: /run/udev"}`

remove any explicit `"/run/udev:/run/udev:ro"` mount from the app runner mounts.  
Wolf already injects its own udev mount for app containers.

If logs show errors like:
- `cp: cannot create regular file '/usr/share/egl/...': Read-only file system`

this comes from base-app's NVIDIA init script trying to copy EGL/Vulkan files
into a read-only container rootfs. This PoC image includes a read-only-safe
`/etc/cont-init.d/30-nvidia.sh` override that skips those copy operations and
uses `/usr/nvidia/...` paths directly via env vars.

If controller hotplug fails with errors around `fake-udev`, verify host path:

```bash
ls -l /etc/wolf/fake-udev
file /etc/wolf/fake-udev
```

It must be an executable file (not a directory).

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

## Key Technical Decisions & Lessons Learned

These are hard-won insights from the initial bring-up. They document *why* things are configured the way they are.

### Wolf volume mount MUST be `/etc/wolf:/etc/wolf`

Wolf inspects its own Docker container mounts to resolve container-internal paths to host paths. It needs to resolve:
- `/etc/wolf` → host path (for app state, config, fake-udev)
- `XDG_RUNTIME_DIR` → host path (for Wayland socket passed to spawned containers)

If you mount to a subdirectory (e.g., `./wolf:/etc/wolf/cfg`), Wolf logs:
```
ERROR | Unable to find docker mount for path: /etc/wolf
ERROR | Unable to find docker mount for path: /tmp/sockets
```
...and spawned containers fail with "Can't connect to a Wayland display" because Wolf can't pass the Wayland socket mount correctly.

### NVIDIA driver volume approach (not `runtime: nvidia` on Wolf)

Wolf supports two NVIDIA approaches:
1. **Container Toolkit** (`runtime: nvidia`, `--gpus=all`) — Wolf container itself runs with NVIDIA runtime
2. **Driver Volume** (`NVIDIA_DRIVER_VOLUME_NAME=nvidia-driver-vol`) — Wolf mounts a pre-populated volume of NVIDIA libs into spawned containers

We use approach 2 (driver volume) because:
- Wolf's internal NVIDIA library discovery is more reliable with it
- Avoids `libnvidia-egl-wayland.so` version mismatch errors between host and container
- The Wolf docs recommend it as the primary NVIDIA approach
- Zero-copy pipeline works correctly with it

Note: `config.toml`'s `base_create_json` may still include `"Runtime": "nvidia"` for the spawned Azahar containers themselves — that's separate from Wolf's own container config.

### Read-only rootfs in spawned containers

Wolf starts app containers with a read-only root filesystem. The GOW `base-app:edge` image includes a `30-nvidia.sh` init script that tries to `cp` EGL/Vulkan config files into `/usr/share/egl/...`. This fails on read-only rootfs with:
```
cp: cannot create regular file '/usr/share/egl/egl_external_platform.d/10_nvidia_wayland.json': Read-only file system
```

Our fix: the Azahar Dockerfile replaces `30-nvidia.sh` with a read-only-safe version (`30-nvidia-readonly-safe.sh`) that skips the copy and sets environment variables to point EGL/Vulkan lookups directly to the driver volume paths:
- `__EGL_VENDOR_LIBRARY_DIRS=/usr/nvidia/share/glvnd/egl_vendor.d`
- `__EGL_EXTERNAL_PLATFORM_CONFIG_DIRS=/usr/nvidia/share/egl/egl_external_platform.d`
- `VK_ICD_FILENAMES=/usr/nvidia/share/vulkan/icd.d/nvidia_icd.json`

### Zero-copy pipeline requires libnvidia-allocator.so

Wolf's zero-copy pipeline uses GStreamer CUDA DMA buffers via GBM (Generic Buffer Management). This requires:
1. `nvidia_drm.modeset=1` kernel parameter
2. `libnvidia-allocator.so` installed on the host
3. GBM backend symlink: `/usr/lib/x86_64-linux-gnu/gbm/nvidia-drm_gbm.so → ../libnvidia-allocator.so.1`

Ubuntu's `-server` NVIDIA packages don't include `libnvidia-allocator.so`. If you see:
```
GsCUDABuf: Failed to create GBM device
```
You need to either install `libnvidia-gl-XXX` (non-server variant) or manually extract the library from the NVIDIA `.run` installer.

The driver volume (`nvidia-driver-vol`) handles this for spawned containers, but the Wolf container itself also needs GBM on the host.

### GCP-specific notes

- **Machine type**: `g2-standard-4` with L4 GPU is the sweet spot for 3DS emulation
- **No `/dev/uhid`**: GCP VMs don't have `/dev/uhid` — don't include it in Wolf's device list
- **Firewall**: GCP requires explicit firewall rules (not just ufw). Use `gcloud compute firewall-rules create`
- **Stopped VM cost**: A stopped GCP VM with an L4 GPU costs ~$2.40/day for the 50GB persistent disk only (no GPU/compute charges while stopped). The GPU reservation is released.

### Wolf config auto-migration

Wolf automatically migrates `config.toml` to newer versions. On first start it may:
- Rename your file to `config.toml.v4.old` (or similar)
- Create a new `config.toml` with `config_version = 6` (or latest)
- Add default apps (like "Wolf UI" test ball)

Your paired clients and custom apps are preserved during migration. Check `config_version` in the file to know which version Wolf created.

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
