#!/usr/bin/env python3
"""End-to-end Windows host orchestration using pure Python tooling.

Flow:
1) Create or reuse TensorDock VM
2) Poll until IP/running
3) Bootstrap management ports via RDP automation
4) Deploy setup + agent via SSH
5) Validate agent health endpoint
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import time
from pathlib import Path
from urllib.request import urlopen

ROOT = Path(__file__).resolve().parents[2]
STATE_DEFAULT = Path(__file__).resolve().parent / "state" / "windows-vm.local.json"


def run_cmd(cmd: list[str], env: dict[str, str] | None = None) -> None:
    print("+", " ".join(cmd))
    subprocess.run(cmd, check=True, env=env)


def read_state(path: Path) -> dict:
    return json.loads(path.read_text(encoding="utf-8"))


def wait_for_status(state_file: Path, token: str | None, timeout_s: int = 900) -> dict:
    # If token isn't available, trust current local state.
    if not token:
        state = read_state(state_file)
        if state.get("ip"):
            print(f"token not provided; using state file ip={state.get('ip')} status={state.get('status')}")
            return state
        raise RuntimeError("No token provided and state file has no IP. Pass --token or set TENSORDOCK_API_TOKEN.")

    deadline = time.time() + timeout_s
    while time.time() < deadline:
        run_cmd(
            [
                sys.executable,
                str(ROOT / "infrastructure" / "windows" / "provision-tensordock-windows.py"),
                "--token",
                token,
                "--state-file",
                str(state_file),
                "status",
            ]
        )
        state = read_state(state_file)
        status = (state.get("status") or "").lower()
        ip = state.get("ip")
        print(f"status={status} ip={ip}")
        if status == "running" and ip:
            return state
        time.sleep(8)
    raise TimeoutError("VM did not become running with an IP before timeout")


def wait_health(ip: str, timeout_s: int = 300) -> None:
    deadline = time.time() + timeout_s
    url = f"http://{ip}:8081/health"
    while time.time() < deadline:
        try:
            with urlopen(url, timeout=4) as resp:
                if resp.status == 200:
                    body = resp.read().decode("utf-8", errors="ignore")
                    print(f"health ok: {body}")
                    return
        except Exception:
            pass
        time.sleep(2)
    raise TimeoutError("Agent health endpoint did not become reachable")


def parse_args() -> argparse.Namespace:
    p = argparse.ArgumentParser(description="Orchestrate Windows VM bootstrap+deploy")
    p.add_argument("--token", default=os.getenv("TENSORDOCK_API_TOKEN"), help="TensorDock API token")
    p.add_argument("--state-file", type=Path, default=STATE_DEFAULT)
    p.add_argument("--create", action="store_true", help="Create a new VM before bootstrapping")
    p.add_argument("--vm-name", default="gamer-windows-gpu")
    p.add_argument("--vm-image", default="windows10")
    p.add_argument("--vm-gpu", default="geforcertx4090-pcie-24gb")
    p.add_argument("--vm-location", default="", help="Optional TensorDock location ID")
    p.add_argument("--vm-city", default="Chubbuck")
    p.add_argument("--vm-state", default="Idaho")
    p.add_argument("--vm-vcpu", type=int, default=8)
    p.add_argument("--vm-ram", type=int, default=32)
    p.add_argument("--vm-storage", type=int, default=200)
    p.add_argument("--skip-rdp-bootstrap", action="store_true")
    p.add_argument("--bootstrap-only", action="store_true", help="Only run bootstrap-windows.ps1 during SSH deploy")
    p.add_argument("--username", default="user")
    p.add_argument("--timeout-vm", type=int, default=900)
    p.add_argument("--timeout-health", type=int, default=300)
    return p.parse_args()


def main() -> None:
    args = parse_args()
    if not args.state_file.exists() and not args.create:
        raise SystemExit(f"State file missing ({args.state_file}); pass --create or provide existing state file.")
    if args.create and not args.token:
        raise SystemExit("Missing TensorDock token: set TENSORDOCK_API_TOKEN or pass --token")

    if args.create:
        create_cmd = [
            sys.executable,
            str(ROOT / "infrastructure" / "windows" / "provision-tensordock-windows.py"),
            "--token",
            args.token,
            "--state-file",
            str(args.state_file),
            "create",
            "--name",
            args.vm_name,
            "--image",
            args.vm_image,
            "--gpu",
            args.vm_gpu,
            "--city",
            args.vm_city,
            "--state",
            args.vm_state,
            "--vcpu",
            str(args.vm_vcpu),
            "--ram",
            str(args.vm_ram),
            "--storage",
            str(args.vm_storage),
        ]
        if args.vm_location:
            create_cmd.extend(["--location", args.vm_location])
        run_cmd(create_cmd)

    state = wait_for_status(args.state_file, args.token, timeout_s=args.timeout_vm)

    if not args.skip_rdp_bootstrap:
        run_cmd(
            [
                sys.executable,
                str(ROOT / "infrastructure" / "windows" / "rdp_bootstrap.py"),
                "--state-file",
                str(args.state_file),
                "--username",
                args.username,
            ]
        )

    deploy_cmd = [
        sys.executable,
        str(ROOT / "infrastructure" / "windows" / "deploy_via_ssh.py"),
        "--state-file",
        str(args.state_file),
        "--username",
        args.username,
    ]
    if args.bootstrap_only:
        deploy_cmd.append("--bootstrap-only")
    run_cmd(deploy_cmd)

    wait_health(state["ip"], timeout_s=args.timeout_health)
    print("Windows orchestration complete.")


if __name__ == "__main__":
    main()
