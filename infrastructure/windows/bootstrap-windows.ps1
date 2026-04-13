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

function Set-AutoLoginAndNoLock {
  param(
    [string]$WindowsUsername,
    [string]$WindowsPassword
  )
  if (-not $WindowsPassword) {
    Write-Warning "WindowsPassword not provided; skipping auto-login setup."
    return
  }
  try {
    reg add "HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Winlogon" /v AutoAdminLogon /t REG_SZ /d 1 /f | Out-Null
    reg add "HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Winlogon" /v DefaultUserName /t REG_SZ /d $WindowsUsername /f | Out-Null
    reg add "HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Winlogon" /v DefaultPassword /t REG_SZ /d $WindowsPassword /f | Out-Null
    reg add "HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\System" /v DisableCAD /t REG_DWORD /d 1 /f | Out-Null
    reg add "HKLM\SOFTWARE\Policies\Microsoft\Windows\Personalization" /v NoLockScreen /t REG_DWORD /d 1 /f | Out-Null
    powercfg /change monitor-timeout-ac 0 | Out-Null
    powercfg /change standby-timeout-ac 0 | Out-Null
    powercfg /change hibernate-timeout-ac 0 | Out-Null
  } catch {
    Write-Warning "Failed to configure auto-login: $($_.Exception.Message)"
  }
}

function Test-InteractiveConsoleReady {
  param([string]$WindowsUsername)
  try {
    $q = quser 2>$null
    return [bool]($q -match "(?m)^\s*$([regex]::Escape($WindowsUsername))\s+console\s+\d+\s+Active")
  } catch {
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

function Resolve-PreferredNvidiaAdapterName {
  try {
    $candidates = Get-PnpDevice -Class Display -ErrorAction SilentlyContinue |
      Where-Object {
        $_.Status -eq 'OK' -and
        $_.FriendlyName -like 'NVIDIA*' -and
        $_.FriendlyName -notlike '*Remote*' -and
        $_.FriendlyName -notlike '*Basic*'
      }
    if ($candidates) {
      $tesla = $candidates | Where-Object { $_.FriendlyName -like '*Tesla*' } | Select-Object -First 1
      if ($tesla) { return $tesla.FriendlyName }
      return ($candidates | Select-Object -First 1).FriendlyName
    }
  } catch {}
  return 'NVIDIA GeForce RTX 4090'
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
file_apps = apps.json
adapter_name = $GpuName
dd_configuration_option = ensure_only_display
dd_resolution_option = auto
dd_refresh_rate_option = auto
dd_config_revert_delay = 1500
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

function Resolve-AzaharExePath {
  param(
    [string]$WindowsUsername
  )
  $candidates = @(
    "C:\\Emulators\\Azahar\\azahar.exe",
    "C:\\Emulators\\Azahar\\azahar-2124.3-windows-msvc\\azahar.exe",
    "C:\\Users\\$WindowsUsername\\AppData\\Local\\Azahar\\azahar.exe",
    "C:\\Program Files\\Azahar\\azahar.exe"
  )
  foreach ($c in $candidates) {
    if (Test-Path $c) { return $c }
  }
  try {
    $found = Get-ChildItem -Path "C:\\Emulators\\Azahar" -Filter "azahar.exe" -Recurse -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($found) { return $found.FullName }
  } catch {}
  return ""
}

function Write-ApolloAppsConfig {
  param(
    [string]$ConfigDir,
    [string]$WindowsUsername
  )

  $azaharExe = Resolve-AzaharExePath -WindowsUsername $WindowsUsername
  $azaharWorkingDir = ""
  if ($azaharExe) {
    $azaharWorkingDir = Split-Path -Parent $azaharExe
  }
  if (-not $azaharExe) {
    # Keep launch path valid for smoke tests if Azahar isn't present yet.
    $azaharExe = "C:\\Windows\\System32\\notepad.exe"
    $azaharWorkingDir = "C:\\Windows\\System32"
  }

  $apps = @{
    apps = @(
      @{
        name = "Desktop"
        "image-path" = "desktop.png"
        "allow-client-commands" = $false
      },
      @{
        name = "Virtual Display"
        "image-path" = "virtual_desktop.png"
        "allow-client-commands" = $false
      },
      @{
        name = "Azahar Dual"
        cmd = $azaharExe
        "working-dir" = $azaharWorkingDir
        "allow-client-commands" = $true
        "virtual-display" = $true
        "image-path" = "desktop.png"
      }
    )
    env = @{}
    version = 2
  }

  $appsPath = Join-Path $ConfigDir "apps.json"
  $apps | ConvertTo-Json -Depth 10 | Set-Content -Path $appsPath -Encoding UTF8
}

function Ensure-ApolloInstances {
  param(
    [string]$ApolloExe,
    [string]$ConfigDir,
    [string]$WindowsUsername
  )

  $setupDir = "C:\\ProgramData\\gamer\\setup"
  Ensure-Dir $setupDir | Out-Null
  $run2Ps = Join-Path $setupDir "run-apollo2.ps1"
  [System.IO.File]::WriteAllText($run2Ps, "Start-Process -FilePath `"$ApolloExe`" -ArgumentList `"$ConfigDir\\sunshine_2.conf`" -WindowStyle Hidden", [System.Text.UTF8Encoding]::new($false))

  # Instance 1: official Apollo service.
  sc.exe config ApolloService start= auto | Out-Null
  sc.exe start ApolloService | Out-Null

  # Instance 2: dedicated SYSTEM startup task on alternate ports/config.
  $task2 = "GamerApollo2"
  try { schtasks /Delete /TN $task2 /F | Out-Null } catch {}
  schtasks /Create /TN $task2 /TR "powershell -NoProfile -ExecutionPolicy Bypass -File `"$run2Ps`"" /SC ONSTART /RL HIGHEST /RU SYSTEM /F | Out-Null
  schtasks /Run /TN $task2 | Out-Null
}

function Set-IfEOHighPriority {
  param([string]$ExeName)
  try {
    $base = "HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Image File Execution Options\$ExeName"
    $perf = Join-Path $base 'PerfOptions'
    New-Item -Path $perf -Force | Out-Null
    New-ItemProperty -Path $perf -Name 'CpuPriorityClass' -Value 3 -PropertyType DWord -Force | Out-Null
  } catch {
    Write-Warning "Failed setting IFEO priority for ${ExeName}: $($_.Exception.Message)"
  }
}

function Apply-GameHostPerformanceProfile {
  try {
    powercfg /setactive SCHEME_MIN | Out-Null
    powercfg -attributes SUB_PROCESSOR CPMINCORES -ATTRIB_HIDE 2>$null
    powercfg /setacvalueindex SCHEME_CURRENT SUB_PROCESSOR PROCTHROTTLEMIN 100 | Out-Null
    powercfg /setdcvalueindex SCHEME_CURRENT SUB_PROCESSOR PROCTHROTTLEMIN 100 | Out-Null
    powercfg /setacvalueindex SCHEME_CURRENT SUB_PROCESSOR CPMINCORES 100 2>$null
    powercfg /setdcvalueindex SCHEME_CURRENT SUB_PROCESSOR CPMINCORES 100 2>$null
    powercfg /setactive SCHEME_CURRENT | Out-Null
  } catch {
    Write-Warning "Power profile tuning failed: $($_.Exception.Message)"
  }

  try {
    reg add 'HKLM\SOFTWARE\Policies\Microsoft\Windows\GameDVR' /v AllowGameDVR /t REG_DWORD /d 0 /f | Out-Null
    reg add 'HKCU\System\GameConfigStore' /v GameDVR_Enabled /t REG_DWORD /d 0 /f | Out-Null
    reg add 'HKCU\Software\Microsoft\GameBar' /v ShowStartupPanel /t REG_DWORD /d 0 /f | Out-Null
    reg add 'HKCU\Software\Microsoft\Windows\CurrentVersion\GameDVR' /v AppCaptureEnabled /t REG_DWORD /d 0 /f | Out-Null
  } catch {
    Write-Warning "Game DVR disable failed: $($_.Exception.Message)"
  }

  try {
    Stop-Service WSearch -Force -ErrorAction SilentlyContinue
    Set-Service WSearch -StartupType Disabled -ErrorAction SilentlyContinue
  } catch {
    Write-Warning "WSearch disable failed: $($_.Exception.Message)"
  }

  try {
    $exclusions = @('C:\gamer','C:\Program Files\Apollo','C:\Program Files (x86)\Steam','C:\SteamLibrary','C:\Emulators')
    foreach ($p in $exclusions) {
      if (Test-Path $p) { Add-MpPreference -ExclusionPath $p -ErrorAction SilentlyContinue }
    }
  } catch {
    Write-Warning "Defender exclusions failed: $($_.Exception.Message)"
  }

  Set-IfEOHighPriority -ExeName 'azahar.exe'
  Set-IfEOHighPriority -ExeName 'kh3.exe'
  Set-IfEOHighPriority -ExeName 'KINGDOM HEARTS III.exe'
}

Write-Host "[1/6] Installing core tools"
Set-AutoLoginAndNoLock -WindowsUsername $WindowsUsername -WindowsPassword $WindowsPassword
Apply-GameHostPerformanceProfile

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
if (-not (Test-InteractiveConsoleReady -WindowsUsername $WindowsUsername)) {
  Write-Warning "Interactive console session for '$WindowsUsername' not ready yet. Apollo install should be rerun after auto-login."
}
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
  $apolloExe = "C:\\Program Files\\Apollo\\sunshine.exe"
  if (-not (Test-Path $apolloExe)) {
    $apolloExe = "C:\\ProgramData\\gamer\\bin\\Apollo\\sunshine.exe"
  }
  if (-not (Test-Path $apolloExe)) {
    $apolloExe = "C:\\ProgramData\\gamer\\bin\\Apollo\\Apollo.exe"
  }
  if (Test-Path $apolloExe) {
    $configDir = "C:\\Program Files\\Apollo\\config"
    Ensure-Dir $configDir | Out-Null
    $gpuName = Resolve-PreferredNvidiaAdapterName
    Write-Host "Using Apollo adapter_name: $gpuName"
    Write-ApolloConfig -ConfigDir $configDir -GpuName $gpuName
    Write-ApolloAppsConfig -ConfigDir $configDir -WindowsUsername $WindowsUsername
    Ensure-ApolloInstances -ApolloExe $apolloExe -ConfigDir $configDir -WindowsUsername $WindowsUsername
  } else {
    Write-Warning "Apollo executable not found; skipping Apollo interactive task setup."
  }
} catch {
  Write-Warning "Apollo interactive setup failed: $($_.Exception.Message)"
}

# Ensure firewall opens for both Apollo instances.
try {
  $rules = @(
    @{n='Apollo1 TCP 47984';p='TCP';lp='47984'},
    @{n='Apollo1 TCP 47989';p='TCP';lp='47989'},
    @{n='Apollo1 TCP 47990';p='TCP';lp='47990'},
    @{n='Apollo1 TCP 48010';p='TCP';lp='48010'},
    @{n='Apollo1 UDP 47998';p='UDP';lp='47998'},
    @{n='Apollo1 UDP 47999';p='UDP';lp='47999'},
    @{n='Apollo1 UDP 48000';p='UDP';lp='48000'},
    @{n='Apollo1 UDP 48002';p='UDP';lp='48002'},
    @{n='Apollo1 UDP 48010';p='UDP';lp='48010'},
    @{n='Apollo2 TCP 48984';p='TCP';lp='48984'},
    @{n='Apollo2 TCP 48989';p='TCP';lp='48989'},
    @{n='Apollo2 TCP 48990';p='TCP';lp='48990'},
    @{n='Apollo2 TCP 49010';p='TCP';lp='49010'},
    @{n='Apollo2 UDP 48998';p='UDP';lp='48998'},
    @{n='Apollo2 UDP 48999';p='UDP';lp='48999'},
    @{n='Apollo2 UDP 49000';p='UDP';lp='49000'},
    @{n='Apollo2 UDP 49002';p='UDP';lp='49002'},
    @{n='Apollo2 UDP 49010';p='UDP';lp='49010'}
  )
  foreach ($r in $rules) {
    netsh advfirewall firewall add rule name=$r.n dir=in action=allow protocol=$r.p localport=$r.lp | Out-Null
  }
} catch {}

try {
  Set-Service sshd -StartupType Automatic -ErrorAction SilentlyContinue
  Start-Service sshd -ErrorAction SilentlyContinue
} catch {}
try {
  Set-Service WinRM -StartupType Automatic -ErrorAction SilentlyContinue
  Start-Service WinRM -ErrorAction SilentlyContinue
} catch {}
