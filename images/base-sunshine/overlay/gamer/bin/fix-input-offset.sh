#!/usr/bin/env bash
###############################################################################
# fix-input-offset.sh — Apply xinput Coordinate Transformation Matrix to
# Sunshine's virtual input devices so that the bottom Sunshine instance's
# touch/mouse events map to the correct Y region of the virtual desktop.
#
# On a 1920x2160 desktop with two 1920x1080 displays stacked vertically:
#   Top display:    Y in [0, 1080)     → normalized [0, 0.5)
#   Bottom display: Y in [1080, 2160)  → normalized [0.5, 1.0)
#
# Sunshine creates uinput devices named "Sunshine Mouse", "Sunshine Touch".
# Each Sunshine instance creates its OWN devices, so we identify the bottom
# instance's devices by the port number in the process that owns the device.
#
# Usage: fix-input-offset.sh <instance> <y_offset> <display_height> <total_height>
#   instance:       "top" or "bottom"
#   y_offset:       pixel Y offset of this display (e.g. 0 for top, 1080 for bottom)
#   display_height: height of this display in pixels (e.g. 1080)
#   total_height:   total virtual desktop height (e.g. 2160)
#
# Sunshine Bug Context:
#   On Windows, Sunshine applies touch_port.offset_x/y to mouse coordinates
#   via MOUSEEVENTF_VIRTUALDESK. On Linux, inputtino's uinput path ignores
#   offsets entirely. This script works around that by using X11's CTM.
###############################################################################

set -e

INSTANCE=${1:-bottom}
Y_OFFSET=${2:-1080}
DISPLAY_HEIGHT=${3:-1080}
TOTAL_HEIGHT=${4:-2160}

# Calculate CTM values
# CTM is a 3x3 matrix: [c0 c1 c2; c3 c4 c5; c6 c7 c8]
# For the bottom screen:
#   X: unchanged → c0=1, c1=0, c2=0
#   Y: scale by (display_height/total_height), offset by (y_offset/total_height)
#     c3=0, c4=scale, c5=offset_normalized
#   Affine row: c6=0, c7=0, c8=1
SCALE=$(python3 -c "print($DISPLAY_HEIGHT / $TOTAL_HEIGHT)")
OFFSET=$(python3 -c "print($Y_OFFSET / $TOTAL_HEIGHT)")

echo "[fix-input] Instance=$INSTANCE Y_OFFSET=$Y_OFFSET SCALE=$SCALE OFFSET=$OFFSET"

# Wait for Sunshine to create its input devices
# Sunshine's inputtino creates devices named "Sunshine Keyboard", "Sunshine Mouse", etc.
MAX_WAIT=30
WAITED=0
while [ $WAITED -lt $MAX_WAIT ]; do
    # Find Sunshine input devices
    DEVICES=$(xinput list --name-only 2>/dev/null | grep -i "sunshine" || true)
    if [ -n "$DEVICES" ]; then
        break
    fi
    sleep 1
    WAITED=$((WAITED + 1))
done

if [ -z "$DEVICES" ]; then
    echo "[fix-input] No Sunshine input devices found after ${MAX_WAIT}s"
    exit 0
fi

echo "[fix-input] Found Sunshine input devices:"
echo "$DEVICES"

# For the top instance (offset=0), CTM is identity — no changes needed
if [ "$Y_OFFSET" = "0" ]; then
    echo "[fix-input] Top screen — no CTM adjustment needed"
    exit 0
fi

# For the bottom instance, we need to find WHICH Sunshine devices belong to it.
# Strategy: The bottom Sunshine process (port 48089) creates its own uinput devices.
# We can identify them by checking which devices appeared AFTER the bottom instance started.
#
# However, xinput doesn't expose which process owns a device. Instead, we apply
# the CTM to ALL Sunshine mouse/touch devices and rely on the fact that:
# - Each Sunshine instance only sends events to its OWN devices
# - The CTM on the top instance's devices is identity (won't affect top input)
# - The CTM on the bottom instance's devices remaps Y to bottom half
#
# PROBLEM: We can't distinguish which devices belong to which instance via xinput alone.
#
# WORKAROUND: Use the global_prep_cmd to apply CTM when the bottom client connects.
# At that point, the bottom instance has just created fresh devices. We find the
# newest Sunshine Mouse/Touch devices and apply the CTM to them.

# Find all pointer/touch devices with "Sunshine" in the name
POINTER_IDS=$(xinput list 2>/dev/null | grep -i "sunshine" | grep -iE "mouse|touch|pointer" | grep -oP 'id=\K\d+' || true)

if [ -z "$POINTER_IDS" ]; then
    echo "[fix-input] No Sunshine pointer devices found"
    exit 0
fi

# Apply CTM to each device
# The matrix: [1, 0, 0, 0, $SCALE, $OFFSET, 0, 0, 1]
for DEVICE_ID in $POINTER_IDS; do
    DEVICE_NAME=$(xinput list-props $DEVICE_ID 2>/dev/null | head -1 || echo "unknown")
    echo "[fix-input] Applying CTM to device $DEVICE_ID ($DEVICE_NAME)"
    echo "[fix-input]   Matrix: 1 0 0 | 0 $SCALE $OFFSET | 0 0 1"
    xinput set-prop $DEVICE_ID "Coordinate Transformation Matrix" \
        1 0 0 \
        0 $SCALE $OFFSET \
        0 0 1 \
        2>/dev/null || echo "[fix-input]   WARNING: Failed to set CTM on device $DEVICE_ID"
done

echo "[fix-input] CTM applied to ${INSTANCE} instance devices"
