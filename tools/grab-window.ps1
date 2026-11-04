param(
  [string]$ProcessName = 'yebian',
  [string]$Out = 'D:\Develop\yebian\shots\grab.png'
)

Add-Type -AssemblyName System.Drawing
. "$PSScriptRoot/window-lib.ps1"
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class GrabWin {
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT lpRect);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left; public int Top; public int Right; public int Bottom; }
}
"@

[GrabWin]::SetProcessDPIAware() | Out-Null
$handle = Get-AppWindowHandle $ProcessName
if ($handle -eq [IntPtr]::Zero) { Write-Error "window not found"; exit 1 }
$rect = New-Object GrabWin+RECT
[GrabWin]::GetWindowRect($handle, [ref]$rect) | Out-Null
$width = $rect.Right - $rect.Left
$height = $rect.Bottom - $rect.Top
$bitmap = New-Object System.Drawing.Bitmap($width, $height)
$graphics = [System.Drawing.Graphics]::FromImage($bitmap)
$graphics.CopyFromScreen($rect.Left, $rect.Top, 0, 0, (New-Object System.Drawing.Size($width, $height)))
$dir = Split-Path -Parent $Out
if (-not (Test-Path $dir)) { New-Item -ItemType Directory -Path $dir | Out-Null }
$bitmap.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
$graphics.Dispose()
$bitmap.Dispose()
Write-Output "$Out ${width}x${height}"
