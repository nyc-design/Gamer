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
# 4. Build and pull images
# ─────────────────────────────────────────────
echo "[4/5] Building Azahar container and pulling Wolf..."

cd "$(dirname "$0")"

# Pull Wolf
docker pull ghcr.io/games-on-whales/wolf:stable

# Build Azahar emulator image
docker compose build azahar

echo "  Images ready:"
docker images | grep -E "wolf|azahar" | head -5

# ─────────────────────────────────────────────
# 5. Configure firewall
# ─────────────────────────────────────────────
echo "[5/5] Configuring firewall for Moonlight..."

# Moonlight protocol ports
# 47984: HTTPS (pairing)
# 47989: HTTP (app list)
# 48010: RTSP (stream setup)
# 47998-48000: RTP video/audio/control (UDP)
if command -v ufw &> /dev/null; then
    ufw allow 47984/tcp   # HTTPS pairing
    ufw allow 47989/tcp   # HTTP discovery
    ufw allow 48010/tcp   # RTSP
    ufw allow 47998:48000/udp  # RTP streams
    echo "  UFW rules added"
elif command -v firewall-cmd &> /dev/null; then
    firewall-cmd --permanent --add-port=47984/tcp
    firewall-cmd --permanent --add-port=47989/tcp
    firewall-cmd --permanent --add-port=48010/tcp
    firewall-cmd --permanent --add-port=47998-48000/udp
    firewall-cmd --reload
    echo "  Firewalld rules added"
else
    echo "  No firewall manager found — ensure ports 47984-48010 are open"
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
echo "   4. Start Wolf:"
echo "        cd $(pwd)"
echo "        docker compose up -d"
echo "   5. Open Moonlight on iPhone → Add Host → enter this VM's IP"
echo "   6. Pair using the PIN shown in Wolf logs:"
echo "        docker compose logs wolf"
echo "   7. Select 'Azahar 3DS' from the app list → play!"
echo ""
echo " Useful commands:"
echo "   docker compose logs -f wolf      # Watch Wolf logs"
echo "   docker logs GamerAzahar          # Watch emulator logs"
echo "   docker compose down              # Stop everything"
echo "   nvidia-smi                       # Check GPU usage"
echo ""
