#!/usr/bin/env bash
set -euo pipefail

if [ "${SCREEN_TOOL_ENABLED:-1}" = "0" ]; then
    echo "[screen-tool-toggle] Disabled via SCREEN_TOOL_ENABLED=0"
    sleep infinity
fi

/gamer/bin/wait-x.sh
sleep 2

mkdir -p /home/gamer/.cache

# Tiny standalone toggle surface. This is intentionally separate from the main
# screen-tool surface so users can always bring the tool back even when the
# secondary emulator window is active.
exec /gamer/bin/screen-tool \
  --toggle-only \
  --title ScreenToolToggle \
  --x 10 \
  --y 10 \
  --width 52 \
  --height 52 \
  --max-fps 30
