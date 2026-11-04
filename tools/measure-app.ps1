param(
  [string]$Exe = 'D:\Develop\yebian\src-tauri\target\release\yebian.exe',
  [int]$Runs = 3,
  [int]$IdleMs = 4000,
  [switch]$KeepRunning
)

Add-Type @"
using System;
using System.Text;
using System.Runtime.InteropServices;
public class PerfWin {
  public delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumWindowsProc callback, IntPtr lParam);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint processId);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT lpRect);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr hWnd, StringBuilder text, int maxCount);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left; public int Top; public int Right; public int Bottom; }

  public static IntPtr FindVisible(uint pid) {
    IntPtr found = IntPtr.Zero;
    EnumWindows((handle, state) => {
      uint owner = 0;
      GetWindowThreadProcessId(handle, out owner);
      if (owner != pid || !IsWindowVisible(handle)) return true;
      RECT rect;
      GetWindowRect(handle, out rect);
      if (rect.Right - rect.Left >= 800 && rect.Bottom - rect.Top >= 500) {
        found = handle;
        return false;
      }
      return true;
    }, IntPtr.Zero);
    return found;
  }
}
"@

[PerfWin]::SetProcessDPIAware() | Out-Null
if (-not (Test-Path $Exe)) { Write-Error "exe not found: $Exe"; exit 1 }

for ($run = 1; $run -le $Runs; $run += 1) {
  Get-Process -Name 'yebian' -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
  Start-Sleep -Milliseconds 800

  $startedAt = Get-Date
  $process = Start-Process -FilePath $Exe -PassThru
  $visibleAt = $null
  for ($i = 0; $i -lt 1200; $i += 1) {
    if ([PerfWin]::FindVisible([uint32]$process.Id) -ne [IntPtr]::Zero) {
      $visibleAt = Get-Date
      break
    }
    Start-Sleep -Milliseconds 10
  }
  $coldMs = if ($visibleAt) { [int]($visibleAt - $startedAt).TotalMilliseconds } else { -1 }

  Start-Sleep -Milliseconds $IdleMs
  $all = Get-CimInstance Win32_Process
  $byParent = @{}
  foreach ($item in $all) {
    if (-not $byParent.ContainsKey([int]$item.ParentProcessId)) { $byParent[[int]$item.ParentProcessId] = @() }
    $byParent[[int]$item.ParentProcessId] += [int]$item.ProcessId
  }
  $tree = New-Object System.Collections.Generic.List[int]
  $queue = New-Object System.Collections.Generic.Queue[int]
  $queue.Enqueue([int]$process.Id)
  while ($queue.Count -gt 0) {
    $pidNow = $queue.Dequeue()
    if ($tree.Contains($pidNow)) { continue }
    $tree.Add($pidNow)
    if ($byParent.ContainsKey($pidNow)) {
      foreach ($child in $byParent[$pidNow]) { $queue.Enqueue($child) }
    }
  }
  $workingSet = 0
  $privateSet = 0
  $count = 0
  foreach ($pidNow in $tree) {
    $proc = Get-Process -Id $pidNow -ErrorAction SilentlyContinue
    if ($proc) {
      $workingSet += $proc.WorkingSet64
      $privateSet += $proc.PrivateMemorySize64
      $count += 1
    }
  }

  $cpuStart = 0
  foreach ($pidNow in $tree) {
    $proc = Get-Process -Id $pidNow -ErrorAction SilentlyContinue
    if ($proc) { $cpuStart += $proc.TotalProcessorTime.TotalMilliseconds }
  }
  $sampleMs = 2000
  Start-Sleep -Milliseconds $sampleMs
  $cpuEnd = 0
  foreach ($pidNow in $tree) {
    $proc = Get-Process -Id $pidNow -ErrorAction SilentlyContinue
    if ($proc) { $cpuEnd += $proc.TotalProcessorTime.TotalMilliseconds }
  }
  $cpuPercent = [math]::Round((($cpuEnd - $cpuStart) / ($sampleMs * [Environment]::ProcessorCount)) * 100, 2)

  [Console]::WriteLine((ConvertTo-Json -Compress -Depth 3 @{
    run = $run
    coldStartMs = $coldMs
    idleWorkingSetMB = [math]::Round($workingSet / 1MB, 1)
    idlePrivateMB = [math]::Round($privateSet / 1MB, 1)
    idleCpuPercent = $cpuPercent
    processCount = $count
    appPid = $process.Id
  }))
}

if (-not $KeepRunning) {
  Get-Process -Name 'yebian' -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
}
