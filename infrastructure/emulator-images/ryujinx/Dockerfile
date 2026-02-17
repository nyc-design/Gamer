###############################################################################
# Ryujinx (Nintendo Switch) — GOW App Container
#
# Built on the Games on Whales base-app image which provides:
#   - Sway/Gamescope Wayland compositor
#   - PulseAudio integration
#   - Mesa/Vulkan GPU drivers
#   - Startup script framework (/opt/gow/startup.sh → /opt/gow/startup-app.sh)
#
# Wolf spawns this container when a Moonlight client connects and selects
# the Ryujinx app. Wolf injects WAYLAND_DISPLAY, PULSE_SERVER, XDG_RUNTIME_DIR.
#
# NOTE: Ryujinx requires Switch keys/firmware in /home/retro/firmware/switch/
#       to run commercial games.
#
# Env vars (set by Wolf config / Gamer Agent):
#   ROM_FILENAME     - ROM file in /home/retro/roms/ (e.g., "botw.xci")
#   FAKETIME         - Optional fake time string
#   RUN_SWAY         - Set to "1" to use Sway compositor (default: 1)
#   ROM_OPTIONAL     - Set to "1" to allow launching without a ROM (settings mode)
###############################################################################
FROM ghcr.io/games-on-whales/base-app:edge

ENV DEBIAN_FRONTEND=noninteractive
ENV EMULATOR_NAME=Ryujinx

# Install dependencies + libfaketime + GPU support
RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates \
        bash \
        wget \
        libfaketime \
        libvulkan1 \
        libegl1 \
        libgl1-mesa-dri \
        libsdl2-2.0-0 \
        libicu74 \
        libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Download Ryujinx (GreemDev maintained fork)
# Ryujinx is distributed as a portable tarball
ARG RYUJINX_VERSION=1.2.83
RUN mkdir -p /Applications && \
    wget -O /tmp/ryujinx.tar.gz \
        "https://github.com/GreemDev/Ryujinx/releases/download/${RYUJINX_VERSION}/ryujinx-${RYUJINX_VERSION}-linux_x64.tar.gz" && \
    tar -xzf /tmp/ryujinx.tar.gz -C /Applications && \
    chmod +x /Applications/publish/Ryujinx && \
    rm -f /tmp/ryujinx.tar.gz

# Ensure EGL platform config dirs are writable for nvidia driver volume setup
RUN mkdir -p /usr/share/egl/egl_external_platform.d \
             /usr/share/glvnd/egl_vendor.d \
             /usr/share/vulkan/icd.d \
             /usr/lib/x86_64-linux-gnu/gbm

# Create ROM, save, config, firmware directories
RUN mkdir -p /home/retro/roms /home/retro/saves /home/retro/config /home/retro/firmware

# Copy startup script
COPY ryujinx/startup-app.sh /opt/gow/startup-app.sh
RUN chmod +x /opt/gow/startup-app.sh

# Override base-app's 30-nvidia.sh with read-only-safe version
COPY _common/30-nvidia-readonly-safe.sh /etc/cont-init.d/30-nvidia.sh
RUN chmod +x /etc/cont-init.d/30-nvidia.sh

# Point EGL/Vulkan lookups to the mounted NVIDIA driver volume paths
ENV __EGL_VENDOR_LIBRARY_DIRS=/usr/nvidia/share/glvnd/egl_vendor.d \
    __EGL_EXTERNAL_PLATFORM_CONFIG_DIRS=/usr/nvidia/share/egl/egl_external_platform.d \
    VK_ICD_FILENAMES=/usr/nvidia/share/vulkan/icd.d/nvidia_icd.json

WORKDIR /home/retro
