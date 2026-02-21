#!/usr/bin/env bash
###############################################################################
# setup-screen-mode.sh — Dynamic resolution change on Sunshine client connect.
#
# Called by Sunshine's global_prep_cmd. Sunshine sets these env vars:
#   SUNSHINE_CLIENT_WIDTH, SUNSHINE_CLIENT_HEIGHT, SUNSHINE_CLIENT_FPS
#
# Args: $1 = xrandr output name (e.g., "DP-0" or "DP-2")
###############################################################################

export DISPLAY=${DISPLAY:-:0}

OUTPUT=${1:-DP-0}
WIDTH=${SUNSHINE_CLIENT_WIDTH:-1920}
HEIGHT=${SUNSHINE_CLIENT_HEIGHT:-1080}
FPS=${SUNSHINE_CLIENT_FPS:-60}

LOG=/tmp/screen-mode.log
echo "[$(date)] $OUTPUT: requested ${WIDTH}x${HEIGHT}@${FPS}Hz" >> "$LOG"

# Check if already at this resolution
CURRENT=$(xrandr --current | grep "^${OUTPUT}" | grep -oP '\d+x\d+\+' | head -1 | tr -d '+')
if [ "$CURRENT" = "${WIDTH}x${HEIGHT}" ]; then
    echo "[$(date)] $OUTPUT: already at ${WIDTH}x${HEIGHT}, skipping" >> "$LOG"
    exit 0
fi

# Check if the mode is already available as a built-in on this output
BUILTIN=$(xrandr --current | sed -n "/^${OUTPUT}/,/^[A-Z]/p" | grep -oP "^\s+${WIDTH}x${HEIGHT}\s" | head -1 | tr -d ' ')
if [ -n "$BUILTIN" ]; then
    echo "[$(date)] $OUTPUT: using built-in mode ${WIDTH}x${HEIGHT}" >> "$LOG"
    xrandr --output "$OUTPUT" --mode "${WIDTH}x${HEIGHT}" 2>>"$LOG"
else
    # Custom resolution: create mode via cvt with a unique timestamp-based name.
    # NVIDIA virtual displays leave orphaned xrandr modes that can't be reused or
    # removed, so we use a unique name per invocation to avoid name collisions.
    MODE_NAME="${WIDTH}x${HEIGHT}_$(date +%s)"

    # Generate modeline values (strip name and quotes — we supply our own name)
    MODELINE_VALS=$(cvt "$WIDTH" "$HEIGHT" "$FPS" 2>&1 | grep Modeline | sed 's/Modeline "[^"]*"//')
    if [ -z "$MODELINE_VALS" ]; then
        echo "[$(date)] $OUTPUT: cvt failed" >> "$LOG"
        exit 1
    fi

    # Create the mode: pass name as a quoted arg, values unquoted
    xrandr --newmode "$MODE_NAME" $MODELINE_VALS 2>>"$LOG"

    # Force xrandr to refresh its cached mode list from the X server.
    # Without this, --addmode cannot find the mode just created by --newmode.
    xrandr --current >/dev/null 2>&1

    # Attach mode to this output
    xrandr --addmode "$OUTPUT" "$MODE_NAME" 2>>"$LOG"

    # Apply the mode
    echo "[$(date)] $OUTPUT: applying custom mode $MODE_NAME" >> "$LOG"
    xrandr --output "$OUTPUT" --mode "$MODE_NAME" 2>>"$LOG"
fi

RESULT=$?
if [ $RESULT -ne 0 ]; then
    echo "[$(date)] $OUTPUT: failed to apply mode (exit $RESULT)" >> "$LOG"
    exit 1
fi

# Reposition stacked layout: bottom display starts where top ends
TOP_HEIGHT=$(xrandr --current | grep "^DP-0" | grep -oP '\d+x\d+\+\d+\+\d+' | head -1 | cut -dx -f2 | cut -d+ -f1)
TOP_HEIGHT=${TOP_HEIGHT:-1080}
xrandr --output DP-2 --pos "0x${TOP_HEIGHT}" 2>/dev/null || true

echo "[$(date)] $OUTPUT: done ${WIDTH}x${HEIGHT}@${FPS}Hz. Bottom at y=${TOP_HEIGHT}" >> "$LOG"

# Reposition emulator windows to fill the new display layout
SCRIPT_DIR=$(dirname "$0")
if [ -x "$SCRIPT_DIR/reposition-windows.sh" ]; then
    "$SCRIPT_DIR/reposition-windows.sh" &
fi

# Note: Bottom screen input remapping is handled by input-remap-proxy.py
# (evdev-level uinput proxy), not CTM. See Sunshine bug #3696.
