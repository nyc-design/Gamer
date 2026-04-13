param(
  [string]$AzaharReleaseUrl = '',
  [string]$GcsConfigRemote = 'gcs:gamer-data/me/configs/azahar'
)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
$root='C:\Emulators\Azahar'
New-Item -ItemType Directory -Path $root -Force | Out-Null
Get-Process azahar -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Start-Sleep -Seconds 1

function Get-RcloneExe {
  $cmd = Get-Command rclone.exe -ErrorAction SilentlyContinue
  if ($cmd) { return $cmd.Source }
  $fallback = 'C:\ProgramData\gamer\bin\rclone\rclone.exe'
  if (Test-Path $fallback) { return $fallback }
  return $null
}

if (-not $AzaharReleaseUrl) {
  $release = Invoke-RestMethod -Uri 'https://api.github.com/repos/azahar-emu/azahar/releases/latest' -Headers @{ 'User-Agent'='gamer-setup' }
  $asset = $release.assets | Where-Object { $_.name -match 'windows-msvc.*\.zip$' } | Select-Object -First 1
  if (-not $asset) { throw 'Azahar windows-msvc asset not found' }
  $AzaharReleaseUrl = $asset.browser_download_url
}

$zip = "$root\azahar.zip"
Invoke-WebRequest -Uri $AzaharReleaseUrl -OutFile $zip
if (Test-Path "$root\azahar-2124.3-windows-msvc") { Remove-Item "$root\azahar-2124.3-windows-msvc" -Recurse -Force -ErrorAction SilentlyContinue }
Expand-Archive -Path $zip -DestinationPath $root -Force
Remove-Item $zip -Force -ErrorAction SilentlyContinue

$exe = Get-ChildItem -Path $root -Filter azahar.exe -Recurse | Select-Object -First 1
if (-not $exe) { throw 'azahar.exe not found after install' }
$exePath = $exe.FullName
$exeDir = $exe.DirectoryName

# Sync config from remote if available
New-Item -ItemType Directory -Path 'C:\Users\user\AppData\Roaming\Azahar' -Force | Out-Null
$rclone = Get-RcloneExe
if ($rclone) {
  & $rclone sync "$GcsConfigRemote" 'C:\Users\user\AppData\Roaming\Azahar' --fast-list --ignore-errors
}

# Force GPU-friendly stable settings
$cfg='C:\Users\user\AppData\Roaming\Azahar\config\qt-config.ini'
if (Test-Path $cfg) {
  $text=Get-Content -Path $cfg -Raw
  $text=[regex]::Replace($text,'(?m)^graphics_api=.*$','graphics_api=0')
  $text=[regex]::Replace($text,'(?m)^spirv_shader_gen=.*$','spirv_shader_gen=false')
  $text=[regex]::Replace($text,'(?m)^use_hw_shader=.*$','use_hw_shader=true')
  $text=[regex]::Replace($text,'(?m)^use_disk_shader_cache=.*$','use_disk_shader_cache=true')
  Set-Content -Path $cfg -Value $text -Encoding UTF8
}

# Update Apollo app entry
$apps='C:\Program Files\Apollo\config\apps.json'
if (Test-Path $apps) {
  $json = Get-Content -Path $apps -Raw | ConvertFrom-Json
  $az = $json.apps | Where-Object { $_.name -eq 'Azahar Dual' } | Select-Object -First 1
  if (-not $az) {
    $json.apps += @{ name='Azahar Dual'; cmd=$exePath; 'working-dir'=$exeDir; 'allow-client-commands'=$true; 'virtual-display'=$true; 'image-path'='desktop.png' }
  } else {
    $az.cmd = $exePath
    $az.'working-dir' = $exeDir
    $az.'virtual-display' = $true
    $az.'allow-client-commands' = $true
  }
  $json | ConvertTo-Json -Depth 10 | Set-Content -Path $apps -Encoding UTF8
}

Write-Host "AZAHAR_EXE=$exePath"
Write-Host "AZAHAR_DIR=$exeDir"
