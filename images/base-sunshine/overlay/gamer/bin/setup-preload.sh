#!/usr/bin/env bash
###############################################################################
# setup-preload.sh — Configure LD_PRELOAD for emulator processes.
#
# Source this script before launching an emulator to set up:
# - libfaketime: In-game clock spoofing for time-sensitive games (Pokemon, etc.)
# - shader-inject: Intercepts glXSwapBuffers to apply RetroArch shaders
#   directly in the emulator's GL pipeline (NVFBC-compatible).
#
# Usage in emulator start scripts:
#   source /gamer/bin/setup-preload.sh
#   exec /path/to/emulator "$@"
#
# Env vars consumed:
#   FAKETIME           - Fake time string (enables libfaketime)
#   SHADER_PRESET      - Path to .slangp preset (enables shader-inject)
#   SUNSHINE_CAPTURE   - Must be "nvfbc" for shader-inject to activate
###############################################################################

# libfaketime — only activate if FAKETIME is set
if [ -n "${FAKETIME:-}" ]; then
    echo "[setup-preload] Enabling libfaketime: ${FAKETIME}"
    if [ -n "${LD_PRELOAD:-}" ]; then
        export LD_PRELOAD="${LD_PRELOAD}:/usr/lib/x86_64-linux-gnu/faketime/libfaketime.so.1"
    else
        export LD_PRELOAD=/usr/lib/x86_64-linux-gnu/faketime/libfaketime.so.1
    fi
    export FAKETIME_NO_CACHE=1
fi

# shader-inject — LD_PRELOAD for NVFBC-compatible shader injection
# Intercepts glXSwapBuffers to apply RetroArch shaders directly in the emulator's
# GL pipeline, avoiding XComposite which breaks NVFBC capture.
if [ -n "${SHADER_PRESET:-}" ] && [ "${SUNSHINE_CAPTURE:-}" = "nvfbc" ] && [ -f /gamer/lib/libshader_inject.so ]; then
    echo "[setup-preload] Enabling shader-inject (LD_PRELOAD, NVFBC-compatible)"
    if [ -n "${LD_PRELOAD:-}" ]; then
        export LD_PRELOAD="${LD_PRELOAD}:/gamer/lib/libshader_inject.so"
    else
        export LD_PRELOAD=/gamer/lib/libshader_inject.so
    fi
fi
