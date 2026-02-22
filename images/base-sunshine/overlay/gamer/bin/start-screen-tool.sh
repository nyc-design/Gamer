#!/usr/bin/env bash
###############################################################################
# start-screen-tool.sh — Launch the secondary screen zoom/crop tool.
#
# Shows the primary emulator window on DP-2, allowing the user to click-drag
# to zoom into a region. Automatically yields when the emulator creates its
# own secondary window (e.g., 3DS bottom screen).
#
# Env vars:
#   SCREEN_TOOL_ENABLED   - Set to "0" to disable (default: "1")
#   SCREEN_TOOL_WINDOW    - Window title pattern to capture (auto-detects if empty)
#   DUAL_SCREEN           - Dual-screen mode flag (tool still runs, but yields)
###############################################################################

if [ "${SCREEN_TOOL_ENABLED:-1}" = "0" ]; then
    echo "[screen-tool] Disabled via SCREEN_TOOL_ENABLED=0"
    sleep infinity
fi

# Wait for X server
/gamer/bin/wait-x.sh

# Wait for emulator and Sunshine to be ready
sleep 5

# Get DP-2 position from xrandr (bottom display)
BOT_INFO=$(xrandr --current 2>/dev/null | grep "^DP-2" | grep -oP '\d+x\d+\+\d+\+\d+' | head -1)
BOT_WIDTH=$(echo "$BOT_INFO" | cut -dx -f1)
BOT_HEIGHT=$(echo "$BOT_INFO" | cut -dx -f2 | cut -d+ -f1)
BOT_X=$(echo "$BOT_INFO" | cut -d+ -f2)
BOT_Y=$(echo "$BOT_INFO" | cut -d+ -f3)

# Defaults if xrandr didn't return anything (single-screen mode)
BOT_WIDTH=${BOT_WIDTH:-1920}
BOT_HEIGHT=${BOT_HEIGHT:-1080}
BOT_X=${BOT_X:-0}
BOT_Y=${BOT_Y:-1080}

# Build args
ARGS=(
    --output-x "$BOT_X"
    --output-y "$BOT_Y"
    --output-width "$BOT_WIDTH"
    --output-height "$BOT_HEIGHT"
)

if [ -n "${SCREEN_TOOL_WINDOW:-}" ]; then
    ARGS+=(--window "$SCREEN_TOOL_WINDOW")
fi

echo "[screen-tool] Starting: screen-tool ${ARGS[*]}"
exec /gamer/bin/screen-tool "${ARGS[@]}"
