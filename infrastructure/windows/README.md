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

Then bootstrap remote-management from this workspace (pure Python + RDP, no manual RDP client):

```powershell
source .venv/bin/activate
python -m pip install aardwolf paramiko
python infrastructure/windows/rdp_bootstrap.py
```

If bootstrap succeeds, port 22/5985 are reachable and you can deploy scripts over SSH:

```bash
source .venv/bin/activate
python infrastructure/windows/deploy_via_ssh.py
```

Faster iteration (push updated scripts/agent without reinstalling Apollo/ShaderGlass):

```bash
python infrastructure/windows/deploy_via_ssh.py --skip-bootstrap
```

One-shot orchestrator (pure Python, production-style flow):

```bash
source .venv/bin/activate
python infrastructure/windows/orchestrate_windows_host.py --create
```

Reuse existing VM state (no create):

```bash
python infrastructure/windows/orchestrate_windows_host.py
```

Optional override URLs:

```bash
python infrastructure/windows/deploy_via_ssh.py \
  --apollo-installer-url "https://github.com/ClassicOldSong/Apollo/releases/download/v0.4.6/Apollo-0.4.6.exe" \
  --shaderglass-installer-url "https://github.com/mausimus/ShaderGlass/releases/download/v1.2.3/ShaderGlass-1.2.3.1-win-x64.zip"
```

Notes:
- For current TensorDock v2 API behavior, `port_forwards` cannot be set when `useDedicatedIp=true`.
- The current Windows path uses dedicated IP + RDP bootstrap to enable OpenSSH/WinRM from inside the guest.
- `provision-tensordock-windows.py` now stores state by default in:
  - `infrastructure/windows/state/windows-vm.local.json`
  - override with `--state-file` when running multiple VMs in parallel.
- Apollo install fallback:
  - if silent installer path does not produce `C:/Program Files/Apollo/Apollo.exe`, bootstrap stages a runnable binary at:
  - `C:/ProgramData/gamer/bin/Apollo/Apollo.exe`
