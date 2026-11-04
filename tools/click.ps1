param(
  [string]$ProcessName = 'yebian',
  [int]$X = 0,
  [int]$Y = 0,
  [int]$DelayMs = 500
)

. "$PSScriptRoot/window-lib.ps1"

Add-Type @"
using System;
using System.Runtime.InteropServices;
public class WinClick {
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT lpRect);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int X, int Y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint dwFlags, uint dx, uint dy, uint dwData, UIntPtr dwExtraInfo);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left; public int Top; public int Right; public int Bottom; }
}
"@

[WinClick]::SetProcessDPIAware() | Out-Null
$handle = Get-AppWindowHandle $ProcessName
if ($handle -eq [IntPtr]::Zero) { Write-Error "window not found for process $ProcessName"; exit 1 }
$rect = New-Object WinClick+RECT
[WinClick]::GetWindowRect($handle, [ref]$rect) | Out-Null
$screenX = $rect.Left + $X
$screenY = $rect.Top + $Y
[WinClick]::SetCursorPos($screenX, $screenY) | Out-Null
Start-Sleep -Milliseconds $DelayMs
$fg = [WinClick]::GetForegroundWindow()
Write-Output ("window=" + $handle + " abs=" + $screenX + "," + $screenY + " foreground=" + $fg + " match=" + ($fg -eq $handle))
[WinClick]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
[WinClick]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
Start-Sleep -Milliseconds 700
