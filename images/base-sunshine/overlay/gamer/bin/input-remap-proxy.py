#!/usr/bin/env python3
"""
uinput proxy to fix Sunshine's multi-monitor absolute input bug (#3696).

Problem: Sunshine captures DP-2 (bottom display at y=TOP_HEIGHT in a stacked
dual-monitor X screen). When a client sends absolute/touch input, Sunshine
writes uinput events scaled to the stream resolution only — ignoring the
display's offset within the virtual desktop. Result: touches on the bottom
screen land on the top display.

Solution: Grab Sunshine's bottom-instance input devices at the evdev level,
remap coordinates to the correct display region, and emit them via a proxy
uinput device.

This script handles BOTH absolute mouse and multitouch devices. It finds
the bottom Sunshine instance's devices by looking for the 2nd occurrence
of each device type (top Sunshine starts first, bottom second).

Integrated into the container via supervisord. Auto-detects display geometry
from xrandr.
"""

import os
import re
import signal
import subprocess
import sys
import time
from typing import Optional

try:
    import evdev
    from evdev import InputDevice, UInput, AbsInfo, ecodes as e, list_devices
except ImportError:
    print("ERROR: python3-evdev not installed. Install with: pip3 install evdev")
    sys.exit(1)

LOG_PREFIX = "[input-remap]"

# ABS codes that represent X/Y coordinates (single-touch and multitouch)
X_CODES = {e.ABS_X, e.ABS_MT_POSITION_X}
Y_CODES = {e.ABS_Y, e.ABS_MT_POSITION_Y}


def log(msg: str):
    print(f"{LOG_PREFIX} {msg}", flush=True)


def get_display_geometry() -> dict:
    """
    Parse xrandr output to get display layout.

    Returns dict with:
        top_w, top_h, top_x, top_y: DP-0 geometry
        bot_w, bot_h, bot_x, bot_y: DP-2 geometry
        virt_w, virt_h: total virtual desktop size
    """
    display = os.environ.get("DISPLAY", ":0")
    try:
        output = subprocess.check_output(
            ["xrandr", "--current"], env={**os.environ, "DISPLAY": display},
            text=True, timeout=5
        )
    except Exception as ex:
        log(f"xrandr failed: {ex}")
        return {}

    geo = {}

    # Parse screen size
    m = re.search(r"current (\d+) x (\d+)", output)
    if m:
        geo["virt_w"] = int(m.group(1))
        geo["virt_h"] = int(m.group(2))

    # Parse each display
    for line in output.splitlines():
        for prefix, key in [("DP-0", "top"), ("DP-2", "bot")]:
            if line.startswith(prefix):
                m = re.search(r"(\d+)x(\d+)\+(\d+)\+(\d+)", line)
                if m:
                    geo[f"{key}_w"] = int(m.group(1))
                    geo[f"{key}_h"] = int(m.group(2))
                    geo[f"{key}_x"] = int(m.group(3))
                    geo[f"{key}_y"] = int(m.group(4))

    return geo


def find_bottom_device(name_pattern: str) -> Optional[InputDevice]:
    """
    Find the bottom Sunshine instance's device by xinput ID order.

    Sunshine assigns xinput IDs in creation order. The bottom instance
    starts after the top, so its devices have HIGHER xinput IDs.
    We query xinput to find the highest-ID device matching the pattern,
    get its /dev/input/eventN node, then open it via evdev.

    Event node paths (/dev/input/eventN) are NOT in creation order —
    the kernel reuses freed minor numbers. xinput IDs are reliable.
    """
    display = os.environ.get("DISPLAY", ":0")
    try:
        output = subprocess.check_output(
            ["xinput", "list"], env={**os.environ, "DISPLAY": display},
            text=True, timeout=5
        )
    except Exception as ex:
        log(f"xinput list failed: {ex}")
        return None

    # Find all xinput IDs matching the pattern (exclude "remapped" proxy devices)
    matching_ids = []
    for line in output.splitlines():
        if name_pattern.lower() in line.lower() and "remapped" not in line.lower():
            m = re.search(r"id=(\d+)", line)
            if m:
                matching_ids.append(int(m.group(1)))

    if len(matching_ids) < 2:
        return None

    # Highest ID = bottom instance
    bottom_id = max(matching_ids)

    # Get device node from xinput
    try:
        props = subprocess.check_output(
            ["xinput", "list-props", str(bottom_id)],
            env={**os.environ, "DISPLAY": display},
            text=True, timeout=5
        )
    except Exception as ex:
        log(f"xinput list-props {bottom_id} failed: {ex}")
        return None

    node_match = re.search(r"Device Node.*\"(/dev/input/event\d+)\"", props)
    if not node_match:
        log(f"No device node for xinput id={bottom_id}")
        return None

    node_path = node_match.group(1)
    try:
        dev = InputDevice(node_path)
        log(f"Found bottom device: xinput id={bottom_id} -> {dev.name} at {node_path}")
        return dev
    except Exception as ex:
        log(f"Could not open {node_path}: {ex}")
        return None


def get_abs_info(dev: InputDevice, axis_code: int) -> Optional[AbsInfo]:
    """Get AbsInfo for a specific axis code."""
    caps = dev.capabilities(absinfo=True)
    for code_info in caps.get(e.EV_ABS, []):
        if isinstance(code_info, tuple) and len(code_info) == 2:
            code, info = code_info
            if code == axis_code:
                return info
    return None


def create_proxy(source: InputDevice) -> UInput:
    """
    Create a proxy UInput device with IDENTICAL axis ranges to the source.

    CRITICAL: Do NOT copy INPUT_PROP_DIRECT. With that property, libinput
    maps the touchscreen to a single display output (typically the first one),
    so coordinates only land on that display regardless of value. Without it,
    libinput maps [0, axis_max] across the full virtual desktop, which is
    what we need for multi-monitor coordinate remapping.
    """
    caps = source.capabilities(absinfo=True)
    new_caps = {}

    for ev_type, codes in caps.items():
        if ev_type == e.EV_SYN:
            continue
        new_caps[ev_type] = codes

    # Deliberately omit input_props — do NOT propagate INPUT_PROP_DIRECT
    proxy = UInput(
        events=new_caps,
        name=f"{source.name} (remapped)",
        vendor=source.info.vendor,
        product=source.info.product,
        version=source.info.version,
        bustype=source.info.bustype,
    )
    log(f"Created proxy: {proxy.name} (no INPUT_PROP_DIRECT)")
    return proxy


def remap(value: int, src_max: int, offset: int) -> int:
    """
    Add a fixed pixel offset and clamp.

    Sunshine writes touch coords normalized to [0, src_max] representing
    position within the captured display. Without INPUT_PROP_DIRECT,
    X11 maps [0, src_max] to the full virtual desktop. Adding the offset
    shifts the coordinates to the bottom display's region.
    """
    return min(value + offset, src_max)


def proxy_loop(source: InputDevice, proxy: UInput,
               axis_info: dict,
               x_offset: int, y_offset: int,
               verbose: bool = False):
    """Read events from source, add offset to coordinates, write to proxy."""
    for event in source.read_loop():
        if event.type == e.EV_ABS:
            if event.code in X_CODES and event.code in axis_info:
                info = axis_info[event.code]
                val = remap(event.value, info.max, x_offset)
                if verbose:
                    log(f"X({event.code}): {event.value} -> {val}")
                proxy.write(e.EV_ABS, event.code, val)
            elif event.code in Y_CODES and event.code in axis_info:
                info = axis_info[event.code]
                val = remap(event.value, info.max, y_offset)
                if verbose:
                    log(f"Y({event.code}): {event.value} -> {val}")
                proxy.write(e.EV_ABS, event.code, val)
            else:
                proxy.write(event.type, event.code, event.value)
        elif event.type == e.EV_SYN:
            proxy.syn()
        else:
            proxy.write(event.type, event.code, event.value)


def run_device_proxy(device_pattern: str,
                     geo: dict, verbose: bool = False):
    """
    Find the bottom Sunshine instance's device, create proxy, grab source, run event loop.
    Returns the source and proxy for cleanup.
    """
    source = None
    proxy = None

    # Wait for device to appear (Sunshine creates them when clients connect)
    log(f"Waiting for bottom device matching '{device_pattern}'...")
    while source is None:
        source = find_bottom_device(device_pattern)
        if source is None:
            time.sleep(2)

    # Collect axis info
    axis_info = {}
    for code in (e.ABS_X, e.ABS_Y, e.ABS_MT_POSITION_X, e.ABS_MT_POSITION_Y):
        info = get_abs_info(source, code)
        if info:
            axis_info[code] = info

    log(f"Source axes: " + ", ".join(
        f"{e.ABS.get(c, c)}=[{i.min},{i.max}]" for c, i in axis_info.items()
    ))

    virt_w = geo.get("virt_w", 1920)
    virt_h = geo.get("virt_h", 2160)
    bot_x = geo.get("bot_x", 0)
    bot_y = geo.get("bot_y", 1080)
    bot_w = geo.get("bot_w", 1920)
    bot_h = geo.get("bot_h", 1080)

    # Compute per-axis offsets. For each axis, the offset in device units
    # equals (display_pixel_offset / virtual_desktop_pixels) * axis_max.
    # Touch source uses ABS_Y max=10800, mouse uses ABS_Y max=12000.
    # We compute offsets per-axis from axis_info.
    y_axis_max = axis_info.get(e.ABS_Y, axis_info.get(e.ABS_MT_POSITION_Y))
    x_axis_max = axis_info.get(e.ABS_X, axis_info.get(e.ABS_MT_POSITION_X))
    y_max = y_axis_max.max if y_axis_max else 10800
    x_max = x_axis_max.max if x_axis_max else 19200

    x_offset = int(round(bot_x / virt_w * x_max)) if virt_w > 0 else 0
    y_offset = int(round(bot_y / virt_h * y_max)) if virt_h > 0 else 0

    log(f"Bottom display: {bot_w}x{bot_h}+{bot_x}+{bot_y} "
        f"in {virt_w}x{virt_h} desktop")
    log(f"Offsets: x={x_offset} (max={x_max}), y={y_offset} (max={y_max})")

    proxy = create_proxy(source)
    time.sleep(0.3)

    source.grab()
    log(f"Grabbed {source.name} — proxy active")

    proxy_loop(source, proxy, axis_info,
               x_offset, y_offset, verbose)

    return source, proxy


def main():
    import threading

    verbose = "--verbose" in sys.argv or "-v" in sys.argv

    # Wait for X server
    log("Waiting for X server...")
    display = os.environ.get("DISPLAY", ":0")
    for _ in range(60):
        try:
            subprocess.check_call(
                ["xdpyinfo"], env={**os.environ, "DISPLAY": display},
                stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=3
            )
            break
        except Exception:
            time.sleep(1)
    else:
        log("ERROR: X server not available after 60s")
        sys.exit(1)

    # Wait a bit for Sunshine to create input devices
    log("Waiting for Sunshine input devices...")
    time.sleep(5)

    # Get display geometry
    geo = get_display_geometry()
    if not geo:
        log("WARNING: Could not get display geometry, using defaults")
        geo = {"virt_w": 1920, "virt_h": 2160,
               "top_w": 1920, "top_h": 1080, "top_x": 0, "top_y": 0,
               "bot_w": 1920, "bot_h": 1080, "bot_x": 0, "bot_y": 1080}

    log(f"Display geometry: {geo}")

    sources = []
    proxies = []

    def cleanup(sig=None, frame=None):
        log("Shutting down...")
        for s in sources:
            try:
                s.ungrab()
            except Exception:
                pass
        for p in proxies:
            try:
                p.close()
            except Exception:
                pass
        sys.exit(0)

    signal.signal(signal.SIGINT, cleanup)
    signal.signal(signal.SIGTERM, cleanup)

    # Proxy both touch and absolute mouse for the bottom Sunshine instance.
    # find_bottom_device uses xinput ID order (highest ID = bottom instance).
    device_patterns = [
        "Mouse passthrough (absolute)",
        "Touch passthrough",
    ]

    threads = []
    for pattern in device_patterns:
        def run(p=pattern):
            try:
                src, prx = run_device_proxy(p, geo, verbose)
                sources.append(src)
                proxies.append(prx)
            except Exception as ex:
                log(f"Proxy for '{p}' #{o} failed: {ex}")

        t = threading.Thread(target=run, daemon=True)
        t.start()
        threads.append(t)

    # Wait for all threads (they run forever until device disconnects)
    for t in threads:
        t.join()

    log("All proxies exited")


if __name__ == "__main__":
    main()
