#!/usr/bin/env bash

# Input remap proxy is intentionally disabled by default.
# Sunshine prerelease now handles second-screen absolute/touch offsets natively.
#
# Set INPUT_REMAP_ENABLED=1 only for emergency diagnostics.
if [ "${INPUT_REMAP_ENABLED:-0}" != "1" ]; then
    echo "[input-remap] Disabled (INPUT_REMAP_ENABLED=${INPUT_REMAP_ENABLED:-0}); using native Sunshine input path."
    sleep infinity
fi

# Skip if dual screen is disabled
if [ "${DUAL_SCREEN:-1}" != "1" ]; then
    echo "[input-remap] DUAL_SCREEN=${DUAL_SCREEN}, not starting input proxy."
    sleep infinity
fi

# Wait for X server
/gamer/bin/wait-x.sh

exec python3 /gamer/bin/input-remap-proxy.py
