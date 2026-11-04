param([string]$Exe = 'D:\Develop\yebian\src-tauri\target\release\yebian.exe')

Get-Process -Name 'yebian' -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Start-Sleep -Milliseconds 700
$app = Start-Process -FilePath $Exe -PassThru
function Get-TreeStats([int]$rootPid) {
  $all = Get-CimInstance Win32_Process
  $byParent = @{}
  foreach ($item in $all) {
    if (-not $byParent.ContainsKey([int]$item.ParentProcessId)) { $byParent[[int]$item.ParentProcessId] = @() }
    $byParent[[int]$item.ParentProcessId] += [int]$item.ProcessId
  }
  $tree = New-Object System.Collections.Generic.List[int]
  $queue = New-Object System.Collections.Generic.Queue[int]
  $queue.Enqueue($rootPid)
  while ($queue.Count -gt 0) {
    $pidNow = $queue.Dequeue()
    if ($tree.Contains($pidNow)) { continue }
    $tree.Add($pidNow)
    if ($byParent.ContainsKey($pidNow)) { foreach ($child in $byParent[$pidNow]) { $queue.Enqueue($child) } }
  }
  $ws = 0; $pv = 0; $count = 0
  foreach ($pidNow in $tree) {
    $proc = Get-Process -Id $pidNow -ErrorAction SilentlyContinue
    if ($proc) { $ws += $proc.WorkingSet64; $pv += $proc.PrivateMemorySize64; $count += 1 }
  }
  [pscustomobject]@{ processCount = $count; workingSetMB = [math]::Round($ws / 1MB, 1); privateMB = [math]::Round($pv / 1MB, 1) }
}
$elapsed = 0
foreach ($delay in @(5, 10, 20, 30)) {
  Start-Sleep -Seconds $delay
  $elapsed += $delay
  $stats = Get-TreeStats $app.Id
  [Console]::WriteLine((ConvertTo-Json -Compress @{ elapsedSec = $elapsed; processCount = $stats.processCount; workingSetMB = $stats.workingSetMB; privateMB = $stats.privateMB }))
}
Get-Process -Name 'yebian' -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
