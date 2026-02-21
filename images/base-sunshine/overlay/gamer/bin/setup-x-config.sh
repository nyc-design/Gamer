#!/usr/bin/env bash
set -e

echo "[setup-x] Configuring Xorg..."

# Detect NVIDIA PCI bus ID and convert to Xorg format
if [ "$NVIDIA_ENABLE" = "true" ]; then
    # nvidia-smi returns hex format: 00000000:01:00.0
    # Xorg needs decimal format: PCI:1:0:0
    NVIDIA_PCI_BUS_ID_HEX=$(nvidia-smi --query-gpu=pci.bus_id --format=csv,noheader 2>/dev/null | head -1)
    if [ -n "$NVIDIA_PCI_BUS_ID_HEX" ]; then
        # Parse hex bus:device.function
        BUS=$(echo "$NVIDIA_PCI_BUS_ID_HEX" | sed -E 's/.*:([0-9A-Fa-f]+):.*/\1/')
        DEVICE=$(echo "$NVIDIA_PCI_BUS_ID_HEX" | sed -E 's/.*:.*:([0-9A-Fa-f]+)\..*/\1/')
        FUNC=$(echo "$NVIDIA_PCI_BUS_ID_HEX" | sed -E 's/.*\.([0-9A-Fa-f]+).*/\1/')
        export NVIDIA_PCI_BUS_ID="$((16#$BUS)):$((16#$DEVICE)):$((16#$FUNC))"
        echo "[setup-x] NVIDIA PCI bus ID: $NVIDIA_PCI_BUS_ID (from $NVIDIA_PCI_BUS_ID_HEX)"
    else
        export NVIDIA_PCI_BUS_ID="1:0:0"
        echo "[setup-x] WARNING: Could not detect NVIDIA PCI bus ID, defaulting to 1:0:0"
    fi

    if [ "$DUAL_SCREEN" = "1" ]; then
        envsubst < /gamer/conf/x11/templates/xorg-nvidia-dual.conf > /etc/X11/xorg.conf
        echo "[setup-x] Using dual-display NVIDIA config"
    else
        envsubst < /gamer/conf/x11/templates/xorg-nvidia-single.conf > /etc/X11/xorg.conf
        echo "[setup-x] Using single-display NVIDIA config"
    fi
else
    cp /gamer/conf/x11/xorg-dummy.conf /etc/X11/xorg.conf
    echo "[setup-x] Using dummy display driver (no NVIDIA)"
fi
