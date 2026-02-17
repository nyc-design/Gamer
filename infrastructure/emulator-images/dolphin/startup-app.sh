#!/bin/bash
set -euo pipefail
source /opt/gow/bash-lib/utils.sh

gow_log "=== Dolphin starting ==="

mkdir -p /home/retro/{roms,saves,config,firmware}
mkdir -p /home/retro/.config /home/retro/.local/share

if [ -d /home/retro/config/dolphin ]; then
  ln -sfn /home/retro/config/dolphin /home/retro/.config/dolphin-emu
  ln -sfn /home/retro/config/dolphin /home/retro/.local/share/dolphin-emu
fi

ROM_PATH=""
if [ -n "${ROM_FILENAME:-}" ]; then
  ROM_PATH="/home/retro/roms/${ROM_FILENAME}"
fi

source /opt/gow/launch-comp.sh

if [ -n "$ROM_PATH" ] && [ -f "$ROM_PATH" ]; then
  launcher dolphin-emu -e "$ROM_PATH"
else
  gow_log "ROM missing or not set (ROM_FILENAME). Launching Dolphin UI only."
  launcher dolphin-emu
fi
