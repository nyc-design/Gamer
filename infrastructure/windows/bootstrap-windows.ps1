param(
  [string]$ApolloInstallerUrl = "",
  [string]$ShaderGlassInstallerUrl = "https://github.com/mausimus/ShaderGlass/releases/latest/download/ShaderGlass.msi",
  [string]$RcloneConfigBase64 = ""
)

$ErrorActionPreference = "Continue"

function Install-WingetPackage($id) {
  try {
    Write-Host "Installing $id via winget"
    winget install --id $id --accept-package-agreements --accept-source-agreements --silent --disable-interactivity
    return $true
  } catch {
    Write-Warning "winget install failed for ${id}: $($_.Exception.Message)"
    return $false
  }
}

function Ensure-Dir($path) {
  try {
    New-Item -ItemType Directory -Path $path -Force | Out-Null
    return $true
  } catch {
    Write-Warning "Failed to create directory ${path}: $($_.Exception.Message)"
    return $false
  }
}

Write-Host "[1/6] Installing core tools"
$null = Install-WingetPackage "Python.Python.3.12"
$null = Install-WingetPackage "Rclone.Rclone"
$null = Install-WingetPackage "AutoHotkey.AutoHotkey"

if (-not (Get-Command python -ErrorAction SilentlyContinue)) {
  Write-Warning "python not found after winget install; using python.org installer fallback"
  try {
    $pyInstaller = "$env:TEMP\\python-installer.exe"
    Invoke-WebRequest -Uri "https://www.python.org/ftp/python/3.12.8/python-3.12.8-amd64.exe" -OutFile $pyInstaller
    Start-Process $pyInstaller -ArgumentList "/quiet InstallAllUsers=1 PrependPath=1 Include_test=0" -Wait
  } catch {
    Write-Warning "Python fallback install failed: $($_.Exception.Message)"
  }
}

Write-Host "[2/6] Installing ShaderGlass"
if ($ShaderGlassInstallerUrl -ne "") {
  try {
    $tmp = "$env:TEMP\\shaderglass.msi"
    Invoke-WebRequest -Uri $ShaderGlassInstallerUrl -OutFile $tmp
    Start-Process msiexec.exe -ArgumentList "/i `"$tmp`" /qn" -Wait
  } catch {
    Write-Warning "ShaderGlass install failed: $($_.Exception.Message)"
  }
}

Write-Host "[3/6] Installing Apollo"
if ($ApolloInstallerUrl -ne "") {
  try {
    $tmp = "$env:TEMP\\apollo-installer.exe"
    Invoke-WebRequest -Uri $ApolloInstallerUrl -OutFile $tmp
    Start-Process $tmp -ArgumentList "/S" -Wait
  } catch {
    Write-Warning "Apollo install failed: $($_.Exception.Message)"
  }
} else {
  Write-Warning "ApolloInstallerUrl not provided; skipping automatic install"
}

Write-Host "[4/6] Preparing gamer folders"
$baseDrive = "D:"
if (-not (Test-Path "D:\\")) {
  $baseDrive = "C:"
}
$paths = @(
  "$baseDrive\\gamer\\roms",
  "$baseDrive\\gamer\\saves",
  "$baseDrive\\gamer\\configs",
  "$baseDrive\\gamer\\firmware",
  "$baseDrive\\gamer\\steam"
)
foreach ($p in $paths) { $null = Ensure-Dir $p }

Write-Host "[5/6] rclone config"
if ($RcloneConfigBase64 -ne "") {
  $cfgDir = "$env:APPDATA\\rclone"
  if (Ensure-Dir $cfgDir) {
    [IO.File]::WriteAllBytes("$cfgDir\\rclone.conf", [Convert]::FromBase64String($RcloneConfigBase64))
  }
}

Write-Host "[6/6] Done"
Write-Host "Now install and start the client agent service"
