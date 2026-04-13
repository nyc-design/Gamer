#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
from pathlib import Path

import paramiko


def read_state(path: Path) -> dict:
    return json.loads(path.read_text(encoding='utf-8'))


def connect_from_state(state_file: Path, username: str, password: str | None = None):
    state = read_state(state_file)
    ip = state.get('ip')
    if not ip:
        raise SystemExit('Missing IP in state file')
    pw = password or state.get('password')
    if not pw:
        raise SystemExit('Missing password (arg or state file)')
    ssh = paramiko.SSHClient()
    ssh.set_missing_host_key_policy(paramiko.AutoAddPolicy())
    ssh.connect(ip, port=22, username=username, password=pw, timeout=30, auth_timeout=30)
    return ssh, ip


def upload_and_run_ps1(ssh, local_script: Path, remote_name: str, extra_args: list[str] | None = None) -> tuple[int, str, str]:
    remote_dir = 'C:/ProgramData/gamer/setup'
    remote_script = f'{remote_dir}/{remote_name}'
    sftp = ssh.open_sftp()
    try:
        try:
            sftp.mkdir(remote_dir)
        except Exception:
            pass
        sftp.put(str(local_script), remote_script)
    finally:
        sftp.close()

    args = ' '.join(extra_args or [])
    cmd = f'powershell -NoProfile -ExecutionPolicy Bypass -File {remote_script} {args}'.strip()
    stdin, stdout, stderr = ssh.exec_command(cmd)
    out = stdout.read().decode('utf-8', errors='ignore')
    err = stderr.read().decode('utf-8', errors='ignore')
    code = stdout.channel.recv_exit_status()
    return code, out, err


def add_common_args(p: argparse.ArgumentParser, default_state: Path):
    p.add_argument('--state-file', type=Path, default=default_state)
    p.add_argument('--username', default='user')
    p.add_argument('--password')

