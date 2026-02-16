#!/bin/bash
set -e
source /opt/gow/bash-lib/utils.sh

###############################################################################
# Azahar 3DS Emulator Startup Script
#
# Wolf calls /opt/gow/startup.sh → this script (/opt/gow/startup-app.sh).
# Wolf injects: WAYLAND_DISPLAY, PULSE_SERVER, XDG_RUNTIME_DIR, DISPLAY_*
#
# Expected env vars (set by Wolf config or Gamer Agent):
#   ROM_FILENAME   - Name of ROM file in /home/retro/roms/ (e.g., "game.3ds")
#   FAKETIME       - Optional fake time string (e.g., "2024-01-01 12:00:00")
#   RUN_SWAY       - Set to "1" to use Sway compositor (recommended for Azahar)
###############################################################################

gow_log "=== Azahar 3DS Emulator Starting ==="
gow_log "ROM_FILENAME=${ROM_FILENAME}"
gow_log "WAYLAND_DISPLAY=${WAYLAND_DISPLAY}"
gow_log "PULSE_SERVER=${PULSE_SERVER}"

# libfaketime — only activate if FAKETIME is set
if [ -n "$FAKETIME" ]; then
    gow_log "Enabling libfaketime: ${FAKETIME}"
    export LD_PRELOAD=/usr/lib/x86_64-linux-gnu/faketime/libfaketime.so.1
    export FAKETIME_NO_CACHE=1
fi

# Copy baked default config on first run (user overrides persist via mount)
if [ ! -f /home/retro/config/azahar-emu/qt-config.ini ]; then
    gow_log "First run — copying default Azahar config"
    mkdir -p /home/retro/config/azahar-emu
    cp -r /defaults/config/azahar-emu/* /home/retro/config/azahar-emu/ 2>/dev/null || true
fi

# Determine ROM path (optional for settings-only mode)
ROM_PATH=""
if [ -n "${ROM_FILENAME}" ]; then
    ROM_PATH="/home/retro/roms/${ROM_FILENAME}"
    if [ ! -f "$ROM_PATH" ]; then
        gow_log "ERROR: ROM not found at ${ROM_PATH}"
        gow_log "Available ROMs:"
        ls -la /home/retro/roms/ 2>/dev/null || echo "  (empty)"
        # Keep container alive for debugging
        sleep 3600
        exit 1
    fi
elif [ "${ROM_OPTIONAL:-0}" != "1" ]; then
    gow_log "ERROR: ROM_FILENAME is not set and ROM_OPTIONAL is not enabled"
    sleep 3600
    exit 1
else
    gow_log "ROM_OPTIONAL=1: launching Azahar without a ROM (settings mode)"
fi

# Symlink Azahar config to expected location
# Azahar looks in ~/.local/share/azahar-emu/ and ~/.config/azahar-emu/
mkdir -p /home/retro/.config
mkdir -p /home/retro/.local/share
ln -sfn /home/retro/config/azahar-emu /home/retro/.config/azahar-emu
ln -sfn /home/retro/config/azahar-emu /home/retro/.local/share/azahar-emu

# Symlink 3DS system files if firmware directory has them
if [ -d /home/retro/firmware/3ds/sysdata ]; then
    gow_log "Linking 3DS system data from firmware mount"
    ln -sfn /home/retro/firmware/3ds/sysdata /home/retro/config/azahar-emu/sysdata
fi

if [ -n "$ROM_PATH" ]; then
    gow_log "Launching Azahar with ROM: ${ROM_PATH}"
else
    gow_log "Launching Azahar without ROM"
fi

# Launch via Sway compositor (required for Wayland rendering)
# The `launcher` function from launch-comp.sh handles Sway/Gamescope setup
source /opt/gow/launch-comp.sh

# Keep stream surface stable by default:
# - AZAHAR_FULLSCREEN=1 -> start in fullscreen (gameplay mode)
# - AZAHAR_FULLSCREEN=0 -> windowed (settings/config mode)
AZAHAR_ARGS=()
if [ "${AZAHAR_FULLSCREEN:-1}" = "1" ]; then
    AZAHAR_ARGS+=("-f")
fi
if [ -n "$ROM_PATH" ]; then
    AZAHAR_ARGS+=("${ROM_PATH}")
fi

launcher /Applications/azahar.AppImage --appimage-extract-and-run "${AZAHAR_ARGS[@]}"
