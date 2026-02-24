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
#
# IMPORTANT:
# Azahar/Qt can stall at boot if CLOCK_MONOTONIC is faked. Keep monotonic real.
# Use libfaketime follow-file mode so ScreenTool can adjust time live.
if [ -n "${FAKETIME:-}" ]; then
    export FAKETIME_TIMESTAMP_FILE="${FAKETIME_TIMESTAMP_FILE:-/home/gamer/.cache/faketime.timestamp}"
    mkdir -p "$(dirname "${FAKETIME_TIMESTAMP_FILE}")"

    # Seed/refresh follow file from env value.
    printf '%s\n' "${FAKETIME}" > "${FAKETIME_TIMESTAMP_FILE}"
    touch -d "${FAKETIME}" "${FAKETIME_TIMESTAMP_FILE}" 2>/dev/null || true

    # Follow-file mode (documented libfaketime path).
    export FAKETIME='%'
    export FAKETIME_FOLLOW_FILE="${FAKETIME_TIMESTAMP_FILE}"

    echo "[setup-preload] Enabling libfaketime (follow-file): ${FAKETIME_FOLLOW_FILE}"
    if [ -n "${LD_PRELOAD:-}" ]; then
        export LD_PRELOAD="${LD_PRELOAD}:/usr/lib/x86_64-linux-gnu/faketime/libfaketime.so.1"
    else
        export LD_PRELOAD=/usr/lib/x86_64-linux-gnu/faketime/libfaketime.so.1
    fi
    export FAKETIME_DONT_RESET=1
    export FAKETIME_NO_CACHE=1
    export FAKETIME_DONT_FAKE_MONOTONIC=1
fi

# shader-inject — LD_PRELOAD for NVFBC-compatible shader injection
# Intercepts glXSwapBuffers to apply RetroArch shaders directly in the emulator's
# GL pipeline, avoiding XComposite which breaks NVFBC capture.
if [ -n "${SHADER_PRESET:-}" ] && [ "${SUNSHINE_CAPTURE:-}" = "nvfbc" ] && [ -f /gamer/lib/libshader_inject.so ]; then
    export SHADER_PRESET_FILE="${SHADER_PRESET_FILE:-/tmp/shader_preset_primary.path}"
    export SHADER_PRESET_BOTTOM_FILE="${SHADER_PRESET_BOTTOM_FILE:-/tmp/shader_preset_secondary.path}"
    # Seed live-preset files so screen-tool can hot-swap them later.
    printf '%s\n' "${SHADER_PRESET}" > "${SHADER_PRESET_FILE}"
    if [ -n "${SHADER_PRESET_BOTTOM:-}" ]; then
        printf '%s\n' "${SHADER_PRESET_BOTTOM}" > "${SHADER_PRESET_BOTTOM_FILE}"
    fi

    echo "[setup-preload] Enabling shader-inject (LD_PRELOAD, NVFBC-compatible)"
    if [ -n "${LD_PRELOAD:-}" ]; then
        export LD_PRELOAD="${LD_PRELOAD}:/gamer/lib/libshader_inject.so"
    else
        export LD_PRELOAD=/gamer/lib/libshader_inject.so
    fi
fi
