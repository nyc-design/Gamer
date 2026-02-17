###############################################################################
# Eden (N64 Emulator) — GOW App Container
#
# Built on the Games on Whales base-app image which provides:
#   - Sway/Gamescope Wayland compositor
#   - PulseAudio integration
#   - Mesa/Vulkan GPU drivers
#   - Startup script framework (/opt/gow/startup.sh → /opt/gow/startup-app.sh)
#
# NOTE: Eden is a placeholder — the N64 emulator binary needs to be
# specified. This image serves as a scaffold for future N64 support.
#
# Env vars (set by Wolf config / Gamer Agent):
#   ROM_FILENAME     - ROM file in /home/retro/roms/ (e.g., "game.z64")
#   APP_COMMAND      - Custom launch command (for generic startup script)
###############################################################################
FROM ghcr.io/games-on-whales/base-app:edge

ENV DEBIAN_FRONTEND=noninteractive
ENV EMULATOR_NAME=Eden

# Install minimal dependencies + libfaketime + GPU support
RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates \
        bash \
        wget \
        libfaketime \
        libvulkan1 \
        libegl1 \
        libgl1-mesa-dri \
        libsdl2-2.0-0 \
    && rm -rf /var/lib/apt/lists/*

# Ensure EGL platform config dirs are writable for nvidia driver volume setup
RUN mkdir -p /usr/share/egl/egl_external_platform.d \
             /usr/share/glvnd/egl_vendor.d \
             /usr/share/vulkan/icd.d \
             /usr/lib/x86_64-linux-gnu/gbm

# Create ROM, save, config, firmware directories
RUN mkdir -p /home/retro/roms /home/retro/saves /home/retro/config /home/retro/firmware

# Copy generic startup script (uses APP_COMMAND env var)
COPY _common/startup-app.sh /opt/gow/startup-app.sh
RUN chmod +x /opt/gow/startup-app.sh

# Override base-app's 30-nvidia.sh with read-only-safe version
COPY _common/30-nvidia-readonly-safe.sh /etc/cont-init.d/30-nvidia.sh
RUN chmod +x /etc/cont-init.d/30-nvidia.sh

# Point EGL/Vulkan lookups to the mounted NVIDIA driver volume paths
ENV __EGL_VENDOR_LIBRARY_DIRS=/usr/nvidia/share/glvnd/egl_vendor.d \
    __EGL_EXTERNAL_PLATFORM_CONFIG_DIRS=/usr/nvidia/share/egl/egl_external_platform.d \
    VK_ICD_FILENAMES=/usr/nvidia/share/vulkan/icd.d/nvidia_icd.json

WORKDIR /home/retro
