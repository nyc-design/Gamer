# Windows Host Infrastructure (Apollo)

This folder contains scripts to provision and bootstrap Windows GPU hosts for Gamer.

## Files

- `provision-tensordock-windows.py` — create/status/delete/list TensorDock Windows VM capacity.
- `startup-windows.ps1` — first-phase host hardening: services, SSH/WinRM, audio services, optional NVIDIA driver, VB-CABLE install.
- `base-windows.ps1` — base gamer stack: rclone pull, Apollo config, display/audio options.
- `azahar-windows.ps1` — installs Azahar and wires Apollo app entry.
- `agent-windows.ps1` — installs/starts client-agent service.
- `bootstrap-windows.ps1` — legacy all-in-one bootstrap (still available).
- `install-agent-service.ps1` — installs and starts the Gamer client-agent as startup service/task.
- `scripts/position-azahar-dual.ps1` — place two Azahar windows across two displays.
- `scripts/move-window-next-monitor.ahk` — hotkey helper (`Ctrl+Alt+Right`) to move active window to next display.
- `scripts/apollo-on-client-connect.ps1` / `apollo-on-client-disconnect.ps1` — simple hooks for dual-screen behavior.
- `deploy_startup.py` / `deploy_base.py` / `deploy_azahar.py` / `deploy_agent.py` — staged deploy runners over SSH.

## Current test VM access (temporary)

- Username: `user`
- Password: `GamerWin4090!`

> Temporary credential for active manual validation. Rotate/remove before production use.

## Quick start

```bash
export TENSORDOCK_API_TOKEN='<token>'
python infrastructure/windows/provision-tensordock-windows.py list-locations --city Chubbuck
python infrastructure/windows/provision-tensordock-windows.py create
python infrastructure/windows/provision-tensordock-windows.py status
```

Create with explicit Chubbuck GPU + shape:

```bash
python infrastructure/windows/provision-tensordock-windows.py create \
  --city Chubbuck --state Idaho \
  --gpu geforcertx4090-pcie-24gb \
  --vcpu 8 --ram 32 --storage 200
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

Staged deploy (recommended for production hardening):

```bash
python infrastructure/windows/deploy_startup.py --state-file infrastructure/windows/state/windows-vm-manassas.local.json
python infrastructure/windows/deploy_base.py --state-file infrastructure/windows/state/windows-vm-manassas.local.json
python infrastructure/windows/deploy_azahar.py --state-file infrastructure/windows/state/windows-vm-manassas.local.json
python infrastructure/windows/deploy_agent.py --state-file infrastructure/windows/state/windows-vm-manassas.local.json
```

Symlink/mount behavior:
- Apollo config dir is symlinked to `C:\gamer\mounts\gcs\configs\apollo`
- Azahar config dir is symlinked to `C:\gamer\mounts\gcs\configs\azahar`
- Both live on rclone mount, so config edits during play persist back through the mounted remote path.

One-shot with explicit Chubbuck placement:

```bash
python infrastructure/windows/orchestrate_windows_host.py --create \
  --vm-city Chubbuck --vm-state Idaho \
  --vm-gpu geforcertx4090-pcie-24gb
```

Reuse existing VM state (no create):

```bash
python infrastructure/windows/orchestrate_windows_host.py
```

Automated validation against a running VM:

```bash
python infrastructure/windows/validate_windows_host.py
```

Skip reboot in quick checks:

```bash
python infrastructure/windows/validate_windows_host.py --skip-reboot
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

## Critical GPU/NVENC fix (Windows)

If Apollo runs as `SYSTEM` (session 0), it can capture `Microsoft Basic Render Driver` at `1Hz` and fall back to software (`libx264`), even when NVIDIA drivers are installed.

This setup now hardens against that by:

1. Installing NVIDIA driver (RTX host must show `NVIDIA GeForce RTX 4090` in `Get-PnpDevice -Class Display`)
2. Disabling/stopping `ApolloService` (SYSTEM service)
3. Launching Apollo via **interactive per-user scheduled tasks** (`/IT`) for both instances:
   - `GamerApollo1` → `sunshine.conf`
   - `GamerApollo2` → `sunshine_2.conf`
4. Pinning Apollo adapter selection in config:
   - `adapter_name = NVIDIA GeForce RTX 4090`
5. Preventing client-agent from spawning Apollo in session0:
   - `APOLLO_MANAGED_EXTERNALLY=true`

Quick verification commands:

```powershell
Get-Process Apollo -IncludeUserName | ft Name,Id,SessionId,UserName,Path -AutoSize
Get-Service ApolloService | ft Name,Status,StartType -AutoSize
Get-PnpDevice -Class Display | ft Status,FriendlyName,InstanceId -AutoSize
```

Expected:
- Apollo processes run as `WIN10-NEW\\user` and **not** session 0 service-owned capture path.
- `ApolloService` is `Disabled`/stopped.
- NVIDIA display adapter status is `OK`.

## Display routing hardening (hide physical monitor while streaming)

Per Sunshine/Apollo display-device options, we now set in `sunshine.conf`:

```ini
dd_configuration_option = ensure_only_display
dd_resolution_option = auto
dd_refresh_rate_option = auto
dd_config_revert_delay = 1500
```

`ensure_only_display` = **“Deactivate other displays and activate only the specified display.”**

## Production readiness target (3-5 minutes)

For sub-5-minute host readiness, use a pre-baked Windows image with:
- NVIDIA driver preinstalled
- Apollo + ShaderGlass preinstalled
- Python + dependencies preinstalled

Then runtime flow is only:
1) Provision VM
2) RDP bootstrap mgmt ports (22/5985)
3) Push manifests/scripts + restart tasks
4) Health check and pair/connect

This avoids repeated large installer downloads during session startup.

## Pairing runbook (do this every time)

### Golden rules

1. Pair against the **right instance**:
   - Apollo1 UI/API: `https://<host>:47990`
   - Apollo2 UI/API: `https://<host>:48990`
2. Use `sunshine.exe` (not `Apollo.exe`) for server runtime.
3. Avoid malformed JSON/quoting when calling API from shell.

### API pairing flow

1. Authenticate (creates session cookie)
2. POST PIN to `/api/pin` with that session

Recommended (automated, avoids remote 403 issues by posting from localhost on VM via SSH):

```bash
python infrastructure/windows/pair_apollo_pin.py \
  --state-file infrastructure/windows/state/windows-vm-manassas.local.json \
  --pin 1234
```

Example (PowerShell, local to VM):

```powershell
[System.Net.ServicePointManager]::ServerCertificateValidationCallback = {$true}
$s = New-Object Microsoft.PowerShell.Commands.WebRequestSession
$login = @{ username = "gamer"; password = "gamer" } | ConvertTo-Json -Compress
Invoke-RestMethod -Method Post -Uri "https://127.0.0.1:47990/api/login" -WebSession $s -ContentType "application/json" -Body $login
$pinBody = @{ pin = "1234" } | ConvertTo-Json -Compress
Invoke-RestMethod -Method Post -Uri "https://127.0.0.1:47990/api/pin" -WebSession $s -ContentType "application/json" -Body $pinBody
```

### Fallback pairing (CLI stdin mode)

```powershell
Set-Location "C:\Program Files\Apollo"
"1234" | .\sunshine.exe "C:\Program Files\Apollo\config\sunshine.conf" -0
```

### Common failure causes

- `401 Unauthorized`: auth cookie/session missing or wrong instance port.
- JSON parse errors in logs: shell quoting broke JSON payload.
- Pairing to wrong instance (A vs B port mix-up).
- `permission denied` when launching apps from Moonlight:
  - Apollo/Sunshine client permissions are per-client cert. If the pairing is not the first/primary client, app-launch permission may be missing by default.
  - Fix from Apollo docs/issues: open Apollo Web UI and explicitly enable app launch permission for that client, or clear paired clients and re-pair in intended order.
- Azahar exits immediately:
  - Do **not** pass `--layout separate` on launch in Apollo app `cmd`; Azahar interprets trailing token as a game path in this setup.
  - Launch plain `azahar.exe` and handle layout from Azahar UI/config.
