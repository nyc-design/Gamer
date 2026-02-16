###############################################################################
# Azahar 3DS Emulator — GOW App Container
#
# Built on the Games on Whales base-app image which provides:
#   - Sway/Gamescope Wayland compositor
#   - PulseAudio integration
#   - Mesa/Vulkan GPU drivers
#   - Startup script framework (/opt/gow/startup.sh → /opt/gow/startup-app.sh)
#
# Wolf spawns this container when a Moonlight client connects and selects
# the Azahar app. Wolf injects WAYLAND_DISPLAY, PULSE_SERVER, XDG_RUNTIME_DIR,
# and mounts /home/retro for persistent state.
###############################################################################
FROM ghcr.io/games-on-whales/base-app:edge

ENV DEBIAN_FRONTEND=noninteractive
# AppImages need FUSE or extract-and-run mode; Docker doesn't have FUSE
ENV APPIMAGE_EXTRACT_AND_RUN=1

# Install dependencies for Azahar and libfaketime
RUN apt-get update && apt-get install -y --no-install-recommends \
        curl \
        wget \
        libsdl2-2.0-0 \
        libvulkan1 \
        libfaketime \
        # Qt6 deps that Azahar may need
        qt6-base-dev \
        libqt6multimedia6 \
        libqt6opengl6-dev \
        # OpenGL/EGL for 3DS GPU emulation
        libglu1-mesa \
        libegl1 \
        libgl1-mesa-dri \
    && rm -rf /var/lib/apt/lists/*

# Download Azahar 3DS emulator AppImage
ARG AZAHAR_VERSION=2124.3
RUN mkdir -p /Applications && \
    wget -O /Applications/azahar.AppImage \
        "https://github.com/azahar-emu/azahar/releases/download/${AZAHAR_VERSION}/azahar.AppImage" && \
    chmod +x /Applications/azahar.AppImage

# Create default config directory structure
# Azahar stores config in ~/.config/azahar-emu/ or ~/.local/share/azahar-emu/
RUN mkdir -p /defaults/config/azahar-emu

# Copy default Azahar config (optimized for streaming)
COPY azahar-config/ /defaults/config/azahar-emu/

# Create ROM and save directories
RUN mkdir -p /home/retro/roms /home/retro/saves /home/retro/config /home/retro/firmware

# Copy the startup script that Wolf will invoke
COPY startup-app.sh /opt/gow/startup-app.sh
RUN chmod +x /opt/gow/startup-app.sh

WORKDIR /home/retro
