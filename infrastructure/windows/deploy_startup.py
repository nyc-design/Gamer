#!/usr/bin/env python3
from __future__ import annotations
import argparse
from pathlib import Path
from deploy_step_common import add_common_args, connect_from_state, upload_and_run_ps1

STATE_DEFAULT = Path(__file__).resolve().parent / 'state' / 'windows-vm-manassas.local.json'

def main():
    p=argparse.ArgumentParser(description='Deploy/start startup-windows.ps1')
    add_common_args(p, STATE_DEFAULT)
    p.add_argument('--nvidia-driver-url', default='')
    args=p.parse_args()
    ssh,ip=connect_from_state(args.state_file,args.username,args.password)
    try:
        script=Path(__file__).resolve().parent/'startup-windows.ps1'
        extra=[]
        if args.nvidia_driver_url:
            extra += ['-NvidiaDriverUrl', f'"{args.nvidia_driver_url}"']
        extra += ['-WindowsUsername', f'"{args.username}"']
        code,out,err=upload_and_run_ps1(ssh,script,'startup-windows.ps1',extra)
        print(out)
        if err.strip(): print('stderr:',err)
        raise SystemExit(code)
    finally:
        ssh.close()

if __name__=='__main__':
    main()
