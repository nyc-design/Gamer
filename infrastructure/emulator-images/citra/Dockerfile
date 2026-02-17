FROM ghcr.io/games-on-whales/base-app:edge

ENV DEBIAN_FRONTEND=noninteractive
ENV APPIMAGE_EXTRACT_AND_RUN=1
ENV EMULATOR_NAME=Citra

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
    && rm -rf /var/lib/apt/lists/*

ARG CITRA_APPIMAGE_URL="https://github.com/PabloMK7/citra/releases/download/r71eca05/citra-linux-appimage-20240513-71eca05.tar.gz"
RUN mkdir -p /Applications && \
    wget -O /tmp/citra.tar.gz "${CITRA_APPIMAGE_URL}" && \
    tar -xzf /tmp/citra.tar.gz -C /tmp && \
    find /tmp -type f -name "*.AppImage" -exec cp {} /Applications/citra.AppImage \; && \
    chmod +x /Applications/citra.AppImage && \
    rm -rf /tmp/citra.tar.gz /tmp/citra-*

COPY citra/startup-app.sh /opt/gow/startup-app.sh
RUN chmod +x /opt/gow/startup-app.sh

WORKDIR /home/retro
