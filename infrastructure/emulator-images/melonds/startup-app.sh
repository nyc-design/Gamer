#!/bin/bash
set -euo pipefail
source /opt/gow/bash-lib/utils.sh

gow_log "=== melonDS starting ==="

mkdir -p /home/retro/{roms,saves,config,firmware}
mkdir -p /home/retro/.config/melonDS

if [ -d /home/retro/config/melonds ]; then
  ln -sfn /home/retro/config/melonds /home/retro/.config/melonDS
fi

ROM_PATH=""
if [ -n "${ROM_FILENAME:-}" ]; then
  ROM_PATH="/home/retro/roms/${ROM_FILENAME}"
fi

source /opt/gow/launch-comp.sh

if [ -n "$ROM_PATH" ] && [ -f "$ROM_PATH" ]; then
  launcher melonds "$ROM_PATH"
else
  gow_log "ROM missing or not set (ROM_FILENAME). Launching melonDS UI only."
  launcher melonds
fi
