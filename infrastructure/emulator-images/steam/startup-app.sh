#!/bin/bash
set -euo pipefail
source /opt/gow/bash-lib/utils.sh

###############################################################################
# Steam (PC Gaming) Startup Script
#
# Wolf calls /opt/gow/startup.sh → this script (/opt/gow/startup-app.sh).
# Wolf injects: WAYLAND_DISPLAY, PULSE_SERVER, XDG_RUNTIME_DIR, DISPLAY_*
#
# Expected env vars (set by Wolf config or Gamer Agent):
#   RUN_SWAY         - Set to "1" to use Sway compositor (default: 1)
#   STEAM_GAME_ID    - Optional: launch directly into a specific game
###############################################################################

gow_log "=== Steam (PC Gaming) Starting ==="
gow_log "WAYLAND_DISPLAY=${WAYLAND_DISPLAY:-<not set>}"
gow_log "PULSE_SERVER=${PULSE_SERVER:-<not set>}"
gow_log "STEAM_GAME_ID=${STEAM_GAME_ID:-<not set>}"

# Create standard directories
mkdir -p /home/retro/config

# Symlink Steam data to expected locations
# Steam looks in ~/.steam/ and ~/.local/share/Steam/
mkdir -p /home/retro/.local/share
if [ -d /home/retro/config/steam ]; then
    ln -sfn /home/retro/config/steam /home/retro/.steam
    ln -sfn /home/retro/config/steam /home/retro/.local/share/Steam
else
    mkdir -p /home/retro/config/steam
    ln -sfn /home/retro/config/steam /home/retro/.steam
    ln -sfn /home/retro/config/steam /home/retro/.local/share/Steam
fi

# Launch via compositor
source /opt/gow/launch-comp.sh

STEAM_ARGS=("-bigpicture" "-noreactlogin")

# If a specific game ID is set, launch directly into it
if [ -n "${STEAM_GAME_ID:-}" ]; then
    gow_log "Launching Steam with game ID: ${STEAM_GAME_ID}"
    STEAM_ARGS+=("-applaunch" "${STEAM_GAME_ID}")
else
    gow_log "Launching Steam Big Picture"
fi

launcher steam "${STEAM_ARGS[@]}"
