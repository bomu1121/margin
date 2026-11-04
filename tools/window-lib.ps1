Add-Type @"
using System;
using System.Text;
using System.Runtime.InteropServices;
public class AppWindowFinder {
  public delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumWindowsProc callback, IntPtr lParam);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint processId);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT lpRect);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr hWnd, StringBuilder text, int maxCount);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left; public int Top; public int Right; public int Bottom; }

  public static IntPtr FindLargest(uint[] pids) {
    IntPtr best = IntPtr.Zero;
    long bestArea = -1;
    EnumWindows((handle, state) => {
      uint owner = 0;
      GetWindowThreadProcessId(handle, out owner);
      if (Array.IndexOf(pids, owner) < 0) return true;
      if (!IsWindowVisible(handle)) return true;
      RECT rect;
      if (!GetWindowRect(handle, out rect)) return true;
      long width = rect.Right - rect.Left;
      long height = rect.Bottom - rect.Top;
      if (width <= 0 || height <= 0) return true;
      long area = width * height;
      if (area > bestArea) {
        bestArea = area;
        best = handle;
      }
      return true;
    }, IntPtr.Zero);
    return best;
  }
}
"@

function Get-AppWindowHandle {
  param([string]$ProcessName = 'yebian')
  [AppWindowFinder]::SetProcessDPIAware() | Out-Null
  $processes = @(Get-Process -Name $ProcessName -ErrorAction SilentlyContinue)
  if ($processes.Count -eq 0) { return [IntPtr]::Zero }
  $pids = [uint32[]]($processes | ForEach-Object { [uint32]$_.Id })
  return [AppWindowFinder]::FindLargest($pids)
}
