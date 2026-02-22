param(
  [string]$RcloneConfigBase64 = '',
  [string]$R2Remote = 'r2:gamer-roms/me',
  [string]$GcsRemote = 'gcs:gamer-data/me',
  [string]$ApolloInstallerUrl = '',
  [string]$ApolloUsername = 'gamer',
  [string]$ApolloPassword = 'gamer',
  [string]$WindowsUsername = 'user'
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

function Resolve-GitHubLatestAssetUrl {
  param(
    [string]$Repo,
    [string]$AssetRegex
  )
  try {
    $release = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases/latest" -Headers @{ 'User-Agent'='gamer-windows-setup' }
    foreach ($asset in $release.assets) {
      if ($asset.name -match $AssetRegex) {
        return $asset.browser_download_url
      }
    }
  } catch {
    Write-Warning "Failed to resolve latest release for ${Repo}: $($_.Exception.Message)"
  }
  return ''
}

function Ensure-ApolloInstalled {
  param(
    [string]$InstallerUrl
  )
  $sunshinePath = 'C:\Program Files\Apollo\sunshine.exe'
  if (Test-Path $sunshinePath) { return $true }

  if (-not $InstallerUrl) {
    $InstallerUrl = Resolve-GitHubLatestAssetUrl -Repo 'ClassicOldSong/Apollo' -AssetRegex '\.exe$'
  }
  if (-not $InstallerUrl) {
    Write-Warning 'Apollo installer URL unavailable.'
    return $false
  }

  try {
    $installer = 'C:\ProgramData\gamer\setup\apollo-installer.exe'
    Invoke-WebRequest -Uri $InstallerUrl -OutFile $installer
    $p = Start-Process -FilePath $installer -ArgumentList '/S' -Wait -PassThru
    if ($p.ExitCode -ne 0 -and -not (Test-Path $sunshinePath)) {
      $p2 = Start-Process -FilePath $installer -ArgumentList '/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART' -Wait -PassThru
      Write-Host "Apollo fallback silent exit code: $($p2.ExitCode)"
    }
  } catch {
    Write-Warning "Apollo install failed: $($_.Exception.Message)"
  }
  return (Test-Path $sunshinePath)
}

function Ensure-ApolloAutostart {
  param(
    [string]$ApolloExePath,
    [string]$ConfigDir
  )

  try {
    # Primary Apollo instance (official service) should always start at boot.
    sc.exe config ApolloService start= auto | Out-Null
  } catch {}

  try {
    # Secondary instance for dual-screen should start on boot using alternate config.
    $setupDir = 'C:\ProgramData\gamer\setup'
    New-Item -ItemType Directory -Path $setupDir -Force | Out-Null
    $run2 = Join-Path $setupDir 'run-apollo2.cmd'
    @"
@echo off
cd /d "C:\Program Files\Apollo"
"$ApolloExePath" "$ConfigDir\sunshine_2.conf"
"@ | Set-Content $run2 -Encoding Ascii

    schtasks /Delete /TN GamerApollo2 /F 2>$null | Out-Null
    schtasks /Create /TN GamerApollo2 /TR $run2 /SC ONSTART /RU SYSTEM /RL HIGHEST /F | Out-Null
  } catch {
    Write-Warning "Failed to configure Apollo secondary autostart: $($_.Exception.Message)"
  }
}

# Ensure this step runs after user auto-login (interactive console session available).
$interactiveReady = $false
try {
  $q = quser 2>$null
  if ($q -match "(?m)^\s*$([regex]::Escape($WindowsUsername))\s+console\s+\d+\s+Active") {
    $interactiveReady = $true
  }
} catch {}
if (-not $interactiveReady) {
  throw "Base step requires active console session for user '$WindowsUsername'. Ensure auto-login has occurred, then rerun deploy_base."
}

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
if (-not (Ensure-ApolloInstalled -InstallerUrl $ApolloInstallerUrl)) {
  throw 'Apollo is not installed; cannot continue base setup.'
}
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
Ensure-ApolloAutostart -ApolloExePath $apollo -ConfigDir $cfg
& $apollo --creds $ApolloUsername $ApolloPassword | Out-Null

# Azahar config symlink to rclone-backed path
$azCfg = 'C:\Users\user\AppData\Roaming\Azahar'
Ensure-SymlinkDir -LinkPath $azCfg -TargetPath "$base\mounts\gcs\configs\azahar"

Restart-Service ApolloService -Force
Start-Sleep -Seconds 2

Get-CimInstance Win32_Process | Where-Object { $_.Name -eq 'sunshine.exe' } | Select ProcessId,CommandLine | Format-Table -AutoSize
netstat -ano | findstr LISTENING | findstr 47990
