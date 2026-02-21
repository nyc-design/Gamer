#!/usr/bin/env bash
set -e

###############################################################################
# Azahar 3DS Emulator Startup Script (Sunshine base)
#
# Runs Azahar with Xorg virtual displays. In dual-screen mode (layout_option=4),
# Azahar creates two X11 windows — this script positions them on the correct
# virtual displays so each Sunshine instance captures the right screen.
#
# Env vars:
#   ROM_FILENAME     - ROM file in /home/gamer/roms/ (auto-detects if not set)
#   FAKETIME         - Optional fake time (e.g., "2024-01-01 12:00:00")
#   LAYOUT_OPTION    - 0=Default, 1=SingleScreen, 4=SeparateWindows (default: 4)
#   AZAHAR_FULLSCREEN - 0=windowed (default), 1=fullscreen
#   DUAL_SCREEN      - 1=dual display mode (default), 0=single display
###############################################################################

echo "[azahar] Starting Azahar 3DS emulator..."

# Wait for X server
/gamer/bin/wait-x.sh

# Wait a bit for Sunshine to be ready
sleep 2

# libfaketime — only activate if FAKETIME is set
if [ -n "$FAKETIME" ]; then
    echo "[azahar] Enabling libfaketime: ${FAKETIME}"
    export LD_PRELOAD=/usr/lib/x86_64-linux-gnu/faketime/libfaketime.so.1
    export FAKETIME_NO_CACHE=1
fi

# Copy default config on first run
if [ ! -f /home/gamer/config/azahar-emu/qt-config.ini ]; then
    echo "[azahar] First run — copying default config"
    cp -r /gamer/conf/azahar/* /home/gamer/config/azahar-emu/ 2>/dev/null || true
fi

# Override layout_option at runtime if set
if [ -n "${LAYOUT_OPTION:-}" ]; then
    CONFIG_FILE="/home/gamer/config/azahar-emu/qt-config.ini"
    if [ -f "$CONFIG_FILE" ]; then
        echo "[azahar] Setting layout_option=${LAYOUT_OPTION}"
        sed -i "s/^layout_option=.*/layout_option=${LAYOUT_OPTION}/" "$CONFIG_FILE"
    fi
fi

# Ensure dirs exist (bind mount may not have them)
mkdir -p /home/gamer/.config /home/gamer/.local/share /home/gamer/config/azahar-emu

# Symlink config to where Azahar expects it
ln -sfn /home/gamer/config/azahar-emu /home/gamer/.config/azahar-emu
ln -sfn /home/gamer/config/azahar-emu /home/gamer/.local/share/azahar-emu

# Symlink 3DS firmware/sysdata if available
if [ -d /home/gamer/firmware/3ds/sysdata ]; then
    echo "[azahar] Linking 3DS system data"
    ln -sfn /home/gamer/firmware/3ds/sysdata /home/gamer/config/azahar-emu/sysdata
fi

# Find ROM
ROM_PATH=""
if [ -n "${ROM_FILENAME:-}" ]; then
    ROM_PATH="/home/gamer/roms/${ROM_FILENAME}"
fi

if [ -z "$ROM_PATH" ] || [ ! -f "$ROM_PATH" ]; then
    # Auto-detect first 3DS ROM
    ROM_PATH=$(find /home/gamer/roms/ -maxdepth 1 \( -type f -o -type l \) 2>/dev/null \
        | grep -iE '\.(3ds|cia|cxi|app)$' | head -1 || true)
fi

if [ -z "$ROM_PATH" ] || [ ! -f "$ROM_PATH" ]; then
    echo "[azahar] ERROR: No ROM found in /home/gamer/roms/"
    ls -la /home/gamer/roms/ 2>/dev/null || echo "  (empty)"
    sleep infinity
    exit 1
fi

echo "[azahar] ROM: ${ROM_PATH}"

# Read active layout
ACTIVE_LAYOUT=$(grep -oP 'layout_option=\K\d+' /home/gamer/config/azahar-emu/qt-config.ini 2>/dev/null || echo "0")
echo "[azahar] Layout: ${ACTIVE_LAYOUT} (4=SeparateWindows)"

# Build launch args
AZAHAR_ARGS=()
if [ "$ACTIVE_LAYOUT" != "4" ] && [ "${AZAHAR_FULLSCREEN:-0}" = "1" ]; then
    AZAHAR_ARGS+=("-f")
fi
AZAHAR_ARGS+=("${ROM_PATH}")

# In dual-screen mode, launch Azahar in background and position windows
if [ "$ACTIVE_LAYOUT" = "4" ] && [ "${DUAL_SCREEN:-1}" = "1" ]; then
    echo "[azahar] Dual-screen mode: launching and positioning windows..."

    # Launch Azahar in background
    /Applications/azahar/AppRun "${AZAHAR_ARGS[@]}" &
    AZAHAR_PID=$!

    # Wait for the Secondary Window to appear (identifies both windows exist)
    echo "[azahar] Waiting for Azahar windows..."
    ATTEMPTS=0
    while [ $ATTEMPTS -lt 60 ]; do
        SECONDARY=$(xdotool search --name "Secondary Window" 2>/dev/null | head -1)
        if [ -n "$SECONDARY" ]; then
            echo "[azahar] Found Secondary Window: $SECONDARY"
            break
        fi
        sleep 0.5
        ATTEMPTS=$((ATTEMPTS + 1))
    done

    # Find both main windows by title
    sleep 1
    # Primary window has the game name but NOT "Secondary Window"
    PRIMARY=$(xdotool search --name "Azahar" 2>/dev/null | while read wid; do
        name=$(xdotool getwindowname $wid 2>/dev/null)
        if echo "$name" | grep -q "Secondary Window"; then continue; fi
        if echo "$name" | grep -qE "Azahar [0-9]"; then echo $wid; break; fi
    done)
    SECONDARY=$(xdotool search --name "Secondary Window" 2>/dev/null | head -1)

    echo "[azahar] Primary (top): $PRIMARY"
    echo "[azahar] Secondary (bottom): $SECONDARY"

    if [ -n "$PRIMARY" ] && [ -n "$SECONDARY" ]; then
        # Get current display layout for positioning
        TOP_HEIGHT=$(xrandr --current | grep "^DP-0" | grep -oP '\d+x\d+\+\d+\+\d+' | head -1 | cut -d'x' -f2 | cut -d'+' -f1)
        TOP_HEIGHT=${TOP_HEIGHT:-1080}
        TOP_WIDTH=$(xrandr --current | grep "^DP-0" | grep -oP '\d+x\d+\+\d+\+\d+' | head -1 | cut -d'x' -f1)
        TOP_WIDTH=${TOP_WIDTH:-1920}
        BOT_WIDTH=$(xrandr --current | grep "^DP-2" | grep -oP '\d+x\d+\+\d+\+\d+' | head -1 | cut -d'x' -f1)
        BOT_WIDTH=${BOT_WIDTH:-1920}
        BOT_HEIGHT=$(xrandr --current | grep "^DP-2" | grep -oP '\d+x\d+\+\d+\+\d+' | head -1 | cut -d'x' -f2 | cut -d'+' -f1)
        BOT_HEIGHT=${BOT_HEIGHT:-1080}

        # Position and resize primary window on top display
        echo "[azahar] Positioning primary at 0,0 (${TOP_WIDTH}x${TOP_HEIGHT})"
        xdotool windowmove $PRIMARY 0 0
        xdotool windowsize $PRIMARY $TOP_WIDTH $TOP_HEIGHT

        # Position and resize secondary window on bottom display
        echo "[azahar] Positioning secondary at 0,${TOP_HEIGHT} (${BOT_WIDTH}x${BOT_HEIGHT})"
        xdotool windowmove $SECONDARY 0 $TOP_HEIGHT
        xdotool windowsize $SECONDARY $BOT_WIDTH $BOT_HEIGHT

        echo "[azahar] Windows positioned."
    else
        echo "[azahar] WARNING: Could not find both windows"
    fi

    # Wait for Azahar to exit
    wait $AZAHAR_PID
else
    # Single-screen mode: just run Azahar directly
    echo "[azahar] Single-screen mode"
    exec /Applications/azahar/AppRun "${AZAHAR_ARGS[@]}"
fi
