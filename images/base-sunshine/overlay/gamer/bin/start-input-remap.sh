#!/usr/bin/env bash

# Sunshine prerelease handles bottom-screen coordinate mapping natively.
# Keep proxying OFF unless explicitly requested for debugging.
if [ "${INPUT_REMAP_ENABLED:-0}" != "1" ]; then
    echo "[input-remap] Disabled. Using native Sunshine input handling."
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
