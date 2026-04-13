param(
  [string]$AgentRoot = 'C:\gamer\client-agent'
)

$ErrorActionPreference='Stop'
New-Item -ItemType Directory -Path $AgentRoot -Force | Out-Null
powershell -ExecutionPolicy Bypass -File C:\ProgramData\gamer\setup\install-agent-service.ps1 -AgentRoot $AgentRoot
