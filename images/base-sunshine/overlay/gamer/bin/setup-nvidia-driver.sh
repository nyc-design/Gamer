#!/usr/bin/env bash
set -e

echo "[setup-nvidia] Installing NVIDIA driver ${NVIDIA_DRIVER_VERSION} (${NVIDIA_DRIVER_TYPE})..."

NVIDIA_DATA_DIR="$GAMER_DATA_DIR/nvidia"
mkdir -p "$NVIDIA_DATA_DIR"

NVIDIA_INSTALLER="$NVIDIA_DATA_DIR/nvidia-${NVIDIA_DRIVER_TYPE}-${NVIDIA_DRIVER_VERSION}.run"
NVIDIA_INSTALL_MARKER="$XDG_RUNTIME_DIR/nvidia-driver-installed"
NVIDIA_DOWNLOAD_MARKER="$NVIDIA_DATA_DIR/nvidia-driver-${NVIDIA_DRIVER_VERSION}.downloaded"

# Download if not cached
if [ ! -f "$NVIDIA_DOWNLOAD_MARKER" ]; then
    # Clean old installers
    find "$NVIDIA_DATA_DIR" -type f -name "nvidia-*.run" -exec rm -f {} +

    if [ "${NVIDIA_DRIVER_TYPE}" = "datacenter" ]; then
        DOWNLOAD_URL="https://us.download.nvidia.com/tesla/${NVIDIA_DRIVER_VERSION}/NVIDIA-Linux-x86_64-${NVIDIA_DRIVER_VERSION}.run"
    else
        DOWNLOAD_URL="https://download.nvidia.com/XFree86/Linux-x86_64/${NVIDIA_DRIVER_VERSION}/NVIDIA-Linux-x86_64-${NVIDIA_DRIVER_VERSION}.run"
    fi

    echo "[setup-nvidia] Downloading from ${DOWNLOAD_URL}..."
    curl -fSL "$DOWNLOAD_URL" -o "$NVIDIA_INSTALLER"
    chmod +x "$NVIDIA_INSTALLER"
    touch "$NVIDIA_DOWNLOAD_MARKER"
fi

# Install if not already done this container lifecycle
if [ ! -f "$NVIDIA_INSTALL_MARKER" ]; then
    echo "[setup-nvidia] Installing driver..."
    "$NVIDIA_INSTALLER" \
        --no-questions \
        --ui=none \
        --accept-license \
        --skip-depmod \
        --skip-module-unload \
        --no-kernel-modules \
        --no-kernel-module-source \
        --no-nouveau-check \
        --no-nvidia-modprobe \
        --no-systemd \
        --no-distro-scripts \
        --no-rpms \
        --no-backup \
        --no-check-for-alternate-installs \
        --no-libglx-indirect \
        --no-install-libglvnd

    touch "$NVIDIA_INSTALL_MARKER"
    echo "[setup-nvidia] Driver installed."
fi
