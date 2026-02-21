param(
  [string]$AgentRoot = "C:\\gamer\\client-agent",
  [int]$Port = 8081
)

$ErrorActionPreference = "Stop"

if (-not (Test-Path $AgentRoot)) {
  throw "AgentRoot not found: $AgentRoot"
}

$venv = Join-Path $AgentRoot ".venv"

function Resolve-PythonCommand {
  # Try explicit common install paths first.
  $candidates = @(
    "C:\\Program Files\\Python312\\python.exe",
    "C:\\Users\\user\\AppData\\Local\\Programs\\Python\\Python312\\python.exe",
    "C:\\Users\\user\\AppData\\Local\\Programs\\Python\\Python311\\python.exe",
    "C:\\Program Files\\Python311\\python.exe",
    "C:\\tools\\python\\py312\\tools\\python.exe"
  )
  $candidates += Get-ChildItem -Path "C:\\Program Files\\Python*\\python.exe" -ErrorAction SilentlyContinue | ForEach-Object { $_.FullName }
  $candidates += Get-ChildItem -Path "C:\\Users\\user\\AppData\\Local\\Programs\\Python\\Python*\\python.exe" -ErrorAction SilentlyContinue | ForEach-Object { $_.FullName }
  foreach ($c in $candidates) {
    if (Test-Path $c) {
      try {
        & $c --version | Out-Null
        if ($LASTEXITCODE -eq 0) {
          return @($c)
        }
      } catch {}
    }
  }

  try {
    & py -3 --version | Out-Null
    if ($LASTEXITCODE -eq 0) {
      return @("py", "-3")
    }
  } catch {}

  try {
    & python --version | Out-Null
    if ($LASTEXITCODE -eq 0) {
      return @("python")
    }
  } catch {}

  return $null
}

$pythonCmd = Resolve-PythonCommand
if (-not $pythonCmd) {
  Write-Warning "Python not found. Attempting automated install..."
  try {
    winget install --id Python.Python.3.12 --accept-package-agreements --accept-source-agreements --silent --disable-interactivity
  } catch {
    Write-Warning "winget Python install failed: $($_.Exception.Message)"
  }

  if (-not (Resolve-PythonCommand)) {
    try {
      Write-Warning "Trying NuGet Python fallback..."
      $ProgressPreference = "SilentlyContinue"
      New-Item -Path "C:\\tools\\python" -ItemType Directory -Force | Out-Null
      $pkg = "C:\\tools\\python\\python.nupkg"
      $zip = "C:\\tools\\python\\python.zip"
      Invoke-WebRequest -Uri "https://www.nuget.org/api/v2/package/python/3.12.8" -OutFile $pkg
      Copy-Item $pkg $zip -Force
      Expand-Archive -Path $zip -DestinationPath "C:\\tools\\python\\py312" -Force
    } catch {
      Write-Warning "NuGet Python fallback failed: $($_.Exception.Message)"
    }
  }

  $pythonCmd = Resolve-PythonCommand
  if (-not $pythonCmd) {
    throw "Python not found. Install Python 3 first."
  }
}

function Invoke-Python {
  param([string[]]$BaseCmd, [string[]]$ExtraArgs)
  if ($BaseCmd.Length -eq 1) {
    & $BaseCmd[0] @ExtraArgs
  } else {
    $prefix = $BaseCmd[1..($BaseCmd.Length - 1)]
    & $BaseCmd[0] @prefix @ExtraArgs
  }
}

if (-not (Test-Path $venv)) {
  Invoke-Python -BaseCmd $pythonCmd -ExtraArgs @("-m","venv",$venv)
}

if (-not (Test-Path "$venv\\Scripts\\python.exe")) {
  throw "Failed to create virtualenv at $venv"
}

& "$venv\\Scripts\\python.exe" -m pip install --upgrade pip
& "$venv\\Scripts\\python.exe" -m pip install -r "$AgentRoot\\requirements.txt"

$svcName = "GamerClientAgent"
$cmd = "`"$venv\\Scripts\\python.exe`" -m uvicorn src.main:APP --host 0.0.0.0 --port $Port"

# Stop existing task/processes for idempotent restart
try { Stop-ScheduledTask -TaskName $svcName -ErrorAction SilentlyContinue | Out-Null } catch {}
Get-CimInstance Win32_Process | Where-Object {
  $_.Name -eq "python.exe" -and $_.CommandLine -like "*uvicorn src.main:APP*"
} | ForEach-Object {
  try { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue } catch {}
}
Get-CimInstance Win32_Process | Where-Object {
  $_.Name -eq "python.exe" -and $_.CommandLine -like "*C:\\tools\\python\\py312\\tools\\python.exe*"
} | ForEach-Object {
  try { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue } catch {}
}

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
  $principal = New-ScheduledTaskPrincipal -UserId "SYSTEM" -LogonType ServiceAccount -RunLevel Highest
  $settings = New-ScheduledTaskSettingsSet `
    -StartWhenAvailable `
    -RestartCount 999 `
    -RestartInterval (New-TimeSpan -Minutes 1) `
    -ExecutionTimeLimit (New-TimeSpan -Hours 0)
  Register-ScheduledTask -TaskName $svcName -Action $action -Trigger $trigger -Principal $principal -Settings $settings -Force | Out-Null
  Start-ScheduledTask -TaskName $svcName
  Write-Host "Startup task installed: $svcName"
}

# Open inbound firewall for agent HTTP endpoint
$ruleName = "GamerClientAgent$Port"
netsh advfirewall firewall add rule name="$ruleName" dir=in action=allow protocol=TCP localport=$Port | Out-Null
Write-Host "Opened firewall TCP $Port for $ruleName"
