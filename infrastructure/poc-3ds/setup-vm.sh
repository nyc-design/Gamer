#!/bin/bash
###############################################################################
# Gaming VM Host Bootstrap
#
# Prepares a bare Ubuntu VM for GPU-accelerated Docker workloads.
# Installs only host-level dependencies — all app logic lives in Docker images.
#
# What this does:
#   1. NVIDIA driver (install or detect)
#   2. Docker + NVIDIA Container Toolkit
#   3. nvidia_drm.modeset
#   4. NVIDIA driver volume for Wolf containers
#   5. Host directories for game data
#   6. Firewall rules for Moonlight protocol
#
# What this does NOT do:
#   - Pull/build any Docker images (that's deploy-tensordock.sh setup)
#   - Write Wolf config (baked into docker-compose.yml)
#   - Start any containers
#
# Usage:
#   sudo ./setup-vm.sh [OPTIONS]
#
# Options:
#   --auto-reboot    Automatically reboot if driver install or modeset needs it
#   --skip-driver    Skip NVIDIA driver installation (already installed)
#
# Exit codes:
#   0  — Host ready for Docker workloads
#   2  — Reboot required (re-run after reboot, or use --auto-reboot)
#   1  — Error
###############################################################################

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LOG_FILE="/var/log/gamer-setup.log"
GAMER_HOME="/home/gamer"
REBOOT_NEEDED=false
SKIP_DRIVER=false
AUTO_REBOOT=false
CONTINUE_AFTER_REBOOT=false

for arg in "$@"; do
    case "$arg" in
        --skip-driver)           SKIP_DRIVER=true ;;
        --auto-reboot)           AUTO_REBOOT=true ;;
        --continue-after-reboot) CONTINUE_AFTER_REBOOT=true ;;
        *) echo "Unknown option: $arg"; exit 1 ;;
    esac
done

exec > >(tee -a "$LOG_FILE") 2>&1

echo "========================================="
echo " Gamer PoC — Host Bootstrap"
echo " $(date -Iseconds)"
echo "========================================="
echo " Options: skip-driver=$SKIP_DRIVER auto-reboot=$AUTO_REBOOT"
echo ""

# ─────────────────────────────────────────────────────────────────────────────
# Step 1: NVIDIA Driver
# ─────────────────────────────────────────────────────────────────────────────
echo "[Step 1/6] NVIDIA driver..."

if command -v nvidia-smi &>/dev/null && nvidia-smi &>/dev/null; then
    echo "  ✓ NVIDIA driver working:"
    echo "    $(nvidia-smi --query-gpu=name,driver_version --format=csv,noheader 2>/dev/null)"
elif [ "$SKIP_DRIVER" = true ]; then
    echo "  ⚠ --skip-driver set but nvidia-smi not working."
    modprobe nvidia 2>/dev/null || true
    if command -v nvidia-smi &>/dev/null && nvidia-smi &>/dev/null; then
        echo "  ✓ Driver loaded after modprobe"
    else
        echo "  ✗ No working NVIDIA driver. May need reboot or driver install."
    fi
else
    echo "  Installing NVIDIA driver..."
    apt-get update -y
    apt-get install -y linux-headers-$(uname -r) 2>/dev/null || true
    if command -v ubuntu-drivers &>/dev/null; then
        ubuntu-drivers install 2>/dev/null || apt-get install -y nvidia-driver-570 2>/dev/null || apt-get install -y nvidia-driver-550
    else
        apt-get install -y nvidia-driver-570 2>/dev/null || apt-get install -y nvidia-driver-550
    fi
    echo "  Driver installed — reboot required."
    REBOOT_NEEDED=true
fi

# ─────────────────────────────────────────────────────────────────────────────
# Step 2: Docker + NVIDIA Container Toolkit
# ─────────────────────────────────────────────────────────────────────────────
echo "[Step 2/6] Docker + NVIDIA Container Toolkit..."

if ! command -v docker &>/dev/null; then
    echo "  Installing Docker via get.docker.com..."
    curl -fsSL https://get.docker.com | sh
    systemctl enable docker
    systemctl start docker
    echo "  ✓ Docker installed"
else
    echo "  ✓ Docker already installed"
fi

if ! docker compose version &>/dev/null; then
    echo "  Installing Docker Compose plugin..."
    apt-get update -y
    apt-get install -y docker-compose-plugin 2>/dev/null || true
fi
if docker compose version &>/dev/null; then
    echo "  ✓ Docker Compose available"
else
    echo "  ⚠ Docker Compose plugin not available — docker compose commands may fail"
fi

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
# Step 3: nvidia_drm.modeset
# ─────────────────────────────────────────────────────────────────────────────
echo "[Step 3/6] nvidia_drm.modeset..."

if [ -f /sys/module/nvidia_drm/parameters/modeset ]; then
    MODESET=$(cat /sys/module/nvidia_drm/parameters/modeset)
    if [ "$MODESET" = "Y" ]; then
        echo "  ✓ nvidia_drm.modeset already enabled"
    else
        echo "  Enabling nvidia_drm.modeset=1..."
        echo 'options nvidia-drm modeset=1' > /etc/modprobe.d/nvidia-drm.conf
        if [ -f /etc/default/grub ] && ! grep -q 'nvidia-drm.modeset=1' /etc/default/grub; then
            sed -i 's/GRUB_CMDLINE_LINUX_DEFAULT="/GRUB_CMDLINE_LINUX_DEFAULT="nvidia-drm.modeset=1 /' /etc/default/grub
            update-grub 2>/dev/null || true
        fi
        update-initramfs -u 2>/dev/null || true
        REBOOT_NEEDED=true
    fi
else
    echo "  ⚠ nvidia_drm module not loaded — setting modeset for after reboot"
    echo 'options nvidia-drm modeset=1' > /etc/modprobe.d/nvidia-drm.conf
    if [ "$REBOOT_NEEDED" = false ]; then
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
        cat > /etc/systemd/system/gamer-setup-continue.service <<SVCEOF
[Unit]
Description=Gamer PoC Setup - Continue After Reboot
After=network-online.target docker.service nvidia-persistenced.service
Wants=network-online.target
ConditionPathExists=/etc/gamer-setup-continue

[Service]
Type=oneshot
ExecStart=/bin/bash $SCRIPT_DIR/setup-vm.sh --skip-driver --continue-after-reboot
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

if [ "$CONTINUE_AFTER_REBOOT" = true ]; then
    echo " (Continuing setup after reboot)"
    rm -f /etc/gamer-setup-continue
    systemctl disable gamer-setup-continue.service 2>/dev/null || true
fi

# ─────────────────────────────────────────────────────────────────────────────
# Step 4: NVIDIA Driver Volume for Wolf
# ─────────────────────────────────────────────────────────────────────────────
echo "[Step 4/6] NVIDIA driver volume..."

driver_volume_has_required_files() {
    docker run --rm -v nvidia-driver-vol:/usr/nvidia:ro alpine sh -c \
        '[ -f /usr/nvidia/share/glvnd/egl_vendor.d/10_nvidia.json ] && [ -f /usr/nvidia/lib/libEGL_nvidia.so.0 ] && [ -f /usr/nvidia/lib/libnvidia-allocator.so.1 ]' \
        >/dev/null 2>&1
}

populate_driver_volume_from_host() {
    echo "  Populating nvidia-driver-vol from host libraries..."
    docker volume create nvidia-driver-vol >/dev/null 2>&1 || true
    docker run --rm \
        -v nvidia-driver-vol:/usr/nvidia \
        -v /usr/lib/x86_64-linux-gnu:/host/lib:ro \
        -v /usr/share:/host/share:ro \
        -v /usr/bin:/host/bin:ro \
        alpine sh -lc '
            set -e
            mkdir -p /usr/nvidia/lib /usr/nvidia/bin /usr/nvidia/share/glvnd/egl_vendor.d /usr/nvidia/share/vulkan/icd.d /usr/nvidia/share/egl/egl_external_platform.d /usr/nvidia/lib/gbm

            for f in /host/lib/libnvidia* /host/lib/libcuda* /host/lib/libEGL_nvidia* /host/lib/libGLX_nvidia* /host/lib/libnv*; do
                [ -e "$f" ] && cp -a "$f" /usr/nvidia/lib/
            done
            [ -d /host/lib/xorg ] && cp -a /host/lib/xorg /usr/nvidia/lib/ || true
            [ -d /host/lib/wine ] && cp -a /host/lib/wine /usr/nvidia/lib/ || true

            for b in nvidia-smi nvidia-debugdump nvidia-settings nvidia-xconfig nvidia-persistenced nvidia-cuda-mps-control nvidia-cuda-mps-server; do
                [ -x "/host/bin/$b" ] && cp -a "/host/bin/$b" /usr/nvidia/bin/ || true
            done

            if [ -f /host/share/glvnd/egl_vendor.d/10_nvidia.json ]; then
                cp -a /host/share/glvnd/egl_vendor.d/10_nvidia.json /usr/nvidia/share/glvnd/egl_vendor.d/
            else
                printf "{\"file_format_version\":\"1.0.0\",\"ICD\":{\"library_path\":\"libEGL_nvidia.so.0\"}}\n" > /usr/nvidia/share/glvnd/egl_vendor.d/10_nvidia.json
            fi

            if [ -f /host/share/vulkan/icd.d/nvidia_icd.json ]; then
                cp -a /host/share/vulkan/icd.d/nvidia_icd.json /usr/nvidia/share/vulkan/icd.d/
            else
                printf "{\"file_format_version\":\"1.0.0\",\"ICD\":{\"library_path\":\"libGLX_nvidia.so.0\",\"api_version\":\"1.3.242\"}}\n" > /usr/nvidia/share/vulkan/icd.d/nvidia_icd.json
            fi

            if [ -f /host/share/egl/egl_external_platform.d/15_nvidia_gbm.json ]; then
                cp -a /host/share/egl/egl_external_platform.d/15_nvidia_gbm.json /usr/nvidia/share/egl/egl_external_platform.d/
            else
                printf "{\"file_format_version\":\"1.0.0\",\"ICD\":{\"library_path\":\"libnvidia-egl-gbm.so.1\"}}\n" > /usr/nvidia/share/egl/egl_external_platform.d/15_nvidia_gbm.json
            fi

            if [ -f /host/share/egl/egl_external_platform.d/10_nvidia_wayland.json ]; then
                cp -a /host/share/egl/egl_external_platform.d/10_nvidia_wayland.json /usr/nvidia/share/egl/egl_external_platform.d/
            else
                printf "{\"file_format_version\":\"1.0.0\",\"ICD\":{\"library_path\":\"libnvidia-egl-wayland.so.1\"}}\n" > /usr/nvidia/share/egl/egl_external_platform.d/10_nvidia_wayland.json
            fi

            ln -sf ../libnvidia-allocator.so.1 /usr/nvidia/lib/gbm/nvidia-drm_gbm.so
        '
}

VOLUME_OK=false
if docker volume inspect nvidia-driver-vol &>/dev/null; then
    FILE_COUNT=$(docker run --rm -v nvidia-driver-vol:/usr/nvidia:ro alpine sh -c 'ls /usr/nvidia/lib/ 2>/dev/null | wc -l' 2>/dev/null || echo "0")
    if [ "$FILE_COUNT" -gt "5" ] && driver_volume_has_required_files; then
        echo "  ✓ nvidia-driver-vol exists and healthy ($FILE_COUNT libs)"
        VOLUME_OK=true
    else
        echo "  ⚠ nvidia-driver-vol incomplete — recreating..."
        docker volume rm nvidia-driver-vol 2>/dev/null || true
    fi
fi

if [ "$VOLUME_OK" = false ]; then
    NV_VERSION=$(nvidia-smi --query-gpu=driver_version --format=csv,noheader 2>/dev/null | head -1 || true)
    if [ -z "$NV_VERSION" ]; then
        echo "  ✗ Cannot determine NVIDIA driver version (nvidia-smi failed)."
        exit 1
    fi

    echo "  Building driver volume for NVIDIA $NV_VERSION..."
    docker volume create nvidia-driver-vol >/dev/null

    OFFICIAL_BUILD_OK=false
    for URL in \
        "https://raw.githubusercontent.com/games-on-whales/wolf/stable/scripts/nvidia-driver-container/Dockerfile" \
        "https://raw.githubusercontent.com/games-on-whales/wolf/main/scripts/nvidia-driver-container/Dockerfile"; do
        if curl -sfL "$URL" -o /tmp/gow-nvidia-driver.Dockerfile && [ -s /tmp/gow-nvidia-driver.Dockerfile ]; then
            if docker build -t gow/nvidia-driver:latest --build-arg NV_VERSION="$NV_VERSION" -f /tmp/gow-nvidia-driver.Dockerfile . >/tmp/gow-nvidia-build.log 2>&1 && \
               docker run --rm --mount source=nvidia-driver-vol,destination=/usr/nvidia gow/nvidia-driver:latest >/tmp/gow-nvidia-populate.log 2>&1; then
                OFFICIAL_BUILD_OK=true
                echo "  ✓ Built driver volume via official Wolf Dockerfile"
                break
            fi
        fi
    done

    if [ "$OFFICIAL_BUILD_OK" = false ]; then
        echo "  ⚠ Official build failed — using host fallback"
        docker volume rm nvidia-driver-vol >/dev/null 2>&1 || true
        populate_driver_volume_from_host
    fi

    if ! driver_volume_has_required_files; then
        echo "  ✗ NVIDIA driver volume still incomplete."
        exit 1
    fi

    FILE_COUNT=$(docker run --rm -v nvidia-driver-vol:/usr/nvidia:ro alpine sh -c 'ls /usr/nvidia/lib/ 2>/dev/null | wc -l' 2>/dev/null || echo "0")
    echo "  ✓ nvidia-driver-vol created ($FILE_COUNT libs)"
fi

# ─────────────────────────────────────────────────────────────────────────────
# Step 5: Host Directories
# ─────────────────────────────────────────────────────────────────────────────
echo "[Step 5/6] Host directories..."

mkdir -p "$GAMER_HOME"/{roms,saves,config,firmware/3ds/sysdata}
mkdir -p /etc/wolf/cfg
chown -R 1000:1000 "$GAMER_HOME"
chmod -R 775 "$GAMER_HOME"

echo "  ✓ $GAMER_HOME/ and /etc/wolf/ ready"

# ─────────────────────────────────────────────────────────────────────────────
# Step 6: Firewall
# ─────────────────────────────────────────────────────────────────────────────
echo "[Step 6/6] Firewall..."

TCP_PORTS="22 47984 47989 47999 48010"
UDP_PORTS="47998:48010"
UDP_EXTRA="48100 48200"

if command -v ufw &>/dev/null && ufw status 2>/dev/null | grep -q "Status: active"; then
    for port in $TCP_PORTS; do ufw allow "$port/tcp" 2>/dev/null; done
    ufw allow "$UDP_PORTS/udp" 2>/dev/null
    for port in $UDP_EXTRA; do ufw allow "$port/udp" 2>/dev/null; done
    echo "  ✓ UFW rules added"
elif command -v iptables &>/dev/null && iptables -L INPUT 2>/dev/null | head -1 | grep -q "DROP"; then
    for port in $TCP_PORTS; do iptables -I INPUT -p tcp --dport "$port" -j ACCEPT 2>/dev/null; done
    iptables -I INPUT -p udp --dport 47998:48010 -j ACCEPT 2>/dev/null
    for port in $UDP_EXTRA; do iptables -I INPUT -p udp --dport "$port" -j ACCEPT 2>/dev/null; done
    echo "  ✓ iptables rules added"
else
    echo "  ✓ No active firewall detected"
fi

# ─────────────────────────────────────────────────────────────────────────────
# Done
# ─────────────────────────────────────────────────────────────────────────────
echo ""
echo "========================================="
echo " Host Bootstrap Complete"
echo "========================================="
echo ""
echo " Next: run deploy-tensordock.sh setup to build images and start Wolf"
echo ""
