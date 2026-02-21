# Azahar 3DS Emulator on Sunshine Base
# Extends base-sunshine with Azahar AppImage, Qt6 deps, and dual-screen window management

FROM gamer/base-sunshine:latest

ENV DEBIAN_FRONTEND=noninteractive

# Azahar AppImage + dependencies
ARG AZAHAR_VERSION=2124.3
RUN --mount=type=cache,target=/var/cache --mount=type=tmpfs,target=/var/log <<EOF
set -e
apt-get update
apt-get install -y --no-install-recommends \
    qt6-base-dev \
    qt6-wayland \
    libqt6multimedia6 \
    libqt6opengl6-dev \
    libglu1-mesa \
    libfaketime \
    fuse3 \
    libfuse3-3

apt-get clean
EOF

# Download Azahar AppImage
RUN mkdir -p /Applications && \
    curl -L -o /Applications/azahar.AppImage \
        "https://github.com/azahar-emu/azahar/releases/download/${AZAHAR_VERSION}/azahar.AppImage" && \
    chmod +x /Applications/azahar.AppImage

# Extract AppImage to avoid FUSE issues in container
RUN cd /Applications && \
    ./azahar.AppImage --appimage-extract && \
    mv squashfs-root azahar && \
    rm azahar.AppImage

# Copy overlay (scripts, configs)
COPY images/azahar-sunshine/overlay/ /

RUN chmod +x /gamer/bin/start-azahar.sh

# Create ROM/save/config dirs
RUN mkdir -p /home/gamer/roms /home/gamer/saves /home/gamer/config/azahar-emu \
    /home/gamer/firmware /home/gamer/.config /home/gamer/.local/share \
    && chown -R gamer:gamer /home/gamer

# Azahar environment
ENV APPIMAGE_EXTRACT_AND_RUN=1
ENV ROM_FILENAME=""
ENV LAYOUT_OPTION=4
ENV AZAHAR_FULLSCREEN=0
ENV FAKETIME=""
