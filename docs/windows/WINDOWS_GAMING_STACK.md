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
- `POST /manifest-set` / `POST /manifest-clear` → set or clear in-memory manifest before start (pre-server integration helper).
- `POST /client-connected` and `POST /client-disconnected` → trigger Apollo connect/disconnect hook scripts with client count.
- `POST /position-dual-now` → force-run window placement script.
- `POST /cleanup-processes` → prune exited process handles from in-memory state.
- `GET /manifest` and `GET /health` (includes script-exit diagnostics and per-process alive status).

Optional env:
- `POWERSHELL_SCRIPT_TIMEOUT_SEC` (default `20`) limits hook script execution time.

Manifest hardcoded at:

- `services/client-agent/manifests/session_manifest.windows.dev.json`

## Provisioning Windows VM

```bash
export TENSORDOCK_API_TOKEN='<token>'
python infrastructure/windows/provision-tensordock-windows.py create
python infrastructure/windows/provision-tensordock-windows.py status
```

VM state saved at:

- `infrastructure/windows/state/windows-vm.local.json`

## On-VM bootstrap flow (pure Python orchestration)

1. Run RDP bootstrap from workspace:
   - `python infrastructure/windows/rdp_bootstrap.py`
2. Deploy scripts and agent over SSH:
   - `python infrastructure/windows/deploy_via_ssh.py`
3. Call agent `POST /start`

Or run the one-shot orchestrator:

- `python infrastructure/windows/orchestrate_windows_host.py --create`

Validate an existing host end-to-end:

- `python infrastructure/windows/validate_windows_host.py`

## Dual-screen convenience

- AutoHotkey helper: `Ctrl+Alt+Right` moves active window to next monitor.
- PowerShell helper `position-azahar-dual.ps1` places first two Azahar windows across first two displays (with retry loop for late window/display availability).
- Agent `/client-connected` and `/client-disconnected` now support either:
  - absolute counts (`connected_clients`), or
  - edge-style events (`client_id`), with stale-count resilience.
- Client agent startup task runs as `SYSTEM` at boot (no user logon required), with automatic restart policy.
