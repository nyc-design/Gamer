#!/usr/bin/env bash
###############################################################################
# start-screen-tool.sh — Launch the secondary screen zoom/crop tool.
#
# Uses NVFBC to capture the primary display (DP-0) and renders a magnified
# view on the secondary display (DP-2). Automatically yields when the emulator
# creates its own secondary window (e.g., 3DS bottom screen).
#
# Env vars:
#   SCREEN_TOOL_ENABLED   - Set to "0" to disable (default: "1")
#   SCREEN_TOOL_MAX_FPS   - Render/update cap to minimize overhead (default: "30")
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

# Build args — NVFBC captures DP-0, output renders on DP-2
ARGS=(
    --capture-output "DP-0"
    --x "$BOT_X"
    --y "$BOT_Y"
    --width "$BOT_WIDTH"
    --height "$BOT_HEIGHT"
)

# Backward/forward compatible: only pass --max-fps if this binary supports it.
if /gamer/bin/screen-tool --help 2>/dev/null | grep -q -- '--max-fps'; then
    ARGS+=(--max-fps "${SCREEN_TOOL_MAX_FPS:-30}")
fi

echo "[screen-tool] Starting: screen-tool ${ARGS[*]}"
exec /gamer/bin/screen-tool "${ARGS[@]}"
