param(
  [string]$NvidiaDriverUrl = '',
  [string]$WindowsUsername = 'user',
  [string]$WindowsPassword = ''
)

$ErrorActionPreference = 'Continue'
[Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor 3072
$ProgressPreference = 'SilentlyContinue'

New-Item -ItemType Directory -Path 'C:\ProgramData\gamer\setup' -Force | Out-Null
$setupDir = 'C:\ProgramData\gamer\setup'

function Invoke-InstallerExe {
  param(
    [string]$Name,
    [string]$Url,
    [string[]]$InstallerArgs = @('/quiet','/norestart')
  )
  try {
    $out = Join-Path $setupDir ("{0}.exe" -f $Name)
    Invoke-WebRequest -Uri $Url -OutFile $out
    $p = Start-Process -FilePath $out -ArgumentList $InstallerArgs -Wait -PassThru
    Write-Host "$Name installer exit code: $($p.ExitCode)"
  } catch {
    Write-Warning "$Name install failed: $($_.Exception.Message)"
  }
}

function Install-MediaFeaturePack {
  try {
    $cap = Get-WindowsCapability -Online -Name 'Media.MediaFeaturePack~~~~0.0.1.0' -ErrorAction SilentlyContinue
    if ($cap -and $cap.State -eq 'Installed') { return $true }

    Write-Host 'Installing Media Feature Pack capability...'
    $dismLog = Join-Path $setupDir 'media_pack_install.log'

    # Attempt direct install first.
    cmd /c "dism /online /add-capability /capabilityname:Media.MediaFeaturePack~~~~0.0.1.0 > `"$dismLog`" 2>&1"
    $cap = Get-WindowsCapability -Online -Name 'Media.MediaFeaturePack~~~~0.0.1.0' -ErrorAction SilentlyContinue
    if ($cap -and $cap.State -eq 'Installed') { return $true }

    # Fallback via SYSTEM scheduled task for cases where SSH session lacks required token.
    Write-Warning 'Direct DISM capability install did not succeed. Retrying via SYSTEM scheduled task.'
    $taskScript = Join-Path $setupDir 'install_media_pack.ps1'
    @"
$ErrorActionPreference='Continue'
cmd /c dism /online /add-capability /capabilityname:Media.MediaFeaturePack~~~~0.0.1.0 > "$dismLog" 2>&1
"@ | Set-Content $taskScript -Encoding Ascii

    schtasks /Delete /TN GamerInstallMediaPack /F 2>$null | Out-Null
    schtasks /Create /TN GamerInstallMediaPack /TR "powershell -NoProfile -ExecutionPolicy Bypass -File $taskScript" /SC ONCE /ST 00:00 /RU SYSTEM /RL HIGHEST /F | Out-Null
    schtasks /Run /TN GamerInstallMediaPack | Out-Null

    Start-Sleep -Seconds 120
    $cap = Get-WindowsCapability -Online -Name 'Media.MediaFeaturePack~~~~0.0.1.0' -ErrorAction SilentlyContinue
    return [bool]($cap -and $cap.State -eq 'Installed')
  } catch {
    Write-Warning "Media Feature Pack install failed: $($_.Exception.Message)"
    return $false
  }
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
  Write-Host "Applying performance profile (power/capture/services/priority)..."
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

function Invoke-WindowsActivationPlaceholder {
  <#
    Placeholder only.
    Intentionally does nothing so teams can implement their own activation flow.
    Keep this function early in startup so activation can happen before app/runtime installs.
  #>
  Write-Host "Windows activation placeholder: no activation action configured."
}

# Core services
Set-Service sshd -StartupType Automatic -ErrorAction SilentlyContinue
Start-Service sshd -ErrorAction SilentlyContinue
Set-Service WinRM -StartupType Automatic -ErrorAction SilentlyContinue
Start-Service WinRM -ErrorAction SilentlyContinue
netsh advfirewall firewall add rule name='OpenSSH Server (sshd)' dir=in action=allow protocol=TCP localport=22 | Out-Null
netsh advfirewall firewall add rule name='WinRM HTTP 5985' dir=in action=allow protocol=TCP localport=5985 | Out-Null

# Audio services
Set-Service AudioEndpointBuilder -StartupType Automatic -ErrorAction SilentlyContinue
Start-Service AudioEndpointBuilder -ErrorAction SilentlyContinue
Set-Service Audiosrv -StartupType Automatic -ErrorAction SilentlyContinue
Start-Service Audiosrv -ErrorAction SilentlyContinue

# Configure auto-login / no lock for headless game sessions.
if ($WindowsPassword) {
  try {
    reg add "HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Winlogon" /v AutoAdminLogon /t REG_SZ /d 1 /f | Out-Null
    reg add "HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Winlogon" /v DefaultUserName /t REG_SZ /d $WindowsUsername /f | Out-Null
    reg add "HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Winlogon" /v DefaultPassword /t REG_SZ /d $WindowsPassword /f | Out-Null
    reg add "HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\System" /v DisableCAD /t REG_DWORD /d 1 /f | Out-Null
    reg add "HKLM\SOFTWARE\Policies\Microsoft\Windows\Personalization" /v NoLockScreen /t REG_DWORD /d 1 /f | Out-Null
    powercfg /change monitor-timeout-ac 0 | Out-Null
    powercfg /change monitor-timeout-dc 0 | Out-Null
    powercfg /change standby-timeout-ac 0 | Out-Null
    powercfg /change standby-timeout-dc 0 | Out-Null
    powercfg /change hibernate-timeout-ac 0 | Out-Null
    powercfg /change hibernate-timeout-dc 0 | Out-Null
    Write-Host "Configured auto-login and disabled lock/sleep for user '$WindowsUsername'."
  } catch {
    Write-Warning "Auto-login setup failed: $($_.Exception.Message)"
  }
} else {
  Write-Warning "WindowsPassword not provided: auto-login not configured."
}

# Performance baseline for cloud gaming (CPU scheduling/capture overhead).
Apply-GameHostPerformanceProfile

# Placeholder hook for Windows activation flow.
Invoke-WindowsActivationPlaceholder

# Required media/runtime deps for Steam + KH + emulator workloads
if (Install-MediaFeaturePack) {
  Set-Content (Join-Path $setupDir 'reboot-required.txt') 'Media Feature Pack installed; reboot required.'
} else {
  Write-Warning 'Media Feature Pack not installed after retries.'
}

Invoke-InstallerExe -Name 'vc_redist_x64' -Url 'https://aka.ms/vs/17/release/vc_redist.x64.exe'
Invoke-InstallerExe -Name 'vc_redist_x86' -Url 'https://aka.ms/vs/17/release/vc_redist.x86.exe'

# DirectX runtime: prefer Steam _CommonRedist when present (works reliably on this image)
try {
  $dxSetup = 'C:\Program Files (x86)\Steam\steamapps\common\Steamworks Shared\_CommonRedist\DirectX\Jun2010\DXSETUP.exe'
  if (Test-Path $dxSetup) {
    Write-Host 'Installing DirectX Jun2010 from Steam _CommonRedist...'
    Start-Process -FilePath $dxSetup -ArgumentList '/silent' -Wait
  } else {
    Write-Host 'DirectX _CommonRedist not found yet; will install after Steam content is present.'
  }
} catch {
  Write-Warning "DirectX install failed: $($_.Exception.Message)"
}

# rclone install (portable fallback, no winget dependency)
try {
  # WinFsp is required for stable Windows mount behavior.
  $wfsp = Get-Service WinFsp.Launcher -ErrorAction SilentlyContinue
  if (-not $wfsp) {
    $wfspMsi = 'C:\ProgramData\gamer\setup\winfsp.msi'
    $wfspApi = Invoke-RestMethod -Uri 'https://api.github.com/repos/winfsp/winfsp/releases/latest' -Headers @{ 'User-Agent'='gamer-setup' }
    $wfspAsset = $wfspApi.assets | Where-Object { $_.name -match '\.msi$' } | Select-Object -First 1
    if ($wfspAsset) {
      Invoke-WebRequest -Uri $wfspAsset.browser_download_url -OutFile $wfspMsi
      Start-Process msiexec.exe -ArgumentList '/i', $wfspMsi, '/qn', '/norestart' -Wait
    }
  }

  $rcloneExe = Get-Command rclone.exe -ErrorAction SilentlyContinue
  if (-not $rcloneExe) {
    $rDir = 'C:\ProgramData\gamer\bin\rclone'
    New-Item -ItemType Directory -Path $rDir -Force | Out-Null
    $zip = Join-Path $rDir 'rclone.zip'
    Invoke-WebRequest -Uri 'https://downloads.rclone.org/rclone-current-windows-amd64.zip' -OutFile $zip
    Expand-Archive -Path $zip -DestinationPath $rDir -Force
    $exe = Get-ChildItem -Path $rDir -Filter rclone.exe -Recurse | Select-Object -First 1
    if ($exe) {
      $dstExe = Join-Path $rDir 'rclone.exe'
      if ($exe.FullName -ne $dstExe) {
        Copy-Item $exe.FullName $dstExe -Force
      }
      $machinePath = [Environment]::GetEnvironmentVariable('Path', 'Machine')
      if ($machinePath -notlike "*$rDir*") {
        [Environment]::SetEnvironmentVariable('Path', "$machinePath;$rDir", 'Machine')
      }
    }
  }
} catch {
  Write-Warning "rclone install failed: $($_.Exception.Message)"
}

# Optional NVIDIA driver install/update
if ($NvidiaDriverUrl) {
  try {
    $drv = 'C:\ProgramData\gamer\setup\nvidia-driver.exe'
    Invoke-WebRequest -Uri $NvidiaDriverUrl -OutFile $drv
    Start-Process -FilePath $drv -ArgumentList '/s','/noreboot' -Wait
  } catch {
    Write-Warning "NVIDIA driver install failed: $($_.Exception.Message)"
  }
}

# Install VB-CABLE virtual audio endpoint for headless streaming
try {
  $base='C:\ProgramData\gamer\setup\vb-audio'
  New-Item -ItemType Directory -Path $base -Force | Out-Null
  $zip=Join-Path $base 'VBCABLE_Driver_Pack45.zip'
  if (-not (Test-Path $zip)) {
    Invoke-WebRequest -Uri 'https://download.vb-audio.com/Download_CABLE/VBCABLE_Driver_Pack45.zip' -OutFile $zip
  }
  Expand-Archive -Path $zip -DestinationPath $base -Force
  $installer=Join-Path $base 'VBCABLE_Setup_x64.exe'
  if (Test-Path $installer) {
    Start-Process -FilePath $installer -ArgumentList '-i','-h' -Wait
  }
} catch {
  Write-Warning "VB-CABLE setup failed: $($_.Exception.Message)"
}

Get-Service sshd,WinRM,AudioEndpointBuilder,Audiosrv | Format-Table Name,Status,StartType -AutoSize
Get-PnpDevice -Class Display | Format-Table FriendlyName,Status -AutoSize
Get-PnpDevice -Class AudioEndpoint | Format-Table FriendlyName,Status -AutoSize
Get-WindowsCapability -Online -Name 'Media.MediaFeaturePack~~~~0.0.1.0' -ErrorAction SilentlyContinue | Format-Table Name,State -AutoSize
