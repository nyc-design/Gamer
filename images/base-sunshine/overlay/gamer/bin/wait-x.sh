#!/usr/bin/env bash
MAX_ATTEMPTS=30
attempt=1
while [ $attempt -le $MAX_ATTEMPTS ]; do
    if xdpyinfo -display ${DISPLAY:-:0} >/dev/null 2>&1; then
        exit 0
    fi
    sleep 1
    attempt=$((attempt + 1))
done
echo "[wait-x] WARNING: X server not available after ${MAX_ATTEMPTS}s, continuing anyway"
