# Gamer Base Sunshine Image
# Two Sunshine instances + dual virtual Xorg displays for single/dual screen streaming.
# Extend this image per emulator (Azahar, melonDS, Dolphin, etc.)
#
# Based on CloudyPad's proven Sunshine container patterns:
# https://github.com/PierreBeucher/cloudypad/tree/master/containers/sunshine

# Build shader-overlay from source
FROM rust:1.85-bookworm AS build-shader-overlay
WORKDIR /build
RUN apt-get update && apt-get install -y \
    libx11-dev libxcomposite-dev libxdamage-dev libxfixes-dev \
    libgl-dev libglu1-mesa-dev pkg-config \
    && rm -rf /var/lib/apt/lists/*
COPY tools/shader-overlay/ .
RUN cargo build --release && strip target/release/shader-overlay

# Download Sunshine .deb in a separate stage
FROM alpine/curl:8.14.1 AS download-sunshine
WORKDIR /app
ARG SUNSHINE_VERSION=2026.220.22826
RUN curl -L -o /app/sunshine.deb \
    "https://github.com/LizardByte/Sunshine/releases/download/v${SUNSHINE_VERSION}/sunshine-ubuntu-24.04-amd64.deb"

# Main image
FROM ubuntu:noble

ENV DEBIAN_FRONTEND=noninteractive

# System packages: X server, input, audio, utilities
RUN --mount=type=cache,target=/var/cache --mount=type=tmpfs,target=/var/log <<EOF
set -e
apt-get update
apt-get install -y \
    xserver-xorg \
    xserver-xorg-core \
    xserver-xorg-input-evdev \
    xserver-xorg-input-libinput \
    xserver-xorg-legacy \
    xserver-xorg-video-dummy \
    x11-utils \
    x11-xserver-utils \
    xauth \
    xdotool \
    xinput \
    xcvt \
    xterm \
    openbox \
    python3 \
    python3-evdev \
    dbus \
    dbus-x11 \
    supervisor \
    pulseaudio \
    curl \
    gettext-base \
    bc \
    sudo \
    wmctrl \
    unzip \
    locales \
    libxcomposite-dev \
    libxcursor1 \
    libopengl0 \
    kmod

# Vulkan, GL, EGL libs
apt-get install -y \
    libvulkan1 \
    libgl1-mesa-dri \
    libgl1 \
    libgbm1 \
    libegl1 \
    libglx-mesa0 \
    libglx0 \
    libgles2 \
    mesa-utils \
    mesa-vulkan-drivers \
    libnvidia-egl-gbm1

# SDL2, PulseAudio client, XCB cursor (needed by emulators)
apt-get install -y \
    libsdl2-2.0-0 \
    libpulse0 \
    libxcb-cursor0

apt-get clean
EOF

# Locale
RUN locale-gen en_US.UTF-8 && update-locale LANG=en_US.UTF-8

# Sunshine
RUN --mount=type=cache,target=/var/cache --mount=type=tmpfs,target=/var/log \
    --mount=type=bind,from=download-sunshine,source=/app,target=/app <<EOF
set -e
apt-get update
apt-get install -y /app/sunshine.deb
EOF

# Environment
ENV DISPLAY=:0
ENV GAMER_USER=gamer
ENV GAMER_HOME=/home/gamer
ENV GAMER_DATA_DIR=/gamer/data
ENV GAMER_LOG_DIR=/gamer/log
ENV GAMER_CONF_DIR=/gamer/conf
ENV GAMER_RUNTIME_DIR=/gamer/runtime
ENV GAMER_BIN_DIR=/gamer/bin
ENV XDG_RUNTIME_DIR=/gamer/runtime
ENV XDG_CONFIG_HOME=/gamer/conf
ENV XDG_DATA_HOME=/gamer/data
ENV XDG_CACHE_HOME=/home/gamer/.cache
ENV DBUS_SYSTEM_BUS_ADDRESS=unix:path=/gamer/runtime/dbus
ENV PATH=$PATH:/gamer/bin

# Dual-screen config (overridable per-emulator)
# Set DUAL_SCREEN=0 for single-screen mode (only top Sunshine instance starts)
ENV DUAL_SCREEN=1
# NVFBC is zero-copy (preferred). Falls back to x11 capture if unavailable.
ENV SUNSHINE_CAPTURE=nvfbc
# Sunshine credentials
ENV SUNSHINE_USERNAME=admin
ENV SUNSHINE_PASSWORD_BASE64=YWRtaW4=

# Shader overlay config (optional — set to enable)
# SHADER_PRESET: path to .slangp file for primary window
# SHADER_PRESET_BOTTOM: path to .slangp file for secondary window (dual-screen)
# SHADER_WINDOW: window name/title to apply primary shader to
# SHADER_WINDOW_BOTTOM: window name/title to apply secondary shader to
ENV SHADER_PRESET=""
ENV SHADER_PRESET_BOTTOM=""
ENV SHADER_WINDOW=""
ENV SHADER_WINDOW_BOTTOM=""

# NVIDIA driver env (set by host/orchestrator)
ENV NVIDIA_ENABLE=true
ENV NVIDIA_DRIVER_VERSION=""
ENV NVIDIA_DRIVER_TYPE=display

# Create user
RUN useradd -m -d /home/gamer -s /bin/bash -u 1001 gamer

# Copy overlay (scripts, configs)
COPY images/base-sunshine/overlay/ /

# shader-overlay binary (from Rust build stage)
COPY --from=build-shader-overlay /build/target/release/shader-overlay /gamer/bin/shader-overlay

# Bundle RetroArch shader presets (subset of slang-shaders)
RUN --mount=type=cache,target=/var/cache <<EOF
set -e
apt-get update && apt-get install -y git
git clone --depth 1 --filter=blob:none --sparse \
    https://github.com/libretro/slang-shaders.git /tmp/slang-shaders
cd /tmp/slang-shaders
git sparse-checkout set crt handheld interpolation misc/shaders
mkdir -p /gamer/shaders
cp -r crt handheld interpolation misc /gamer/shaders/
rm -rf /tmp/slang-shaders
apt-get remove -y git && apt-get autoremove -y
EOF

# Make scripts executable
RUN chmod +x /gamer/bin/*.sh /gamer/entrypoint.sh

# Create directories
RUN mkdir -p /gamer/data /gamer/log /gamer/runtime /gamer/conf /home/gamer/.cache \
    && chown -R gamer:gamer /gamer /home/gamer

HEALTHCHECK --interval=10s --timeout=5s --start-period=30s --retries=3 \
    CMD /gamer/bin/healthcheck.sh

ENTRYPOINT ["/gamer/entrypoint.sh"]
