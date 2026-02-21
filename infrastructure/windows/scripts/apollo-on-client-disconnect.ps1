param(
  [int]$ConnectedClients = 0
)

$ErrorActionPreference = "Continue"
Write-Host "Apollo client disconnected. connected_clients=$ConnectedClients"

# Keep behavior simple for now. Could restore single-screen layout here if needed.
