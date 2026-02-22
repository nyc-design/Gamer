param(
  [string]$NvidiaDriverUrl = '',
  [string]$WindowsUsername = 'user'
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
    [string[]]$Args = @('/quiet','/norestart')
  )
  try {
    $out = Join-Path $setupDir ("{0}.exe" -f $Name)
    Invoke-WebRequest -Uri $Url -OutFile $out
    $p = Start-Process -FilePath $out -ArgumentList $Args -Wait -PassThru
    Write-Host "$Name installer exit code: $($p.ExitCode)"
  } catch {
    Write-Warning "$Name install failed: $($_.Exception.Message)"
  }
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

# Required media/runtime deps for Steam + KH + emulator workloads
try {
  $mediaCap = Get-WindowsCapability -Online -Name 'Media.MediaFeaturePack~~~~0.0.1.0' -ErrorAction SilentlyContinue
  if ($mediaCap -and $mediaCap.State -ne 'Installed') {
    Write-Host 'Installing Media Feature Pack capability...'
    $dismLog = Join-Path $setupDir 'media_pack_install.log'
    cmd /c "dism /online /add-capability /capabilityname:Media.MediaFeaturePack~~~~0.0.1.0 > `"$dismLog`" 2>&1"
    $mediaCap = Get-WindowsCapability -Online -Name 'Media.MediaFeaturePack~~~~0.0.1.0' -ErrorAction SilentlyContinue
    if ($mediaCap -and $mediaCap.State -eq 'Installed') {
      Set-Content (Join-Path $setupDir 'reboot-required.txt') 'Media Feature Pack installed; reboot required.'
    }
  }
} catch {
  Write-Warning "Media Feature Pack install failed: $($_.Exception.Message)"
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
