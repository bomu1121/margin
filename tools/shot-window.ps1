param(
  [string]$ProcessName = 'yebian',
  [string]$Out = 'D:\Develop\yebian\shots\app.png'
)

Add-Type -AssemblyName System.Drawing
. "$PSScriptRoot/window-lib.ps1"
Add-Type @"
using System;
using System.Text;
using System.Runtime.InteropServices;
public class WinCapture {
  public delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumWindowsProc callback, IntPtr lParam);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr hWnd, StringBuilder text, int maxCount);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT lpRect);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);
  [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr hWnd, IntPtr hWndInsertAfter, int X, int Y, int cx, int cy, uint uFlags);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left; public int Top; public int Right; public int Bottom; }

  public static IntPtr FindByTitle(string needle) {
    IntPtr found = IntPtr.Zero;
    EnumWindows((handle, state) => {
      var text = new StringBuilder(256);
      GetWindowText(handle, text, 256);
      if (text.ToString().IndexOf(needle, StringComparison.OrdinalIgnoreCase) >= 0) {
        found = handle;
        return false;
      }
      return true;
    }, IntPtr.Zero);
    return found;
  }
}
"@

[WinCapture]::SetProcessDPIAware() | Out-Null
# Pick the largest visible top-level window owned by the process. MainWindowHandle
# can point at Tauri's 11x11 helper window in a debug build.
$handle = Get-AppWindowHandle $ProcessName
if ($handle -eq [IntPtr]::Zero) {
  Write-Error "window not found for process $ProcessName"
  exit 1
}

[WinCapture]::ShowWindow($handle, 9) | Out-Null
# Force the target above other windows for the capture; restore afterwards.
[WinCapture]::SetWindowPos($handle, [IntPtr](-1), 0, 0, 0, 0, 0x0001 -bor 0x0002 -bor 0x0040) | Out-Null
[WinCapture]::SetForegroundWindow($handle) | Out-Null
Start-Sleep -Milliseconds 500

$rect = New-Object WinCapture+RECT
[WinCapture]::GetWindowRect($handle, [ref]$rect) | Out-Null
$width = $rect.Right - $rect.Left
$height = $rect.Bottom - $rect.Top
if ($width -le 0 -or $height -le 0) {
  Write-Error "invalid window size ${width}x${height}"
  exit 1
}

$bitmap = New-Object System.Drawing.Bitmap($width, $height)
$graphics = [System.Drawing.Graphics]::FromImage($bitmap)
$graphics.CopyFromScreen($rect.Left, $rect.Top, 0, 0, (New-Object System.Drawing.Size($width, $height)))
$dir = Split-Path -Parent $Out
if (-not (Test-Path $dir)) { New-Item -ItemType Directory -Path $dir | Out-Null }
$bitmap.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
$graphics.Dispose()
$bitmap.Dispose()
[WinCapture]::SetWindowPos($handle, [IntPtr](-2), 0, 0, 0, 0, 0x0001 -bor 0x0002) | Out-Null
Write-Output "$Out ${width}x${height}"
