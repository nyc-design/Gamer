#!/usr/bin/env bash
set -e

echo "[setup-pulseaudio] Disabling autospawn..."
sed -i 's/^.*autospawn.*$/autospawn = no/' /etc/pulse/client.conf 2>/dev/null || true
