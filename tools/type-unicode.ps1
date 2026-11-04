param(
  [string]$ProcessName = 'yebian',
  [Parameter(Mandatory = $true)][string]$TextFile,
  [int]$X = 0,
  [int]$Y = 0,
  [int]$SaveX = -1,
  [int]$SaveY = -1
)

Add-Type @"
using System;
using System.Runtime.InteropServices;
public class WinType {
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left; public int Top; public int Right; public int Bottom; }
  [StructLayout(LayoutKind.Sequential)] public struct KEYBDINPUT { public ushort wVk; public ushort wScan; public uint dwFlags; public uint time; public IntPtr dwExtraInfo; }
  [StructLayout(LayoutKind.Explicit, Size = 32)] public struct INPUTUNION { [FieldOffset(0)] public KEYBDINPUT ki; }
  [StructLayout(LayoutKind.Sequential)] public struct INPUT { public uint type; public INPUTUNION u; }

  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint flags, uint dx, uint dy, uint data, UIntPtr extra);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool BringWindowToTop(IntPtr h);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] public static extern bool AttachThreadInput(uint attach, uint attachTo, bool value);
  [DllImport("user32.dll", SetLastError = true)] public static extern uint SendInput(uint count, INPUT[] inputs, int size);

  public static void Click(int x, int y) {
    SetCursorPos(x, y);
    System.Threading.Thread.Sleep(250);
    mouse_event(0x0002, 0, 0, 0, UIntPtr.Zero);
    mouse_event(0x0004, 0, 0, 0, UIntPtr.Zero);
  }

  public static uint Type(string text) {
    INPUT[] inputs = new INPUT[text.Length * 2];
    for (int i = 0; i < text.Length; i++) {
      inputs[i * 2] = Key(text[i], 0x0004);
      inputs[i * 2 + 1] = Key(text[i], 0x0006);
    }
    return SendInput((uint)inputs.Length, inputs, Marshal.SizeOf(typeof(INPUT)));
  }

  private static INPUT Key(char ch, uint flags) {
    INPUT input = new INPUT();
    input.type = 1;
    input.u.ki = new KEYBDINPUT();
    input.u.ki.wVk = 0;
    input.u.ki.wScan = ch;
    input.u.ki.dwFlags = flags;
    input.u.ki.time = 0;
    input.u.ki.dwExtraInfo = IntPtr.Zero;
    return input;
  }
}
"@

[WinType]::SetProcessDPIAware() | Out-Null
$p = Get-Process -Name $ProcessName -ErrorAction SilentlyContinue |
  Where-Object { $_.MainWindowHandle -ne 0 } |
  Select-Object -First 1
if (-not $p) { Write-Error "window not found for process $ProcessName"; exit 1 }
$handle = $p.MainWindowHandle
$rect = New-Object WinType+RECT
[WinType]::GetWindowRect($handle, [ref]$rect) | Out-Null

$foreground = [WinType]::GetForegroundWindow()
$fgProc = 0
$fgThread = 0
if ($foreground -ne [IntPtr]::Zero) { $fgThread = [WinType]::GetWindowThreadProcessId($foreground, [ref]$fgProc) }
$procId = 0
$targetThread = [WinType]::GetWindowThreadProcessId($handle, [ref]$procId)
if ($fgThread -ne 0 -and $fgThread -ne $targetThread) {
  [WinType]::AttachThreadInput($fgThread, $targetThread, $true) | Out-Null
}
[WinType]::BringWindowToTop($handle) | Out-Null
[WinType]::SetForegroundWindow($handle) | Out-Null
Start-Sleep -Milliseconds 250
if ($fgThread -ne 0 -and $fgThread -ne $targetThread) {
  [WinType]::AttachThreadInput($fgThread, $targetThread, $false) | Out-Null
}

[WinType]::Click(($rect.Left + $X), ($rect.Top + $Y))
Start-Sleep -Milliseconds 350
$text = [System.IO.File]::ReadAllText($TextFile, [System.Text.Encoding]::UTF8).Trim()
$sent = [WinType]::Type($text)
Write-Output ("sent=" + $sent + " chars=" + $text.Length + " foreground=" + [WinType]::GetForegroundWindow())
Start-Sleep -Milliseconds 500

if ($SaveX -ge 0 -and $SaveY -ge 0) {
  [WinType]::Click(($rect.Left + $SaveX), ($rect.Top + $SaveY))
  Start-Sleep -Milliseconds 900
}
