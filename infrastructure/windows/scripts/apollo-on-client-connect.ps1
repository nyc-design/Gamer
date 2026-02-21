param(
  [int]$ConnectedClients = 1
)

$ErrorActionPreference = "Continue"
Write-Host "Apollo client connected. connected_clients=$ConnectedClients"

# If second client connects, place dual Azahar windows across first two displays.
if ($ConnectedClients -ge 2) {
  $script = Join-Path $PSScriptRoot "position-azahar-dual.ps1"
  if (Test-Path $script) {
    # Give the second window/display a moment to appear, then place with retries.
    Start-Sleep -Milliseconds 400
    & $script -MaxAttempts 30 -SleepMs 400
  }
}
