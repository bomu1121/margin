param([string]$ProcessName = 'yebian')

. "$PSScriptRoot/window-lib.ps1"

Add-Type @"
using System;
using System.Runtime.InteropServices;
public class WinActivate {
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint processId);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern bool AttachThreadInput(uint idAttach, uint idAttachTo, bool fAttach);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern bool BringWindowToTop(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);
  [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr hWnd, IntPtr hWndInsertAfter, int X, int Y, int cx, int cy, uint uFlags);
}
"@

[WinActivate]::SetProcessDPIAware() | Out-Null
$handle = Get-AppWindowHandle $ProcessName
if ($handle -eq [IntPtr]::Zero) {
  Write-Error "window not found for process $ProcessName"
  exit 1
}

[WinActivate]::ShowWindow($handle, 9) | Out-Null
[WinActivate]::SetWindowPos($handle, [IntPtr](-1), 0, 0, 0, 0, 0x43) | Out-Null

$processId = 0
$targetThread = [WinActivate]::GetWindowThreadProcessId($handle, [ref]$processId)
$foreground = [WinActivate]::GetForegroundWindow()
$fgProcessId = 0
$fgThread = 0
if ($foreground -ne [IntPtr]::Zero) {
  $fgThread = [WinActivate]::GetWindowThreadProcessId($foreground, [ref]$fgProcessId)
}

if ($fgThread -ne 0 -and $fgThread -ne $targetThread) {
  [WinActivate]::AttachThreadInput($fgThread, $targetThread, $true) | Out-Null
}
[WinActivate]::BringWindowToTop($handle) | Out-Null
$ok = [WinActivate]::SetForegroundWindow($handle)
Start-Sleep -Milliseconds 120
if ($fgThread -ne 0 -and $fgThread -ne $targetThread) {
  [WinActivate]::AttachThreadInput($fgThread, $targetThread, $false) | Out-Null
}
$now = [WinActivate]::GetForegroundWindow()
Write-Output ("target=" + $handle + " ok=" + $ok + " foreground=" + $now + " match=" + ($now -eq $handle))
