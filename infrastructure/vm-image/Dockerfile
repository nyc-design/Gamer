# Custom Wolf app image with emulators and client agent
FROM ghcr.io/games-on-whales/wolf:stable AS wolf-base

# Install system dependencies for emulators
RUN apt-get update && apt-get install -y \
    python3 \
    python3-pip \
    curl \
    wget \
    unzip \
    && rm -rf /var/lib/apt/lists/*

# Install emulators
WORKDIR /emulators

# Install mGBA for Game Boy/Color/Advance
RUN wget -O mgba.tar.xz https://github.com/mgba-emu/mgba/releases/latest/download/mgba-0.10.3-linux-x64.tar.xz \
    && tar -xf mgba.tar.xz \
    && rm mgba.tar.xz

# Install melonDS for Nintendo DS
RUN wget -O melonds.tar.xz https://github.com/melonDS-emu/melonDS/releases/latest/download/melonDS-linux-x64.tar.xz \
    && tar -xf melonds.tar.xz \
    && rm melonds.tar.xz

# Install Citra for Nintendo 3DS
RUN wget -O citra.tar.gz https://github.com/citra-emu/citra-nightly/releases/latest/download/citra-linux-x64.tar.gz \
    && tar -xf citra.tar.gz \
    && rm citra.tar.gz

# Install Dolphin for GameCube/Wii
RUN wget -O dolphin.tar.xz https://github.com/dolphin-emu/dolphin/releases/latest/download/dolphin-x86_64.tar.xz \
    && tar -xf dolphin.tar.xz \
    && rm dolphin.tar.xz

# Copy emulator configurations
COPY emulators/ /emulators/
COPY configs/ /configs/

# Install client agent
WORKDIR /agent
COPY ../client-agent/requirements.txt .
RUN pip3 install --no-cache-dir -r requirements.txt
COPY ../client-agent/ .

# Create startup script
RUN echo '#!/bin/bash\n\
# Start client agent in background\n\
python3 /agent/src/main.py &\n\
\n\
# Start Wolf\n\
exec "$@"' > /startup.sh && chmod +x /startup.sh

ENTRYPOINT ["/startup.sh"]