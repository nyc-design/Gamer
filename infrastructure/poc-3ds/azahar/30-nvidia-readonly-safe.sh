#!/bin/bash
set -e

source /opt/gow/bash-lib/utils.sh

# Wolf app containers often run with read-only rootfs. The base-app nvidia init
# script copies files into /usr/share/* and fails hard under read-only mode.
# For this PoC image we skip those copies and rely on NVIDIA driver volume paths
# exposed via environment variables in the Dockerfile.
if [ -d /usr/nvidia ]; then
  gow_log "Nvidia driver volume detected (readonly-safe init)"
  gow_log "Skipping /usr/share copy steps; using /usr/nvidia paths via env vars"
  ldconfig || true
fi
