#!/bin/bash
###############################################################################
# Gaming VM Setup Script — Zero-Touch Deployment
#
# Fully automated setup for Wolf + Azahar 3DS streaming on any NVIDIA GPU VM.
# Handles all states: fresh Ubuntu, pre-installed NVIDIA drivers (TensorDock),
# reboot-required scenarios, and idempotent re-runs.
#
# Usage:
#   sudo ./setup-vm.sh [OPTIONS]
#
# Options:
#   --skip-driver       Skip NVIDIA driver installation (for VMs with pre-installed drivers)
#   --auto-reboot       Automatically reboot if nvidia_drm.modeset needs enabling
#   --no-start          Don't start Wolf at the end (just prepare everything)
#
# The script is idempotent — safe to re-run at any point. Each step checks
# whether it's already been completed and skips if so.
#
# Exit codes:
#   0  — Success (everything set up and Wolf started)
#   2  — Reboot required (re-run after reboot to complete setup)
#   1  — Error
###############################################################################

set -euo pipefail

# ── Parse arguments ──────────────────────────────────────────────────────────
SKIP_DRIVER=false
AUTO_REBOOT=false
NO_START=false
CONTINUE_AFTER_REBOOT=false

for arg in "$@"; do
    case "$arg" in
        --skip-driver)       SKIP_DRIVER=true ;;
        --auto-reboot)       AUTO_REBOOT=true ;;
        --no-start)          NO_START=true ;;
        --continue-after-reboot) CONTINUE_AFTER_REBOOT=true ;;
        *) echo "Unknown option: $arg"; exit 1 ;;
    esac
done

# ── Resolve script directory (works from any cwd) ───────────────────────────
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LOG_FILE="/var/log/gamer-setup.log"
GAMER_HOME="/home/gamer"
REBOOT_NEEDED=false
COMPOSE_BIN=""
WOLF_IMAGE_SELECTED="ghcr.io/games-on-whales/wolf:stable"
ENABLE_DUAL_WOLF_BUILD="${ENABLE_DUAL_WOLF_BUILD:-1}"
WOLF_DUAL_GST_WD_REPO="${WOLF_DUAL_GST_WD_REPO:-https://github.com/nyc-design/gst-wayland-display.git}"
WOLF_DUAL_GST_WD_BRANCH="${WOLF_DUAL_GST_WD_BRANCH:-multi-output}"
WOLF_DUAL_WOLF_TAG="${WOLF_DUAL_WOLF_TAG:-stable}"
WOLF_DUAL_GST_TAG="${WOLF_DUAL_GST_TAG:-1.26.7}"

# Log everything to both console and log file
exec > >(tee -a "$LOG_FILE") 2>&1

echo "========================================="
echo " Gamer PoC — Gaming VM Setup"
echo " $(date -Iseconds)"
echo "========================================="
echo " Script dir: $SCRIPT_DIR"
echo " Options: skip-driver=$SKIP_DRIVER auto-reboot=$AUTO_REBOOT no-start=$NO_START"
echo ""

# ── Docker compose command detection wrapper ────────────────────────────────
detect_compose() {
    if docker compose version &>/dev/null; then
        COMPOSE_BIN="docker compose"
        return
    fi
    if command -v docker-compose &>/dev/null; then
        COMPOSE_BIN="docker-compose"
        return
    fi
    COMPOSE_BIN=""
}

compose() {
    if [ -z "$COMPOSE_BIN" ]; then
        echo "  ✗ Docker Compose not found"
        exit 1
    fi
    if [ "$COMPOSE_BIN" = "docker compose" ]; then
        docker compose "$@"
    else
        docker-compose "$@"
    fi
}

# ─────────────────────────────────────────────────────────────────────────────
# Step 1: NVIDIA Driver
# ─────────────────────────────────────────────────────────────────────────────
echo "[Step 1/9] NVIDIA driver..."

if command -v nvidia-smi &> /dev/null; then
    echo "  ✓ NVIDIA driver installed:"
    echo "    $(nvidia-smi --query-gpu=name,driver_version --format=csv,noheader 2>/dev/null || echo 'query failed')"
elif [ "$SKIP_DRIVER" = true ]; then
    echo "  ⚠ nvidia-smi not found but --skip-driver set. Checking if driver loads after modprobe..."
    modprobe nvidia 2>/dev/null || true
    if command -v nvidia-smi &> /dev/null; then
        echo "  ✓ Driver loaded after modprobe"
    else
        echo "  ✗ No NVIDIA driver found even after modprobe. May need reboot or driver install."
    fi
else
    echo "  Installing NVIDIA driver..."
    apt-get update -y
    apt-get install -y linux-headers-$(uname -r) 2>/dev/null || true
    # Try ubuntu-drivers first, fall back to manual
    if command -v ubuntu-drivers &> /dev/null; then
        ubuntu-drivers install 2>/dev/null || apt-get install -y nvidia-driver-570 2>/dev/null || apt-get install -y nvidia-driver-550
    else
        apt-get install -y nvidia-driver-570 2>/dev/null || apt-get install -y nvidia-driver-550
    fi
    echo "  Driver installed. Reboot required."
    REBOOT_NEEDED=true
fi

# ─────────────────────────────────────────────────────────────────────────────
# Step 2: Docker + NVIDIA Container Toolkit
# ─────────────────────────────────────────────────────────────────────────────
echo "[Step 2/9] Docker + NVIDIA Container Toolkit..."

if ! command -v docker &> /dev/null; then
    echo "  Installing Docker..."
    curl -fsSL https://get.docker.com | sh
    systemctl enable docker
    systemctl start docker
    echo "  ✓ Docker installed"
else
    echo "  ✓ Docker already installed"
fi

detect_compose
if [ -z "$COMPOSE_BIN" ]; then
    echo "  Installing Docker Compose..."
    apt-get update -y
    apt-get install -y docker-compose-plugin 2>/dev/null || apt-get install -y docker-compose
    detect_compose
    if [ -z "$COMPOSE_BIN" ]; then
        echo "  ✗ Failed to install Docker Compose"
        exit 1
    fi
fi
echo "  ✓ Compose command: $COMPOSE_BIN"

if ! dpkg -l 2>/dev/null | grep -q nvidia-container-toolkit; then
    echo "  Installing NVIDIA Container Toolkit..."
    curl -fsSL https://nvidia.github.io/libnvidia-container/gpgkey | \
        gpg --dearmor -o /usr/share/keyrings/nvidia-container-toolkit-keyring.gpg 2>/dev/null || true
    curl -s -L https://nvidia.github.io/libnvidia-container/stable/deb/nvidia-container-toolkit.list | \
        sed 's#deb https://#deb [signed-by=/usr/share/keyrings/nvidia-container-toolkit-keyring.gpg] https://#g' | \
        tee /etc/apt/sources.list.d/nvidia-container-toolkit.list > /dev/null
    apt-get update -y
    apt-get install -y nvidia-container-toolkit
    nvidia-ctk runtime configure --runtime=docker
    systemctl restart docker
    echo "  ✓ NVIDIA Container Toolkit installed"
else
    echo "  ✓ NVIDIA Container Toolkit already installed"
fi

# ─────────────────────────────────────────────────────────────────────────────
# Step 3: nvidia_drm.modeset (required for zero-copy pipeline)
# ─────────────────────────────────────────────────────────────────────────────
echo "[Step 3/9] nvidia_drm.modeset..."

if [ -f /sys/module/nvidia_drm/parameters/modeset ]; then
    MODESET=$(cat /sys/module/nvidia_drm/parameters/modeset)
    if [ "$MODESET" = "Y" ]; then
        echo "  ✓ nvidia_drm.modeset already enabled"
    else
        echo "  Enabling nvidia_drm.modeset=1..."
        echo 'options nvidia-drm modeset=1' > /etc/modprobe.d/nvidia-drm.conf
        # Also add to kernel command line for reliability
        if [ -f /etc/default/grub ]; then
            if ! grep -q 'nvidia-drm.modeset=1' /etc/default/grub; then
                sed -i 's/GRUB_CMDLINE_LINUX_DEFAULT="/GRUB_CMDLINE_LINUX_DEFAULT="nvidia-drm.modeset=1 /' /etc/default/grub
                update-grub 2>/dev/null || true
            fi
        fi
        update-initramfs -u 2>/dev/null || true
        REBOOT_NEEDED=true
        echo "  ⚠ Reboot required for modeset"
    fi
else
    echo "  ⚠ nvidia_drm module not loaded. Setting modeset for after reboot."
    echo 'options nvidia-drm modeset=1' > /etc/modprobe.d/nvidia-drm.conf
    if [ "$REBOOT_NEEDED" = false ] && command -v nvidia-smi &>/dev/null; then
        # Driver exists but module not loaded — try loading it
        modprobe nvidia-drm modeset=1 2>/dev/null || REBOOT_NEEDED=true
    fi
fi

# ─────────────────────────────────────────────────────────────────────────────
# Handle reboot if needed
# ─────────────────────────────────────────────────────────────────────────────
if [ "$REBOOT_NEEDED" = true ]; then
    echo ""
    echo "========================================="
    echo " Reboot Required"
    echo "========================================="

    if [ "$AUTO_REBOOT" = true ]; then
        echo " Setting up auto-continue after reboot..."
        # Create a systemd service that continues setup after reboot
        cat > /etc/systemd/system/gamer-setup-continue.service <<SVCEOF
[Unit]
Description=Gamer PoC Setup - Continue After Reboot
After=network-online.target docker.service nvidia-persistenced.service
Wants=network-online.target
ConditionPathExists=/etc/gamer-setup-continue

[Service]
Type=oneshot
ExecStart=/bin/bash $SCRIPT_DIR/setup-vm.sh --skip-driver --continue-after-reboot $([ "$NO_START" = true ] && echo "--no-start")
ExecStartPost=/bin/rm -f /etc/gamer-setup-continue
ExecStartPost=/bin/systemctl disable gamer-setup-continue.service
StandardOutput=journal+console
StandardError=journal+console
RemainAfterExit=yes

[Install]
WantedBy=multi-user.target
SVCEOF
        touch /etc/gamer-setup-continue
        systemctl daemon-reload
        systemctl enable gamer-setup-continue.service
        echo " Rebooting in 5 seconds..."
        sleep 5
        reboot
    else
        echo " Please reboot and re-run: sudo $SCRIPT_DIR/setup-vm.sh --skip-driver"
        exit 2
    fi
fi

# Clean up continue service if we're running after a reboot
if [ "$CONTINUE_AFTER_REBOOT" = true ]; then
    echo " (Continuing setup after reboot)"
    rm -f /etc/gamer-setup-continue
    systemctl disable gamer-setup-continue.service 2>/dev/null || true
fi

# ─────────────────────────────────────────────────────────────────────────────
# Step 4: NVIDIA Driver Volume for Wolf
# ─────────────────────────────────────────────────────────────────────────────
echo "[Step 4/9] NVIDIA driver volume..."

# Check if volume exists and is populated
VOLUME_OK=false
if docker volume inspect nvidia-driver-vol &>/dev/null; then
    # Check if it actually has files
    FILE_COUNT=$(docker run --rm -v nvidia-driver-vol:/usr/nvidia:ro alpine sh -c 'ls /usr/nvidia/lib/ 2>/dev/null | wc -l' 2>/dev/null || echo "0")
    if [ "$FILE_COUNT" -gt "5" ]; then
        echo "  ✓ nvidia-driver-vol already exists and populated ($FILE_COUNT libs)"
        VOLUME_OK=true
    else
        echo "  ⚠ nvidia-driver-vol exists but appears empty. Recreating..."
        docker volume rm nvidia-driver-vol 2>/dev/null || true
    fi
fi

if [ "$VOLUME_OK" = false ]; then
    NV_VERSION=$(nvidia-smi --query-gpu=driver_version --format=csv,noheader 2>/dev/null | head -1 || true)
    if [ -z "$NV_VERSION" ]; then
        echo "  ✗ Cannot determine NVIDIA driver version. nvidia-smi failed."
        echo "    This may indicate the driver needs a reboot to activate."
        exit 1
    fi
    echo "  Building driver volume for NVIDIA $NV_VERSION..."
    # Use Wolf's official driver container Dockerfile
    curl -sf "https://raw.githubusercontent.com/games-on-whales/wolf/stable/scripts/nvidia-driver-container/Dockerfile" | \
        docker build -t gow/nvidia-driver:latest --build-arg NV_VERSION="$NV_VERSION" -f - . 2>&1 | tail -5
    docker volume create nvidia-driver-vol
    # The container copies driver libs to the volume on creation
    docker run --rm --mount source=nvidia-driver-vol,destination=/usr/nvidia gow/nvidia-driver:latest
    # Verify
    FILE_COUNT=$(docker run --rm -v nvidia-driver-vol:/usr/nvidia:ro alpine sh -c 'ls /usr/nvidia/lib/ 2>/dev/null | wc -l' 2>/dev/null || echo "0")
    echo "  ✓ nvidia-driver-vol created ($FILE_COUNT libs)"
fi

# ─────────────────────────────────────────────────────────────────────────────
# Step 5: Directory Structure
# ─────────────────────────────────────────────────────────────────────────────
echo "[Step 5/9] Directory structure..."

mkdir -p "$GAMER_HOME"/{roms,saves,config,firmware/3ds/sysdata}
# Wolf containers run as retro user UID 1000
chown -R 1000:1000 "$GAMER_HOME"
chmod -R 775 "$GAMER_HOME"

echo "  ✓ Directories ready at $GAMER_HOME/"

# ─────────────────────────────────────────────────────────────────────────────
# Step 6: Wolf Configuration
# ─────────────────────────────────────────────────────────────────────────────
echo "[Step 6/9] Wolf configuration..."

mkdir -p /etc/wolf/cfg
# Copy config.toml from repo — preserves any existing paired_clients if Wolf
# already migrated the config to a newer version. Only copy if repo version
# is newer or config doesn't exist yet.
if [ ! -f /etc/wolf/cfg/config.toml ]; then
    cp "$SCRIPT_DIR/wolf/config.toml" /etc/wolf/cfg/config.toml
    echo "  ✓ Copied config.toml to /etc/wolf/cfg/"
else
    echo "  ✓ /etc/wolf/cfg/config.toml already exists (keeping existing — may have paired clients)"
    echo "    To force update: sudo cp $SCRIPT_DIR/wolf/config.toml /etc/wolf/cfg/config.toml"
fi

# ─────────────────────────────────────────────────────────────────────────────
# Step 7: Build/Pull Images
# ─────────────────────────────────────────────────────────────────────────────
echo "[Step 7/9] Building Azahar image and pulling Wolf..."

cd "$SCRIPT_DIR"

# Pull Wolf (streaming server)
docker pull ghcr.io/games-on-whales/wolf:stable 2>&1 | tail -3

# Optionally build dual-screen Wolf image with custom gst-wayland-display.
# Falls back to stock Wolf automatically if build fails.
if [ "$ENABLE_DUAL_WOLF_BUILD" = "1" ] || [ "$ENABLE_DUAL_WOLF_BUILD" = "true" ]; then
    if [ -f "$SCRIPT_DIR/wolf/Dockerfile.wolf-dual" ]; then
        echo "  Building wolf-dual (repo=$WOLF_DUAL_GST_WD_REPO branch=$WOLF_DUAL_GST_WD_BRANCH)..."
        if compose build wolf-dual \
            --build-arg GST_WD_REPO="$WOLF_DUAL_GST_WD_REPO" \
            --build-arg GST_WD_BRANCH="$WOLF_DUAL_GST_WD_BRANCH" \
            --build-arg WOLF_TAG="$WOLF_DUAL_WOLF_TAG" \
            --build-arg GST_TAG="$WOLF_DUAL_GST_TAG" 2>&1 | tail -10; then
            WOLF_IMAGE_SELECTED="wolf-dual"
            echo "  ✓ wolf-dual built successfully"
        else
            echo "  ⚠ wolf-dual build failed — using stock Wolf image"
            WOLF_IMAGE_SELECTED="ghcr.io/games-on-whales/wolf:stable"
        fi
    else
        echo "  ⚠ wolf dual Dockerfile missing — using stock Wolf image"
    fi
else
    echo "  ℹ ENABLE_DUAL_WOLF_BUILD=$ENABLE_DUAL_WOLF_BUILD, skipping wolf-dual build"
fi

# Build Azahar emulator image
compose build azahar 2>&1 | tail -5

echo "  ✓ Images ready:"
docker images --format '  {{.Repository}}:{{.Tag}} ({{.Size}})' | grep -E "wolf|azahar" | head -5

# ─────────────────────────────────────────────────────────────────────────────
# Step 8: Firewall
# ─────────────────────────────────────────────────────────────────────────────
echo "[Step 8/9] Firewall..."

# SSH + Moonlight protocol ports
TCP_PORTS="22 47984 47989 47999 48010"
UDP_PORTS="47998:48010"
UDP_EXTRA="48100 48200"

if command -v ufw &> /dev/null && ufw status 2>/dev/null | grep -q "Status: active"; then
    for port in $TCP_PORTS; do ufw allow "$port/tcp" 2>/dev/null; done
    ufw allow "$UDP_PORTS/udp" 2>/dev/null
    for port in $UDP_EXTRA; do ufw allow "$port/udp" 2>/dev/null; done
    echo "  ✓ UFW rules added"
elif command -v firewall-cmd &> /dev/null; then
    for port in $TCP_PORTS; do firewall-cmd --permanent --add-port="$port/tcp" 2>/dev/null; done
    firewall-cmd --permanent --add-port="$UDP_PORTS/udp" 2>/dev/null
    for port in $UDP_EXTRA; do firewall-cmd --permanent --add-port="$port/udp" 2>/dev/null; done
    firewall-cmd --reload 2>/dev/null
    echo "  ✓ Firewalld rules added"
elif command -v iptables &> /dev/null; then
    # Check if iptables has a default DROP policy (meaning firewall is active)
    if iptables -L INPUT 2>/dev/null | head -1 | grep -q "DROP"; then
        for port in $TCP_PORTS; do
            iptables -I INPUT -p tcp --dport "$port" -j ACCEPT 2>/dev/null
        done
        iptables -I INPUT -p udp --dport 47998:48010 -j ACCEPT 2>/dev/null
        for port in $UDP_EXTRA; do
            iptables -I INPUT -p udp --dport "$port" -j ACCEPT 2>/dev/null
        done
        echo "  ✓ iptables rules added"
    else
        echo "  ✓ No active firewall detected (iptables ACCEPT policy)"
    fi
else
    echo "  ℹ No firewall manager found — ports should be open by default"
    echo "    If using a cloud provider, ensure these ports are open in security groups:"
    echo "    TCP: $TCP_PORTS"
    echo "    UDP: $UDP_PORTS, $UDP_EXTRA"
fi

# ─────────────────────────────────────────────────────────────────────────────
# Step 9: Start Wolf
# ─────────────────────────────────────────────────────────────────────────────
if [ "$NO_START" = true ]; then
    echo "[Step 9/9] Skipping Wolf start (--no-start)"
else
    echo "[Step 9/9] Starting Wolf..."
    cd "$SCRIPT_DIR"
    echo "  Using image: $WOLF_IMAGE_SELECTED"
    WOLF_IMAGE="$WOLF_IMAGE_SELECTED" compose up -d wolf
    echo "  ✓ Wolf started"
    sleep 3
    if compose ps wolf 2>/dev/null | grep -q "running"; then
        echo "  ✓ Wolf is running"
    else
        echo "  ⚠ Wolf may have issues. Check: $COMPOSE_BIN logs wolf"
    fi
fi

# ─────────────────────────────────────────────────────────────────────────────
# Done
# ─────────────────────────────────────────────────────────────────────────────
PUBLIC_IP=$(curl -sf https://ifconfig.me 2>/dev/null || curl -sf https://api.ipify.org 2>/dev/null || echo "<unknown>")

echo ""
echo "========================================="
echo " Setup Complete! ✓"
echo "========================================="
echo ""
echo " VM Public IP: $PUBLIC_IP"
echo ""
echo " Next steps:"
echo "   1. Place your 3DS ROM in $GAMER_HOME/roms/"
echo "      ROM filename must match ROM_FILENAME in Wolf config."
echo "      Currently set to: pokemon-alpha-sapphire.3ds"
echo ""
echo "   2. Open Moonlight on your device → Add Host → $PUBLIC_IP"
echo "   3. Check Wolf logs for pairing PIN:"
echo "      docker compose -f $SCRIPT_DIR/docker-compose.yml logs wolf 2>&1 | grep -i pin"
echo "   4. Enter the PIN in Moonlight to pair"
echo "   5. Select 'Azahar 3DS' → play!"
echo ""
echo " Useful commands:"
echo "   cd $SCRIPT_DIR"
echo "   $COMPOSE_BIN logs -f wolf            # Watch Wolf logs"
echo "   docker logs GamerAzahar 2>&1 | tail  # Emulator logs"
echo "   $COMPOSE_BIN down                    # Stop Wolf"
echo "   WOLF_IMAGE=$WOLF_IMAGE_SELECTED $COMPOSE_BIN up -d wolf  # Restart Wolf"
echo "   nvidia-smi                           # GPU status"
echo ""
echo " Setup log saved to: $LOG_FILE"
echo ""
