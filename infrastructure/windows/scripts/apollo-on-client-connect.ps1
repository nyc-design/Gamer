param(
  [int]$ConnectedClients = 1
)

$ErrorActionPreference = "Continue"
Write-Host "Apollo client connected. connected_clients=$ConnectedClients"

# If second client connects, place dual Azahar windows across first two displays.
if ($ConnectedClients -ge 2) {
  $script = Join-Path $PSScriptRoot "position-azahar-dual.ps1"
  if (Test-Path $script) {
    # Fire-and-forget async placement so client-connected API stays responsive.
    # The positioning script itself has retries for late window/display availability.
    $args = "-ExecutionPolicy Bypass -File `"$script`" -MaxAttempts 30 -SleepMs 400"
    Start-Process -FilePath "powershell.exe" -ArgumentList $args -WindowStyle Hidden | Out-Null
    Write-Host "Spawned async dual-window placement"
  }
}
