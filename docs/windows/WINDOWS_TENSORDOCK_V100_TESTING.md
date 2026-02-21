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

At time of implementation, instance API reports `running`, but direct checks from this workspace to common remote-management ports (3389/5985/5986/22) timed out.

That means code-level implementation proceeded, but full remote in-VM bootstrap automation from this environment could not be completed end-to-end yet.

