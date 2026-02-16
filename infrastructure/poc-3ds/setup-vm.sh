#!/bin/bash
###############################################################################
# Gaming VM Setup Script — PoC for 3DS Streaming
#
# Run this on a fresh GPU VM (Ubuntu 22.04/24.04) to set up everything needed
# for Wolf + Azahar 3DS streaming via Moonlight.
#
# Prerequisites:
#   - Ubuntu 22.04 or 24.04
#   - NVIDIA GPU (T4, L4, RTX 3060+, etc.)
#   - Root or sudo access
#   - Internet connectivity
#
# Usage:
#   chmod +x setup-vm.sh
#   sudo ./setup-vm.sh
###############################################################################

set -euo pipefail

echo "========================================="
echo " Gamer PoC — Gaming VM Setup"
echo "========================================="

# ─────────────────────────────────────────────
# 1. Install NVIDIA driver (if not already installed)
# ─────────────────────────────────────────────
if ! command -v nvidia-smi &> /dev/null; then
    echo "[1/5] Installing NVIDIA driver..."
    apt-get update
    apt-get install -y linux-headers-$(uname -r)
    # Install the recommended driver
    ubuntu-drivers install
    echo "NVIDIA driver installed. A reboot may be required."
    echo "After reboot, re-run this script."
else
    echo "[1/5] NVIDIA driver already installed"
    nvidia-smi --query-gpu=name,driver_version --format=csv,noheader
fi

# ─────────────────────────────────────────────
# 2. Install Docker + NVIDIA Container Toolkit
# ─────────────────────────────────────────────
if ! command -v docker &> /dev/null; then
    echo "[2/5] Installing Docker..."
    curl -fsSL https://get.docker.com | sh
else
    echo "[2/5] Docker already installed"
fi

if ! dpkg -l | grep -q nvidia-container-toolkit; then
    echo "[2/5] Installing NVIDIA Container Toolkit..."
    curl -fsSL https://nvidia.github.io/libnvidia-container/gpgkey | \
        gpg --dearmor -o /usr/share/keyrings/nvidia-container-toolkit-keyring.gpg
    curl -s -L https://nvidia.github.io/libnvidia-container/stable/deb/nvidia-container-toolkit.list | \
        sed 's#deb https://#deb [signed-by=/usr/share/keyrings/nvidia-container-toolkit-keyring.gpg] https://#g' | \
        tee /etc/apt/sources.list.d/nvidia-container-toolkit.list
    apt-get update
    apt-get install -y nvidia-container-toolkit
    nvidia-ctk runtime configure --runtime=docker
    systemctl restart docker
else
    echo "[2/5] NVIDIA Container Toolkit already installed"
fi

# ─────────────────────────────────────────────
# 3. Create directory structure
# ─────────────────────────────────────────────
echo "[3/5] Creating directory structure..."
GAMER_HOME="/home/gamer"
mkdir -p "$GAMER_HOME"/{roms,saves,config,firmware/3ds/sysdata}

# Set permissions (Wolf containers run as retro user UID 1000)
chown -R 1000:1000 "$GAMER_HOME"
chmod -R 775 "$GAMER_HOME"

echo "  Created:"
echo "    $GAMER_HOME/roms/        ← Place your .3ds/.cia ROM files here"
echo "    $GAMER_HOME/saves/       ← Azahar save files"
echo "    $GAMER_HOME/config/      ← Persistent emulator config"
echo "    $GAMER_HOME/firmware/    ← 3DS firmware/system files (optional)"

# ─────────────────────────────────────────────
# 4. Enable nvidia_drm.modeset (for zero-copy pipeline)
# ─────────────────────────────────────────────
echo "[4/7] Checking nvidia_drm.modeset..."
if [ -f /sys/module/nvidia_drm/parameters/modeset ]; then
    MODESET=$(cat /sys/module/nvidia_drm/parameters/modeset)
    if [ "$MODESET" != "Y" ]; then
        echo "  Enabling nvidia_drm.modeset=1 (requires reboot)"
        echo 'options nvidia-drm modeset=1' > /etc/modprobe.d/nvidia-drm.conf
        update-initramfs -u
        echo "  WARNING: Reboot required for modeset. Re-run this script after reboot."
    else
        echo "  nvidia_drm.modeset already enabled"
    fi
else
    echo "  nvidia_drm module not loaded yet (driver may need reboot)"
fi

# ─────────────────────────────────────────────
# 5. Create NVIDIA driver volume for Wolf
# ─────────────────────────────────────────────
echo "[5/7] Setting up NVIDIA driver volume..."

if docker volume inspect nvidia-driver-vol &>/dev/null; then
    echo "  nvidia-driver-vol already exists"
else
    NV_VERSION=$(nvidia-smi --query-gpu=driver_version --format=csv,noheader 2>/dev/null | head -1)
    if [ -z "$NV_VERSION" ]; then
        echo "  WARNING: nvidia-smi not available. Skipping driver volume (reboot may be needed)."
    else
        echo "  Building driver volume for NVIDIA $NV_VERSION..."
        curl -s "https://raw.githubusercontent.com/games-on-whales/wolf/stable/scripts/nvidia-driver-container/Dockerfile" | \
            docker build -t gow/nvidia-driver:latest --build-arg NV_VERSION="$NV_VERSION" -
        docker volume create nvidia-driver-vol
        docker create --rm --mount source=nvidia-driver-vol,destination=/usr/nvidia gow/nvidia-driver:latest sh
        echo "  nvidia-driver-vol created and populated"
    fi
fi

# ─────────────────────────────────────────────
# 6. Build and pull images
# ─────────────────────────────────────────────
echo "[6/7] Building Azahar container and pulling Wolf..."

cd "$(dirname "$0")"

# Pull Wolf
docker pull ghcr.io/games-on-whales/wolf:stable

# Build Azahar emulator image
docker compose build azahar

echo "  Images ready:"
docker images | grep -E "wolf|azahar" | head -5

# ─────────────────────────────────────────────
# 7. Configure firewall
# ─────────────────────────────────────────────
echo "[7/7] Configuring firewall for Moonlight..."

# Moonlight protocol ports:
# 47984: HTTPS (pairing/server info)
# 47989: HTTP (app list/discovery)
# 47999: Control server (TCP)
# 48010: RTSP (stream setup)
# 47998-48010: RTP video/audio/control (UDP)
# 48100, 48200: RTP ping (UDP)
if command -v ufw &> /dev/null; then
    ufw allow 47984/tcp
    ufw allow 47989/tcp
    ufw allow 47999/tcp
    ufw allow 48010/tcp
    ufw allow 47998:48010/udp
    ufw allow 48100/udp
    ufw allow 48200/udp
    echo "  UFW rules added"
elif command -v firewall-cmd &> /dev/null; then
    firewall-cmd --permanent --add-port=47984/tcp
    firewall-cmd --permanent --add-port=47989/tcp
    firewall-cmd --permanent --add-port=47999/tcp
    firewall-cmd --permanent --add-port=48010/tcp
    firewall-cmd --permanent --add-port=47998-48010/udp
    firewall-cmd --permanent --add-port=48100/udp
    firewall-cmd --permanent --add-port=48200/udp
    firewall-cmd --reload
    echo "  Firewalld rules added"
else
    echo "  No firewall manager found."
    echo "  Ensure these ports are open: TCP 47984,47989,47999,48010 / UDP 47998-48010,48100,48200"
    echo "  For GCP, use: gcloud compute firewall-rules create allow-moonlight \\"
    echo "    --allow=tcp:47984,tcp:47989,tcp:47999,tcp:48010,udp:47998-48010,udp:48100,udp:48200"
fi

echo ""
echo "========================================="
echo " Setup Complete!"
echo "========================================="
echo ""
echo " Next steps:"
echo "   1. Place your 3DS ROM in $GAMER_HOME/roms/"
echo "   2. Edit wolf/config.toml: set ROM_FILENAME to your ROM file"
echo "   3. (Optional) Place 3DS firmware in $GAMER_HOME/firmware/3ds/"
echo "   4. Copy Wolf config to /etc/wolf/cfg/:"
echo "        sudo mkdir -p /etc/wolf/cfg"
echo "        sudo cp wolf/config.toml /etc/wolf/cfg/"
echo "   5. Start Wolf:"
echo "        cd $(pwd)"
echo "        docker compose up -d"
echo "   6. Open Moonlight on iPhone → Add Host → enter this VM's IP"
echo "   7. Pair using the PIN shown in Wolf logs:"
echo "        docker compose logs wolf 2>&1 | grep -i pin"
echo "   8. Select 'Azahar 3DS' from the app list → play!"
echo ""
echo " Useful commands:"
echo "   docker compose logs -f wolf      # Watch Wolf logs"
echo "   docker logs GamerAzahar          # Watch emulator logs"
echo "   docker compose down              # Stop everything"
echo "   nvidia-smi                       # Check GPU usage"
echo ""
echo " If you see 'nvidia_drm.modeset' warnings, reboot and re-run this script."
echo ""
