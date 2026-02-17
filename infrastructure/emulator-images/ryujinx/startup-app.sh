#!/bin/bash
set -euo pipefail
source /opt/gow/bash-lib/utils.sh

###############################################################################
# Ryujinx (Nintendo Switch) Startup Script
#
# Wolf calls /opt/gow/startup.sh → this script (/opt/gow/startup-app.sh).
# Wolf injects: WAYLAND_DISPLAY, PULSE_SERVER, XDG_RUNTIME_DIR, DISPLAY_*
#
# Expected env vars (set by Wolf config or Gamer Agent):
#   ROM_FILENAME     - Name of ROM file in /home/retro/roms/ (e.g., "game.xci")
#   FAKETIME         - Optional fake time string
#   RUN_SWAY         - Set to "1" to use Sway compositor (default: 1)
#   ROM_OPTIONAL     - Set to "1" to allow launching without a ROM (settings mode)
###############################################################################

gow_log "=== Ryujinx (Nintendo Switch) Starting ==="
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

# Ryujinx config directory: ~/.config/Ryujinx/
mkdir -p /home/retro/.config
if [ -d /home/retro/config/ryujinx ]; then
    ln -sfn /home/retro/config/ryujinx /home/retro/.config/Ryujinx
else
    mkdir -p /home/retro/config/ryujinx
    ln -sfn /home/retro/config/ryujinx /home/retro/.config/Ryujinx
fi

# Ryujinx needs Switch keys in ~/.config/Ryujinx/system/
# Symlink from our canonical firmware path
if [ -d /home/retro/firmware/switch ]; then
    gow_log "Linking Switch firmware/keys from firmware mount"
    mkdir -p /home/retro/.config/Ryujinx/system
    for f in /home/retro/firmware/switch/*; do
        [ -e "$f" ] && ln -sfn "$f" /home/retro/.config/Ryujinx/system/"$(basename "$f")" || true
    done
fi

# Determine ROM path
ROM_PATH=""
if [ -n "${ROM_FILENAME:-}" ]; then
    ROM_PATH="/home/retro/roms/${ROM_FILENAME}"
    if [ ! -f "$ROM_PATH" ]; then
        gow_log "ROM not found at exact path: ${ROM_PATH}"
        ROM_PATH=$(find /home/retro/roms/ -maxdepth 1 \( -type f -o -type l \) 2>/dev/null \
            | grep -iE '\.(xci|nsp|nca|nro|nso)$' | head -1 || true)
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
        | grep -iE '\.(xci|nsp|nca|nro|nso)$' | head -1 || true)
    if [ -n "$ROM_PATH" ] && [ -f "$ROM_PATH" ]; then
        gow_log "No ROM_FILENAME set — auto-detected: ${ROM_PATH}"
    else
        gow_log "ERROR: ROM_FILENAME is not set and no ROM files found"
        sleep 3600
        exit 1
    fi
else
    gow_log "ROM_OPTIONAL=1: launching Ryujinx without a ROM (settings mode)"
fi

# Launch via compositor
source /opt/gow/launch-comp.sh

RYUJINX_BIN="/Applications/publish/Ryujinx"

if [ -n "$ROM_PATH" ]; then
    gow_log "Launching Ryujinx with ROM: ${ROM_PATH}"
    launcher "$RYUJINX_BIN" --fullscreen "$ROM_PATH"
else
    gow_log "Launching Ryujinx UI only"
    launcher "$RYUJINX_BIN"
fi
