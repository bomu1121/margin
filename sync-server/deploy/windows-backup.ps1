$ErrorActionPreference = 'Stop'
$root = 'C:\YebianSync'
$dest = Join-Path $root 'backups'
$stamp = Get-Date -Format 'yyyyMMdd-HHmmss'
$snap = Join-Path $dest $stamp
New-Item -ItemType Directory -Force -Path $snap | Out-Null

# Stop the relay for a consistent SQLite + WAL copy, then start it again.
Stop-ScheduledTask -TaskName 'YebianSync' -ErrorAction SilentlyContinue
Start-Sleep -Seconds 1
Copy-Item (Join-Path $root 'data\sync.sqlite*') $snap -Force
Copy-Item (Join-Path $root 'data\blobs') (Join-Path $snap 'blobs') -Recurse -Force
Start-ScheduledTask -TaskName 'YebianSync'

Get-ChildItem $dest -Directory | Sort-Object Name -Descending |
  Select-Object -Skip 14 | Remove-Item -Recurse -Force

Write-Output "backup: $snap"
