#!/usr/bin/env python3
from __future__ import annotations
import argparse
from pathlib import Path
from deploy_step_common import add_common_args, connect_from_state, upload_and_run_ps1, read_state

STATE_DEFAULT = Path(__file__).resolve().parent / 'state' / 'windows-vm-manassas.local.json'

def main():
    p=argparse.ArgumentParser(description='Deploy/start startup-windows.ps1')
    add_common_args(p, STATE_DEFAULT)
    p.add_argument('--nvidia-driver-url', default='')
    p.add_argument('--windows-username', default='user')
    p.add_argument('--windows-password', default='')
    args=p.parse_args()
    ssh,ip=connect_from_state(args.state_file,args.username,args.password)
    try:
        script=Path(__file__).resolve().parent/'startup-windows.ps1'
        state = read_state(args.state_file)
        effective_windows_password = args.windows_password or args.password or state.get('password', '')
        extra=[]
        if args.nvidia_driver_url:
            extra += ['-NvidiaDriverUrl', f'"{args.nvidia_driver_url}"']
        extra += ['-WindowsUsername', f'"{args.windows_username}"']
        if effective_windows_password:
            extra += ['-WindowsPassword', f'"{effective_windows_password}"']
        code,out,err=upload_and_run_ps1(ssh,script,'startup-windows.ps1',extra)
        print(out)
        if err.strip(): print('stderr:',err)
        raise SystemExit(code)
    finally:
        ssh.close()

if __name__=='__main__':
    main()
