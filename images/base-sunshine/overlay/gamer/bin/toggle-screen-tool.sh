#!/usr/bin/env bash
set -euo pipefail

MODE_FILE="${SCREEN_TOOL_MODE_FILE:-/home/gamer/.cache/screen-tool.mode}"
SCREEN_TOOL_NAME="ScreenTool"
SECONDARY_PATTERN="${SECONDARY_PATTERN:-Secondary Window|Bottom Screen|Subscreen|Screen 2}"
REQUESTED_MODE="${1:-}"

/gamer/bin/wait-x.sh >/dev/null 2>&1 || true

is_bottom_stream_connected() {
  if command -v ss >/dev/null 2>&1; then
    if ss -Htan 2>/dev/null | awk '{print $1, $4, $5}' | grep -E '^ESTAB ' | grep -E '(:48089)( |$)' >/dev/null 2>&1; then
      return 0
    fi
    if ss -Huan 2>/dev/null | awk '$4 ~ /:48089$/ && $5 !~ /^\*:/ && $5 !~ /^0\.0\.0\.0:/ && $5 !~ /^\[::\]:/ { found=1 } END { exit(found ? 0 : 1) }'; then
      return 0
    fi
  fi
  return 1
}

move_tool_to_target() {
  local wid="$1"
  local target_display info w h x y
  if is_bottom_stream_connected; then
    target_display="DP-2"
  else
    target_display="DP-0"
  fi
  info=$(xrandr --current 2>/dev/null | awk -v d="$target_display" '$1==d {match($0, /[0-9]+x[0-9]+\+[0-9]+\+[0-9]+/); if (RSTART) print substr($0, RSTART, RLENGTH)}' | head -1)
  w=$(echo "$info" | cut -dx -f1)
  h=$(echo "$info" | cut -dx -f2 | cut -d+ -f1)
  x=$(echo "$info" | cut -d+ -f2)
  y=$(echo "$info" | cut -d+ -f3)
  [ -n "$w" ] && [ -n "$h" ] && [ -n "$x" ] && [ -n "$y" ] || return 0
  xdotool windowmove "$wid" "$x" "$y" 2>/dev/null || true
  xdotool windowsize "$wid" "$w" "$h" 2>/dev/null || true
}

MODE="auto"
if [ -f "$MODE_FILE" ]; then
  MODE="$(cat "$MODE_FILE" 2>/dev/null | tr -d '[:space:]')"
fi

if [ "$REQUESTED_MODE" = "auto" ] || [ "$REQUESTED_MODE" = "force_show" ] || [ "$REQUESTED_MODE" = "force_hide" ]; then
  NEXT_MODE="$REQUESTED_MODE"
elif [ "$MODE" = "force_show" ]; then
  NEXT_MODE="force_hide"
else
  NEXT_MODE="force_show"
fi

echo "$NEXT_MODE" > "$MODE_FILE"
echo "[toggle-screen-tool] mode=$NEXT_MODE"

# Apply immediately (visibility daemon still enforces ongoing policy).
WID="$(xdotool search --name '^ScreenTool$' 2>/dev/null | head -n1 || true)"
if [ -n "$WID" ]; then
  if [ "$NEXT_MODE" = "force_show" ]; then
    move_tool_to_target "$WID"
    xdotool windowmap "$WID" 2>/dev/null || true
    xdotool windowraise "$WID" 2>/dev/null || true
  elif [ "$NEXT_MODE" = "force_hide" ]; then
    xdotool windowunmap "$WID" 2>/dev/null || true
  fi
fi
