#!/usr/bin/env bash
set -euo pipefail

MODE_FILE="${SCREEN_TOOL_MODE_FILE:-/home/gamer/.cache/screen-tool.mode}"
REQUESTED_MODE="${1:-}"

/gamer/bin/wait-x.sh >/dev/null 2>&1 || true

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
  if [ "$NEXT_MODE" = "force_hide" ]; then
    xdotool windowunmap "$WID" 2>/dev/null || true
  fi
fi
