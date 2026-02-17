#!/bin/bash
set -euo pipefail
source /opt/gow/bash-lib/utils.sh

###############################################################################
# PPSSPP (PlayStation Portable) Startup Script
#
# Wolf calls /opt/gow/startup.sh → this script (/opt/gow/startup-app.sh).
# Wolf injects: WAYLAND_DISPLAY, PULSE_SERVER, XDG_RUNTIME_DIR, DISPLAY_*
#
# Expected env vars (set by Wolf config or Gamer Agent):
#   ROM_FILENAME     - Name of ROM file in /home/retro/roms/ (e.g., "game.iso")
#   FAKETIME         - Optional fake time string
#   RUN_SWAY         - Set to "1" to use Sway compositor (default: 1)
#   ROM_OPTIONAL     - Set to "1" to allow launching without a ROM (settings mode)
###############################################################################

gow_log "=== PPSSPP (PlayStation Portable) Starting ==="
gow_log "ROM_FILENAME=${ROM_FILENAME:-<not set>}"
gow_log "WAYLAND_DISPLAY=${WAYLAND_DISPLAY:-<not set>}"
gow_log "PULSE_SERVER=${PULSE_SERVER:-<not set>}"

# libfaketime — only activate if FAKETIME is set
if [ -n "${FAKETIME:-}" ]; then
    gow_log "Enabling libfaketime: ${FAKETIME}"
    export LD_PRELOAD=/usr/lib/x86_64-linux-gnu/faketime/libfaketime.so.1
    export FAKETIME_NO_CACHE=1
fi

# Create standard directories
mkdir -p /home/retro/{roms,saves,config,firmware}

# Symlink PPSSPP config to expected location
# PPSSPP looks in ~/.config/ppsspp/PSP/
mkdir -p /home/retro/.config
if [ -d /home/retro/config/ppsspp ]; then
    ln -sfn /home/retro/config/ppsspp /home/retro/.config/ppsspp
else
    mkdir -p /home/retro/config/ppsspp
    ln -sfn /home/retro/config/ppsspp /home/retro/.config/ppsspp
fi

# PPSSPP save data goes in PSP/SAVEDATA/ — symlink to canonical saves path
mkdir -p /home/retro/config/ppsspp/PSP
if [ -d /home/retro/saves ]; then
    ln -sfn /home/retro/saves /home/retro/config/ppsspp/PSP/SAVEDATA
fi

# Determine ROM path
ROM_PATH=""
if [ -n "${ROM_FILENAME:-}" ]; then
    ROM_PATH="/home/retro/roms/${ROM_FILENAME}"
    if [ ! -f "$ROM_PATH" ]; then
        gow_log "ROM not found at exact path: ${ROM_PATH}"
        # Auto-detect: find first PSP ROM
        ROM_PATH=$(find /home/retro/roms/ -maxdepth 1 \( -type f -o -type l \) 2>/dev/null \
            | grep -iE '\.(iso|cso|pbp|elf|prx)$' | head -1 || true)
        if [ -n "$ROM_PATH" ] && [ -f "$ROM_PATH" ]; then
            gow_log "Auto-detected ROM: ${ROM_PATH}"
        else
            gow_log "ERROR: No ROM files found in /home/retro/roms/"
            ls -la /home/retro/roms/ 2>/dev/null || echo "  (empty)"
            sleep 3600
            exit 1
        fi
    fi
elif [ "${ROM_OPTIONAL:-0}" != "1" ]; then
    ROM_PATH=$(find /home/retro/roms/ -maxdepth 1 \( -type f -o -type l \) 2>/dev/null \
        | grep -iE '\.(iso|cso|pbp|elf|prx)$' | head -1 || true)
    if [ -n "$ROM_PATH" ] && [ -f "$ROM_PATH" ]; then
        gow_log "No ROM_FILENAME set — auto-detected: ${ROM_PATH}"
    else
        gow_log "ERROR: ROM_FILENAME is not set and no ROM files found"
        sleep 3600
        exit 1
    fi
else
    gow_log "ROM_OPTIONAL=1: launching PPSSPP without a ROM (settings mode)"
fi

# Launch via compositor
source /opt/gow/launch-comp.sh

if [ -n "$ROM_PATH" ]; then
    gow_log "Launching PPSSPP with ROM: ${ROM_PATH}"
    # PPSSPP SDL variant uses --fullscreen for gameplay
    launcher ppsspp --fullscreen "$ROM_PATH"
else
    gow_log "Launching PPSSPP UI only"
    launcher ppsspp
fi
