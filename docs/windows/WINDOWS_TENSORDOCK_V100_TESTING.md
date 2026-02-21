# TensorDock Windows V100 Test VM (current branch)

A Windows 10 TensorDock VM with V100 was provisioned for implementation testing using:

- GPU: `v100-sxm2-16gb`
- Location: `9e1f2c34-7b58-4a3d-b6c9-0f1e2d3c4b5a`
- Instance name: `gamer-windows-v100`

> Local sensitive state (password/token-derived metadata) is intentionally ignored from git and stored only in local workspace files.

## Scripted management

Use:

```bash
export TENSORDOCK_API_TOKEN='<token>'
python infrastructure/windows/provision-tensordock-windows.py status
python infrastructure/windows/provision-tensordock-windows.py delete
```

## Current connectivity note

Current validated bootstrap path:

1. VM reports `running` and is reachable on RDP `3390`.
2. `infrastructure/windows/rdp_bootstrap.py` (pure Python, `aardwolf`) logs in and launches elevated PowerShell.
3. Script enables:
   - OpenSSH server (`sshd`) + firewall rule on TCP 22
   - WinRM service + firewall rule on TCP 5985
4. SSH login from workspace succeeds (`user@<vm-ip>`).

Follow-up deployment runs through `infrastructure/windows/deploy_via_ssh.py`.
