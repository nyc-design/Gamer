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
  "$base\configs\apollo2",
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

function Ensure-SymlinkDir {
  param([string]$LinkPath,[string]$TargetPath)
  New-Item -ItemType Directory -Path $TargetPath -Force | Out-Null
  if (Test-Path $LinkPath) {
    $item = Get-Item -LiteralPath $LinkPath -Force
    if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) {
      Remove-Item -LiteralPath $LinkPath -Force -ErrorAction SilentlyContinue
    } elseif ($item.PSIsContainer) {
      $backup = "$LinkPath.bak.$((Get-Date).ToString('yyyyMMddHHmmss'))"
      Move-Item -LiteralPath $LinkPath -Destination $backup -Force
    } else {
      Remove-Item -LiteralPath $LinkPath -Force
    }
  }
  New-Item -ItemType SymbolicLink -Path $LinkPath -Target $TargetPath | Out-Null
}

if ($RcloneConfigBase64) {
  $cfgDir = "$env:APPDATA\rclone"
  New-Item -ItemType Directory -Path $cfgDir -Force | Out-Null
  [IO.File]::WriteAllBytes("$cfgDir\rclone.conf", [Convert]::FromBase64String($RcloneConfigBase64))
}

# Start/refresh rclone mounts (for live config sync back to cloud)
$rclone = Get-RcloneExe
if ($rclone) {
  Get-CimInstance Win32_Process | Where-Object { $_.Name -eq 'rclone.exe' } | Stop-Process -Force -ErrorAction SilentlyContinue
  Start-Sleep -Seconds 1
  Start-Process -FilePath $rclone -ArgumentList @(
    'mount', $R2Remote, "$base\mounts\r2",
    '--vfs-cache-mode','full',
    '--dir-cache-time','10s',
    '--poll-interval','10s',
    '--network-mode'
  ) -WindowStyle Hidden
  Start-Process -FilePath $rclone -ArgumentList @(
    'mount', $GcsRemote, "$base\mounts\gcs",
    '--vfs-cache-mode','full',
    '--dir-cache-time','10s',
    '--poll-interval','10s',
    '--network-mode'
  ) -WindowStyle Hidden
  Start-Sleep -Seconds 3
  # Lightweight fallback sync for ROMs if mount read is slow
  & $rclone copy "$R2Remote" "$base\roms" --max-transfer 5G --transfers 8 --checkers 16 --ignore-errors
} else {
  Write-Warning 'rclone not found; skipping remote sync'
}

# Apollo config + credentials
$apollo='C:\Program Files\Apollo\sunshine.exe'
$cfg='C:\Program Files\Apollo\config'
Stop-Service ApolloService -Force -ErrorAction SilentlyContinue
Ensure-SymlinkDir -LinkPath $cfg -TargetPath "$base\mounts\gcs\configs\apollo"

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
if (-not (Test-Path "$cfg\sunshine_2.conf")) {
  @"
sunshine_name = GamerApollo2
port = 48989
file_state = sunshine_state2.json
log_path = sunshine2.log
file_apps = apps2.json
adapter_name = NVIDIA GeForce RTX 4090
dd_configuration_option = ensure_only_display
dd_resolution_option = auto
dd_refresh_rate_option = auto
dd_config_revert_delay = 1500
audio_sink = Speakers (VB-Audio Virtual Cable)
stream_audio = true
install_steam_audio_drivers = false
"@ | Set-Content "$cfg\sunshine_2.conf" -Encoding UTF8
}
& $apollo --creds $ApolloUsername $ApolloPassword | Out-Null

# Azahar config symlink to rclone-backed path
$azCfg = 'C:\Users\user\AppData\Roaming\Azahar'
Ensure-SymlinkDir -LinkPath $azCfg -TargetPath "$base\mounts\gcs\configs\azahar"

Restart-Service ApolloService -Force
Start-Sleep -Seconds 2

Get-CimInstance Win32_Process | Where-Object { $_.Name -eq 'sunshine.exe' } | Select ProcessId,CommandLine | Format-Table -AutoSize
netstat -ano | findstr LISTENING | findstr 47990
