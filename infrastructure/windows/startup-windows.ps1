param(
  [string]$NvidiaDriverUrl = '',
  [string]$WindowsUsername = 'user'
)

$ErrorActionPreference = 'Continue'
[Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor 3072
$ProgressPreference = 'SilentlyContinue'

New-Item -ItemType Directory -Path 'C:\ProgramData\gamer\setup' -Force | Out-Null

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

# rclone install (portable fallback, no winget dependency)
try {
  $rcloneExe = Get-Command rclone.exe -ErrorAction SilentlyContinue
  if (-not $rcloneExe) {
    $rDir = 'C:\ProgramData\gamer\bin\rclone'
    New-Item -ItemType Directory -Path $rDir -Force | Out-Null
    $zip = Join-Path $rDir 'rclone.zip'
    Invoke-WebRequest -Uri 'https://downloads.rclone.org/rclone-current-windows-amd64.zip' -OutFile $zip
    Expand-Archive -Path $zip -DestinationPath $rDir -Force
    $exe = Get-ChildItem -Path $rDir -Filter rclone.exe -Recurse | Select-Object -First 1
    if ($exe) {
      Copy-Item $exe.FullName (Join-Path $rDir 'rclone.exe') -Force
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
