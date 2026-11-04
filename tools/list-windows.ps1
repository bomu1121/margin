param([string]$ProcessName = 'yebian')
Add-Type @"
using System;
using System.Text;
using System.Runtime.InteropServices;
public class WinEnum {
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumWindowsProc cb, IntPtr l);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint processId);
  [DllImport("user32.dll")] public static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  public delegate bool EnumWindowsProc(IntPtr h, IntPtr l);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
}
"@
$target = (Get-Process -Name $ProcessName -ErrorAction SilentlyContinue | Select-Object -First 1).Id
if (-not $target) { Write-Output 'process not found'; exit 1 }
[WinEnum]::EnumWindows({
  param($handle, $state)
  $processId = 0
  [WinEnum]::GetWindowThreadProcessId($handle, [ref]$processId) | Out-Null
  if ($processId -eq $target) {
    $text = New-Object System.Text.StringBuilder 256
    [WinEnum]::GetWindowText($handle, $text, 256) | Out-Null
    $rect = New-Object WinEnum+RECT
    [WinEnum]::GetWindowRect($handle, [ref]$rect) | Out-Null
    [Console]::WriteLine("hwnd={0} visible={1} rect={2},{3},{4},{5} title={6}", $handle, [WinEnum]::IsWindowVisible($handle), $rect.Left, $rect.Top, $rect.Right, $rect.Bottom, $text.ToString())
  }
  return $true
}, [IntPtr]::Zero) | Out-Null
