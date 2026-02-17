FROM ghcr.io/games-on-whales/base-app:edge

ENV DEBIAN_FRONTEND=noninteractive
ENV EMULATOR_NAME=Dolphin

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    bash \
    wget \
    xz-utils \
    libfuse2 \
    libopengl0 \
    dolphin-emu \
    && rm -rf /var/lib/apt/lists/*

COPY dolphin/startup-app.sh /opt/gow/startup-app.sh
RUN chmod +x /opt/gow/startup-app.sh

WORKDIR /home/retro
