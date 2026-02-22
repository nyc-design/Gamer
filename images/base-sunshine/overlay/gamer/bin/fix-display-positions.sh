#!/usr/bin/env bash
###############################################################################
# fix-display-positions.sh — Force correct display positions after Xorg starts.
#
# NVIDIA's driver ignores MetaModes +0+0 offsets for virtual displays, centering
# them in the virtual desktop instead (e.g., DP-0 at +960+840 in a 3840x3840
# desktop). This script corrects the positions so NVFBC captures the right areas
# and windows align with displays.
#
# Runs once at startup (priority 30, after Xorg at 20, before Sunshine at 40).
###############################################################################

export DISPLAY=${DISPLAY:-:0}
LOG=/tmp/screen-mode.log

# Wait for X server
/gamer/bin/wait-x.sh

# Small delay for NVIDIA driver to finish display init
sleep 1

echo "[fix-display] Checking display positions..." >> "$LOG"
BEFORE_DP0=$(xrandr --current | grep "^DP-0" | grep -oP '\d+x\d+\+\d+\+\d+' | head -1)
BEFORE_DP2=$(xrandr --current | grep "^DP-2" | grep -oP '\d+x\d+\+\d+\+\d+' | head -1)
echo "[fix-display] Before: DP-0=${BEFORE_DP0} DP-2=${BEFORE_DP2}" >> "$LOG"

# Force DP-0 to origin
xrandr --output DP-0 --pos 0x0 2>>"$LOG" || true

# Get DP-0 height to stack DP-2 below it
TOP_HEIGHT=$(xrandr --current | grep "^DP-0" | grep -oP '\d+x\d+\+\d+\+\d+' | head -1 | cut -dx -f2 | cut -d+ -f1)
TOP_HEIGHT=${TOP_HEIGHT:-1080}

# Force DP-2 directly below DP-0
xrandr --output DP-2 --pos "0x${TOP_HEIGHT}" 2>>"$LOG" || true

AFTER_DP0=$(xrandr --current | grep "^DP-0" | grep -oP '\d+x\d+\+\d+\+\d+' | head -1)
AFTER_DP2=$(xrandr --current | grep "^DP-2" | grep -oP '\d+x\d+\+\d+\+\d+' | head -1)
echo "[fix-display] After:  DP-0=${AFTER_DP0} DP-2=${AFTER_DP2}" >> "$LOG"

echo "[fix-display] Display positions fixed." >> "$LOG"
