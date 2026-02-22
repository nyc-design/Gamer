#!/usr/bin/env python3
"""
uinput proxy to fix Sunshine's multi-monitor absolute input bug (#3696).

Problem: Sunshine captures DP-2 (bottom display at y=TOP_HEIGHT in a stacked
dual-monitor X screen). When a client sends absolute/touch input, Sunshine
writes uinput events scaled to the stream resolution only — ignoring the
display's offset within the virtual desktop. Result: touches on the bottom
screen land on the top display.

Solution: proxy Sunshine's bottom-instance input devices, with per-device mode:
- Mouse passthrough (absolute): remap coordinates into the DP-2 region.
- Touch passthrough: preserve direct-touch props and map via XInput CTM.

Integrated into the container via supervisord. Auto-detects display geometry
from xrandr and reconnects on device churn.
"""

import os
import re
import signal
import subprocess
import sys
import time
from dataclasses import dataclass
from typing import Optional

try:
    import evdev
    from evdev import InputDevice, UInput, AbsInfo, ecodes as e
except ImportError:
    print("ERROR: python3-evdev not installed. Install with: pip3 install evdev")
    sys.exit(1)

LOG_PREFIX = "[input-remap]"

# ABS codes that represent X/Y coordinates (single-touch and multitouch)
X_CODES = {e.ABS_X, e.ABS_MT_POSITION_X}
Y_CODES = {e.ABS_Y, e.ABS_MT_POSITION_Y}


@dataclass
class SourceDevice:
    device: InputDevice
    xinput_id: int


@dataclass
class ProxyOptions:
    name_pattern: str
    remap_coords: bool
    preserve_input_props: bool
    grab_source: bool
    disable_source_xinput: bool


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

    m = re.search(r"current (\d+) x (\d+)", output)
    if m:
        geo["virt_w"] = int(m.group(1))
        geo["virt_h"] = int(m.group(2))

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


def find_bottom_device(name_pattern: str) -> Optional[SourceDevice]:
    """
    Find the bottom Sunshine instance's device by xinput ID order.

    Sunshine assigns xinput IDs in creation order. The bottom instance starts
    after the top, so its devices have higher IDs.
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

    matching_ids = []
    for line in output.splitlines():
        if name_pattern.lower() in line.lower() and "remapped" not in line.lower():
            m = re.search(r"id=(\d+)", line)
            if m:
                matching_ids.append(int(m.group(1)))

    if len(matching_ids) < 1:
        return None

    bottom_id = max(matching_ids)

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
        return SourceDevice(device=dev, xinput_id=bottom_id)
    except Exception as ex:
        log(f"Could not open {node_path}: {ex}")
        return None


def find_xinput_id_by_name(name: str) -> Optional[int]:
    display = os.environ.get("DISPLAY", ":0")
    try:
        output = subprocess.check_output(
            ["xinput", "list"], env={**os.environ, "DISPLAY": display},
            text=True, timeout=5
        )
    except Exception:
        return None

    matches = []
    for line in output.splitlines():
        if name in line:
            m = re.search(r"id=(\d+)", line)
            if m:
                matches.append(int(m.group(1)))
    return max(matches) if matches else None


def set_xinput_device_enabled(device_id: int, enabled: bool):
    display = os.environ.get("DISPLAY", ":0")
    state = "1" if enabled else "0"
    try:
        subprocess.check_call(
            ["xinput", "set-prop", str(device_id), "Device Enabled", state],
            env={**os.environ, "DISPLAY": display},
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            timeout=3,
        )
    except Exception as ex:
        log(f"Could not set Device Enabled={state} on id={device_id}: {ex}")


def apply_bottom_ctm(device_id: int, geo: dict):
    display = os.environ.get("DISPLAY", ":0")
    virt_w = geo.get("virt_w", 1920)
    virt_h = geo.get("virt_h", 2160)
    bot_x = geo.get("bot_x", 0)
    bot_y = geo.get("bot_y", 1080)
    bot_w = geo.get("bot_w", 1920)
    bot_h = geo.get("bot_h", 1080)

    if virt_w <= 0 or virt_h <= 0:
        return

    ctm = [
        bot_w / float(virt_w), 0.0, bot_x / float(virt_w),
        0.0, bot_h / float(virt_h), bot_y / float(virt_h),
        0.0, 0.0, 1.0,
    ]
    vals = [f"{x:.10f}" for x in ctm]

    try:
        subprocess.check_call(
            ["xinput", "set-prop", str(device_id), "Coordinate Transformation Matrix", *vals],
            env={**os.environ, "DISPLAY": display},
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            timeout=3,
        )
        log(f"Applied bottom CTM to id={device_id}: {' '.join(vals)}")
    except Exception as ex:
        log(f"Failed to apply CTM to id={device_id}: {ex}")


def get_abs_info(dev: InputDevice, axis_code: int) -> Optional[AbsInfo]:
    caps = dev.capabilities(absinfo=True)
    for code_info in caps.get(e.EV_ABS, []):
        if isinstance(code_info, tuple) and len(code_info) == 2:
            code, info = code_info
            if code == axis_code:
                return info
    return None


def create_proxy(source: InputDevice, preserve_input_props: bool) -> UInput:
    """
    Create a proxy uinput device with source capabilities.

    For touch we preserve input_props (including INPUT_PROP_DIRECT) and use CTM.
    For absolute mouse we omit input_props and remap coords in evdev units.
    """
    caps = source.capabilities(absinfo=True)
    new_caps = {}

    for ev_type, codes in caps.items():
        if ev_type == e.EV_SYN:
            continue
        new_caps[ev_type] = codes

    kwargs = {}
    if preserve_input_props:
        kwargs["input_props"] = source.input_props()

    proxy = UInput(
        events=new_caps,
        name=f"{source.name} (remapped)",
        vendor=source.info.vendor,
        product=source.info.product,
        version=source.info.version,
        bustype=source.info.bustype,
        **kwargs,
    )

    if preserve_input_props:
        log(f"Created proxy: {proxy.name} (preserving input_props)")
    else:
        log(f"Created proxy: {proxy.name} (no INPUT_PROP_DIRECT)")

    return proxy


def remap(value: int, src_max: int, dst_min: int, dst_span: int) -> int:
    """
    Scale from [0, src_max] into [dst_min, dst_min + dst_span], then clamp.
    """
    if src_max <= 0:
        return dst_min

    scaled = dst_min + int(round((value / float(src_max)) * dst_span))
    hi = dst_min + dst_span
    if scaled < dst_min:
        return dst_min
    if scaled > hi:
        return hi
    return scaled


def proxy_loop(source: InputDevice, proxy: UInput,
               axis_info: dict,
               x_map: tuple[int, int], y_map: tuple[int, int],
               remap_coords: bool,
               verbose: bool = False):
    for event in source.read_loop():
        if event.type == e.EV_ABS:
            if remap_coords and event.code in X_CODES and event.code in axis_info:
                info = axis_info[event.code]
                val = remap(event.value, info.max, x_map[0], x_map[1])
                if verbose:
                    log(f"X({event.code}): {event.value} -> {val}")
                proxy.write(e.EV_ABS, event.code, val)
            elif remap_coords and event.code in Y_CODES and event.code in axis_info:
                info = axis_info[event.code]
                val = remap(event.value, info.max, y_map[0], y_map[1])
                if verbose:
                    log(f"Y({event.code}): {event.value} -> {val}")
                proxy.write(e.EV_ABS, event.code, val)
            else:
                proxy.write(event.type, event.code, event.value)
        elif event.type == e.EV_SYN:
            proxy.syn()
        else:
            proxy.write(event.type, event.code, event.value)


def run_device_proxy(options: ProxyOptions, verbose: bool = False):
    source = None
    proxy = None
    source_xinput_id = None

    log(f"Waiting for bottom device matching '{options.name_pattern}'...")
    while source is None:
        found = find_bottom_device(options.name_pattern)
        if found is not None:
            source = found.device
            source_xinput_id = found.xinput_id
        else:
            time.sleep(2)

    axis_info = {}
    for code in (e.ABS_X, e.ABS_Y, e.ABS_MT_POSITION_X, e.ABS_MT_POSITION_Y):
        info = get_abs_info(source, code)
        if info:
            axis_info[code] = info

    log("Source axes: " + ", ".join(
        f"{e.ABS.get(c, c)}=[{i.min},{i.max}]" for c, i in axis_info.items()
    ))

    geo = get_display_geometry()
    if not geo:
        geo = {
            "virt_w": 1920,
            "virt_h": 2160,
            "top_w": 1920,
            "top_h": 1080,
            "top_x": 0,
            "top_y": 0,
            "bot_w": 1920,
            "bot_h": 1080,
            "bot_x": 0,
            "bot_y": 1080,
        }

    virt_w = geo.get("virt_w", 1920)
    virt_h = geo.get("virt_h", 2160)
    bot_x = geo.get("bot_x", 0)
    bot_y = geo.get("bot_y", 1080)
    bot_w = geo.get("bot_w", 1920)
    bot_h = geo.get("bot_h", 1080)

    y_axis_max = axis_info.get(e.ABS_Y, axis_info.get(e.ABS_MT_POSITION_Y))
    x_axis_max = axis_info.get(e.ABS_X, axis_info.get(e.ABS_MT_POSITION_X))
    y_max = y_axis_max.max if y_axis_max else 10800
    x_max = x_axis_max.max if x_axis_max else 19200

    x_min = int(round(bot_x / virt_w * x_max)) if virt_w > 0 else 0
    y_min = int(round(bot_y / virt_h * y_max)) if virt_h > 0 else 0
    x_span = int(round(bot_w / virt_w * x_max)) if virt_w > 0 else x_max
    y_span = int(round(bot_h / virt_h * y_max)) if virt_h > 0 else y_max

    log(f"Bottom display: {bot_w}x{bot_h}+{bot_x}+{bot_y} in {virt_w}x{virt_h} desktop")
    if options.remap_coords:
        log(f"Mapping: X=[{x_min},{x_min + x_span}] of max={x_max}, "
            f"Y=[{y_min},{y_min + y_span}] of max={y_max}")

    proxy = create_proxy(source, preserve_input_props=options.preserve_input_props)
    time.sleep(0.3)

    if options.disable_source_xinput and source_xinput_id is not None:
        set_xinput_device_enabled(source_xinput_id, False)
        log(f"Disabled source xinput id={source_xinput_id}")

    if options.grab_source:
        source.grab()
        log(f"Grabbed {source.name} — proxy active")
    else:
        log(f"Proxying {source.name} without evdev grab — proxy active")

    if options.preserve_input_props:
        proxy_id = None
        for _ in range(12):
            proxy_id = find_xinput_id_by_name(proxy.name)
            if proxy_id is not None:
                break
            time.sleep(0.25)
        if proxy_id is not None:
            apply_bottom_ctm(proxy_id, geo)
        else:
            log(f"Could not resolve xinput id for proxy '{proxy.name}'")

    proxy_loop(
        source,
        proxy,
        axis_info,
        (x_min, x_span),
        (y_min, y_span),
        options.remap_coords,
        verbose,
    )

    return source, proxy, source_xinput_id


def main():
    import threading

    verbose = "--verbose" in sys.argv or "-v" in sys.argv

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

    log("Waiting for Sunshine input devices...")
    time.sleep(5)

    geo = get_display_geometry()
    if not geo:
        log("WARNING: Could not get display geometry, using defaults")
        geo = {
            "virt_w": 1920,
            "virt_h": 2160,
            "top_w": 1920,
            "top_h": 1080,
            "top_x": 0,
            "top_y": 0,
            "bot_w": 1920,
            "bot_h": 1080,
            "bot_x": 0,
            "bot_y": 1080,
        }
    log(f"Display geometry: {geo}")

    sources = []
    proxies = []
    disabled_source_ids = set()

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
        for source_id in sorted(disabled_source_ids):
            set_xinput_device_enabled(source_id, True)
        sys.exit(0)

    signal.signal(signal.SIGINT, cleanup)
    signal.signal(signal.SIGTERM, cleanup)

    proxy_options = [
        ProxyOptions(
            name_pattern="Mouse passthrough (absolute)",
            remap_coords=True,
            preserve_input_props=False,
            grab_source=True,
            disable_source_xinput=False,
        ),
        ProxyOptions(
            name_pattern="Touch passthrough",
            remap_coords=False,
            preserve_input_props=True,
            grab_source=False,
            disable_source_xinput=True,
        ),
    ]

    threads = []
    for options in proxy_options:
        def run(opts=options):
            while True:
                src = None
                prx = None
                src_id = None
                try:
                    src, prx, src_id = run_device_proxy(opts, verbose)
                    sources.append(src)
                    proxies.append(prx)
                    if opts.disable_source_xinput and src_id is not None:
                        disabled_source_ids.add(src_id)
                except Exception as ex:
                    log(f"Proxy for '{opts.name_pattern}' failed: {ex}")
                finally:
                    if src_id is not None and opts.disable_source_xinput:
                        set_xinput_device_enabled(src_id, True)
                        if src_id in disabled_source_ids:
                            disabled_source_ids.remove(src_id)
                    if prx is not None:
                        try:
                            prx.close()
                        except Exception:
                            pass
                    if src is not None:
                        try:
                            src.ungrab()
                        except Exception:
                            pass
                    log(f"Reconnecting '{opts.name_pattern}' in 3s...")
                    time.sleep(3)

        t = threading.Thread(target=run, daemon=True)
        t.start()
        threads.append(t)

    for t in threads:
        t.join()

    log("All proxies exited")


if __name__ == "__main__":
    main()
