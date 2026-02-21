#!/usr/bin/env bash

# Shader overlay — applies RetroArch .slangp shaders to any X11 window
# Skips if SHADER_PRESET is not set.
# In dual-screen mode, optionally applies a different shader to the secondary window.
#
# Env vars:
#   SHADER_PRESET         - Path to .slangp preset for primary window (required to start)
#   SHADER_PRESET_BOTTOM  - Path to .slangp preset for secondary window (optional, dual-screen)
#   SHADER_WINDOW         - Primary window target (name/title substring or window ID)
#   SHADER_WINDOW_BOTTOM  - Secondary window target (name/title substring or window ID)
#   DUAL_SCREEN           - 1 to enable dual-screen mode

if [ -z "${SHADER_PRESET:-}" ]; then
    echo "[shader-overlay] No SHADER_PRESET set, not starting."
    sleep infinity
fi

# Wait for X server
/gamer/bin/wait-x.sh

# Wait for the primary window to appear
echo "[shader-overlay] Waiting for window '${SHADER_WINDOW:-}' to appear..."
ATTEMPTS=0
while [ $ATTEMPTS -lt 120 ]; do
    if [ -n "${SHADER_WINDOW:-}" ]; then
        # Search by name
        WID=$(xdotool search --name "${SHADER_WINDOW}" 2>/dev/null | head -1)
    fi
    if [ -n "${WID:-}" ]; then
        echo "[shader-overlay] Found primary window: $WID"
        break
    fi
    sleep 1
    ATTEMPTS=$((ATTEMPTS + 1))
done

if [ -z "${WID:-}" ]; then
    echo "[shader-overlay] ERROR: Timed out waiting for window '${SHADER_WINDOW:-}'"
    sleep infinity
fi

# Build args
ARGS="--window ${SHADER_WINDOW}:${SHADER_PRESET}"

# Dual-screen: add bottom screen shader
if [ "${DUAL_SCREEN:-0}" = "1" ] && [ -n "${SHADER_PRESET_BOTTOM:-}" ] && [ -n "${SHADER_WINDOW_BOTTOM:-}" ]; then
    echo "[shader-overlay] Dual-screen mode: waiting for secondary window '${SHADER_WINDOW_BOTTOM}'..."
    ATTEMPTS=0
    while [ $ATTEMPTS -lt 60 ]; do
        WID2=$(xdotool search --name "${SHADER_WINDOW_BOTTOM}" 2>/dev/null | head -1)
        if [ -n "${WID2:-}" ]; then
            echo "[shader-overlay] Found secondary window: $WID2"
            break
        fi
        sleep 1
        ATTEMPTS=$((ATTEMPTS + 1))
    done
    if [ -n "${WID2:-}" ]; then
        ARGS="$ARGS --window ${SHADER_WINDOW_BOTTOM}:${SHADER_PRESET_BOTTOM}"
    else
        echo "[shader-overlay] WARNING: Secondary window not found, running single shader only"
    fi
fi

echo "[shader-overlay] Starting: shader-overlay $ARGS"
exec /gamer/bin/shader-overlay $ARGS
