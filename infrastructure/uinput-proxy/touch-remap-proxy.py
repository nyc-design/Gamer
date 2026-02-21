#!/usr/bin/env python3
"""
uinput proxy for remapping multitouch absolute coordinates to a target display region.

This handles Sunshine's Touch passthrough device which uses ABS_MT_POSITION_X/Y
in addition to ABS_X/ABS_Y. The multitouch protocol (type B) uses ABS_MT_SLOT
and ABS_MT_TRACKING_ID to manage multiple fingers.

The remapping logic is the same as the mouse version, but this script also
transforms ABS_MT_POSITION_X and ABS_MT_POSITION_Y events.

Usage:
    sudo python3 touch-remap-proxy.py --target-y-offset 1080 --target-height 1080

For single-touch/absolute mouse, use input-remap-proxy.py instead.
"""

import argparse
import signal
import sys
import time
from typing import Optional

import evdev
from evdev import InputDevice, UInput, AbsInfo, ecodes as e, list_devices


# All the ABS codes that represent X coordinates
X_CODES = {e.ABS_X, e.ABS_MT_POSITION_X}
# All the ABS codes that represent Y coordinates
Y_CODES = {e.ABS_Y, e.ABS_MT_POSITION_Y}


def find_device(name_pattern: str) -> Optional[InputDevice]:
    """Find an evdev device whose name contains the pattern."""
    for path in list_devices():
        try:
            dev = InputDevice(path)
            if name_pattern.lower() in dev.name.lower():
                caps = dev.capabilities(absinfo=True)
                abs_caps = caps.get(e.EV_ABS, [])
                abs_codes = set()
                for item in abs_caps:
                    if isinstance(item, tuple) and len(item) == 2:
                        abs_codes.add(item[0])
                    elif isinstance(item, int):
                        abs_codes.add(item)

                # Must have at least ABS_X and ABS_Y
                if e.ABS_X in abs_codes and e.ABS_Y in abs_codes:
                    has_mt = e.ABS_MT_POSITION_X in abs_codes
                    print(f"Found device: {dev.name} at {dev.path}")
                    print(f"  Multitouch: {'yes' if has_mt else 'no'}")
                    return dev
        except (PermissionError, OSError):
            continue
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


def build_proxy_capabilities(source_dev: InputDevice,
                             virtual_width: int,
                             virtual_height: int) -> dict:
    """
    Build UInput capabilities dict from source device, adjusting
    all X/Y axis ranges to cover the full virtual desktop.
    """
    caps = source_dev.capabilities(absinfo=True)
    new_caps = {}

    for ev_type, codes in caps.items():
        if ev_type == e.EV_SYN:
            continue

        if ev_type == e.EV_ABS:
            new_abs = []
            for code_info in codes:
                if isinstance(code_info, tuple) and len(code_info) == 2:
                    code, info = code_info
                    if code in X_CODES:
                        new_info = AbsInfo(
                            value=0, min=0, max=virtual_width,
                            fuzz=info.fuzz, flat=info.flat,
                            resolution=info.resolution
                        )
                        new_abs.append((code, new_info))
                    elif code in Y_CODES:
                        new_info = AbsInfo(
                            value=0, min=0, max=virtual_height,
                            fuzz=info.fuzz, flat=info.flat,
                            resolution=info.resolution
                        )
                        new_abs.append((code, new_info))
                    else:
                        new_abs.append((code, info))
                else:
                    new_abs.append(code_info)
            new_caps[ev_type] = new_abs
        else:
            new_caps[ev_type] = codes

    return new_caps


def remap(value: int, src_min: int, src_max: int,
          dst_offset: int, dst_size: int) -> int:
    """
    Remap a coordinate from [src_min, src_max] to the sub-region
    [dst_offset, dst_offset + dst_size] in pixel space.

    Returns the pixel coordinate directly (proxy device axis max = virtual desktop size).
    """
    src_range = src_max - src_min
    if src_range == 0:
        return dst_offset
    normalized = (value - src_min) / src_range
    return int(round(dst_offset + normalized * dst_size))


def run_proxy(
    device_name: str,
    target_x_offset: int,
    target_y_offset: int,
    target_width: int,
    target_height: int,
    virtual_width: int,
    virtual_height: int,
    retry_interval: float = 2.0,
    verbose: bool = False
):
    """Main proxy loop with multitouch support."""
    source_dev = None
    proxy_dev = None

    def cleanup(sig=None, frame=None):
        nonlocal source_dev, proxy_dev
        print("\nCleaning up...")
        try:
            if source_dev:
                source_dev.ungrab()
        except Exception:
            pass
        try:
            if proxy_dev:
                proxy_dev.close()
        except Exception:
            pass
        sys.exit(0)

    signal.signal(signal.SIGINT, cleanup)
    signal.signal(signal.SIGTERM, cleanup)

    print(f"Searching for device matching: '{device_name}'")
    while source_dev is None:
        source_dev = find_device(device_name)
        if source_dev is None:
            print(f"  Not found, retrying in {retry_interval}s...")
            time.sleep(retry_interval)

    # Read source axis ranges for all X/Y axes
    axis_info = {}
    for code in (e.ABS_X, e.ABS_Y, e.ABS_MT_POSITION_X, e.ABS_MT_POSITION_Y):
        info = get_abs_info(source_dev, code)
        if info:
            axis_info[code] = info
            name = e.ABS[code] if code in e.ABS else str(code)
            print(f"  {name}: [{info.min}, {info.max}]")

    # Create proxy
    new_caps = build_proxy_capabilities(source_dev, virtual_width, virtual_height)

    input_props = None
    try:
        props = source_dev.input_props()
        if props:
            input_props = list(props)
    except Exception:
        pass

    proxy_dev = UInput(
        events=new_caps,
        name=f"{source_dev.name} (remapped)",
        vendor=source_dev.info.vendor,
        product=source_dev.info.product,
        version=source_dev.info.version,
        bustype=source_dev.info.bustype,
        input_props=input_props
    )

    print(f"Created proxy: {proxy_dev.name}")
    time.sleep(0.5)

    source_dev.grab()
    print("Source grabbed. Proxy active.\n")

    try:
        for event in source_dev.read_loop():
            if event.type == e.EV_ABS:
                if event.code in X_CODES and event.code in axis_info:
                    info = axis_info[event.code]
                    new_val = remap(event.value, info.min, info.max,
                                    target_x_offset, target_width)
                    if verbose:
                        print(f"  X({event.code}): {event.value} -> {new_val}")
                    proxy_dev.write(e.EV_ABS, event.code, new_val)

                elif event.code in Y_CODES and event.code in axis_info:
                    info = axis_info[event.code]
                    new_val = remap(event.value, info.min, info.max,
                                    target_y_offset, target_height)
                    if verbose:
                        print(f"  Y({event.code}): {event.value} -> {new_val}")
                    proxy_dev.write(e.EV_ABS, event.code, new_val)

                else:
                    proxy_dev.write(event.type, event.code, event.value)

            elif event.type == e.EV_SYN:
                proxy_dev.syn()
            else:
                proxy_dev.write(event.type, event.code, event.value)

    except OSError as ex:
        print(f"\nDevice error: {ex}")
        cleanup()


def main():
    parser = argparse.ArgumentParser(
        description="Multitouch-aware uinput proxy for coordinate remapping"
    )
    parser.add_argument("--device-name", default="touch",
                        help="Substring to match device name (default: 'touch')")
    parser.add_argument("--target-x-offset", type=int, default=0)
    parser.add_argument("--target-y-offset", type=int, default=1080)
    parser.add_argument("--target-width", type=int, default=1920)
    parser.add_argument("--target-height", type=int, default=1080)
    parser.add_argument("--virtual-width", type=int, default=1920)
    parser.add_argument("--virtual-height", type=int, default=2160)
    parser.add_argument("--retry-interval", type=float, default=2.0)
    parser.add_argument("--verbose", "-v", action="store_true")
    args = parser.parse_args()

    run_proxy(
        device_name=args.device_name,
        target_x_offset=args.target_x_offset,
        target_y_offset=args.target_y_offset,
        target_width=args.target_width,
        target_height=args.target_height,
        virtual_width=args.virtual_width,
        virtual_height=args.virtual_height,
        retry_interval=args.retry_interval,
        verbose=args.verbose
    )


if __name__ == "__main__":
    main()
