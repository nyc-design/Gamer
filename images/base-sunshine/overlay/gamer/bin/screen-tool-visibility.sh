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
TOGGLE_NAME="ScreenToolToggle"
HIDDEN=false
ACTIVE_STREAK=0
INACTIVE_STREAK=0
BOTTOM_ON_STREAK=0
BOTTOM_OFF_STREAK=0
BOTTOM_CONNECTED=false
LAST_PINNED_GEOM=""
LAST_TARGET_DISPLAY=""
POLL_INTERVAL="${SCREEN_TOOL_VIS_POLL_SEC:-0.25}"
ACTIVE_DEBOUNCE_POLLS="${SCREEN_TOOL_VIS_ACTIVE_POLLS:-2}"
INACTIVE_DEBOUNCE_POLLS="${SCREEN_TOOL_VIS_INACTIVE_POLLS:-2}"
BOTTOM_DEBOUNCE_POLLS="${SCREEN_TOOL_VIS_BOTTOM_POLLS:-3}"
CURSOR_REFRESH_POLLS="${SCREEN_TOOL_CURSOR_REFRESH_POLLS:-4}"
CURSOR_TICK=0
MODE_FILE="${SCREEN_TOOL_MODE_FILE:-/home/gamer/.cache/screen-tool.mode}"
CAPTURE_OUTPUT_FILE="${SCREEN_TOOL_CAPTURE_OUTPUT_FILE:-/home/gamer/.cache/screen-tool.capture-output}"

find_window_exact() {
    local exact_name="$1"
    local wid
    for wid in $(xdotool search --name "$exact_name" 2>/dev/null); do
        local got
        got=$(xdotool getwindowname "$wid" 2>/dev/null || true)
        if [ "$got" = "$exact_name" ]; then
            echo "$wid"
            return 0
        fi
    done
    return 1
}

is_bottom_stream_connected() {
    # Detect active Moonlight/VoidLink client to bottom Sunshine instance.
    # 48089 is unique to the bottom Sunshine instance.
    if command -v ss >/dev/null 2>&1; then
        # TCP control channel (common path)
        if ss -Htan 2>/dev/null | awk '{print $1, $4, $5}' | grep -E '^ESTAB ' | grep -E '(:48089)( |$)' >/dev/null 2>&1; then
            return 0
        fi
        # UDP/QUIC path on some clients/builds
        if ss -Huan 2>/dev/null | awk '{print $4, $5}' | grep -E '(:48089)( |$)' >/dev/null 2>&1; then
            return 0
        fi
    fi

    # Fallback for minimal containers without `ss`
    if awk '
      $4=="01" {
        split($2,a,":");
        p=toupper(a[2]);
        if (p=="BBD9") { found=1 }
      }
      END { exit(found ? 0 : 1) }
    ' /proc/net/tcp /proc/net/tcp6 2>/dev/null; then
        return 0
    fi

    # Last fallback: active bottom-stream keep-alive updates sunshine-bottom.log
    # frequently while a client is connected.
    if [ -f /gamer/log/sunshine-bottom.log ]; then
        local now ts
        now=$(date +%s)
        ts=$(stat -c %Y /gamer/log/sunshine-bottom.log 2>/dev/null || echo 0)
        if [ $((now - ts)) -le 5 ]; then
            return 0
        fi
    fi
    return 1
}

# Wait for X server
/gamer/bin/wait-x.sh

# Wait for screen-tool to start
sleep 8

echo "[screen-tool-visibility] Monitoring for '$PATTERN', managing '$SCREEN_TOOL_NAME'"

# Default behavior: keep ScreenTool hidden until explicitly toggled on.
# This avoids stealing the top stream on connect.
if [ ! -f "$MODE_FILE" ]; then
    echo "force_hide" > "$MODE_FILE"
fi

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

is_window_on_dp2() {
    local wid="$1"
    local bot_info bot_w bot_h bot_x bot_y
    local abs_x abs_y win_w win_h

    bot_info=$(xrandr --current 2>/dev/null | awk '/^DP-2 / {match($0, /[0-9]+x[0-9]+\+[0-9]+\+[0-9]+/); if (RSTART) print substr($0, RSTART, RLENGTH)}' | head -1)
    bot_w=$(echo "$bot_info" | cut -dx -f1)
    bot_h=$(echo "$bot_info" | cut -dx -f2 | cut -d+ -f1)
    bot_x=$(echo "$bot_info" | cut -d+ -f2)
    bot_y=$(echo "$bot_info" | cut -d+ -f3)
    [ -n "$bot_w" ] && [ -n "$bot_h" ] && [ -n "$bot_x" ] && [ -n "$bot_y" ] || return 1

    abs_x=$(xwininfo -id "$wid" 2>/dev/null | awk -F: '/Absolute upper-left X/ {gsub(/^[ \t]+/, "", $2); print $2}')
    abs_y=$(xwininfo -id "$wid" 2>/dev/null | awk -F: '/Absolute upper-left Y/ {gsub(/^[ \t]+/, "", $2); print $2}')
    win_w=$(xwininfo -id "$wid" 2>/dev/null | awk -F: '/Width/ {gsub(/^[ \t]+/, "", $2); print $2; exit}')
    win_h=$(xwininfo -id "$wid" 2>/dev/null | awk -F: '/Height/ {gsub(/^[ \t]+/, "", $2); print $2; exit}')
    [ -n "$abs_x" ] && [ -n "$abs_y" ] && [ -n "$win_w" ] && [ -n "$win_h" ] || return 1

    local min_w min_h dx dy
    min_w=$((bot_w * 60 / 100))
    min_h=$((bot_h * 60 / 100))
    dx=$((abs_x - bot_x)); [ "$dx" -lt 0 ] && dx=$(( -dx ))
    dy=$((abs_y - bot_y)); [ "$dy" -lt 0 ] && dy=$(( -dy ))
    [ "$win_w" -ge "$min_w" ] && [ "$win_h" -ge "$min_h" ] && [ "$dx" -le 140 ] && [ "$dy" -le 140 ]
}

pin_screen_tool_to_target() {
    local wid="$1"
    local target_display target_info target_w target_h target_x target_y
    local cur_x cur_y cur_w cur_h

    if [ "$BOTTOM_CONNECTED" = "true" ]; then
        target_display="DP-2"
        # When tool is on bottom, default crop source should be top.
        if [ "$LAST_TARGET_DISPLAY" != "DP-2" ] || [ "$(cat "$CAPTURE_OUTPUT_FILE" 2>/dev/null | tr -d '[:space:]')" != "DP-0" ]; then
            printf 'DP-0\n' > "$CAPTURE_OUTPUT_FILE" 2>/dev/null || true
        fi
    else
        target_display="DP-0"
        # When tool is on top, default crop source should be bottom (avoid recursion).
        if [ "$LAST_TARGET_DISPLAY" != "DP-0" ] || [ "$(cat "$CAPTURE_OUTPUT_FILE" 2>/dev/null | tr -d '[:space:]')" != "DP-2" ]; then
            printf 'DP-2\n' > "$CAPTURE_OUTPUT_FILE" 2>/dev/null || true
        fi
    fi

    target_info=$(xrandr --current 2>/dev/null | awk -v d="$target_display" '$1==d {match($0, /[0-9]+x[0-9]+\+[0-9]+\+[0-9]+/); if (RSTART) print substr($0, RSTART, RLENGTH)}' | head -1)
    target_w=$(echo "$target_info" | cut -dx -f1)
    target_h=$(echo "$target_info" | cut -dx -f2 | cut -d+ -f1)
    target_x=$(echo "$target_info" | cut -d+ -f2)
    target_y=$(echo "$target_info" | cut -d+ -f3)

    [ -n "$target_w" ] && [ -n "$target_h" ] && [ -n "$target_x" ] && [ -n "$target_y" ] || return 0

    eval "$(xdotool getwindowgeometry --shell "$wid" 2>/dev/null | sed 's/^X=/cur_x=/;s/^Y=/cur_y=/;s/^WIDTH=/cur_w=/;s/^HEIGHT=/cur_h=/')"

    local target_geom="${target_x},${target_y},${target_w},${target_h}"
    if [ "$LAST_PINNED_GEOM" != "$target_geom" ] || \
       [ "${cur_x:-}" != "$target_x" ] || [ "${cur_y:-}" != "$target_y" ] || \
       [ "${cur_w:-}" != "$target_w" ] || [ "${cur_h:-}" != "$target_h" ]; then
        xdotool windowmove "$wid" "$target_x" "$target_y" 2>/dev/null
        xdotool windowsize "$wid" "$target_w" "$target_h" 2>/dev/null
        LAST_PINNED_GEOM="$target_geom"
        LAST_TARGET_DISPLAY="$target_display"
        echo "[screen-tool-visibility] Pinned screen-tool to ${target_display} ${target_w}x${target_h}+${target_x}+${target_y}"
        if [ -x /gamer/bin/set-dot-cursor.py ]; then
            SECONDARY_PATTERN="$PATTERN" python3 /gamer/bin/set-dot-cursor.py --target both >/dev/null 2>&1 || true
        fi
    fi
}

pin_toggle_button_to_target() {
    local wid="$1"
    local target_display target_info target_w target_h target_x target_y
    local btn=52
    local margin=12

    if [ "$BOTTOM_CONNECTED" = "true" ]; then
        target_display="DP-2"
    else
        target_display="DP-0"
    fi
    target_info=$(xrandr --current 2>/dev/null | awk -v d="$target_display" '$1==d {match($0, /[0-9]+x[0-9]+\+[0-9]+\+[0-9]+/); if (RSTART) print substr($0, RSTART, RLENGTH)}' | head -1)
    target_w=$(echo "$target_info" | cut -dx -f1)
    target_h=$(echo "$target_info" | cut -dx -f2 | cut -d+ -f1)
    target_x=$(echo "$target_info" | cut -d+ -f2)
    target_y=$(echo "$target_info" | cut -d+ -f3)
    [ -n "$target_w" ] && [ -n "$target_h" ] && [ -n "$target_x" ] && [ -n "$target_y" ] || return 0

    local x y
    x=$((target_x + target_w - btn - margin))
    y=$((target_y + (target_h / 2) - (btn / 2)))
    xdotool windowsize "$wid" "$btn" "$btn" 2>/dev/null || true
    xdotool windowmove "$wid" "$x" "$y" 2>/dev/null || true
    xprop -id "$wid" -f _NET_WM_WINDOW_TYPE 32a -set _NET_WM_WINDOW_TYPE "_NET_WM_WINDOW_TYPE_DOCK" >/dev/null 2>&1 || true
    xprop -id "$wid" -f _NET_WM_STATE 32a -set _NET_WM_STATE "_NET_WM_STATE_ABOVE,_NET_WM_STATE_STICKY" >/dev/null 2>&1 || true
    xdotool windowraise "$wid" 2>/dev/null || true
}

while true; do
    sleep "$POLL_INTERVAL"

    MODE="auto"
    if [ -f "$MODE_FILE" ]; then
        MODE="$(cat "$MODE_FILE" 2>/dev/null | tr -d '[:space:]')"
    fi

    # Debounced bottom-stream detection to avoid flapping on transient sockets.
    if is_bottom_stream_connected; then
        BOTTOM_ON_STREAK=$((BOTTOM_ON_STREAK + 1))
        BOTTOM_OFF_STREAK=0
    else
        BOTTOM_OFF_STREAK=$((BOTTOM_OFF_STREAK + 1))
        BOTTOM_ON_STREAK=0
    fi
    if [ "$BOTTOM_ON_STREAK" -ge "$BOTTOM_DEBOUNCE_POLLS" ]; then
        BOTTOM_CONNECTED=true
    elif [ "$BOTTOM_OFF_STREAK" -ge "$BOTTOM_DEBOUNCE_POLLS" ]; then
        BOTTOM_CONNECTED=false
    fi

    # Find secondary window.
    # IMPORTANT: only hide screen-tool when BOTH are true:
    #   1) bottom stream is actually connected
    #   2) secondary window is active on bottom display
    # This prevents top-only sessions from immediately hiding the tool.
    SECONDARY_VISIBLE=false
    if [ "$BOTTOM_CONNECTED" = "true" ]; then
        for wid in $(xdotool search --name "$PATTERN" 2>/dev/null); do
            if is_secondary_active_on_bottom "$wid"; then
                SECONDARY_VISIBLE=true
                break
            fi
        done
    fi

    # Find screen-tool window
    SCREEN_TOOL_WID="$(find_window_exact "$SCREEN_TOOL_NAME" || true)"
    TOGGLE_WID="$(find_window_exact "$TOGGLE_NAME" || true)"

    if [ -z "$SCREEN_TOOL_WID" ]; then
        if [ -n "$TOGGLE_WID" ]; then
            pin_toggle_button_to_target "$TOGGLE_WID"
        fi
        continue
    fi

    if [ "$MODE" = "force_hide" ]; then
        if [ "$HIDDEN" = "false" ]; then
            xdotool windowunmap "$SCREEN_TOOL_WID" 2>/dev/null
            HIDDEN=true
        fi
        if [ -n "$TOGGLE_WID" ]; then
            pin_toggle_button_to_target "$TOGGLE_WID"
        fi
        continue
    fi

    if [ "$MODE" = "force_show" ]; then
        if [ "$HIDDEN" = "true" ]; then
            xdotool windowmap "$SCREEN_TOOL_WID" 2>/dev/null
            HIDDEN=false
        fi
        xdotool windowraise "$SCREEN_TOOL_WID" 2>/dev/null
        pin_screen_tool_to_target "$SCREEN_TOOL_WID"
        if [ -n "$TOGGLE_WID" ]; then
            pin_toggle_button_to_target "$TOGGLE_WID"
        fi
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
    if [ "$ACTIVE_STREAK" -ge "$ACTIVE_DEBOUNCE_POLLS" ] && [ "$HIDDEN" = "false" ] && is_window_on_dp2 "$SCREEN_TOOL_WID"; then
        echo "[screen-tool-visibility] Secondary active, hiding screen-tool"
        xdotool windowunmap "$SCREEN_TOOL_WID" 2>/dev/null
        HIDDEN=true
    fi

    # Debounce: require 2 consecutive inactive polls before showing.
    if [ "$INACTIVE_STREAK" -ge "$INACTIVE_DEBOUNCE_POLLS" ] && [ "$HIDDEN" = "true" ]; then
        echo "[screen-tool-visibility] Secondary inactive, showing screen-tool"
        xdotool windowmap "$SCREEN_TOOL_WID" 2>/dev/null
        xdotool windowraise "$SCREEN_TOOL_WID" 2>/dev/null
        if [ -x /gamer/bin/set-dot-cursor.py ]; then
            SECONDARY_PATTERN="$PATTERN" python3 /gamer/bin/set-dot-cursor.py --target both >/dev/null 2>&1 || true
        fi
        HIDDEN=false
    fi

    # Keep screen-tool anchored to DP-2 when visible so top-screen mode changes
    # cannot strand it on the primary display.
    if [ "$HIDDEN" = "false" ]; then
        pin_screen_tool_to_target "$SCREEN_TOOL_WID"
    fi
    if [ -n "$TOGGLE_WID" ]; then
        pin_toggle_button_to_target "$TOGGLE_WID"
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
