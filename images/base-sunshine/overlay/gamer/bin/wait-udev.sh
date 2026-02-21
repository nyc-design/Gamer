#!/usr/bin/env bash
MAX_ATTEMPTS=20
attempt=1
while [ $attempt -le $MAX_ATTEMPTS ]; do
    if udevadm control --ping >/dev/null 2>&1; then
        udevadm settle
        exit 0
    fi
    sleep 1
    attempt=$((attempt + 1))
done
echo "[wait-udev] WARNING: udev not available after ${MAX_ATTEMPTS}s, continuing anyway"
