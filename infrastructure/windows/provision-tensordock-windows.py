#!/usr/bin/env python3
import argparse
import json
import os
import secrets
import string
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path

API_BASE = "https://dashboard.tensordock.com/api/v2"
STATE_PATH = Path(__file__).resolve().parent / "state" / "windows-vm.local.json"


def api_request(token: str, method: str, path: str, payload: dict | None = None, retries: int = 4) -> dict:
    data = json.dumps(payload).encode() if payload is not None else None
    headers = {"Authorization": f"Bearer {token}", "Accept": "application/json", "Content-Type": "application/json"}
    last_err = None
    for attempt in range(1, retries + 1):
        try:
            req = urllib.request.Request(
                f"{API_BASE}{path}",
                data=data,
                method=method,
                headers=headers,
            )
            with urllib.request.urlopen(req, timeout=120) as resp:
                raw = resp.read().decode()
                return json.loads(raw)
        except urllib.error.HTTPError as e:
            if e.code < 500 or attempt == retries:
                body = e.read().decode("utf-8", errors="ignore")
                raise RuntimeError(f"TensorDock API {method} {path} failed [{e.code}]: {body}") from e
            last_err = e
            time.sleep(min(2**attempt, 8))
        except Exception as e:
            last_err = e
            if attempt == retries:
                raise
            time.sleep(min(2**attempt, 8))
    raise RuntimeError(f"TensorDock API request failed after retries: {last_err}")


def public_request(path: str) -> dict:
    req = urllib.request.Request(
        f"{API_BASE}{path}",
        headers={"Accept": "application/json", "User-Agent": "gamer-windows-provisioner"},
    )
    with urllib.request.urlopen(req, timeout=60) as resp:
        return json.loads(resp.read().decode("utf-8"))


def gen_password(length: int = 22) -> str:
    chars = string.ascii_letters + string.digits + "!@#$%^&*()_+-="
    return "".join(secrets.choice(chars) for _ in range(length))


def find_local_ssh_public_key() -> str:
    env_key = os.getenv("TENSORDOCK_SSH_PUBLIC_KEY") or os.getenv("SSH_PUBLIC_KEY")
    if env_key:
        return env_key.strip()
    for p in (Path.home() / ".ssh" / "id_ed25519.pub", Path.home() / ".ssh" / "id_rsa.pub"):
        if p.exists():
            return p.read_text(encoding="utf-8").strip()
    return ""


def list_locations() -> list[dict]:
    data = public_request("/locations")
    return data.get("data", {}).get("locations", [])


def list_hostnodes(token: str) -> list[dict]:
    data = api_request(token, "GET", "/hostnodes")
    return data.get("data", {}).get("hostnodes", [])


def select_location_and_gpu(city: str, state: str | None, gpu: str | None) -> tuple[str, str]:
    locs = list_locations()
    city_l = city.strip().lower()
    state_l = (state or "").strip().lower()
    matched: list[tuple[dict, dict]] = []
    for loc in locs:
        if (loc.get("city") or "").strip().lower() != city_l:
            continue
        if state_l and (loc.get("stateprovince") or "").strip().lower() != state_l:
            continue
        for g in loc.get("gpus", []):
            if g.get("max_count", 0) < 1:
                continue
            if not g.get("network_features", {}).get("dedicated_ip_available", False):
                continue
            key = (g.get("v0Name") or "").lower()
            display = (g.get("displayName") or "").lower()
            if gpu and gpu.lower() not in key and gpu.lower() not in display:
                continue
            matched.append((loc, g))
    if not matched:
        state_msg = f", {state}" if state else ""
        gpu_msg = f", gpu={gpu}" if gpu else ""
        raise RuntimeError(f"No matching TensorDock location/gpu for city={city}{state_msg}{gpu_msg}")
    matched.sort(key=lambda x: (float(x[1].get("price_per_hr", 9999)), -(int(x[1].get("max_count", 0)))))
    loc, g = matched[0]
    return loc["id"], g["v0Name"]


def select_hostnode_and_gpu(
    token: str,
    city: str,
    state: str | None,
    gpu: str | None,
    vcpu: int,
    ram: int,
    storage: int,
    require_public_ip: bool = True,
) -> tuple[str, str, str]:
    city_l = city.strip().lower()
    state_l = (state or "").strip().lower()
    matched: list[tuple[float, float, dict, dict]] = []
    for hn in list_hostnodes(token):
        loc = hn.get("location") or {}
        if (loc.get("city") or "").strip().lower() != city_l:
            continue
        if state_l and (loc.get("stateprovince") or "").strip().lower() != state_l:
            continue
        ar = hn.get("available_resources") or {}
        if int(ar.get("vcpu_count") or 0) < vcpu:
            continue
        if int(ar.get("ram_gb") or 0) < ram:
            continue
        if int(ar.get("storage_gb") or 0) < storage:
            continue
        if require_public_ip and not bool(ar.get("has_public_ip_available")):
            continue
        for g in ar.get("gpus", []):
            if int(g.get("availableCount") or 0) < 1:
                continue
            key = (g.get("v0Name") or "").lower()
            if gpu and gpu.lower() not in key:
                continue
            price = float(g.get("price_per_hr") or 9999)
            uptime = float(hn.get("uptime_percentage") or 0)
            matched.append((price, -uptime, hn, g))
    if not matched:
        state_msg = f", {state}" if state else ""
        gpu_msg = f", gpu={gpu}" if gpu else ""
        raise RuntimeError(f"No matching hostnode in city={city}{state_msg}{gpu_msg} for requested resources")
    matched.sort(key=lambda x: (x[0], x[1]))
    _, _, hn, g = matched[0]
    return hn["id"], g["v0Name"], hn.get("location_id", "")


def cmd_create(args: argparse.Namespace) -> None:
    password = args.password or gen_password()
    location = args.location
    gpu = args.gpu
    hostnode_id = args.hostnode_id
    if not hostnode_id:
        hostnode_id, gpu, location_from_hn = select_hostnode_and_gpu(
            token=args.token,
            city=args.city,
            state=args.state,
            gpu=args.gpu,
            vcpu=args.vcpu,
            ram=args.ram,
            storage=args.storage,
            require_public_ip=True,
        )
        if not location:
            location = location_from_hn

    is_windows = args.image.lower().startswith("windows")
    ssh_key = ""
    if not is_windows:
        ssh_key = args.ssh_key or find_local_ssh_public_key()
        if not ssh_key:
            raise RuntimeError("Missing SSH public key. Set --ssh-key or TENSORDOCK_SSH_PUBLIC_KEY, or create ~/.ssh/id_ed25519.pub")

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
                    "gpus": {gpu: {"count": 1}},
                },
                "hostnode_id": hostnode_id,
                "useDedicatedIp": True,
            },
        }
    }
    if ssh_key:
        payload["data"]["attributes"]["ssh_key"] = ssh_key
    res = api_request(args.token, "POST", "/instances", payload)
    if "data" not in res or "id" not in res.get("data", {}):
        raise RuntimeError(f"Unexpected TensorDock create response: {json.dumps(res)}")
    instance_id = res["data"]["id"]

    state = {
        "instance_id": instance_id,
        "name": args.name,
        "password": password,
        "image": args.image,
        "gpu": gpu,
        "location_id": location,
        "hostnode_id": hostnode_id,
        "created_at": time.time(),
    }
    if ssh_key:
        state["ssh_key_fingerprint"] = ssh_key.split()[1][:16] if len(ssh_key.split()) > 1 else ""

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
    c.add_argument("--name", default="gamer-windows-gpu")
    c.add_argument("--image", default="windows10")
    c.add_argument("--gpu", default="geforcertx4090-pcie-24gb")
    c.add_argument("--location", default="", help="TensorDock location ID (if omitted, resolved from --city/--state)")
    c.add_argument("--city", default="Chubbuck")
    c.add_argument("--state", default="Idaho")
    c.add_argument("--password", default="", help="Optional explicit Windows password")
    c.add_argument("--ssh-key", default="", help="Optional SSH public key for VM creation")
    c.add_argument("--hostnode-id", default="", help="Optional explicit hostnode UUID")
    c.add_argument("--vcpu", type=int, default=8)
    c.add_argument("--ram", type=int, default=32)
    c.add_argument("--storage", type=int, default=200)

    s = sub.add_parser("status")
    s.add_argument("--instance-id")

    d = sub.add_parser("delete")
    d.add_argument("--instance-id")

    l = sub.add_parser("list-locations")
    l.add_argument("--city", default="")
    l.add_argument("--state", default="")
    l.add_argument("--gpu", default="")

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
    elif args.cmd == "list-locations":
        rows = []
        for loc in list_locations():
            city = loc.get("city", "")
            state = loc.get("stateprovince", "")
            if args.city and city.lower() != args.city.lower():
                continue
            if args.state and state.lower() != args.state.lower():
                continue
            for g in loc.get("gpus", []):
                if g.get("max_count", 0) < 1:
                    continue
                if args.gpu:
                    needle = args.gpu.lower()
                    if needle not in (g.get("v0Name", "").lower() + " " + g.get("displayName", "").lower()):
                        continue
                rows.append(
                    {
                        "location_id": loc.get("id"),
                        "city": city,
                        "state": state,
                        "gpu_name": g.get("v0Name"),
                        "gpu_display": g.get("displayName"),
                        "max_count": g.get("max_count"),
                        "price_per_hr": g.get("price_per_hr"),
                        "dedicated_ip": g.get("network_features", {}).get("dedicated_ip_available", False),
                    }
                )
        print(json.dumps(rows, indent=2))
    else:
        raise SystemExit(f"Unknown command: {args.cmd}")


if __name__ == "__main__":
    main()
