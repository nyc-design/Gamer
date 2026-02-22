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

is_secondary_active_on_bottom() {
    local wid="$1"
    local bot_info bot_w bot_h bot_x bot_y
    local map_state abs_x abs_y win_w win_h

    # Must be viewable first.
    map_state=$(xwininfo -id "$wid" 2>/dev/null | awk -F: '/Map State/ {gsub(/^[ \t]+/, "", $2); print $2}')
    [ "$map_state" = "IsViewable" ] || return 1

    # Bottom display geometry.
    bot_info=$(xrandr --current 2>/dev/null | awk '/^DP-2 / {match($0, /[0-9]+x[0-9]+\+[0-9]+\+[0-9]+/); if (RSTART) print substr($0, RSTART, RLENGTH)}' | head -1)
    bot_w=$(echo "$bot_info" | cut -dx -f1)
    bot_h=$(echo "$bot_info" | cut -dx -f2 | cut -d+ -f1)
    bot_x=$(echo "$bot_info" | cut -d+ -f2)
    bot_y=$(echo "$bot_info" | cut -d+ -f3)

    # If we can't read DP-2, fallback to viewable=true behavior.
    if [ -z "$bot_w" ] || [ -z "$bot_h" ] || [ -z "$bot_x" ] || [ -z "$bot_y" ]; then
        return 0
    fi

    abs_x=$(xwininfo -id "$wid" 2>/dev/null | awk -F: '/Absolute upper-left X/ {gsub(/^[ \t]+/, "", $2); print $2}')
    abs_y=$(xwininfo -id "$wid" 2>/dev/null | awk -F: '/Absolute upper-left Y/ {gsub(/^[ \t]+/, "", $2); print $2}')
    win_w=$(xwininfo -id "$wid" 2>/dev/null | awk -F: '/Width/ {gsub(/^[ \t]+/, "", $2); print $2; exit}')
    win_h=$(xwininfo -id "$wid" 2>/dev/null | awk -F: '/Height/ {gsub(/^[ \t]+/, "", $2); print $2; exit}')

    [ -n "$abs_x" ] && [ -n "$abs_y" ] && [ -n "$win_w" ] && [ -n "$win_h" ] || return 1

    # Consider secondary "active" only if it's basically occupying bottom display.
    # This prevents accidental hiding when a dormant/small secondary window exists.
    local min_w min_h dx dy
    min_w=$((bot_w * 60 / 100))
    min_h=$((bot_h * 60 / 100))
    dx=$((abs_x - bot_x)); [ "$dx" -lt 0 ] && dx=$(( -dx ))
    dy=$((abs_y - bot_y)); [ "$dy" -lt 0 ] && dy=$(( -dy ))

    [ "$win_w" -ge "$min_w" ] && [ "$win_h" -ge "$min_h" ] && [ "$dx" -le 140 ] && [ "$dy" -le 140 ]
}

while true; do
    sleep 1

    # Find secondary window.
    # We only count it as active when it's viewable and occupying bottom display.
    SECONDARY_VISIBLE=false
    for wid in $(xdotool search --name "$PATTERN" 2>/dev/null); do
        if is_secondary_active_on_bottom "$wid"; then
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
        xdotool windowraise "$SCREEN_TOOL_WID" 2>/dev/null
        # Let reposition-windows.sh handle the correct placement
        /gamer/bin/reposition-windows.sh 2>/dev/null || true
        HIDDEN=false
    fi
done
