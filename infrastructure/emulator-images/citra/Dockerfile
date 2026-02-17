###############################################################################
# Citra (Nintendo 3DS — legacy) — GOW App Container
#
# Built on the Games on Whales base-app image which provides:
#   - Sway/Gamescope Wayland compositor
#   - PulseAudio integration
#   - Mesa/Vulkan GPU drivers
#   - Startup script framework (/opt/gow/startup.sh → /opt/gow/startup-app.sh)
#
# Wolf spawns this container when a Moonlight client connects and selects
# the Citra app. Wolf injects WAYLAND_DISPLAY, PULSE_SERVER, XDG_RUNTIME_DIR.
#
# NOTE: Citra is a legacy 3DS emulator. Azahar (in poc-3ds/) is the preferred
# 3DS emulator with better SeparateWindows dual-screen support.
#
# Env vars (set by Wolf config / Gamer Agent):
#   ROM_FILENAME     - ROM file in /home/retro/roms/ (e.g., "game.3ds")
#   FAKETIME         - Optional fake time string
#   RUN_SWAY         - Set to "1" to use Sway compositor (default: 1)
#   ROM_OPTIONAL     - Set to "1" to allow launching without a ROM (settings mode)
###############################################################################
FROM ghcr.io/games-on-whales/base-app:edge

ENV DEBIAN_FRONTEND=noninteractive
ENV APPIMAGE_EXTRACT_AND_RUN=1
ENV EMULATOR_NAME=Citra

# Install Citra dependencies + libfaketime + GPU support
RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates \
        bash \
        wget \
        xz-utils \
        libfuse2 \
        libsdl2-2.0-0 \
        libvulkan1 \
        libglu1-mesa \
        libegl1 \
        libgl1-mesa-dri \
        libfaketime \
    && rm -rf /var/lib/apt/lists/*

# Download Citra AppImage
ARG CITRA_APPIMAGE_URL="https://github.com/PabloMK7/citra/releases/download/r71eca05/citra-linux-appimage-20240513-71eca05.tar.gz"
RUN mkdir -p /Applications && \
    wget -O /tmp/citra.tar.gz "${CITRA_APPIMAGE_URL}" && \
    tar -xzf /tmp/citra.tar.gz -C /tmp && \
    find /tmp -type f -name "*.AppImage" -exec cp {} /Applications/citra.AppImage \; && \
    chmod +x /Applications/citra.AppImage && \
    rm -rf /tmp/citra.tar.gz /tmp/citra-*

# Ensure EGL platform config dirs are writable for nvidia driver volume setup
RUN mkdir -p /usr/share/egl/egl_external_platform.d \
             /usr/share/glvnd/egl_vendor.d \
             /usr/share/vulkan/icd.d \
             /usr/lib/x86_64-linux-gnu/gbm

# Create ROM, save, config, firmware directories
RUN mkdir -p /home/retro/roms /home/retro/saves /home/retro/config /home/retro/firmware

# Copy startup script
COPY citra/startup-app.sh /opt/gow/startup-app.sh
RUN chmod +x /opt/gow/startup-app.sh

# Override base-app's 30-nvidia.sh with read-only-safe version
COPY _common/30-nvidia-readonly-safe.sh /etc/cont-init.d/30-nvidia.sh
RUN chmod +x /etc/cont-init.d/30-nvidia.sh

# Point EGL/Vulkan lookups to the mounted NVIDIA driver volume paths
ENV __EGL_VENDOR_LIBRARY_DIRS=/usr/nvidia/share/glvnd/egl_vendor.d \
    __EGL_EXTERNAL_PLATFORM_CONFIG_DIRS=/usr/nvidia/share/egl/egl_external_platform.d \
    VK_ICD_FILENAMES=/usr/nvidia/share/vulkan/icd.d/nvidia_icd.json

WORKDIR /home/retro
