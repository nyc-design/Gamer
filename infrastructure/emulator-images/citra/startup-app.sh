#!/bin/bash
set -euo pipefail
source /opt/gow/bash-lib/utils.sh

gow_log "=== Citra starting ==="

mkdir -p /home/retro/{roms,saves,config,firmware}
mkdir -p /home/retro/.config /home/retro/.local/share

if [ -d /home/retro/config/citra-emu ]; then
  ln -sfn /home/retro/config/citra-emu /home/retro/.config/citra-emu
  ln -sfn /home/retro/config/citra-emu /home/retro/.local/share/citra-emu
fi

ROM_PATH=""
if [ -n "${ROM_FILENAME:-}" ]; then
  ROM_PATH="/home/retro/roms/${ROM_FILENAME}"
fi

source /opt/gow/launch-comp.sh

ARGS=(--appimage-extract-and-run)
if [ -n "$ROM_PATH" ] && [ -f "$ROM_PATH" ]; then
  ARGS+=(-f "$ROM_PATH")
fi

launcher /Applications/citra.AppImage "${ARGS[@]}"
