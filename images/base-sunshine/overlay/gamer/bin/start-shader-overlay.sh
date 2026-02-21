#!/usr/bin/env bash

# Shader overlay — applies RetroArch .slangp shaders to any X11 window
# Skips if SHADER_PRESET is not set.
# In dual-screen mode, passes both window specs — shader-overlay handles
# dynamically attaching/detaching pipelines as windows appear and disappear.
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

# Build args as array to preserve quoting
ARGS=(--window "${SHADER_WINDOW}:${SHADER_PRESET}")

# Dual-screen: always pass secondary spec — shader-overlay polls for it dynamically
if [ "${DUAL_SCREEN:-0}" = "1" ] && [ -n "${SHADER_PRESET_BOTTOM:-}" ] && [ -n "${SHADER_WINDOW_BOTTOM:-}" ]; then
    echo "[shader-overlay] Dual-screen mode: will attach secondary window '${SHADER_WINDOW_BOTTOM}' when it appears"
    ARGS+=(--window "${SHADER_WINDOW_BOTTOM}:${SHADER_PRESET_BOTTOM}")
fi

echo "[shader-overlay] Starting: shader-overlay ${ARGS[*]}"
exec /gamer/bin/shader-overlay "${ARGS[@]}"
