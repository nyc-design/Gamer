#!/usr/bin/env python3
"""Deploy Windows host scripts over OpenSSH after RDP bootstrap."""

from __future__ import annotations

import argparse
import json
import re
import time
import urllib.request
from pathlib import Path

import paramiko

STATE_DEFAULT = Path(__file__).resolve().parent / "state" / "windows-vm.local.json"
ROOT = Path(__file__).resolve().parents[2]


def load_state(path: Path) -> dict:
    if not path.exists():
        raise SystemExit(f"State file missing: {path}")
    return json.loads(path.read_text(encoding="utf-8"))


def run(ssh: paramiko.SSHClient, command: str) -> tuple[int, str, str]:
    stdin, stdout, stderr = ssh.exec_command(command)
    out = stdout.read().decode("utf-8", errors="ignore")
    err = stderr.read().decode("utf-8", errors="ignore")
    code = stdout.channel.recv_exit_status()
    return code, out, err


def upload_file(sftp: paramiko.SFTPClient, local: Path, remote: str) -> None:
    parent = str(Path(remote).parent).replace("\\", "/")
    try:
        sftp.stat(parent)
    except FileNotFoundError:
        parts = parent.split("/")
        cur = ""
        for p in parts:
            if not p:
                continue
            cur = f"{cur}/{p}" if cur else p
            try:
                sftp.stat(cur)
            except FileNotFoundError:
                sftp.mkdir(cur)
    sftp.put(str(local), remote)


def resolve_github_latest_asset(repo: str, asset_regex: str) -> str:
    req = urllib.request.Request(
        f"https://api.github.com/repos/{repo}/releases/latest",
        headers={"User-Agent": "gamer-windows-deployer"},
    )
    with urllib.request.urlopen(req, timeout=30) as resp:
        release = json.loads(resp.read().decode("utf-8"))
    pattern = re.compile(asset_regex)
    for asset in release.get("assets", []):
        name = asset.get("name", "")
        if pattern.search(name):
            return asset.get("browser_download_url", "")
    return ""


def parse_args() -> argparse.Namespace:
    p = argparse.ArgumentParser(description="Upload and execute Windows setup scripts via SSH")
    p.add_argument("--state-file", type=Path, default=STATE_DEFAULT)
    p.add_argument("--ip")
    p.add_argument("--username", default="user")
    p.add_argument("--password")
    p.add_argument("--apollo-installer-url", default="", help="Optional explicit Apollo installer URL")
    p.add_argument("--shaderglass-installer-url", default="", help="Optional explicit ShaderGlass package URL")
    p.add_argument("--ssh-retries", type=int, default=8, help="SSH connect retries")
    p.add_argument("--windows-username", default="user", help="Windows username for interactive Apollo tasks")
    p.add_argument("--bootstrap-only", action="store_true", help="Run only bootstrap-windows.ps1")
    p.add_argument("--skip-bootstrap", action="store_true", help="Skip bootstrap-windows.ps1")
    p.add_argument("--skip-agent-install", action="store_true", help="Skip install-agent-service.ps1")
    return p.parse_args()


def main() -> None:
    args = parse_args()
    state = load_state(args.state_file)
    ip = args.ip or state.get("ip")
    username = args.username
    password = args.password or state.get("password")
    if not ip or not password:
        raise SystemExit("Missing ip/password. Provide --ip --password or valid state file.")

    local_bootstrap = ROOT / "infrastructure" / "windows" / "bootstrap-windows.ps1"
    local_install = ROOT / "infrastructure" / "windows" / "install-agent-service.ps1"
    local_connect = ROOT / "infrastructure" / "windows" / "scripts" / "apollo-on-client-connect.ps1"
    local_disconnect = ROOT / "infrastructure" / "windows" / "scripts" / "apollo-on-client-disconnect.ps1"
    local_position = ROOT / "infrastructure" / "windows" / "scripts" / "position-azahar-dual.ps1"
    local_move = ROOT / "infrastructure" / "windows" / "scripts" / "move-window-next-monitor.ahk"
    local_agent_main = ROOT / "services" / "client-agent" / "src" / "main.py"
    local_agent_requirements = ROOT / "services" / "client-agent" / "requirements.txt"
    local_agent_manifest = ROOT / "services" / "client-agent" / "manifests" / "session_manifest.windows.dev.json"

    remote_root = "C:/ProgramData/gamer/setup"
    remote_bootstrap = f"{remote_root}/bootstrap-windows.ps1"
    remote_install = f"{remote_root}/install-agent-service.ps1"
    remote_connect = f"{remote_root}/apollo-on-client-connect.ps1"
    remote_disconnect = f"{remote_root}/apollo-on-client-disconnect.ps1"
    remote_position = f"{remote_root}/position-azahar-dual.ps1"
    remote_move = f"{remote_root}/move-window-next-monitor.ahk"
    remote_agent_root = "C:/gamer/client-agent"
    remote_agent_main = f"{remote_agent_root}/src/main.py"
    remote_agent_requirements = f"{remote_agent_root}/requirements.txt"
    remote_agent_manifest = f"{remote_agent_root}/manifests/session_manifest.windows.dev.json"

    ssh = paramiko.SSHClient()
    ssh.set_missing_host_key_policy(paramiko.AutoAddPolicy())
    print(f"Connecting to {username}@{ip}:22 ...")
    last_err = None
    for attempt in range(1, args.ssh_retries + 1):
        try:
            ssh.connect(ip, port=22, username=username, password=password, timeout=20, allow_agent=False, look_for_keys=False)
            last_err = None
            break
        except Exception as e:
            last_err = e
            print(f"SSH connect attempt {attempt}/{args.ssh_retries} failed: {e}")
            time.sleep(2.0)
    if last_err is not None:
        raise SystemExit(f"SSH connection failed after retries: {last_err}")
    print("SSH connected.")

    try:
        code, out, err = run(ssh, "powershell -NoProfile -Command \"New-Item -ItemType Directory -Path 'C:\\ProgramData\\gamer\\setup' -Force | Out-Null\"")
        if code != 0:
            raise RuntimeError(f"mkdir failed: {err or out}")

        with ssh.open_sftp() as sftp:
            upload_file(sftp, local_bootstrap, remote_bootstrap)
            upload_file(sftp, local_install, remote_install)
            upload_file(sftp, local_connect, remote_connect)
            upload_file(sftp, local_disconnect, remote_disconnect)
            upload_file(sftp, local_position, remote_position)
            upload_file(sftp, local_move, remote_move)
            upload_file(sftp, local_agent_main, remote_agent_main)
            upload_file(sftp, local_agent_requirements, remote_agent_requirements)
            upload_file(sftp, local_agent_manifest, remote_agent_manifest)
        print("Uploaded setup scripts.")

        apollo_url = args.apollo_installer_url
        shader_url = args.shaderglass_installer_url
        if not apollo_url:
            try:
                apollo_url = resolve_github_latest_asset("ClassicOldSong/Apollo", r"\.exe$")
            except Exception as e:
                print(f"Warning: failed to resolve Apollo URL: {e}")
                apollo_url = ""
        if not shader_url:
            try:
                shader_url = resolve_github_latest_asset("mausimus/ShaderGlass", r"win-x64\.zip$")
            except Exception as e:
                print(f"Warning: failed to resolve ShaderGlass URL: {e}")
                shader_url = ""

        if not args.skip_bootstrap:
            def _ps_single_quote(v: str) -> str:
                return "'" + v.replace("'", "''") + "'"

            bootstrap_cmd = "powershell -ExecutionPolicy Bypass -File C:\\ProgramData\\gamer\\setup\\bootstrap-windows.ps1"
            if apollo_url:
                bootstrap_cmd += f' -ApolloInstallerUrl "{apollo_url}"'
            if shader_url:
                bootstrap_cmd += f' -ShaderGlassInstallerUrl "{shader_url}"'
            if args.windows_username:
                bootstrap_cmd += f" -WindowsUsername {_ps_single_quote(args.windows_username)}"
            if password:
                bootstrap_cmd += f" -WindowsPassword {_ps_single_quote(password)}"
            code, out, err = run(
                ssh,
                bootstrap_cmd,
            )
            print(out)
            if code != 0:
                raise RuntimeError(f"bootstrap-windows.ps1 failed: {err}")

        if not args.bootstrap_only and not args.skip_agent_install:
            code, out, err = run(
                ssh,
                "powershell -ExecutionPolicy Bypass -File C:\\ProgramData\\gamer\\setup\\install-agent-service.ps1 -AgentRoot C:\\gamer\\client-agent",
            )
            print(out)
            if code != 0:
                raise RuntimeError(f"install-agent-service.ps1 failed: {err}")

        # Enforce Apollo launch in interactive user session (avoid SYSTEM/session0 capture path).
        if password:
            safe_pw = password.replace('"', '\\"')
            safe_user = (args.windows_username or "user").replace('"', '\\"')
            run(
                ssh,
                "powershell -NoProfile -Command "
                "\"$ErrorActionPreference='Continue'; "
                "$setup='C:\\ProgramData\\gamer\\setup'; New-Item -ItemType Directory -Path $setup -Force | Out-Null; "
                "$run1=Join-Path $setup 'run-apollo1.cmd'; "
                "$run2=Join-Path $setup 'run-apollo2.cmd'; "
                "[IO.File]::WriteAllText($run1,'\\\"C:\\ProgramData\\gamer\\bin\\Apollo\\Apollo.exe\\\" \\\"C:\\Program Files\\Apollo\\config\\sunshine.conf\\\"`r`n'); "
                "[IO.File]::WriteAllText($run2,'\\\"C:\\ProgramData\\gamer\\bin\\Apollo\\Apollo.exe\\\" \\\"C:\\Program Files\\Apollo\\config\\sunshine_2.conf\\\"`r`n'); "
                "Stop-Service ApolloService -Force -ErrorAction SilentlyContinue; "
                "Set-Service ApolloService -StartupType Disabled -ErrorAction SilentlyContinue; "
                "taskkill /IM sunshinesvc.exe /F 2>$null; taskkill /IM Apollo.exe /F 2>$null; "
                f"schtasks /Create /TN GamerApollo1 /TR $run1 /SC ONLOGON /RL HIGHEST /RU \\\"{safe_user}\\\" /RP \\\"{safe_pw}\\\" /F /IT | Out-Null; "
                f"schtasks /Create /TN GamerApollo2 /TR $run2 /SC ONLOGON /RL HIGHEST /RU \\\"{safe_user}\\\" /RP \\\"{safe_pw}\\\" /F /IT | Out-Null; "
                "schtasks /Run /TN GamerApollo1 | Out-Null; "
                "schtasks /Run /TN GamerApollo2 | Out-Null; "
                "Start-Sleep -Seconds 2; "
                "Get-Process Apollo -IncludeUserName -ErrorAction SilentlyContinue | "
                "Select-Object Name,Id,SessionId,UserName,Path | Format-Table -AutoSize\"",
            )

        code, out, err = run(ssh, "powershell -NoProfile -Command \"Get-Service sshd,WinRM | Format-Table Name,Status,StartType -AutoSize\"")
        print(out)
        if err.strip():
            print("stderr:", err.strip())

        # Quick diagnostic: verify Apollo is not SYSTEM/session0 if interactive tasks are configured.
        code, out, err = run(
            ssh,
            "powershell -NoProfile -Command \"Get-Service ApolloService -ErrorAction SilentlyContinue | "
            "Select-Object Name,Status,StartType | Format-Table -AutoSize; "
            "Get-Process Apollo -IncludeUserName -ErrorAction SilentlyContinue | "
            "Select-Object Name,Id,SessionId,UserName,Path | Format-Table -AutoSize\"",
        )
        if out.strip():
            print(out)
        if err.strip():
            print("stderr:", err.strip())

        print("Windows deploy via SSH completed.")
    finally:
        ssh.close()


if __name__ == "__main__":
    main()
