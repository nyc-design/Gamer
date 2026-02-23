#!/usr/bin/env bash
set -euo pipefail

MODE_FILE="${SCREEN_TOOL_MODE_FILE:-/tmp/screen-tool.mode}"
SCREEN_TOOL_NAME="ScreenTool"
SECONDARY_PATTERN="${SECONDARY_PATTERN:-Secondary Window|Bottom Screen|Subscreen|Screen 2}"

/gamer/bin/wait-x.sh >/dev/null 2>&1 || true

MODE="auto"
if [ -f "$MODE_FILE" ]; then
  MODE="$(cat "$MODE_FILE" 2>/dev/null | tr -d '[:space:]')"
fi

if [ "$MODE" = "force_show" ]; then
  echo "auto" > "$MODE_FILE"
  echo "[toggle-screen-tool] mode=auto"
else
  echo "force_show" > "$MODE_FILE"
  echo "[toggle-screen-tool] mode=force_show"
fi
