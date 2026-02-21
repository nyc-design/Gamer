param(
  [string]$ApolloInstallerUrl = "",
  [string]$ShaderGlassInstallerUrl = "",
  [string]$RcloneConfigBase64 = ""
)

$ErrorActionPreference = "Continue"
[Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor 3072
$ProgressPreference = "SilentlyContinue"

if ($ApolloInstallerUrl) { $ApolloInstallerUrl = $ApolloInstallerUrl.Trim("'`"") }
if ($ShaderGlassInstallerUrl) { $ShaderGlassInstallerUrl = $ShaderGlassInstallerUrl.Trim("'`"") }

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

function Get-GitHubLatestAssetUrl($repo, $assetPattern) {
  try {
    $release = Invoke-RestMethod -Uri "https://api.github.com/repos/$repo/releases/latest" -Headers @{ "User-Agent" = "gamer-bootstrap" }
    foreach ($asset in $release.assets) {
      if ($asset.name -match $assetPattern) {
        return $asset.browser_download_url
      }
    }
  } catch {
    Write-Warning "Failed to resolve latest asset for ${repo}: $($_.Exception.Message)"
  }
  return ""
}

function Download-File($url, $outFile) {
  $wc = New-Object System.Net.WebClient
  $wc.Headers.Add("User-Agent", "gamer-bootstrap")
  $wc.DownloadFile($url, $outFile)
}

function Stop-ProcessIfRunningByPath($exePath) {
  try {
    $procs = Get-CimInstance Win32_Process | Where-Object { $_.ExecutablePath -eq $exePath }
    foreach ($p in $procs) {
      Stop-Process -Id $p.ProcessId -Force -ErrorAction SilentlyContinue
    }
  } catch {}
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
if ($ShaderGlassInstallerUrl -eq "") {
  $ShaderGlassInstallerUrl = Get-GitHubLatestAssetUrl "mausimus/ShaderGlass" "win-x64\\.zip$"
}
Write-Host "ShaderGlass URL: $ShaderGlassInstallerUrl"
if ($ShaderGlassInstallerUrl -ne "") {
  try {
    $tmp = "$env:TEMP\\shaderglass.zip"
    $dest = "C:\\Program Files\\ShaderGlass"
    Download-File $ShaderGlassInstallerUrl $tmp
    Ensure-Dir $dest | Out-Null
    Expand-Archive -Path $tmp -DestinationPath $dest -Force
    Write-Host "ShaderGlass extracted to $dest"
  } catch {
    Write-Warning "ShaderGlass install failed: $($_.Exception.Message)"
  }
} else {
  Write-Warning "ShaderGlass installer URL not found; skipping."
}

Write-Host "[3/6] Installing Apollo"
if ($ApolloInstallerUrl -eq "") {
  $ApolloInstallerUrl = Get-GitHubLatestAssetUrl "ClassicOldSong/Apollo" "\\.exe$"
}
Write-Host "Apollo URL: $ApolloInstallerUrl"
if ($ApolloInstallerUrl -ne "") {
  try {
    $tmp = "$env:TEMP\\apollo-installer.exe"
    $fallbackDir = "C:\\ProgramData\\gamer\\bin\\Apollo"
    Ensure-Dir $fallbackDir | Out-Null
    Download-File $ApolloInstallerUrl $tmp
    # Apollo release installers are NSIS/Inno-like. Try common silent switches.
    $proc = Start-Process $tmp -ArgumentList "/S" -Wait -PassThru
    if ($proc.ExitCode -ne 0) {
      Write-Warning "Apollo /S exit code: $($proc.ExitCode). Retrying /VERYSILENT /SUPPRESSMSGBOXES /NORESTART"
      $proc2 = Start-Process $tmp -ArgumentList "/VERYSILENT /SUPPRESSMSGBOXES /NORESTART" -Wait -PassThru
      if ($proc2.ExitCode -ne 0) {
        Write-Warning "Apollo silent installer exits non-zero ($($proc2.ExitCode)); using portable fallback copy"
      }
    }
    if (-not (Test-Path "C:\\Program Files\\Apollo\\Apollo.exe")) {
      $fallbackExe = Join-Path $fallbackDir "Apollo.exe"
      Stop-ProcessIfRunningByPath $fallbackExe
      if (Test-Path $fallbackExe) {
        try {
          Remove-Item $fallbackExe -Force -ErrorAction SilentlyContinue
        } catch {}
      }
      Copy-Item $tmp $fallbackExe -Force
      Write-Host "Apollo fallback binary staged at $fallbackDir\\Apollo.exe"
    }
  } catch {
    Write-Warning "Apollo install failed: $($_.Exception.Message)"
    try {
      $fallbackDir = "C:\\ProgramData\\gamer\\bin\\Apollo"
      Ensure-Dir $fallbackDir | Out-Null
      if (Test-Path "$env:TEMP\\apollo-installer.exe") {
        Copy-Item "$env:TEMP\\apollo-installer.exe" (Join-Path $fallbackDir "Apollo.exe") -Force
        Write-Host "Apollo fallback binary staged after exception: $fallbackDir\\Apollo.exe"
      }
    } catch {}
  }
} else {
  Write-Warning "Apollo installer URL not found; skipping automatic install"
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
