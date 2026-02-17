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
#   ROM_FILENAME     - Name of ROM file in /home/retro/roms/ (e.g., "game.3ds")
#   FAKETIME         - Optional fake time string (e.g., "2024-01-01 12:00:00")
#   RUN_SWAY         - Set to "1" to use Sway compositor (single-screen/settings).
#                      Keep "0" for dual-screen SeparateWindows to avoid nested
#                      compositor stutter under Wolf.
#   LAYOUT_OPTION    - Override layout_option in qt-config.ini at startup
#                      0=Default(stacked), 1=SingleScreen, 4=SeparateWindows
#   AZAHAR_FULLSCREEN - 0=windowed, 1=fullscreen (ignored for SeparateWindows)
#   ROM_OPTIONAL     - Set to "1" to allow launching without a ROM (settings mode)
###############################################################################

gow_log "=== Azahar 3DS Emulator Starting ==="
gow_log "ROM_FILENAME=${ROM_FILENAME:-<not set>}"
gow_log "LAYOUT_OPTION=${LAYOUT_OPTION:-<default from config>}"
gow_log "WAYLAND_DISPLAY=${WAYLAND_DISPLAY:-<not set>}"
gow_log "PULSE_SERVER=${PULSE_SERVER:-<not set>}"

# GOW launch-comp treats RUN_SWAY / RUN_GAMESCOPE as "enabled if non-empty"
# (it does not parse boolean values). Normalize common false values so that
# RUN_SWAY=0 actually disables Sway.
#
# We also track explicit disables via _DISABLED flags so we don't
# auto-enable compositors that were explicitly turned off.
case "${RUN_SWAY:-}" in
    0|false|FALSE|no|NO|off|OFF) unset RUN_SWAY; RUN_SWAY_DISABLED=1 ;;
esac
case "${RUN_GAMESCOPE:-}" in
    0|false|FALSE|no|NO|off|OFF) unset RUN_GAMESCOPE; RUN_GAMESCOPE_DISABLED=1 ;;
esac

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

# Runtime layout override — patch qt-config.ini if LAYOUT_OPTION is set
if [ -n "${LAYOUT_OPTION:-}" ]; then
    CONFIG_FILE="/home/retro/config/azahar-emu/qt-config.ini"
    if [ -f "$CONFIG_FILE" ]; then
        gow_log "Overriding layout_option to ${LAYOUT_OPTION}"
        sed -i "s/^layout_option=.*/layout_option=${LAYOUT_OPTION}/" "$CONFIG_FILE"
    fi
fi

# Determine ROM path (optional for settings-only mode)
ROM_PATH=""
if [ -n "${ROM_FILENAME:-}" ]; then
    ROM_PATH="/home/retro/roms/${ROM_FILENAME}"
    if [ ! -f "$ROM_PATH" ]; then
        gow_log "ROM not found at exact path: ${ROM_PATH}"
        # Auto-detect: find first .3ds/.cia/.cxi/.app file in roms directory
        ROM_PATH=$(find /home/retro/roms/ -maxdepth 1 \( -type f -o -type l \) 2>/dev/null \
            | grep -iE '\.(3ds|cia|cxi|app)$' | head -1 || true)
        if [ -n "$ROM_PATH" ] && [ -f "$ROM_PATH" ]; then
            gow_log "Auto-detected ROM: ${ROM_PATH}"
        else
            gow_log "ERROR: No ROM files found in /home/retro/roms/"
            gow_log "Available files:"
            ls -la /home/retro/roms/ 2>/dev/null || echo "  (empty)"
            sleep 3600
            exit 1
        fi
    fi
elif [ "${ROM_OPTIONAL:-0}" != "1" ]; then
    # No ROM_FILENAME set — try auto-detect
    ROM_PATH=$(find /home/retro/roms/ -maxdepth 1 -type f -o -type l 2>/dev/null \
        | grep -iE '\.(3ds|cia|cxi|app)$' | head -1 || true)
    if [ -n "$ROM_PATH" ] && [ -f "$ROM_PATH" ]; then
        gow_log "No ROM_FILENAME set — auto-detected: ${ROM_PATH}"
    else
        gow_log "ERROR: ROM_FILENAME is not set and no ROM files found in /home/retro/roms/"
        sleep 3600
        exit 1
    fi
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

# Read the active layout option from config
ACTIVE_LAYOUT=$(grep -oP 'layout_option=\K\d+' /home/retro/config/azahar-emu/qt-config.ini 2>/dev/null || echo "0")
gow_log "Active layout_option=${ACTIVE_LAYOUT}"

# In dual-screen mode, prefer Gamescope over Sway by default.
# Azahar's AppImage bundles Qt/XCB (and may not include a compatible Wayland
# plugin), so pure direct-Wayland launch can fail. Gamescope provides the
# required Xwayland path with much lower overhead than full Sway.
#
# HOWEVER: If RUN_GAMESCOPE was explicitly set to 0/false, respect that.
# This allows Wolf's custom waylanddisplaysrc to see Azahar's windows directly.
if [ "$ACTIVE_LAYOUT" = "4" ] && [ -z "${RUN_SWAY:-}" ] && [ -z "${RUN_GAMESCOPE:-}" ] && [ -z "${RUN_GAMESCOPE_DISABLED:-}" ]; then
    export RUN_GAMESCOPE=1
    gow_log "Dual-screen mode: enabling RUN_GAMESCOPE=1 (no RUN_SWAY)"
fi

# Launch via GOW compositor wrapper.
# The `launcher` function from launch-comp.sh handles direct launch vs
# Sway/Gamescope based on RUN_* env vars.
source /opt/gow/launch-comp.sh

# Build launch arguments
AZAHAR_ARGS=()

# Fullscreen handling:
# - SeparateWindows (layout=4): Never use -f flag. Sway auto-tiles both windows.
# - Other layouts: Use -f for gameplay, skip for settings mode.
if [ "$ACTIVE_LAYOUT" = "4" ]; then
    gow_log "SeparateWindows mode: Sway will tile both windows (no fullscreen flag)"
elif [ "${AZAHAR_FULLSCREEN:-1}" = "1" ]; then
    AZAHAR_ARGS+=("-f")
fi

if [ -n "$ROM_PATH" ]; then
    AZAHAR_ARGS+=("${ROM_PATH}")
fi

launcher /Applications/azahar.AppImage --appimage-extract-and-run "${AZAHAR_ARGS[@]}"
