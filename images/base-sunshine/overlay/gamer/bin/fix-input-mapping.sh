#!/usr/bin/env bash
###############################################################################
# fix-input-mapping.sh — Remap bottom Sunshine's input to bottom display.
#
# Both Sunshine instances create virtual input devices on the same X screen.
# Absolute/touch input from the bottom instance maps to the full virtual desktop
# by default, so touches on the bottom screen appear on the top display.
#
# This script applies an xinput Coordinate Transformation Matrix (CTM) to the
# bottom instance's absolute and touch devices so their coordinates map to only
# the bottom display region.
#
# Each Sunshine instance creates (in order):
#   Mouse passthrough (relative), Mouse passthrough (absolute),
#   Keyboard passthrough, Pen passthrough, Touch passthrough
# Top starts first, so the second occurrence of each = bottom.
#
# Called by setup-screen-mode.sh after resolution changes.
###############################################################################

export DISPLAY=${DISPLAY:-:0}
LOG=/tmp/screen-mode.log
touch "$LOG" 2>/dev/null || LOG=/dev/null

# Get total screen size and bottom display geometry
SCREEN_INFO=$(xrandr --current | grep "^Screen" | grep -oP 'current \d+ x \d+')
SCR_W=$(echo "$SCREEN_INFO" | grep -oP '\d+' | head -1)
SCR_H=$(echo "$SCREEN_INFO" | grep -oP '\d+' | tail -1)

BOT_INFO=$(xrandr --current | grep "^DP-2" | grep -oP '\d+x\d+\+\d+\+\d+' | head -1)
BOT_W=$(echo "$BOT_INFO" | cut -dx -f1)
BOT_H=$(echo "$BOT_INFO" | cut -dx -f2 | cut -d+ -f1)
BOT_X=$(echo "$BOT_INFO" | cut -d+ -f2)
BOT_Y=$(echo "$BOT_INFO" | cut -d+ -f3)

if [ -z "$SCR_W" ] || [ -z "$BOT_W" ]; then
    echo "[$(date)] input-mapping: could not determine screen geometry, skipping" >> "$LOG"
    exit 0
fi

echo "[$(date)] input-mapping: screen=${SCR_W}x${SCR_H} bottom=${BOT_W}x${BOT_H}+${BOT_X}+${BOT_Y}" >> "$LOG"

# Calculate CTM for the bottom display region
CTM=$(python3 -c "
sw, sh = ${SCR_W}, ${SCR_H}
bw, bh, bx, by = ${BOT_W}, ${BOT_H}, ${BOT_X}, ${BOT_Y}
print(f'{bw/sw:.6f} 0.000000 {bx/sw:.6f} 0.000000 {bh/sh:.6f} {by/sh:.6f} 0.000000 0.000000 1.000000')
")

echo "[$(date)] input-mapping: CTM=$CTM" >> "$LOG"

# Find bottom Sunshine's absolute and touch devices (second occurrence of each)
BOTTOM_ABS_ID=$(xinput list 2>/dev/null | grep "Mouse passthrough (absolute)" | \
    sed -n '2p' | grep -oP 'id=\K\d+')
BOTTOM_TOUCH_ID=$(xinput list 2>/dev/null | grep "Touch passthrough" | \
    sed -n '2p' | grep -oP 'id=\K\d+')

APPLIED=0

for DEV_ID in $BOTTOM_ABS_ID $BOTTOM_TOUCH_ID; do
    if [ -n "$DEV_ID" ]; then
        DEV_NAME=$(xinput list --name-only "$DEV_ID" 2>/dev/null)
        xinput set-prop "$DEV_ID" "Coordinate Transformation Matrix" $CTM 2>>"$LOG"
        echo "[$(date)] input-mapping: applied CTM to id=$DEV_ID ($DEV_NAME)" >> "$LOG"
        APPLIED=$((APPLIED + 1))
    fi
done

if [ $APPLIED -eq 0 ]; then
    echo "[$(date)] input-mapping: no bottom devices found, skipping" >> "$LOG"
else
    echo "[$(date)] input-mapping: done ($APPLIED devices)" >> "$LOG"
fi
