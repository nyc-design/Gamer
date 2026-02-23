#!/usr/bin/env bash
set -euo pipefail

ACTION="${1:-}"
OUTPUT="${2:-}"
STATE_DIR="${SCREEN_TOOL_STATE_DIR:-/home/gamer/.cache}"
STATE_FILE="${STATE_DIR}/stream-active-${OUTPUT}"

mkdir -p "$STATE_DIR"

case "$ACTION" in
  start)
    # Apply requested client mode/position first.
    /gamer/bin/setup-screen-mode.sh "${OUTPUT}"
    # Mark this Sunshine output as actively streamed.
    date +%s > "$STATE_FILE"
    ;;
  stop)
    rm -f "$STATE_FILE"
    ;;
  *)
    echo "usage: sunshine-stream-hook.sh <start|stop> <DP-0|DP-2>" >&2
    exit 2
    ;;
esac

