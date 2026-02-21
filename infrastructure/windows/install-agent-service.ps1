param(
  [string]$AgentRoot = "C:\\gamer\\client-agent",
  [int]$Port = 8081
)

$ErrorActionPreference = "Stop"

if (-not (Test-Path $AgentRoot)) {
  throw "AgentRoot not found: $AgentRoot"
}

$venv = Join-Path $AgentRoot ".venv"
if (-not (Test-Path $venv)) {
  py -3 -m venv $venv
}

& "$venv\\Scripts\\python.exe" -m pip install --upgrade pip
& "$venv\\Scripts\\python.exe" -m pip install -r "$AgentRoot\\requirements.txt"

$svcName = "GamerClientAgent"
$cmd = "`"$venv\\Scripts\\python.exe`" -m uvicorn src.main:APP --host 0.0.0.0 --port $Port"

# Use NSSM if available, fallback to scheduled task
$nssm = Get-Command nssm -ErrorAction SilentlyContinue
if ($nssm) {
  nssm install $svcName "$venv\\Scripts\\python.exe" "-m" "uvicorn" "src.main:APP" "--host" "0.0.0.0" "--port" "$Port"
  nssm set $svcName AppDirectory $AgentRoot
  nssm start $svcName
  Write-Host "Service installed via NSSM: $svcName"
} else {
  $action = New-ScheduledTaskAction -Execute "$venv\\Scripts\\python.exe" -Argument "-m uvicorn src.main:APP --host 0.0.0.0 --port $Port" -WorkingDirectory $AgentRoot
  $trigger = New-ScheduledTaskTrigger -AtStartup
  Register-ScheduledTask -TaskName $svcName -Action $action -Trigger $trigger -RunLevel Highest -Force | Out-Null
  Start-ScheduledTask -TaskName $svcName
  Write-Host "Startup task installed: $svcName"
}
