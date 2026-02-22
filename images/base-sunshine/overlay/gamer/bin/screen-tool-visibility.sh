#!/usr/bin/env bash
###############################################################################
# screen-tool-visibility.sh — Controls screen-tool visibility.
#
# Polls every 1 second. When a "Secondary Window" exists (emulator dual-window
# mode), hides the screen-tool. When it disappears, shows and repositions it.
#
# Env vars:
#   SCREEN_TOOL_ENABLED   - Set to "0" to disable (default: "1")
#   SECONDARY_PATTERN     - Window name pattern to look for (default: "Secondary Window")
###############################################################################

if [ "${SCREEN_TOOL_ENABLED:-1}" = "0" ]; then
    echo "[screen-tool-visibility] Disabled via SCREEN_TOOL_ENABLED=0"
    sleep infinity
fi

PATTERN="${SECONDARY_PATTERN:-Secondary Window}"
if [ -z "$PATTERN" ]; then
    PATTERN="Secondary Window"
fi
SCREEN_TOOL_NAME="ScreenTool"
HIDDEN=false

# Wait for X server
/gamer/bin/wait-x.sh

# Wait for screen-tool to start
sleep 8

echo "[screen-tool-visibility] Monitoring for '$PATTERN', managing '$SCREEN_TOOL_NAME'"

while true; do
    sleep 1

    # Find secondary window — must be MAPPED (visible) to count.
    # xdotool search matches unmapped windows too, so we check map state.
    SECONDARY_VISIBLE=false
    for wid in $(xdotool search --name "$PATTERN" 2>/dev/null); do
        if xwininfo -id "$wid" 2>/dev/null | grep -q "Map State: IsViewable"; then
            SECONDARY_VISIBLE=true
            break
        fi
    done

    # Find screen-tool window
    SCREEN_TOOL_WID=$(xdotool search --name "$SCREEN_TOOL_NAME" 2>/dev/null | head -1)

    if [ -z "$SCREEN_TOOL_WID" ]; then
        continue
    fi

    if [ "$SECONDARY_VISIBLE" = "true" ] && [ "$HIDDEN" = "false" ]; then
        # Secondary window is visible — hide screen-tool
        echo "[screen-tool-visibility] Secondary window visible, hiding screen-tool"
        xdotool windowunmap "$SCREEN_TOOL_WID" 2>/dev/null
        HIDDEN=true
    elif [ "$SECONDARY_VISIBLE" = "false" ] && [ "$HIDDEN" = "true" ]; then
        # Secondary window hidden/gone — show and reposition screen-tool
        echo "[screen-tool-visibility] Secondary window hidden, showing screen-tool"
        xdotool windowmap "$SCREEN_TOOL_WID" 2>/dev/null
        # Let reposition-windows.sh handle the correct placement
        /gamer/bin/reposition-windows.sh 2>/dev/null || true
        HIDDEN=false
    fi
done
