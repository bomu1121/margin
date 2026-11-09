$ErrorActionPreference = 'Stop'
$root = 'C:\YebianSync'
$taskName = 'YebianSync'

$action = New-ScheduledTaskAction `
  -Execute "$root\yebian-sync-server.exe" `
  -Argument "--db $root\data\sync.sqlite --blobs $root\data\blobs --bind 127.0.0.1:8787"
$trigger = New-ScheduledTaskTrigger -AtStartup
$principal = New-ScheduledTaskPrincipal -UserId 'SYSTEM' -LogonType ServiceAccount -RunLevel Highest
$settings = New-ScheduledTaskSettingsSet `
  -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries `
  -RestartCount 3 -RestartInterval (New-TimeSpan -Minutes 1)
$settings.ExecutionTimeLimit = 'PT0S'

Register-ScheduledTask `
  -TaskName $taskName -Action $action -Trigger $trigger `
  -Principal $principal -Settings $settings -Force | Out-Null
Start-ScheduledTask -TaskName $taskName
Start-Sleep -Seconds 3

Get-ScheduledTask -TaskName $taskName | Select-Object TaskName, State
try {
  (Invoke-WebRequest -UseBasicParsing -TimeoutSec 5 http://127.0.0.1:8787/v1/health).Content
} catch {
  Write-Output "health check failed: $_"
  Get-ScheduledTaskInfo -TaskName $taskName | Format-List
}
