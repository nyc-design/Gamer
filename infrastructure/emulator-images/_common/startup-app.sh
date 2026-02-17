#!/bin/bash
set -euo pipefail

source /opt/gow/bash-lib/utils.sh

gow_log "=== ${EMULATOR_NAME:-emulator} container starting ==="
gow_log "WAYLAND_DISPLAY=${WAYLAND_DISPLAY:-<unset>}"
gow_log "PULSE_SERVER=${PULSE_SERVER:-<unset>}"

mkdir -p /home/retro/{roms,saves,config,firmware}

if [ -n "${APP_COMMAND:-}" ]; then
  gow_log "Launching APP_COMMAND: ${APP_COMMAND}"
  exec bash -lc "${APP_COMMAND}"
fi

gow_log "No APP_COMMAND set for ${EMULATOR_NAME:-emulator}. Sleeping."
exec sleep infinity
