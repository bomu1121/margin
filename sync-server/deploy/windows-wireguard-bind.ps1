$ErrorActionPreference = 'Stop'
$root = 'C:\YebianSync'
$action = New-ScheduledTaskAction -Execute "$root\yebian-sync-server.exe" `
  -Argument "--db $root\data\sync.sqlite --blobs $root\data\blobs --bind 0.0.0.0:8787"
Set-ScheduledTask -TaskName 'YebianSync' -Action $action | Out-Null
Stop-ScheduledTask -TaskName 'YebianSync' -ErrorAction SilentlyContinue
Start-Sleep -Seconds 1
Start-ScheduledTask -TaskName 'YebianSync'
Start-Sleep -Seconds 3

Get-NetTCPConnection -LocalPort 8787 -State Listen -ErrorAction SilentlyContinue |
  Select-Object LocalAddress, LocalPort | Format-List
Get-NetFirewallRule -Name 'YebianSync-WG' -ErrorAction SilentlyContinue |
  Select-Object DisplayName, Enabled, Direction, Action | Format-List
try {
  (Invoke-WebRequest -UseBasicParsing -TimeoutSec 5 http://127.0.0.1:8787/v1/health).Content
} catch {
  "health failed: $_"
}
