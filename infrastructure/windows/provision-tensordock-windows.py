#!/usr/bin/env python3
import argparse
import json
import os
import secrets
import string
import sys
import time
import urllib.request
from pathlib import Path

API_BASE = "https://dashboard.tensordock.com/api/v2"
STATE_PATH = Path(__file__).resolve().parent / "state" / "windows-vm.local.json"


def api_request(token: str, method: str, path: str, payload: dict | None = None) -> dict:
    data = json.dumps(payload).encode() if payload is not None else None
    req = urllib.request.Request(
        f"{API_BASE}{path}",
        data=data,
        method=method,
        headers={"Authorization": f"Bearer {token}", "Accept": "application/json", "Content-Type": "application/json"},
    )
    with urllib.request.urlopen(req, timeout=120) as resp:
        raw = resp.read().decode()
        return json.loads(raw)


def gen_password(length: int = 22) -> str:
    chars = string.ascii_letters + string.digits + "!@#$%^&*()_+-="
    return "".join(secrets.choice(chars) for _ in range(length))


def cmd_create(args: argparse.Namespace) -> None:
    password = gen_password()
    payload = {
        "data": {
            "type": "virtualmachine",
            "attributes": {
                "name": args.name,
                "type": "virtualmachine",
                "image": args.image,
                "password": password,
                "resources": {
                    "vcpu_count": args.vcpu,
                    "ram_gb": args.ram,
                    "storage_gb": args.storage,
                    "gpus": {args.gpu: {"count": 1}},
                },
                "location_id": args.location,
                "useDedicatedIp": True,
            },
        }
    }
    res = api_request(args.token, "POST", "/instances", payload)
    instance_id = res["data"]["id"]

    state = {
        "instance_id": instance_id,
        "name": args.name,
        "password": password,
        "image": args.image,
        "gpu": args.gpu,
        "location_id": args.location,
        "created_at": time.time(),
    }

    args.state_file.parent.mkdir(parents=True, exist_ok=True)
    args.state_file.write_text(json.dumps(state, indent=2), encoding="utf-8")
    print(json.dumps(state, indent=2))


def fetch_instance(token: str, instance_id: str) -> dict:
    return api_request(token, "GET", f"/instances/{instance_id}")


def cmd_status(args: argparse.Namespace) -> None:
    if args.instance_id:
        instance_id = args.instance_id
        state = {}
    else:
        if not args.state_file.exists():
            raise SystemExit("No saved state and --instance-id not provided")
        state = json.loads(args.state_file.read_text(encoding="utf-8"))
        instance_id = state["instance_id"]

    data = fetch_instance(args.token, instance_id)
    state.update({
        "instance_id": instance_id,
        "status": data.get("status"),
        "ip": data.get("ipAddress"),
        "rate_hourly": data.get("rateHourly"),
    })
    if not args.instance_id:
        args.state_file.write_text(json.dumps(state, indent=2), encoding="utf-8")
    print(json.dumps(state, indent=2))


def cmd_delete(args: argparse.Namespace) -> None:
    if args.instance_id:
        instance_id = args.instance_id
    else:
        if not args.state_file.exists():
            raise SystemExit("No saved state and --instance-id not provided")
        state = json.loads(args.state_file.read_text(encoding="utf-8"))
        instance_id = state["instance_id"]

    api_request(args.token, "DELETE", f"/instances/{instance_id}")
    if args.state_file.exists() and not args.instance_id:
        args.state_file.unlink()
    print(json.dumps({"deleted": True, "instance_id": instance_id}))


def parse_args() -> argparse.Namespace:
    p = argparse.ArgumentParser(description="Provision TensorDock Windows GPU VM")
    p.add_argument("--token", default=os.getenv("TENSORDOCK_API_TOKEN"), help="TensorDock API token")
    p.add_argument("--state-file", type=Path, default=STATE_PATH, help="Local path to persist VM state JSON")
    sub = p.add_subparsers(dest="cmd", required=True)

    c = sub.add_parser("create")
    c.add_argument("--name", default="gamer-windows-v100")
    c.add_argument("--image", default="windows10")
    c.add_argument("--gpu", default="v100-sxm2-16gb")
    c.add_argument("--location", default="9e1f2c34-7b58-4a3d-b6c9-0f1e2d3c4b5a")
    c.add_argument("--vcpu", type=int, default=8)
    c.add_argument("--ram", type=int, default=32)
    c.add_argument("--storage", type=int, default=200)

    s = sub.add_parser("status")
    s.add_argument("--instance-id")

    d = sub.add_parser("delete")
    d.add_argument("--instance-id")

    return p.parse_args()


def main() -> None:
    args = parse_args()
    if not args.token:
        raise SystemExit("Missing --token or TENSORDOCK_API_TOKEN")

    if args.cmd == "create":
        cmd_create(args)
    elif args.cmd == "status":
        cmd_status(args)
    elif args.cmd == "delete":
        cmd_delete(args)
    else:
        raise SystemExit(f"Unknown command: {args.cmd}")


if __name__ == "__main__":
    main()
