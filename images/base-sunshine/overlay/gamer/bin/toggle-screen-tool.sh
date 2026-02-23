#!/usr/bin/env bash
set -euo pipefail

MODE_FILE="${SCREEN_TOOL_MODE_FILE:-/tmp/screen-tool.mode}"
SCREEN_TOOL_NAME="ScreenTool"
SECONDARY_PATTERN="${SECONDARY_PATTERN:-Secondary Window|Bottom Screen|Subscreen|Screen 2}"
REQUESTED_MODE="${1:-}"

/gamer/bin/wait-x.sh >/dev/null 2>&1 || true

MODE="auto"
if [ -f "$MODE_FILE" ]; then
  MODE="$(cat "$MODE_FILE" 2>/dev/null | tr -d '[:space:]')"
fi

if [ "$REQUESTED_MODE" = "auto" ] || [ "$REQUESTED_MODE" = "force_show" ]; then
  NEXT_MODE="$REQUESTED_MODE"
elif [ "$MODE" = "force_show" ]; then
  NEXT_MODE="auto"
else
  NEXT_MODE="force_show"
fi

if [ "$NEXT_MODE" = "auto" ]; then
  echo "auto" > "$MODE_FILE"
  echo "[toggle-screen-tool] mode=auto"
else
  echo "force_show" > "$MODE_FILE"
  echo "[toggle-screen-tool] mode=force_show"
fi
