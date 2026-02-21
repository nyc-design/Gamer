# Windows Host Infrastructure (Apollo)

This folder contains scripts to provision and bootstrap Windows GPU hosts for Gamer.

## Files

- `provision-tensordock-windows.py` — create/status/delete TensorDock Windows VM.
- `bootstrap-windows.ps1` — installs core runtime tools (Apollo/rclone/ShaderGlass/AutoHotkey) and folder layout.
- `install-agent-service.ps1` — installs and starts the Gamer client-agent as startup service/task.
- `scripts/position-azahar-dual.ps1` — place two Azahar windows across two displays.
- `scripts/move-window-next-monitor.ahk` — hotkey helper (`Ctrl+Alt+Right`) to move active window to next display.
- `scripts/apollo-on-client-connect.ps1` / `apollo-on-client-disconnect.ps1` — simple hooks for dual-screen behavior.

## Quick start

```bash
export TENSORDOCK_API_TOKEN='<token>'
python infrastructure/windows/provision-tensordock-windows.py create
python infrastructure/windows/provision-tensordock-windows.py status
```

Then on Windows VM:

```powershell
powershell -ExecutionPolicy Bypass -File infrastructure/windows/bootstrap-windows.ps1
powershell -ExecutionPolicy Bypass -File infrastructure/windows/install-agent-service.ps1
```

Agent API should then expose `/health`, `/start`, `/stop`.

