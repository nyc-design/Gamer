FROM ghcr.io/games-on-whales/base-app:edge

ENV DEBIAN_FRONTEND=noninteractive
ENV EMULATOR_NAME=melonDS

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    bash \
    wget \
    xz-utils \
    libfuse2 \
    libsdl2-2.0-0 \
    libopengl0 \
    libqt6widgets6 \
    libqt6opengl6 \
    melonds \
    && rm -rf /var/lib/apt/lists/*

COPY melonds/startup-app.sh /opt/gow/startup-app.sh
RUN chmod +x /opt/gow/startup-app.sh

WORKDIR /home/retro
