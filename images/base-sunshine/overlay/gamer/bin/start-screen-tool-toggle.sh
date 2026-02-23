#!/usr/bin/env bash
set -euo pipefail

if [ "${SCREEN_TOOL_ENABLED:-1}" = "0" ]; then
    echo "[screen-tool-toggle] Disabled via SCREEN_TOOL_ENABLED=0"
    sleep infinity
fi

/gamer/bin/wait-x.sh
sleep 2

mkdir -p /home/gamer/.cache

exec python3 /gamer/bin/screen-tool-toggle-button.py
