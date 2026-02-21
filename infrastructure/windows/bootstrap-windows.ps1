param(
  [string]$ApolloInstallerUrl = "",
  [string]$ShaderGlassInstallerUrl = "",
  [string]$RcloneConfigBase64 = "",
  [string]$WindowsUsername = "user",
  [string]$WindowsPassword = ""
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

function Write-ApolloConfig {
  param(
    [string]$ConfigDir,
    [string]$GpuName
  )

  Ensure-Dir $ConfigDir | Out-Null

  $sunshineConf = @"
sunshine_name = Apollo
port = 47989
file_state = sunshine_state.json
log_path = sunshine.log
adapter_name = $GpuName
"@
  $sunshine2Conf = @"
port = 48989
sunshine_name = Apollo2
file_state = apollo_state2.json
log_path = apollo2.log
adapter_name = $GpuName
"@

  [System.IO.File]::WriteAllText((Join-Path $ConfigDir "sunshine.conf"), $sunshineConf, [System.Text.UTF8Encoding]::new($false))
  [System.IO.File]::WriteAllText((Join-Path $ConfigDir "sunshine_2.conf"), $sunshine2Conf, [System.Text.UTF8Encoding]::new($false))
}

function Ensure-ApolloInteractiveTasks {
  param(
    [string]$ApolloExe,
    [string]$ConfigDir,
    [string]$Username,
    [string]$Password
  )

  # Apollo service in SYSTEM/session0 frequently captures Microsoft Basic Render Driver.
  # We force interactive user-session launch via scheduled tasks.
  try {
    Stop-Service ApolloService -Force -ErrorAction SilentlyContinue
    Set-Service ApolloService -StartupType Disabled -ErrorAction SilentlyContinue
  } catch {}

  try { taskkill /IM sunshinesvc.exe /F | Out-Null } catch {}
  try { taskkill /IM Apollo.exe /F | Out-Null } catch {}

  $task1 = "GamerApollo1"
  $task2 = "GamerApollo2"
  try { schtasks /Delete /TN $task1 /F | Out-Null } catch {}
  try { schtasks /Delete /TN $task2 /F | Out-Null } catch {}

  if (-not $Password) {
    Write-Warning "WindowsPassword not provided; cannot create /IT Apollo tasks."
    return
  }

  $setupDir = "C:\\ProgramData\\gamer\\setup"
  Ensure-Dir $setupDir | Out-Null
  $run1 = Join-Path $setupDir "run-apollo1.cmd"
  $run2 = Join-Path $setupDir "run-apollo2.cmd"
  [System.IO.File]::WriteAllText($run1, "`"$ApolloExe`" `"$ConfigDir\\sunshine.conf`"`r`n", [System.Text.UTF8Encoding]::new($false))
  [System.IO.File]::WriteAllText($run2, "`"$ApolloExe`" `"$ConfigDir\\sunshine_2.conf`"`r`n", [System.Text.UTF8Encoding]::new($false))

  schtasks /Create /TN $task1 /TR $run1 /SC ONLOGON /RL HIGHEST /RU $Username /RP $Password /F /IT | Out-Null
  schtasks /Create /TN $task2 /TR $run2 /SC ONLOGON /RL HIGHEST /RU $Username /RP $Password /F /IT | Out-Null
  schtasks /Run /TN $task1 | Out-Null
  schtasks /Run /TN $task2 | Out-Null
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
      $fallbackAlt = Join-Path $fallbackDir "Apollo-installer.exe"
      Stop-ProcessIfRunningByPath $fallbackExe
      if (Test-Path $fallbackExe) {
        try {
          Remove-Item $fallbackExe -Force -ErrorAction SilentlyContinue
        } catch {}
      }
      try {
        Copy-Item $tmp $fallbackExe -Force
        Write-Host "Apollo fallback binary staged at $fallbackDir\\Apollo.exe"
      } catch {
        Copy-Item $tmp $fallbackAlt -Force
        Write-Warning "Could not overwrite Apollo.exe; staged fallback at $fallbackAlt"
      }
    }
  } catch {
    Write-Warning "Apollo install failed: $($_.Exception.Message)"
    try {
      $fallbackDir = "C:\\ProgramData\\gamer\\bin\\Apollo"
      Ensure-Dir $fallbackDir | Out-Null
      if (Test-Path "$env:TEMP\\apollo-installer.exe") {
        $fallbackExe = Join-Path $fallbackDir "Apollo.exe"
        $fallbackAlt = Join-Path $fallbackDir "Apollo-installer.exe"
        try {
          Copy-Item "$env:TEMP\\apollo-installer.exe" $fallbackExe -Force
          Write-Host "Apollo fallback binary staged after exception: $fallbackExe"
        } catch {
          Copy-Item "$env:TEMP\\apollo-installer.exe" $fallbackAlt -Force
          Write-Warning "Apollo.exe locked; staged fallback after exception: $fallbackAlt"
        }
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

Write-Host "[extra] Configuring Apollo for interactive user-session launch (GPU capture)"
try {
  $apolloExe = "C:\\Program Files\\Apollo\\Apollo.exe"
  if (-not (Test-Path $apolloExe)) {
    $apolloExe = "C:\\ProgramData\\gamer\\bin\\Apollo\\Apollo.exe"
  }
  if (Test-Path $apolloExe) {
    $configDir = "C:\\Program Files\\Apollo\\config"
    Ensure-Dir $configDir | Out-Null
    Write-ApolloConfig -ConfigDir $configDir -GpuName "NVIDIA GeForce RTX 4090"
    Ensure-ApolloInteractiveTasks -ApolloExe $apolloExe -ConfigDir $configDir -Username $WindowsUsername -Password $WindowsPassword
  } else {
    Write-Warning "Apollo executable not found; skipping Apollo interactive task setup."
  }
} catch {
  Write-Warning "Apollo interactive setup failed: $($_.Exception.Message)"
}

try {
  Set-Service sshd -StartupType Automatic -ErrorAction SilentlyContinue
  Start-Service sshd -ErrorAction SilentlyContinue
} catch {}
try {
  Set-Service WinRM -StartupType Automatic -ErrorAction SilentlyContinue
  Start-Service WinRM -ErrorAction SilentlyContinue
} catch {}
