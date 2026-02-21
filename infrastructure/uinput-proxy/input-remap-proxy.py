#!/usr/bin/env python3
"""
uinput coordinate remapping proxy for dual-monitor Sunshine setups.

Problem: Sunshine captures a single display (e.g., DP-2 at position 0,1080 in a stacked
dual-monitor setup). When a client touches the center of the streamed display, Sunshine
writes absolute coordinates to a uinput device as if the input covers the full virtual
desktop (0 to total_height). We need to remap these coordinates so they land only in
the target display region.

This script:
1. Finds Sunshine's absolute mouse uinput device by name
2. Grabs it exclusively (so the raw events don't reach X/Wayland)
3. Reads ABS_X/ABS_Y events
4. Transforms Y coordinates: maps full-range [0, ABS_MAX] -> target region [y_offset, y_offset + display_height]
5. Writes transformed events to a new uinput device

Usage:
    sudo python3 input-remap-proxy.py [options]

Examples:
    # Stacked monitors: bottom display at y=1080, each 1920x1080
    sudo python3 input-remap-proxy.py --target-y-offset 1080 --target-height 1080 --target-width 1920

    # Custom device name pattern
    sudo python3 input-remap-proxy.py --device-name "Mouse passthrough" --target-y-offset 1080

    # Also remap X for side-by-side or offset displays
    sudo python3 input-remap-proxy.py --target-x-offset 1920 --target-width 1920 --target-y-offset 0 --target-height 1080
"""

import argparse
import signal
import sys
import time
from typing import Optional

import evdev
from evdev import InputDevice, UInput, AbsInfo, ecodes as e, list_devices


def find_device(name_pattern: str) -> Optional[InputDevice]:
    """
    Find an evdev input device whose name contains the given pattern.

    Sunshine/inputtino creates devices with names like:
    - "Sunshine Mouse (absolute)" for absolute mouse
    - "Sunshine Touch" for touchscreen
    - "Wolf Mouse (absolute)" for Wolf's absolute mouse
    - "Mouse passthrough (absolute)" — naming depends on version

    Args:
        name_pattern: Substring to match against device names (case-insensitive)

    Returns:
        InputDevice if found, None otherwise
    """
    for path in list_devices():
        try:
            dev = InputDevice(path)
            if name_pattern.lower() in dev.name.lower():
                print(f"Found device: {dev.name} at {dev.path}")
                print(f"  phys: {dev.phys}")

                # Verify it has ABS_X and ABS_Y
                caps = dev.capabilities(absinfo=True)
                abs_caps = caps.get(e.EV_ABS, [])
                abs_codes = [code if isinstance(code, int) else code[0] for code in abs_caps]

                if e.ABS_X in abs_codes and e.ABS_Y in abs_codes:
                    print(f"  Has ABS_X and ABS_Y - suitable for remapping")
                    return dev
                else:
                    print(f"  No ABS_X/ABS_Y - skipping")
        except (PermissionError, OSError) as ex:
            continue
    return None


def get_abs_info(dev: InputDevice, axis_code: int) -> Optional[AbsInfo]:
    """
    Get AbsInfo for a specific axis from the device capabilities.

    Args:
        dev: The input device
        axis_code: e.g., e.ABS_X or e.ABS_Y

    Returns:
        AbsInfo namedtuple or None
    """
    caps = dev.capabilities(absinfo=True)
    for code_info in caps.get(e.EV_ABS, []):
        if isinstance(code_info, tuple) and len(code_info) == 2:
            code, info = code_info
            if code == axis_code:
                return info
    return None


def create_proxy_device(
    source_dev: InputDevice,
    virtual_width: int,
    virtual_height: int,
    name_suffix: str = "remapped"
) -> UInput:
    """
    Create a new UInput device that mirrors the source device's capabilities
    but with adjusted absolute axis ranges to cover the full virtual desktop.

    The proxy device's ABS range covers the full virtual desktop so that
    when we write offset coordinates, they map correctly.

    Args:
        source_dev: The original input device to mirror
        virtual_width: Total virtual desktop width
        virtual_height: Total virtual desktop height
        name_suffix: Suffix for the new device name

    Returns:
        UInput virtual device
    """
    caps = source_dev.capabilities(absinfo=True)

    # Build new capabilities with adjusted axis ranges
    new_caps = {}
    for ev_type, codes in caps.items():
        # Skip SYN events (handled automatically by UInput)
        if ev_type == e.EV_SYN:
            continue

        if ev_type == e.EV_ABS:
            new_abs = []
            for code_info in codes:
                if isinstance(code_info, tuple) and len(code_info) == 2:
                    code, info = code_info
                    if code == e.ABS_X:
                        # Set range to full virtual desktop width
                        new_info = AbsInfo(
                            value=0,
                            min=0,
                            max=virtual_width,
                            fuzz=info.fuzz,
                            flat=info.flat,
                            resolution=info.resolution
                        )
                        new_abs.append((code, new_info))
                    elif code == e.ABS_Y:
                        # Set range to full virtual desktop height
                        new_info = AbsInfo(
                            value=0,
                            min=0,
                            max=virtual_height,
                            fuzz=info.fuzz,
                            flat=info.flat,
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

    # Get input properties from source (e.g., INPUT_PROP_DIRECT for touchscreens)
    input_props = None
    try:
        props = source_dev.input_props()
        if props:
            input_props = list(props)
    except Exception:
        pass

    ui = UInput(
        events=new_caps,
        name=f"{source_dev.name} ({name_suffix})",
        vendor=source_dev.info.vendor,
        product=source_dev.info.product,
        version=source_dev.info.version,
        bustype=source_dev.info.bustype,
        input_props=input_props
    )

    print(f"Created proxy device: {ui.name}")
    print(f"  devnode: {ui.device.path}")
    return ui


def remap_coordinate(value: int, src_min: int, src_max: int,
                     dst_offset: int, dst_size: int,
                     dst_total: int) -> int:
    """
    Remap an absolute coordinate from source range to a sub-region of the destination.

    The source device reports values in [src_min, src_max] representing the full
    client surface. We need to map that to [dst_offset, dst_offset + dst_size]
    within the total virtual desktop of [0, dst_total].

    Args:
        value: Raw coordinate from source device
        src_min: Minimum value of source axis
        src_max: Maximum value of source axis
        dst_offset: Pixel offset of the target region in the virtual desktop
        dst_size: Pixel size of the target region
        dst_total: Total virtual desktop size (used as the proxy device's axis range)

    Returns:
        Remapped coordinate value

    Example:
        # Client sends y=6000 on a device with range [0, 12000]
        # Target is bottom display at y=1080, height=1080, total=2160
        # Normalized: 6000/12000 = 0.5 (center of client)
        # Mapped: 1080 + 0.5 * 1080 = 1620 (center of bottom display)
        # Scaled to proxy range: 1620/2160 * proxy_max
    """
    # Normalize to [0.0, 1.0]
    src_range = src_max - src_min
    if src_range == 0:
        return dst_offset

    normalized = (value - src_min) / src_range

    # Map to target region in pixel space
    pixel_coord = dst_offset + normalized * dst_size

    # Scale to proxy device axis range [0, dst_total]
    # (The proxy device's ABS range is set to dst_total)
    return int(round(pixel_coord / dst_total * dst_total))
    # Simplifies to: int(round(pixel_coord)) since dst_total cancels out,
    # but keeping it explicit for when proxy ABS max != dst_total


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
    """
    Main proxy loop. Finds the source device, grabs it, creates a proxy,
    and forwards events with coordinate remapping.

    Args:
        device_name: Pattern to match the source device name
        target_x_offset: X pixel offset of target display in virtual desktop
        target_y_offset: Y pixel offset of target display in virtual desktop
        target_width: Width of the target display in pixels
        target_height: Height of the target display in pixels
        virtual_width: Total virtual desktop width
        virtual_height: Total virtual desktop height
        retry_interval: Seconds between device discovery retries
        verbose: Print every event if True
    """
    source_dev = None
    proxy_dev = None

    def cleanup(sig=None, frame=None):
        nonlocal source_dev, proxy_dev
        print("\nCleaning up...")
        if source_dev:
            try:
                source_dev.ungrab()
                print("Released grab on source device")
            except Exception:
                pass
        if proxy_dev:
            try:
                proxy_dev.close()
                print("Closed proxy device")
            except Exception:
                pass
        sys.exit(0)

    signal.signal(signal.SIGINT, cleanup)
    signal.signal(signal.SIGTERM, cleanup)

    # Wait for the device to appear (Sunshine creates it when a client connects)
    print(f"Searching for device matching: '{device_name}'")
    while source_dev is None:
        source_dev = find_device(device_name)
        if source_dev is None:
            print(f"  Device not found, retrying in {retry_interval}s...")
            time.sleep(retry_interval)

    # Read source axis info
    src_x_info = get_abs_info(source_dev, e.ABS_X)
    src_y_info = get_abs_info(source_dev, e.ABS_Y)

    if not src_x_info or not src_y_info:
        print("ERROR: Could not read ABS_X/ABS_Y info from source device")
        sys.exit(1)

    print(f"\nSource device axis ranges:")
    print(f"  ABS_X: [{src_x_info.min}, {src_x_info.max}]")
    print(f"  ABS_Y: [{src_y_info.min}, {src_y_info.max}]")
    print(f"\nTarget region: ({target_x_offset}, {target_y_offset}) "
          f"size {target_width}x{target_height}")
    print(f"Virtual desktop: {virtual_width}x{virtual_height}")

    # Create proxy device with axis ranges covering the full virtual desktop
    proxy_dev = create_proxy_device(source_dev, virtual_width, virtual_height)

    # Give udev a moment to set up the new device
    time.sleep(0.5)

    # Grab the source device so its raw events don't reach the display server
    print("Grabbing source device...")
    source_dev.grab()
    print("Source device grabbed exclusively\n")

    print("=== Proxy active. Forwarding events with coordinate remapping ===\n")

    try:
        for event in source_dev.read_loop():
            if event.type == e.EV_ABS:
                if event.code == e.ABS_X:
                    new_val = remap_coordinate(
                        event.value,
                        src_x_info.min, src_x_info.max,
                        target_x_offset, target_width,
                        virtual_width
                    )
                    if verbose:
                        print(f"  ABS_X: {event.value} -> {new_val}")
                    proxy_dev.write(e.EV_ABS, e.ABS_X, new_val)

                elif event.code == e.ABS_Y:
                    new_val = remap_coordinate(
                        event.value,
                        src_y_info.min, src_y_info.max,
                        target_y_offset, target_height,
                        virtual_height
                    )
                    if verbose:
                        print(f"  ABS_Y: {event.value} -> {new_val}")
                    proxy_dev.write(e.EV_ABS, e.ABS_Y, new_val)

                else:
                    # Pass through other ABS events (MT slots, pressure, etc.)
                    proxy_dev.write(event.type, event.code, event.value)

            elif event.type == e.EV_SYN:
                proxy_dev.syn()

            else:
                # Pass through all non-ABS events (KEY, MSC, etc.)
                proxy_dev.write(event.type, event.code, event.value)

    except OSError as ex:
        print(f"\nDevice error (likely disconnected): {ex}")
        cleanup()


def main():
    parser = argparse.ArgumentParser(
        description="uinput proxy that remaps absolute coordinates to a target display region",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:

  # Stacked monitors, bottom display (each 1920x1080):
  sudo python3 input-remap-proxy.py \\
      --target-y-offset 1080 \\
      --target-height 1080 \\
      --target-width 1920 \\
      --virtual-height 2160

  # Side-by-side monitors, right display:
  sudo python3 input-remap-proxy.py \\
      --target-x-offset 1920 \\
      --target-width 1920 \\
      --target-height 1080 \\
      --virtual-width 3840

  # Custom Sunshine device name:
  sudo python3 input-remap-proxy.py \\
      --device-name "Mouse passthrough" \\
      --target-y-offset 1080
        """
    )

    parser.add_argument(
        "--device-name",
        default="absolute",
        help="Substring to match in device name (default: 'absolute'). "
             "Common values: 'absolute', 'Mouse passthrough', 'Touch passthrough', "
             "'Sunshine Mouse'"
    )
    parser.add_argument(
        "--target-x-offset",
        type=int, default=0,
        help="X pixel offset of target display in virtual desktop (default: 0)"
    )
    parser.add_argument(
        "--target-y-offset",
        type=int, default=1080,
        help="Y pixel offset of target display in virtual desktop (default: 1080)"
    )
    parser.add_argument(
        "--target-width",
        type=int, default=1920,
        help="Width of target display in pixels (default: 1920)"
    )
    parser.add_argument(
        "--target-height",
        type=int, default=1080,
        help="Height of target display in pixels (default: 1080)"
    )
    parser.add_argument(
        "--virtual-width",
        type=int, default=1920,
        help="Total virtual desktop width in pixels (default: 1920)"
    )
    parser.add_argument(
        "--virtual-height",
        type=int, default=2160,
        help="Total virtual desktop height in pixels (default: 2160)"
    )
    parser.add_argument(
        "--retry-interval",
        type=float, default=2.0,
        help="Seconds between device discovery retries (default: 2.0)"
    )
    parser.add_argument(
        "--verbose", "-v",
        action="store_true",
        help="Print every remapped coordinate"
    )

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
