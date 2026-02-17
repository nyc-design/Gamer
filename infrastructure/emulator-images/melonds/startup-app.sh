#!/bin/bash
set -euo pipefail
source /opt/gow/bash-lib/utils.sh

###############################################################################
# melonDS (Nintendo DS) Startup Script
#
# Wolf calls /opt/gow/startup.sh → this script (/opt/gow/startup-app.sh).
# Wolf injects: WAYLAND_DISPLAY, PULSE_SERVER, XDG_RUNTIME_DIR, DISPLAY_*
#
# Expected env vars (set by Wolf config or Gamer Agent):
#   ROM_FILENAME     - Name of ROM file in /home/retro/roms/ (e.g., "game.nds")
#   FAKETIME         - Optional fake time string (e.g., "2024-01-01 12:00:00")
#   RUN_SWAY         - Set to "1" to use Sway compositor (default: 1)
#   ROM_OPTIONAL     - Set to "1" to allow launching without a ROM (settings mode)
###############################################################################

gow_log "=== melonDS (Nintendo DS) Starting ==="
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

# Symlink melonDS config to expected location
# melonDS looks in ~/.config/melonDS/
mkdir -p /home/retro/.config
if [ -d /home/retro/config/melonds ]; then
    ln -sfn /home/retro/config/melonds /home/retro/.config/melonDS
else
    mkdir -p /home/retro/config/melonds
    ln -sfn /home/retro/config/melonds /home/retro/.config/melonDS
fi

# Symlink DS firmware/BIOS files if firmware directory has them
# melonDS expects BIOS files in its config directory
if [ -d /home/retro/firmware/ds ]; then
    gow_log "Linking DS firmware from firmware mount"
    for f in /home/retro/firmware/ds/*; do
        [ -e "$f" ] && ln -sfn "$f" /home/retro/.config/melonDS/"$(basename "$f")" || true
    done
fi

# Determine ROM path
ROM_PATH=""
if [ -n "${ROM_FILENAME:-}" ]; then
    ROM_PATH="/home/retro/roms/${ROM_FILENAME}"
    if [ ! -f "$ROM_PATH" ]; then
        gow_log "ROM not found at exact path: ${ROM_PATH}"
        # Auto-detect: find first .nds/.dsi file in roms directory
        ROM_PATH=$(find /home/retro/roms/ -maxdepth 1 \( -type f -o -type l \) 2>/dev/null \
            | grep -iE '\.(nds|dsi)$' | head -1 || true)
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
    # No ROM_FILENAME set — try auto-detect
    ROM_PATH=$(find /home/retro/roms/ -maxdepth 1 \( -type f -o -type l \) 2>/dev/null \
        | grep -iE '\.(nds|dsi)$' | head -1 || true)
    if [ -n "$ROM_PATH" ] && [ -f "$ROM_PATH" ]; then
        gow_log "No ROM_FILENAME set — auto-detected: ${ROM_PATH}"
    else
        gow_log "ERROR: ROM_FILENAME is not set and no ROM files found"
        sleep 3600
        exit 1
    fi
else
    gow_log "ROM_OPTIONAL=1: launching melonDS without a ROM (settings mode)"
fi

# Launch via compositor
source /opt/gow/launch-comp.sh

if [ -n "$ROM_PATH" ]; then
    gow_log "Launching melonDS with ROM: ${ROM_PATH}"
    launcher melonds "$ROM_PATH"
else
    gow_log "Launching melonDS UI only"
    launcher melonds
fi
