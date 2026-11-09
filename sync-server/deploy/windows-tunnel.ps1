# Install the SSH tunnel in the current user's Startup folder (no elevation).
# It runs hidden at logon and reconnects every 5 seconds if the tunnel drops.
# Usage (from the repo root):
#   powershell -NoProfile -ExecutionPolicy Bypass -File sync-server/deploy/windows-tunnel.ps1

$ErrorActionPreference = 'Stop'
$startup = [Environment]::GetFolderPath('Startup')
$vbs = Join-Path $startup 'YebianSyncTunnel.vbs'
$ssh = 'C:\Windows\System32\OpenSSH\ssh.exe'
if (!(Test-Path $ssh)) { $ssh = 'C:\Program Files\Git\usr\bin\ssh.exe' }
$key = Join-Path $env:USERPROFILE '.ssh\yebian_sync_deploy'

$script = @"
Set shell = CreateObject("WScript.Shell")
q = Chr(34)
ssh = "$ssh"
key = "$key"
args = "-N -L 8787:127.0.0.1:8787 -i " & q & key & q & " -o StrictHostKeyChecking=no -o UserKnownHostsFile=NUL -o ServerAliveInterval=30 -o ExitOnForwardFailure=yes Administrator@119.91.210.85"
cmd = q & ssh & q & " " & args
Do
  shell.Run cmd, 0, True
  WScript.Sleep 5000
Loop
"@
Set-Content -Path $vbs -Value $script -Encoding ASCII

Start-Process -FilePath 'wscript.exe' -ArgumentList "`"$vbs`"" -WindowStyle Hidden
Start-Sleep -Seconds 3

if (Test-Path $vbs) { Write-Output "startup script: $vbs" }
try {
  (Invoke-WebRequest -UseBasicParsing -TimeoutSec 5 http://127.0.0.1:8787/v1/health).Content
} catch {
  Write-Output "health check failed: $_"
}
