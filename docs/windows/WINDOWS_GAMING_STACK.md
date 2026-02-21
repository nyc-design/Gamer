# Windows Gaming Stack (Apollo-first)

This branch implements a Windows-first gaming host flow aligned with Linux contracts while using Apollo for streaming.

## Current status

- Windows client agent scaffold with hardcoded manifest support is implemented.
- TensorDock Windows VM provisioning script is implemented.
- Bootstrap scripts for Apollo/rclone/ShaderGlass/AutoHotkey are implemented.
- Dual-screen helper scripts for moving emulator windows across displays are included.

## Agent behavior

The agent currently supports:

- `POST /start` → load hardcoded manifest (or `SESSION_MANIFEST_PATH`), ensure paths, start Apollo/shader/emulator helpers.
- `POST /stop` → stop launched processes.
- `GET /manifest` and `GET /health`.

Manifest hardcoded at:

- `services/client-agent/manifests/session_manifest.windows.dev.json`

## Provisioning Windows VM

```bash
export TENSORDOCK_API_TOKEN='<token>'
python infrastructure/windows/provision-tensordock-windows.py create
python infrastructure/windows/provision-tensordock-windows.py status
```

VM state saved at:

- `infrastructure/windows/state/windows-vm.json`

## On-VM bootstrap flow

1. Copy repo to VM
2. Run `infrastructure/windows/bootstrap-windows.ps1`
3. Run `infrastructure/windows/install-agent-service.ps1`
4. Call agent `POST /start`

## Dual-screen convenience

- AutoHotkey helper: `Ctrl+Alt+Right` moves active window to next monitor.
- PowerShell helper `position-azahar-dual.ps1` places first two Azahar windows across first two displays.

