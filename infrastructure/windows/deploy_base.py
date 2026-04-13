#!/usr/bin/env python3
from __future__ import annotations
import argparse
from pathlib import Path
from deploy_step_common import add_common_args, connect_from_state, upload_and_run_ps1

STATE_DEFAULT = Path(__file__).resolve().parent / 'state' / 'windows-vm-manassas.local.json'

def main():
    p=argparse.ArgumentParser(description='Deploy/start base-windows.ps1')
    add_common_args(p, STATE_DEFAULT)
    p.add_argument('--rclone-config-base64', default='')
    p.add_argument('--r2-remote', default='r2:gamer-roms/me')
    p.add_argument('--gcs-remote', default='gcs:gamer-data/me')
    p.add_argument('--apollo-installer-url', default='')
    p.add_argument('--apollo-username', default='gamer')
    p.add_argument('--apollo-password', default='gamer')
    p.add_argument('--windows-username', default='user')
    args=p.parse_args()
    ssh,ip=connect_from_state(args.state_file,args.username,args.password)
    try:
        script=Path(__file__).resolve().parent/'base-windows.ps1'
        extra=['-R2Remote', f'"{args.r2_remote}"', '-GcsRemote', f'"{args.gcs_remote}"', '-ApolloUsername', f'"{args.apollo_username}"', '-ApolloPassword', f'"{args.apollo_password}"', '-WindowsUsername', f'"{args.windows_username}"']
        if args.apollo_installer_url:
            extra += ['-ApolloInstallerUrl', f'"{args.apollo_installer_url}"']
        if args.rclone_config_base64:
            extra += ['-RcloneConfigBase64', f'"{args.rclone_config_base64}"']
        code,out,err=upload_and_run_ps1(ssh,script,'base-windows.ps1',extra)
        print(out)
        if err.strip(): print('stderr:',err)
        raise SystemExit(code)
    finally:
        ssh.close()

if __name__=='__main__':
    main()
