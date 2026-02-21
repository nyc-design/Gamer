param(
  [string]$RcloneConfigBase64 = '',
  [string]$R2Remote = 'r2:gamer-roms/me',
  [string]$GcsRemote = 'gcs:gamer-data/me',
  [string]$ApolloUsername = 'gamer',
  [string]$ApolloPassword = 'gamer'
)

$ErrorActionPreference = 'Continue'
$ProgressPreference = 'SilentlyContinue'

$base='C:\gamer'
$dirs=@(
  "$base\mounts\r2",
  "$base\mounts\gcs",
  "$base\roms",
  "$base\saves",
  "$base\configs\apollo",
  "$base\configs\azahar",
  "$base\firmware"
)
$dirs | ForEach-Object { New-Item -ItemType Directory -Path $_ -Force | Out-Null }

# rclone
function Get-RcloneExe {
  $cmd = Get-Command rclone.exe -ErrorAction SilentlyContinue
  if ($cmd) { return $cmd.Source }
  $fallback = 'C:\ProgramData\gamer\bin\rclone\rclone.exe'
  if (Test-Path $fallback) { return $fallback }
  return $null
}

if ($RcloneConfigBase64) {
  $cfgDir = "$env:APPDATA\rclone"
  New-Item -ItemType Directory -Path $cfgDir -Force | Out-Null
  [IO.File]::WriteAllBytes("$cfgDir\rclone.conf", [Convert]::FromBase64String($RcloneConfigBase64))
}

# Pull baseline content locally (reliable fallback vs mount flakiness)
$rclone = Get-RcloneExe
if ($rclone) {
  & $rclone sync "$R2Remote" "$base\roms" --fast-list --transfers 8 --checkers 16 --ignore-errors
  & $rclone sync "$GcsRemote/configs/apollo" "$base\configs\apollo" --fast-list --ignore-errors
  & $rclone sync "$GcsRemote/configs/azahar" "$base\configs\azahar" --fast-list --ignore-errors
} else {
  Write-Warning 'rclone not found; skipping remote sync'
}

# Apollo config + credentials
$apollo='C:\Program Files\Apollo\sunshine.exe'
$cfg='C:\Program Files\Apollo\config'
New-Item -ItemType Directory -Path $cfg -Force | Out-Null
if (Test-Path "$base\configs\apollo\sunshine.conf") { Copy-Item "$base\configs\apollo\sunshine.conf" "$cfg\sunshine.conf" -Force }
if (Test-Path "$base\configs\apollo\apps.json") { Copy-Item "$base\configs\apollo\apps.json" "$cfg\apps.json" -Force }
if (-not (Test-Path "$cfg\sunshine.conf")) {
  @"
sunshine_name = GamerApollo
port = 47989
file_state = sunshine_state.json
log_path = sunshine.log
file_apps = apps.json
adapter_name = NVIDIA GeForce RTX 4090
dd_configuration_option = ensure_only_display
dd_resolution_option = auto
dd_refresh_rate_option = auto
dd_config_revert_delay = 1500
audio_sink = Speakers (VB-Audio Virtual Cable)
stream_audio = true
install_steam_audio_drivers = false
"@ | Set-Content "$cfg\sunshine.conf" -Encoding UTF8
}
& $apollo --creds $ApolloUsername $ApolloPassword | Out-Null

# Make Apollo use managed config from gamer dir (copy-back for now)
Copy-Item "$cfg\sunshine.conf" "$base\configs\apollo\sunshine.conf" -Force
if (Test-Path "$cfg\apps.json") { Copy-Item "$cfg\apps.json" "$base\configs\apollo\apps.json" -Force }

Restart-Service ApolloService -Force
Start-Sleep -Seconds 2

Get-CimInstance Win32_Process | Where-Object { $_.Name -eq 'sunshine.exe' } | Select ProcessId,CommandLine | Format-Table -AutoSize
netstat -ano | findstr LISTENING | findstr 47990
