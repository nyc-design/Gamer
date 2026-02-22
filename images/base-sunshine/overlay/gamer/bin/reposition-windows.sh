#!/usr/bin/env bash
###############################################################################
# reposition-windows.sh — Move/resize emulator windows to fill current displays.
#
# Called after resolution changes (by setup-screen-mode.sh) and at startup.
# Finds emulator windows by title and positions them on the correct displays.
#
# In a dual-display stacked layout:
#   DP-0 (top)    at 0,0           — Primary window
#   DP-2 (bottom) at 0,TOP_HEIGHT  — Secondary window
###############################################################################

export DISPLAY=${DISPLAY:-:0}
LOG=/tmp/screen-mode.log
touch "$LOG" 2>/dev/null || LOG=/dev/null

# Get current display dimensions
TOP_INFO=$(xrandr --current | grep "^DP-0" | grep -oP '\d+x\d+\+\d+\+\d+' | head -1)
BOT_INFO=$(xrandr --current | grep "^DP-2" | grep -oP '\d+x\d+\+\d+\+\d+' | head -1)

TOP_WIDTH=$(echo "$TOP_INFO" | cut -dx -f1)
TOP_HEIGHT=$(echo "$TOP_INFO" | cut -dx -f2 | cut -d+ -f1)
TOP_X=$(echo "$TOP_INFO" | cut -d+ -f2)
TOP_Y=$(echo "$TOP_INFO" | cut -d+ -f3)

BOT_WIDTH=$(echo "$BOT_INFO" | cut -dx -f1)
BOT_HEIGHT=$(echo "$BOT_INFO" | cut -dx -f2 | cut -d+ -f1)
BOT_X=$(echo "$BOT_INFO" | cut -d+ -f2)
BOT_Y=$(echo "$BOT_INFO" | cut -d+ -f3)

TOP_WIDTH=${TOP_WIDTH:-1920}
TOP_HEIGHT=${TOP_HEIGHT:-1080}
TOP_X=${TOP_X:-0}
TOP_Y=${TOP_Y:-0}
BOT_WIDTH=${BOT_WIDTH:-1920}
BOT_HEIGHT=${BOT_HEIGHT:-1080}
BOT_X=${BOT_X:-0}
BOT_Y=${BOT_Y:-1080}

echo "[$(date)] reposition: DP-0=${TOP_WIDTH}x${TOP_HEIGHT}+${TOP_X}+${TOP_Y} DP-2=${BOT_WIDTH}x${BOT_HEIGHT}+${BOT_X}+${BOT_Y}" >> "$LOG"

# Find emulator windows by title (supports Azahar, melonDS, Dolphin, etc.)
PRIMARY=""
SECONDARY=""
SCREEN_TOOL=""
for wid in $(xdotool search --name "Azahar|melonDS|Dolphin|PPSSPP|Ryujinx|Steam|ScreenTool" 2>/dev/null); do
    name=$(xdotool getwindowname "$wid" 2>/dev/null)
    # Skip shader output windows
    echo "$name" | grep -q "^Shader: " && continue
    if echo "$name" | grep -q "^ScreenTool"; then
        SCREEN_TOOL=$wid
    elif echo "$name" | grep -q "Secondary Window"; then
        SECONDARY=$wid
    elif echo "$name" | grep -qE "(Azahar|melonDS|Dolphin|PPSSPP|Ryujinx|Steam) [0-9]"; then
        PRIMARY=$wid
    fi
done

# Find shader output windows (created by shader-overlay tool)
SHADER_PRIMARY=""
SHADER_SECONDARY=""
for wid in $(xdotool search --name "^Shader: " 2>/dev/null); do
    name=$(xdotool getwindowname "$wid" 2>/dev/null)
    if echo "$name" | grep -q "Secondary Window"; then
        SHADER_SECONDARY=$wid
    else
        SHADER_PRIMARY=$wid
    fi
done

if [ -z "$PRIMARY" ] && [ -z "$SECONDARY" ] && [ -z "$SHADER_PRIMARY" ] && [ -z "$SHADER_SECONDARY" ] && [ -z "$SCREEN_TOOL" ]; then
    echo "[$(date)] reposition: no emulator, shader, or screen-tool windows found, skipping" >> "$LOG"
    exit 0
fi

# Position primary emulator window on top display
if [ -n "$PRIMARY" ]; then
    echo "[$(date)] reposition: primary $PRIMARY -> ${TOP_X},${TOP_Y} ${TOP_WIDTH}x${TOP_HEIGHT}" >> "$LOG"
    xdotool windowmove "$PRIMARY" "$TOP_X" "$TOP_Y"
    xdotool windowsize "$PRIMARY" "$TOP_WIDTH" "$TOP_HEIGHT"
fi

# Position secondary emulator window on bottom display
if [ -n "$SECONDARY" ]; then
    echo "[$(date)] reposition: secondary $SECONDARY -> ${BOT_X},${BOT_Y} ${BOT_WIDTH}x${BOT_HEIGHT}" >> "$LOG"
    xdotool windowmove "$SECONDARY" "$BOT_X" "$BOT_Y"
    xdotool windowsize "$SECONDARY" "$BOT_WIDTH" "$BOT_HEIGHT"
fi

# Position shader output windows on their respective displays (same positions)
# Shader windows must be ABOVE emulator windows since NVFBC captures the composited display
if [ -n "$SHADER_PRIMARY" ]; then
    echo "[$(date)] reposition: shader-primary $SHADER_PRIMARY -> ${TOP_X},${TOP_Y} ${TOP_WIDTH}x${TOP_HEIGHT}" >> "$LOG"
    xdotool windowmove "$SHADER_PRIMARY" "$TOP_X" "$TOP_Y"
    xdotool windowsize "$SHADER_PRIMARY" "$TOP_WIDTH" "$TOP_HEIGHT"
    xdotool windowraise "$SHADER_PRIMARY"
fi

if [ -n "$SHADER_SECONDARY" ]; then
    echo "[$(date)] reposition: shader-secondary $SHADER_SECONDARY -> ${BOT_X},${BOT_Y} ${BOT_WIDTH}x${BOT_HEIGHT}" >> "$LOG"
    xdotool windowmove "$SHADER_SECONDARY" "$BOT_X" "$BOT_Y"
    xdotool windowsize "$SHADER_SECONDARY" "$BOT_WIDTH" "$BOT_HEIGHT"
    xdotool windowraise "$SHADER_SECONDARY"
fi

# Always position screen-tool on bottom display (it manages its own Z-order
# relative to the secondary emulator window based on pixel content)
if [ -n "$SCREEN_TOOL" ]; then
    echo "[$(date)] reposition: screen-tool $SCREEN_TOOL -> ${BOT_X},${BOT_Y} ${BOT_WIDTH}x${BOT_HEIGHT}" >> "$LOG"
    xdotool windowmove "$SCREEN_TOOL" "$BOT_X" "$BOT_Y"
    xdotool windowsize "$SCREEN_TOOL" "$BOT_WIDTH" "$BOT_HEIGHT"
fi

# Apply dot cursor to bottom screen window for touch-friendly UX
SCRIPT_DIR=$(dirname "$0")
if [ -n "$SECONDARY" ] && [ -f "$SCRIPT_DIR/set-dot-cursor.py" ]; then
    python3 "$SCRIPT_DIR/set-dot-cursor.py" >> "$LOG" 2>&1 &
fi

echo "[$(date)] reposition: done" >> "$LOG"
