#!/usr/bin/env python3
"""Pair Moonlight client PIN to Apollo by posting PIN on the Windows host itself.

Why this exists:
- Posting to Apollo API from outside the host can return 403 in some setups.
- This script always executes login+pin from localhost on the VM via SSH.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path

import paramiko

STATE_DEFAULT = Path(__file__).resolve().parent / "state" / "windows-vm-manassas.local.json"


def read_state(path: Path) -> dict:
    return json.loads(path.read_text(encoding="utf-8"))


def build_ps(pin: str, api_port: int, username: str, password: str) -> str:
    return f"""[System.Net.ServicePointManager]::ServerCertificateValidationCallback = {{$true}}
$s = New-Object Microsoft.PowerShell.Commands.WebRequestSession
$loginBody = @{{ username = '{username}'; password = '{password}' }} | ConvertTo-Json -Compress
$login = Invoke-RestMethod -Method Post -Uri 'https://127.0.0.1:{api_port}/api/login' -WebSession $s -ContentType 'application/json' -Body $loginBody
$pinBody = @{{ pin = '{pin}' }} | ConvertTo-Json -Compress
$resp = Invoke-RestMethod -Method Post -Uri 'https://127.0.0.1:{api_port}/api/pin' -WebSession $s -ContentType 'application/json' -Body $pinBody
$resp | ConvertTo-Json -Compress
"""


def main() -> None:
    p = argparse.ArgumentParser(description="Pair Moonlight PIN with Apollo over host-local API")
    p.add_argument("--pin", required=True)
    p.add_argument("--state-file", type=Path, default=STATE_DEFAULT)
    p.add_argument("--windows-username", default="user")
    p.add_argument("--windows-password")
    p.add_argument("--apollo-api-port", type=int, default=47990)
    p.add_argument("--apollo-username", default="gamer")
    p.add_argument("--apollo-password", default="gamer")
    args = p.parse_args()

    state = read_state(args.state_file)
    ip = state.get("ip")
    if not ip:
        raise SystemExit("Missing VM IP in state file")
    win_pw = args.windows_password or state.get("password")
    if not win_pw:
        raise SystemExit("Missing Windows password")

    ps = build_ps(args.pin, args.apollo_api_port, args.apollo_username, args.apollo_password)

    ssh = paramiko.SSHClient()
    ssh.set_missing_host_key_policy(paramiko.AutoAddPolicy())
    ssh.connect(ip, username=args.windows_username, password=win_pw, port=22, timeout=20, auth_timeout=20)
    try:
        sftp = ssh.open_sftp()
        remote = "C:/ProgramData/gamer/setup/pair-pin.ps1"
        with sftp.file(remote, "w") as f:
            f.write(ps)
        sftp.close()

        _, stdout, stderr = ssh.exec_command(f"powershell -NoProfile -ExecutionPolicy Bypass -File {remote}")
        out = stdout.read().decode("utf-8", errors="ignore").strip()
        err = stderr.read().decode("utf-8", errors="ignore").strip()
        if err:
            raise SystemExit(err)
        print(out)
    finally:
        ssh.close()


if __name__ == "__main__":
    main()
