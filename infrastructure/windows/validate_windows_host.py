#!/usr/bin/env python3
"""Automated validation for a provisioned Windows gaming host.

Checks:
1) Agent health endpoint reachable
2) /start works and processes appear
3) dual-client connect/disconnect bookkeeping works
4) (optional) reboot persistence for SSH + agent health
"""

from __future__ import annotations

import argparse
import json
import socket
import time
from pathlib import Path
from urllib.error import URLError
from urllib.request import Request, urlopen

import paramiko

STATE_DEFAULT = Path(__file__).resolve().parent / "state" / "windows-vm.local.json"


def read_state(path: Path) -> dict:
    if not path.exists():
        raise SystemExit(f"State file missing: {path}")
    return json.loads(path.read_text(encoding="utf-8"))


def post_json(base: str, route: str, payload: dict) -> dict:
    data = json.dumps(payload).encode("utf-8")
    req = Request(
        f"{base}{route}",
        data=data,
        headers={"Content-Type": "application/json"},
        method="POST",
    )
    with urlopen(req, timeout=15) as resp:
        return json.loads(resp.read().decode("utf-8"))


def get_json(base: str, route: str) -> dict:
    with urlopen(f"{base}{route}", timeout=15) as resp:
        return json.loads(resp.read().decode("utf-8"))


def wait_tcp(ip: str, port: int, timeout_s: int) -> bool:
    deadline = time.time() + timeout_s
    while time.time() < deadline:
        try:
            with socket.create_connection((ip, port), timeout=2):
                return True
        except Exception:
            time.sleep(1)
    return False


def wait_tcp_down(ip: str, port: int, timeout_s: int) -> bool:
    deadline = time.time() + timeout_s
    while time.time() < deadline:
        try:
            with socket.create_connection((ip, port), timeout=2):
                pass
            time.sleep(1)
        except Exception:
            return True
    return False


def wait_health(base: str, timeout_s: int = 180) -> dict:
    deadline = time.time() + timeout_s
    last_err = None
    while time.time() < deadline:
        try:
            return get_json(base, "/health")
        except Exception as e:
            last_err = e
            time.sleep(2)
    raise TimeoutError(f"/health not reachable within {timeout_s}s: {last_err}")


def reboot_windows(ip: str, username: str, password: str) -> None:
    ssh = paramiko.SSHClient()
    ssh.set_missing_host_key_policy(paramiko.AutoAddPolicy())
    ssh.connect(ip, port=22, username=username, password=password, timeout=20, auth_timeout=20)
    # Use shutdown.exe for more reliable behavior over non-interactive SSH sessions.
    ssh.exec_command("shutdown /r /t 0 /f")
    ssh.close()


def get_last_boot_time(ip: str, username: str, password: str) -> str:
    ssh = paramiko.SSHClient()
    ssh.set_missing_host_key_policy(paramiko.AutoAddPolicy())
    ssh.connect(ip, port=22, username=username, password=password, timeout=20, auth_timeout=20)
    _, stdout, _ = ssh.exec_command(
        'powershell -NoProfile -Command "(Get-CimInstance Win32_OperatingSystem).LastBootUpTime.ToString(\'o\')"'
    )
    val = stdout.read().decode("utf-8", errors="ignore").strip()
    ssh.close()
    return val


def wait_boot_time_change(ip: str, username: str, password: str, previous: str, timeout_s: int = 300) -> str:
    deadline = time.time() + timeout_s
    last = previous
    while time.time() < deadline:
        try:
            cur = get_last_boot_time(ip, username, password)
            last = cur
            if cur and cur != previous:
                return cur
        except Exception:
            pass
        time.sleep(2)
    return last


def main() -> None:
    p = argparse.ArgumentParser(description="Validate Windows gaming host")
    p.add_argument("--state-file", type=Path, default=STATE_DEFAULT)
    p.add_argument("--ip")
    p.add_argument("--username", default="user")
    p.add_argument("--password")
    p.add_argument("--skip-reboot", action="store_true")
    args = p.parse_args()

    state = read_state(args.state_file)
    ip = args.ip or state.get("ip")
    password = args.password or state.get("password")
    if not ip:
        raise SystemExit("Missing VM IP")
    if not password and not args.skip_reboot:
        raise SystemExit("Missing VM password (needed for reboot validation)")

    base = f"http://{ip}:8081"
    results: dict = {"ip": ip, "checks": []}

    # 1) health
    health = wait_health(base, timeout_s=180)
    results["checks"].append({"name": "health", "ok": True, "data": health})

    # 2) start
    post_json(base, "/stop", {})
    try:
        post_json(base, "/manifest-clear", {})
    except Exception:
        pass
    start = post_json(base, "/start", {})
    health = wait_health(base, timeout_s=30)
    alive = health.get("alive_processes", 0)
    results["checks"].append({"name": "start", "ok": start.get("ok", False), "data": start})
    results["checks"].append({"name": "processes_alive", "ok": alive >= 1, "data": {"alive_processes": alive}})

    # 3) connect/disconnect semantics
    c1 = post_json(base, "/client-connected", {"connected_clients": 1})
    c2 = post_json(base, "/client-connected", {"connected_clients": 1})
    h2 = wait_health(base, timeout_s=10)
    d1 = post_json(base, "/client-disconnected", {"connected_clients": 1})
    h3 = wait_health(base, timeout_s=10)
    logic_ok = (h2.get("connected_clients") == 2 and h3.get("connected_clients") == 1)
    results["checks"].append(
        {
            "name": "connect_disconnect_logic",
            "ok": logic_ok,
            "data": {
                "connect1": c1.get("message"),
                "connect2": c2.get("message"),
                "after_connect": h2.get("connected_clients"),
                "disconnect1": d1.get("message"),
                "after_disconnect": h3.get("connected_clients"),
            },
        }
    )

    # 4) reboot persistence
    if not args.skip_reboot:
        boot_before = get_last_boot_time(ip, args.username, password)
        reboot_windows(ip, args.username, password)
        ssh_down_observed = wait_tcp_down(ip, 22, timeout_s=90)
        ssh_up = wait_tcp(ip, 22, timeout_s=240)
        boot_after = wait_boot_time_change(ip, args.username, password, boot_before, timeout_s=240) if ssh_up else ""
        reboot_observed = bool(boot_before and boot_after and boot_before != boot_after)
        h4 = wait_health(base, timeout_s=240)
        results["checks"].append(
            {
                "name": "reboot_ssh_cycle",
                "ok": ssh_up and reboot_observed,
                "data": {
                    "ssh_went_down_observed": ssh_down_observed,
                    "ssh_back": ssh_up,
                    "boot_before": boot_before,
                    "boot_after": boot_after,
                    "reboot_observed": reboot_observed,
                },
            }
        )
        results["checks"].append({"name": "reboot_agent_health", "ok": bool(h4.get("ok")), "data": h4})

    overall = all(c["ok"] for c in results["checks"])
    results["ok"] = overall
    print(json.dumps(results, indent=2))
    if not overall:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
