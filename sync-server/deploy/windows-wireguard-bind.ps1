param(
  [string]$Root = 'C:\YebianSync',
  [string]$BindAddress = '10.88.0.1:8787',
  [string]$TaskName = 'YebianSync',
  [switch]$AllowPublicBind
)

$ErrorActionPreference = 'Stop'

$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$principal = New-Object Security.Principal.WindowsPrincipal($identity)
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
  throw 'Run this script as Administrator.'
}
if ($BindAddress -like '0.0.0.0:*' -and -not $AllowPublicBind) {
  throw 'Refusing to bind 0.0.0.0. Use 10.88.0.1:8787, or pass -AllowPublicBind explicitly.'
}
if (-not (Test-Path "$Root\yebian-sync-server.exe")) {
  throw "Missing $Root\yebian-sync-server.exe. Run windows-install.ps1 first."
}
$task = Get-ScheduledTask -TaskName $TaskName -ErrorAction SilentlyContinue
if (-not $task) {
  throw "Scheduled task $TaskName not found. Run windows-install.ps1 first."
}

$action = New-ScheduledTaskAction -Execute "$Root\yebian-sync-server.exe" `
  -Argument "--db $Root\data\sync.sqlite --blobs $Root\data\blobs --bind $BindAddress"
Set-ScheduledTask -TaskName $TaskName -Action $action | Out-Null
Stop-ScheduledTask -TaskName $TaskName -ErrorAction SilentlyContinue
Start-Sleep -Seconds 1
Start-ScheduledTask -TaskName $TaskName

$parts = $BindAddress.Split(':')
$healthHost = $parts[0]
$healthPort = if ($parts.Length -gt 1) { $parts[1] } else { '8787' }
if ($healthHost -eq '0.0.0.0' -or $healthHost -eq '::') { $healthHost = '127.0.0.1' }
$healthUrl = "http://${healthHost}:${healthPort}/v1/health"
$healthy = $false
for ($i = 0; $i -lt 10; $i++) {
  Start-Sleep -Seconds 1
  try {
    $response = Invoke-WebRequest -UseBasicParsing -TimeoutSec 3 $healthUrl
    if ($response.StatusCode -ge 200 -and $response.StatusCode -lt 300) {
      Write-Output "health_ok=$healthUrl"
      $healthy = $true
      break
    }
  } catch {
    # Keep retrying while the scheduled task starts the server.
  }
}
if (-not $healthy) {
  throw "Health check failed: $healthUrl"
}

Get-NetTCPConnection -LocalPort $healthPort -State Listen -ErrorAction SilentlyContinue |
  Select-Object LocalAddress, LocalPort | Format-List
Get-NetFirewallRule -Name 'YebianSync-WG' -ErrorAction SilentlyContinue |
  Select-Object DisplayName, Enabled, Direction, Action | Format-List
