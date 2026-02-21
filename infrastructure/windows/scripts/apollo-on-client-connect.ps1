param(
  [int]$ConnectedClients = 1
)

$ErrorActionPreference = "Continue"
Write-Host "Apollo client connected. connected_clients=$ConnectedClients"

# If second client connects, place dual Azahar windows across first two displays.
if ($ConnectedClients -ge 2) {
  $script = Join-Path $PSScriptRoot "position-azahar-dual.ps1"
  if (Test-Path $script) {
    & $script
  }
}
