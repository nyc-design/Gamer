param(
  [string]$ApolloInstallerUrl = "",
  [string]$ShaderGlassInstallerUrl = "https://github.com/mausimus/ShaderGlass/releases/latest/download/ShaderGlass.msi",
  [string]$RcloneConfigBase64 = ""
)

$ErrorActionPreference = "Stop"

function Install-WingetPackage($id) {
  Write-Host "Installing $id"
  winget install --id $id --accept-package-agreements --accept-source-agreements --silent --disable-interactivity
}

Write-Host "[1/6] Installing core tools"
Install-WingetPackage "Rclone.Rclone"
Install-WingetPackage "AutoHotkey.AutoHotkey"

Write-Host "[2/6] Installing ShaderGlass"
if ($ShaderGlassInstallerUrl -ne "") {
  $tmp = "$env:TEMP\\shaderglass.msi"
  Invoke-WebRequest -Uri $ShaderGlassInstallerUrl -OutFile $tmp
  Start-Process msiexec.exe -ArgumentList "/i `"$tmp`" /qn" -Wait
}

Write-Host "[3/6] Installing Apollo"
if ($ApolloInstallerUrl -ne "") {
  $tmp = "$env:TEMP\\apollo-installer.exe"
  Invoke-WebRequest -Uri $ApolloInstallerUrl -OutFile $tmp
  Start-Process $tmp -ArgumentList "/S" -Wait
} else {
  Write-Warning "ApolloInstallerUrl not provided; skipping automatic install"
}

Write-Host "[4/6] Preparing gamer folders"
$paths = @("D:\\gamer\\roms","D:\\gamer\\saves","D:\\gamer\\configs","D:\\gamer\\firmware","D:\\gamer\\steam")
foreach ($p in $paths) { New-Item -ItemType Directory -Path $p -Force | Out-Null }

Write-Host "[5/6] rclone config"
if ($RcloneConfigBase64 -ne "") {
  $cfgDir = "$env:APPDATA\\rclone"
  New-Item -ItemType Directory -Path $cfgDir -Force | Out-Null
  [IO.File]::WriteAllBytes("$cfgDir\\rclone.conf", [Convert]::FromBase64String($RcloneConfigBase64))
}

Write-Host "[6/6] Done"
Write-Host "Now install and start the client agent service"
