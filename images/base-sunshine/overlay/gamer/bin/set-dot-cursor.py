#!/usr/bin/env python3
"""
Set a tiny dot cursor on the active bottom-screen window for touch input.

Creates an 8x8 cursor with a 4x4 white dot at center, applied per-window
via XDefineCursor. Top screen keeps the normal arrow cursor.

By default ("auto"), prefers Secondary Window, then ScreenTool, constrained
to windows currently on DP-2.
"""

import ctypes
import ctypes.util
import subprocess
import sys
import os
import argparse

LOG_PREFIX = "[dot-cursor]"


def log(msg):
    print(f"{LOG_PREFIX} {msg}", flush=True)


def parse_dp2_geometry():
    """Return DP-2 geometry tuple (x, y, w, h) or None."""
    try:
        out = subprocess.run(
            ["xrandr", "--current"], capture_output=True, text=True, timeout=5
        ).stdout
    except Exception:
        return None
    for line in out.splitlines():
        if not line.startswith("DP-2 "):
            continue
        import re
        m = re.search(r"(\d+)x(\d+)\+(\d+)\+(\d+)", line)
        if not m:
            continue
        w, h, x, y = map(int, m.groups())
        return (x, y, w, h)
    return None


def get_window_geometry(wid_str):
    try:
        geom = subprocess.run(
            ["xdotool", "getwindowgeometry", "--shell", wid_str],
            capture_output=True, text=True, timeout=5
        ).stdout
    except Exception:
        return None
    vals = {}
    for line in geom.splitlines():
        if "=" in line:
            k, v = line.split("=", 1)
            vals[k.strip()] = v.strip()
    try:
        return (
            int(vals.get("X", "0")),
            int(vals.get("Y", "0")),
            int(vals.get("WIDTH", "0")),
            int(vals.get("HEIGHT", "0")),
        )
    except ValueError:
        return None


def is_on_dp2(wid_str, dp2):
    if dp2 is None:
        return True
    wg = get_window_geometry(wid_str)
    if wg is None:
        return False
    wx, wy, ww, wh = wg
    dx, dy, dw, dh = dp2
    # Large-overlap check with minimum size guard
    if ww < 120 or wh < 120:
        return False
    overlap_w = max(0, min(wx + ww, dx + dw) - max(wx, dx))
    overlap_h = max(0, min(wy + wh, dy + dh) - max(wy, dy))
    return overlap_w * overlap_h >= int(0.5 * ww * wh)


def find_bottom_window(target="auto"):
    """Find the bottom window by target mode: auto|secondary|screentool."""
    dp2 = parse_dp2_geometry()
    patterns = []
    if target == "secondary":
        patterns = ["Secondary Window"]
    elif target == "screentool":
        patterns = ["^ScreenTool"]
    else:
        # Prefer emulator bottom window; fallback to screen-tool.
        patterns = ["Secondary Window", "^ScreenTool"]

    for pattern in patterns:
        try:
            result = subprocess.run(
                ["xdotool", "search", "--name", pattern],
                capture_output=True, text=True, timeout=5
            )
        except Exception as e:
            log(f"xdotool search failed for '{pattern}': {e}")
            continue

        for wid_str in result.stdout.strip().split():
            if is_on_dp2(wid_str, dp2):
                return int(wid_str)
    return None
def apply_dot_cursor(window_id):
    """Create and apply an 8x8 cursor with 4x4 white dot to the given window."""
    lib_path = ctypes.util.find_library("X11")
    if not lib_path:
        log("ERROR: libX11 not found")
        return False

    xlib = ctypes.cdll.LoadLibrary(lib_path)
    display = xlib.XOpenDisplay(None)
    if not display:
        log("ERROR: Cannot open display")
        return False

    try:
        screen = xlib.XDefaultScreen(display)
        root = xlib.XRootWindow(display, screen)

        # 8x8 bitmap, 4x4 white dot centered at (2,2)
        source = xlib.XCreatePixmap(display, root, 8, 8, 1)
        mask = xlib.XCreatePixmap(display, root, 8, 8, 1)
        gc = xlib.XCreateGC(display, source, 0, None)

        # Clear both
        xlib.XSetForeground(display, gc, 0)
        xlib.XFillRectangle(display, source, gc, 0, 0, 8, 8)
        xlib.XFillRectangle(display, mask, gc, 0, 0, 8, 8)

        # 4x4 dot at center
        xlib.XSetForeground(display, gc, 1)
        xlib.XFillRectangle(display, source, gc, 2, 2, 4, 4)
        xlib.XFillRectangle(display, mask, gc, 2, 2, 4, 4)

        # XColor struct
        class XColor(ctypes.Structure):
            _fields_ = [
                ("pixel", ctypes.c_ulong),
                ("red", ctypes.c_ushort),
                ("green", ctypes.c_ushort),
                ("blue", ctypes.c_ushort),
                ("flags", ctypes.c_char),
                ("pad", ctypes.c_char),
            ]

        fg = XColor()
        fg.red = fg.green = fg.blue = 65535  # white
        bg = XColor()
        bg.red = bg.green = bg.blue = 0  # black

        # Create cursor with hotspot at center (4,4)
        cursor = xlib.XCreatePixmapCursor(
            display, source, mask,
            ctypes.byref(fg), ctypes.byref(bg),
            4, 4
        )
        xlib.XDefineCursor(display, window_id, cursor)
        xlib.XFlush(display)

        # Cleanup
        xlib.XFreePixmap(display, source)
        xlib.XFreePixmap(display, mask)
        xlib.XFreeGC(display, gc)

        log(f"Applied 4x4 dot cursor to window 0x{window_id:x}")
        return True
    finally:
        xlib.XCloseDisplay(display)


def main():
    os.environ.setdefault("DISPLAY", ":0")

    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--target",
        choices=["auto", "secondary", "screentool"],
        default="auto",
        help="Window target for dot cursor",
    )
    args = parser.parse_args()

    window_id = find_bottom_window(args.target)
    if not window_id:
        log(f"No target bottom window found (target={args.target}), skipping")
        sys.exit(0)

    if not apply_dot_cursor(window_id):
        sys.exit(1)


if __name__ == "__main__":
    main()
