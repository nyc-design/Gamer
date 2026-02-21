#!/usr/bin/env python3
from __future__ import annotations
import argparse
from pathlib import Path
from deploy_step_common import add_common_args, connect_from_state, upload_and_run_ps1

STATE_DEFAULT = Path(__file__).resolve().parent / 'state' / 'windows-vm-manassas.local.json'

def main():
    p=argparse.ArgumentParser(description='Deploy/start azahar-windows.ps1')
    add_common_args(p, STATE_DEFAULT)
    p.add_argument('--azahar-release-url', default='')
    p.add_argument('--gcs-config-remote', default='gcs:gamer-data/me/configs/azahar')
    args=p.parse_args()
    ssh,ip=connect_from_state(args.state_file,args.username,args.password)
    try:
        script=Path(__file__).resolve().parent/'azahar-windows.ps1'
        extra=['-GcsConfigRemote', f'"{args.gcs_config_remote}"']
        if args.azahar_release_url:
            extra += ['-AzaharReleaseUrl', f'"{args.azahar_release_url}"']
        code,out,err=upload_and_run_ps1(ssh,script,'azahar-windows.ps1',extra)
        print(out)
        if err.strip(): print('stderr:',err)
        raise SystemExit(code)
    finally:
        ssh.close()

if __name__=='__main__':
    main()
