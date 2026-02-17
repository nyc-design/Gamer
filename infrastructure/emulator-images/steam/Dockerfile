###############################################################################
# Steam (PC Gaming) — GOW App Container
#
# Built on the Games on Whales base-app image which provides:
#   - Sway/Gamescope Wayland compositor
#   - PulseAudio integration
#   - Mesa/Vulkan GPU drivers
#   - Startup script framework (/opt/gow/startup.sh → /opt/gow/startup-app.sh)
#
# Wolf spawns this container when a Moonlight client connects and selects
# the Steam app. Wolf injects WAYLAND_DISPLAY, PULSE_SERVER, XDG_RUNTIME_DIR.
#
# Steam persistent state (install, library, saves) is mounted via
# /home/retro/config/steam → user's GCS steam directory.
#
# Env vars (set by Wolf config / Gamer Agent):
#   RUN_SWAY         - Set to "1" to use Sway compositor (default: 1)
###############################################################################
FROM ghcr.io/games-on-whales/base-app:edge

ENV DEBIAN_FRONTEND=noninteractive
ENV EMULATOR_NAME=Steam

# Enable 32-bit architecture (Steam requires it)
RUN dpkg --add-architecture i386

# Install Steam dependencies + libfaketime + GPU support
# Steam itself will be installed via the bootstrap .deb
RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates \
        bash \
        wget \
        curl \
        libfaketime \
        libvulkan1 \
        libvulkan1:i386 \
        libegl1 \
        libgl1-mesa-dri \
        libgl1-mesa-dri:i386 \
        libgl1-mesa-glx:i386 \
        libsdl2-2.0-0 \
        libc6:i386 \
        libstdc++6:i386 \
        libx11-6:i386 \
        libxext6:i386 \
        libxrandr2:i386 \
        libfreetype6:i386 \
        xdg-utils \
        zenity \
        fonts-liberation \
        libnss3 \
        libgbm1 \
        libnspr4 \
    && rm -rf /var/lib/apt/lists/*

# Install Steam .deb bootstrap
RUN wget -O /tmp/steam.deb "https://cdn.akamai.steamstatic.com/client/installer/steam.deb" && \
    dpkg -i /tmp/steam.deb || apt-get install -f -y && \
    rm -f /tmp/steam.deb

# Ensure EGL platform config dirs are writable for nvidia driver volume setup
RUN mkdir -p /usr/share/egl/egl_external_platform.d \
             /usr/share/glvnd/egl_vendor.d \
             /usr/share/vulkan/icd.d \
             /usr/lib/x86_64-linux-gnu/gbm

# Create config directory (Steam state persists here)
RUN mkdir -p /home/retro/config /home/retro/.steam

# Copy startup script
COPY steam/startup-app.sh /opt/gow/startup-app.sh
RUN chmod +x /opt/gow/startup-app.sh

# Override base-app's 30-nvidia.sh with read-only-safe version
COPY _common/30-nvidia-readonly-safe.sh /etc/cont-init.d/30-nvidia.sh
RUN chmod +x /etc/cont-init.d/30-nvidia.sh

# Point EGL/Vulkan lookups to the mounted NVIDIA driver volume paths
ENV __EGL_VENDOR_LIBRARY_DIRS=/usr/nvidia/share/glvnd/egl_vendor.d \
    __EGL_EXTERNAL_PLATFORM_CONFIG_DIRS=/usr/nvidia/share/egl/egl_external_platform.d \
    VK_ICD_FILENAMES=/usr/nvidia/share/vulkan/icd.d/nvidia_icd.json

WORKDIR /home/retro
