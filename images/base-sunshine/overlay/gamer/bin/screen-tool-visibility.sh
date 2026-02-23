#!/usr/bin/env bash
###############################################################################
# screen-tool-visibility.sh — Controls screen-tool visibility.
#
# Polls every 1 second. When a "Secondary Window" is active on DP-2, unmaps
# screen-tool so it cannot steal touch/focus. When secondary is inactive for a
# short debounce period, maps screen-tool back.
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
ACTIVE_STREAK=0
INACTIVE_STREAK=0
LAST_PINNED_GEOM=""
POLL_INTERVAL="${SCREEN_TOOL_VIS_POLL_SEC:-0.25}"
ACTIVE_DEBOUNCE_POLLS="${SCREEN_TOOL_VIS_ACTIVE_POLLS:-2}"
INACTIVE_DEBOUNCE_POLLS="${SCREEN_TOOL_VIS_INACTIVE_POLLS:-2}"
CURSOR_REFRESH_POLLS="${SCREEN_TOOL_CURSOR_REFRESH_POLLS:-4}"
CURSOR_TICK=0
MODE_FILE="${SCREEN_TOOL_MODE_FILE:-/home/gamer/.cache/screen-tool.mode}"

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

pin_screen_tool_to_bottom() {
    local wid="$1"
    local bot_info bot_w bot_h bot_x bot_y
    local cur_x cur_y cur_w cur_h

    bot_info=$(xrandr --current 2>/dev/null | awk '/^DP-2 / {match($0, /[0-9]+x[0-9]+\+[0-9]+\+[0-9]+/); if (RSTART) print substr($0, RSTART, RLENGTH)}' | head -1)
    bot_w=$(echo "$bot_info" | cut -dx -f1)
    bot_h=$(echo "$bot_info" | cut -dx -f2 | cut -d+ -f1)
    bot_x=$(echo "$bot_info" | cut -d+ -f2)
    bot_y=$(echo "$bot_info" | cut -d+ -f3)

    [ -n "$bot_w" ] && [ -n "$bot_h" ] && [ -n "$bot_x" ] && [ -n "$bot_y" ] || return 0

    eval "$(xdotool getwindowgeometry --shell "$wid" 2>/dev/null | sed 's/^X=/cur_x=/;s/^Y=/cur_y=/;s/^WIDTH=/cur_w=/;s/^HEIGHT=/cur_h=/')"

    local target_geom="${bot_x},${bot_y},${bot_w},${bot_h}"
    if [ "$LAST_PINNED_GEOM" != "$target_geom" ] || \
       [ "${cur_x:-}" != "$bot_x" ] || [ "${cur_y:-}" != "$bot_y" ] || \
       [ "${cur_w:-}" != "$bot_w" ] || [ "${cur_h:-}" != "$bot_h" ]; then
        xdotool windowmove "$wid" "$bot_x" "$bot_y" 2>/dev/null
        xdotool windowsize "$wid" "$bot_w" "$bot_h" 2>/dev/null
        LAST_PINNED_GEOM="$target_geom"
        echo "[screen-tool-visibility] Pinned screen-tool to DP-2 ${bot_w}x${bot_h}+${bot_x}+${bot_y}"
        if [ -x /gamer/bin/set-dot-cursor.py ]; then
            SECONDARY_PATTERN="$PATTERN" python3 /gamer/bin/set-dot-cursor.py --target both >/dev/null 2>&1 || true
        fi
    fi
}

while true; do
    sleep "$POLL_INTERVAL"

    MODE="auto"
    if [ -f "$MODE_FILE" ]; then
        MODE="$(cat "$MODE_FILE" 2>/dev/null | tr -d '[:space:]')"
    fi

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

    if [ "$MODE" = "force_show" ]; then
        if [ "$HIDDEN" = "true" ]; then
            xdotool windowmap "$SCREEN_TOOL_WID" 2>/dev/null
            HIDDEN=false
        fi
        xdotool windowraise "$SCREEN_TOOL_WID" 2>/dev/null
        pin_screen_tool_to_bottom "$SCREEN_TOOL_WID"
        continue
    fi

    if [ "$SECONDARY_VISIBLE" = "true" ]; then
        ACTIVE_STREAK=$((ACTIVE_STREAK + 1))
        INACTIVE_STREAK=0
    else
        INACTIVE_STREAK=$((INACTIVE_STREAK + 1))
        ACTIVE_STREAK=0
    fi

    # Debounce: require 2 consecutive active polls before hiding.
    if [ "$ACTIVE_STREAK" -ge "$ACTIVE_DEBOUNCE_POLLS" ] && [ "$HIDDEN" = "false" ]; then
        echo "[screen-tool-visibility] Secondary active, hiding screen-tool"
        xdotool windowunmap "$SCREEN_TOOL_WID" 2>/dev/null
        HIDDEN=true
    fi

    # Debounce: require 2 consecutive inactive polls before showing.
    if [ "$INACTIVE_STREAK" -ge "$INACTIVE_DEBOUNCE_POLLS" ] && [ "$HIDDEN" = "true" ]; then
        echo "[screen-tool-visibility] Secondary inactive, showing screen-tool"
        xdotool windowmap "$SCREEN_TOOL_WID" 2>/dev/null
        xdotool windowraise "$SCREEN_TOOL_WID" 2>/dev/null
        /gamer/bin/reposition-windows.sh 2>/dev/null || true
        if [ -x /gamer/bin/set-dot-cursor.py ]; then
            SECONDARY_PATTERN="$PATTERN" python3 /gamer/bin/set-dot-cursor.py --target both >/dev/null 2>&1 || true
        fi
        HIDDEN=false
    fi

    # Keep screen-tool anchored to DP-2 when visible so top-screen mode changes
    # cannot strand it on the primary display.
    if [ "$HIDDEN" = "false" ]; then
        pin_screen_tool_to_bottom "$SCREEN_TOOL_WID"
    fi

    # Periodically refresh dot cursor on both windows to survive cursor resets
    # from WM/focus changes.
    CURSOR_TICK=$((CURSOR_TICK + 1))
    if [ "$CURSOR_TICK" -ge "$CURSOR_REFRESH_POLLS" ]; then
        CURSOR_TICK=0
        if [ -x /gamer/bin/set-dot-cursor.py ]; then
            SECONDARY_PATTERN="$PATTERN" python3 /gamer/bin/set-dot-cursor.py --target both >/dev/null 2>&1 || true
        fi
    fi
done
