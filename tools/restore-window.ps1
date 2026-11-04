param([string]$ProcessName = 'yebian')

. "$PSScriptRoot/window-lib.ps1"

Add-Type @"
using System;
using System.Runtime.InteropServices;
public class WinFix {
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int n);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr h);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L,T,R,B; }
}
"@

[WinFix]::SetProcessDPIAware() | Out-Null
$h = Get-AppWindowHandle $ProcessName
if ($h -eq [IntPtr]::Zero) { Write-Error "window not found for process $ProcessName"; exit 1 }
Write-Output ("handle=" + $h + " iconic=" + [WinFix]::IsIconic($h))
[WinFix]::ShowWindow($h, 9) | Out-Null
Start-Sleep -Milliseconds 600
$r = New-Object WinFix+RECT
[WinFix]::GetWindowRect($h, [ref]$r) | Out-Null
Write-Output ("rect=" + $r.L + "," + $r.T + "," + $r.R + "," + $r.B + " iconic=" + [WinFix]::IsIconic($h))
