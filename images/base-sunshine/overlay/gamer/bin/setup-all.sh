#!/usr/bin/env bash
set -e

echo "[setup] Starting Gamer base setup..."

source /gamer/bin/setup-dirs.sh
source /gamer/bin/setup-user.sh

if [ "$NVIDIA_ENABLE" = "true" ] && [ -n "$NVIDIA_DRIVER_VERSION" ]; then
    source /gamer/bin/setup-nvidia-driver.sh
fi

source /gamer/bin/setup-x-config.sh
source /gamer/bin/setup-pulseaudio.sh
source /gamer/bin/setup-sunshine.sh

echo "[setup] Setup complete."
