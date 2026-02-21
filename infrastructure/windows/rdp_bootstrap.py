#!/usr/bin/env python3
"""Pure-Python RDP bootstrap for TensorDock Windows hosts.

Uses aardwolf RDP automation to:
1) Log in via RDP on 3390
2) Launch *elevated* PowerShell (UAC accepted through keyboard flow)
3) Enable OpenSSH + WinRM + firewall rules
4) Drop bootstrap completion marker
"""

from __future__ import annotations

import argparse
import asyncio
import json
import socket
import time
import urllib.parse
from pathlib import Path

from aardwolf.commons.factory import RDPConnectionFactory
from aardwolf.commons.iosettings import RDPIOSettings
from aardwolf.commons.queuedata.constants import MOUSEBUTTON, VIDEO_FORMAT
from aardwolf.commons.queuedata.keyboard import RDP_KEYBOARD_SCANCODE, RDP_KEYBOARD_UNICODE
from aardwolf.commons.queuedata.mouse import RDP_MOUSE

STATE_DEFAULT = Path(__file__).resolve().parent / "state" / "windows-vm.local.json"


def load_state(path: Path) -> dict:
    if not path.exists():
        raise SystemExit(f"State file missing: {path}")
    return json.loads(path.read_text(encoding="utf-8"))


class RDPSession:
    def __init__(self, conn):
        self.conn = conn

    async def move(self, x: int, y: int) -> None:
        evt = RDP_MOUSE()
        evt.button = MOUSEBUTTON.MOUSEBUTTON_HOVER
        evt.is_pressed = False
        evt.xPos = x
        evt.yPos = y
        await self.conn.ext_in_queue.put(evt)

    async def click_left(self, x: int, y: int, delay: float = 0.06) -> None:
        await self.move(x, y)
        await asyncio.sleep(delay)

        down = RDP_MOUSE()
        down.button = MOUSEBUTTON.MOUSEBUTTON_LEFT
        down.is_pressed = True
        down.xPos = x
        down.yPos = y
        await self.conn.ext_in_queue.put(down)

        await asyncio.sleep(delay)

        up = RDP_MOUSE()
        up.button = MOUSEBUTTON.MOUSEBUTTON_LEFT
        up.is_pressed = False
        up.xPos = x
        up.yPos = y
        await self.conn.ext_in_queue.put(up)

    async def key_vk(self, name: str, pressed: bool, *, extended: bool = False, keycode: int | None = None) -> None:
        evt = RDP_KEYBOARD_SCANCODE()
        evt.vk_code = name
        evt.is_pressed = pressed
        evt.is_extended = extended
        if keycode is not None:
            evt.keyCode = keycode
        await self.conn.ext_in_queue.put(evt)

    async def tap_vk(self, name: str, *, delay: float = 0.06) -> None:
        await self.key_vk(name, True)
        await asyncio.sleep(delay)
        await self.key_vk(name, False)

    async def press_enter(self) -> None:
        await self.tap_vk("VK_RETURN")

    async def type_text(self, text: str, key_delay: float = 0.01) -> None:
        for ch in text:
            evt_down = RDP_KEYBOARD_UNICODE()
            evt_down.char = ch
            evt_down.is_pressed = True
            await self.conn.ext_in_queue.put(evt_down)

            evt_up = RDP_KEYBOARD_UNICODE()
            evt_up.char = ch
            evt_up.is_pressed = False
            await self.conn.ext_in_queue.put(evt_up)
            await asyncio.sleep(key_delay)

    async def run_ps_line(self, line: str, *, wait_after: float = 0.9) -> None:
        await self.type_text(line)
        await self.press_enter()
        await asyncio.sleep(wait_after)

    async def open_start_and_launch_powershell(self) -> None:
        # Win key -> start search -> powershell -> enter
        await self.tap_vk("VK_LWIN")
        await asyncio.sleep(0.8)
        await self.type_text("powershell")
        await asyncio.sleep(0.2)
        await self.press_enter()
        await asyncio.sleep(1.8)

    async def elevate_current_powershell(self) -> None:
        # Start-Process ... -Verb RunAs + UAC accept flow.
        await self.run_ps_line("Start-Process PowerShell -Verb RunAs", wait_after=1.5)

        # UAC confirmation flow. In this Windows image, LEFT then ENTER confirms.
        await self.tap_vk("VK_LEFT")
        await asyncio.sleep(0.2)
        await self.tap_vk("VK_RETURN")
        await asyncio.sleep(2.4)

        # Backup acceptance attempt: Alt+Y using scancode for 'y' (21)
        await self.key_vk("VK_LMENU", True)
        await self.key_vk(None, True, keycode=21)
        await asyncio.sleep(0.05)
        await self.key_vk(None, False, keycode=21)
        await self.key_vk("VK_LMENU", False)
        await asyncio.sleep(1.3)


def wait_for_tcp(ip: str, port: int, timeout_s: int = 45) -> bool:
    deadline = time.time() + timeout_s
    while time.time() < deadline:
        s = socket.socket()
        s.settimeout(2)
        try:
            s.connect((ip, port))
            s.close()
            return True
        except Exception:
            s.close()
            time.sleep(1.5)
    return False


async def run_bootstrap(
    ip: str,
    username: str,
    password: str,
    width: int,
    height: int,
    out_dir: Path,
    rdp_port: int,
) -> None:
    out_dir.mkdir(parents=True, exist_ok=True)

    ios = RDPIOSettings()
    ios.video_width = width
    ios.video_height = height
    ios.video_bpp_min = 15
    ios.video_bpp_max = 32
    ios.clipboard_use_pyperclip = False

    enc_pw = urllib.parse.quote(password, safe="")
    url = f"rdp+ntlm-password://{username}:{enc_pw}@{ip}:{rdp_port}"
    factory = RDPConnectionFactory.from_url(url, ios)

    async with factory.create_connection_newtarget(ip, ios) as conn:
        ok, err = await conn.connect()
        if err is not None or not ok:
            raise SystemExit(f"RDP login failed: {err}")

        session = RDPSession(conn)
        await asyncio.sleep(1.8)

        shot1 = out_dir / f"step-1-desktop-{int(time.time())}.png"
        conn.get_desktop_buffer(VIDEO_FORMAT.PIL).save(shot1)
        print(f"Saved {shot1}")

        await session.open_start_and_launch_powershell()
        shot2 = out_dir / f"step-2-powershell-{int(time.time())}.png"
        conn.get_desktop_buffer(VIDEO_FORMAT.PIL).save(shot2)
        print(f"Saved {shot2}")

        await session.elevate_current_powershell()
        shot3 = out_dir / f"step-3-elevated-{int(time.time())}.png"
        conn.get_desktop_buffer(VIDEO_FORMAT.PIL).save(shot3)
        print(f"Saved {shot3}")

        commands = [
            "$ErrorActionPreference='Continue'",
            "New-Item -Path C:\\ProgramData\\gamer -ItemType Directory -Force | Out-Null",
            "'rdp bootstrap started' | Set-Content C:\\ProgramData\\gamer\\rdp-bootstrap.log",
            "whoami /all | Out-File C:\\ProgramData\\gamer\\whoami.txt",
            # OpenSSH
            "Add-WindowsCapability -Online -Name OpenSSH.Server~~~~0.0.1.0",
            "Set-Service -Name sshd -StartupType Automatic",
            "Start-Service sshd",
            "Set-Service -Name ssh-agent -StartupType Manual",
            "if (-Not (Test-Path 'C:\\ProgramData\\ssh\\sshd_config')) { New-Item -Path 'C:\\ProgramData\\ssh' -ItemType Directory -Force | Out-Null }",
            # WinRM
            "winrm quickconfig -q",
            "Set-Service -Name WinRM -StartupType Automatic",
            "Start-Service WinRM",
            # Firewall
            'netsh advfirewall firewall add rule name="OpenSSH Server (sshd)" dir=in action=allow protocol=TCP localport=22',
            'netsh advfirewall firewall add rule name="WinRM HTTP 5985" dir=in action=allow protocol=TCP localport=5985',
            # verification markers
            "Get-Service sshd,WinRM | Format-Table -AutoSize | Out-File C:\\ProgramData\\gamer\\services.txt",
            "netstat -ano | findstr :22 | Out-File C:\\ProgramData\\gamer\\port22.txt",
            "netstat -ano | findstr :5985 | Out-File C:\\ProgramData\\gamer\\port5985.txt",
            "'rdp bootstrap completed' | Add-Content C:\\ProgramData\\gamer\\rdp-bootstrap.log",
        ]

        for cmd in commands:
            print(f"Running: {cmd}")
            await session.run_ps_line(cmd, wait_after=1.1)

        await asyncio.sleep(2.0)
        shot4 = out_dir / f"step-4-complete-{int(time.time())}.png"
        conn.get_desktop_buffer(VIDEO_FORMAT.PIL).save(shot4)
        print(f"Saved {shot4}")

    print("Verifying remote management ports...")
    ssh_ok = wait_for_tcp(ip, 22, timeout_s=60)
    winrm_ok = wait_for_tcp(ip, 5985, timeout_s=60)
    print(f"TCP 22 reachable: {ssh_ok}")
    print(f"TCP 5985 reachable: {winrm_ok}")
    if not (ssh_ok and winrm_ok):
        raise SystemExit("Bootstrap finished, but TCP 22/5985 not reachable yet.")



def parse_args() -> argparse.Namespace:
    p = argparse.ArgumentParser(description="Bootstrap Windows host via pure Python RDP automation")
    p.add_argument("--state-file", type=Path, default=STATE_DEFAULT)
    p.add_argument("--ip")
    p.add_argument("--username", default="user")
    p.add_argument("--password")
    p.add_argument("--rdp-port", type=int, default=3390)
    p.add_argument("--width", type=int, default=1280)
    p.add_argument("--height", type=int, default=720)
    p.add_argument("--out-dir", type=Path, default=Path("/tmp/windows-rdp-bootstrap"))
    return p.parse_args()


if __name__ == "__main__":
    args = parse_args()
    state = load_state(args.state_file)
    ip = args.ip or state.get("ip")
    username = args.username
    password = args.password or state.get("password")

    if not ip or not password:
        raise SystemExit("Missing ip/password. Provide --ip --password or valid state file.")

    asyncio.run(
        run_bootstrap(
            ip=ip,
            username=username,
            password=password,
            width=args.width,
            height=args.height,
            out_dir=args.out_dir,
            rdp_port=args.rdp_port,
        )
    )
