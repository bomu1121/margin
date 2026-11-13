param(
  [string]$Root = 'C:\YebianSync',
  [string]$ServerEndpoint = '119.91.210.85',
  [int]$ListenPort = 51820,
  [int]$ClientCount = 2,
  [switch]$Rotate
)

$ErrorActionPreference = 'Stop'

function Assert-Administrator {
  $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
  $principal = New-Object Security.Principal.WindowsPrincipal($identity)
  if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw 'Run this script as Administrator.'
  }
}

function Protect-SecretFile {
  param([string]$Path)
  & icacls.exe $Path /inheritance:r /grant:r '*S-1-5-32-544:F' '*S-1-5-18:F' | Out-Null
  if ($LASTEXITCODE -ne 0) {
    throw "icacls failed for $Path"
  }
}

Assert-Administrator
if ([string]::IsNullOrWhiteSpace($ServerEndpoint)) {
  throw 'ServerEndpoint is required.'
}
if ($ListenPort -lt 1 -or $ListenPort -gt 65535) {
  throw 'ListenPort must be between 1 and 65535.'
}
if ($ClientCount -lt 1 -or $ClientCount -gt 16) {
  throw 'ClientCount must be between 1 and 16.'
}

$wg = 'C:\Program Files\WireGuard\wg.exe'
$wireguard = 'C:\Program Files\WireGuard\wireguard.exe'
if (-not (Test-Path $wg)) {
  throw "Missing $wg. Install the WireGuard for Windows client first."
}
if (-not (Test-Path $wireguard)) {
  throw "Missing $wireguard. Install the WireGuard for Windows client first."
}

New-Item -ItemType Directory -Force -Path $Root | Out-Null
$serverConf = Join-Path $Root 'wg0.conf'
$existing = @(Get-ChildItem -Path $Root -File -ErrorAction SilentlyContinue | Where-Object {
  $_.Name -eq 'wg0.conf' -or $_.Name -like 'client*.conf'
})
if ($existing.Count -gt 0 -and -not $Rotate) {
  throw 'WireGuard configs already exist. Use -Rotate to back up and regenerate them; re-running without -Rotate would invalidate existing clients.'
}
if ($Rotate -and $existing.Count -gt 0) {
  $stamp = Get-Date -Format 'yyyyMMdd-HHmmss'
  $backup = Join-Path $Root "wg-backup-$stamp"
  New-Item -ItemType Directory -Force -Path $backup | Out-Null
  foreach ($file in $existing) {
    Copy-Item -LiteralPath $file.FullName -Destination (Join-Path $backup $file.Name) -Force
    Protect-SecretFile (Join-Path $backup $file.Name)
    if ($file.Name -like 'client*.conf') {
      Remove-Item -LiteralPath $file.FullName -Force
    }
  }
  Write-Output "backup=$backup"
}

$serverPrivate = (& $wg genkey).Trim()
$serverPublic = ($serverPrivate | & $wg pubkey).Trim()
$serverLines = @(
  '[Interface]',
  'Address = 10.88.0.1/24',
  "ListenPort = $ListenPort",
  "PrivateKey = $serverPrivate",
  ''
)

$clientPublicKeys = @()
$clientPaths = @()
for ($i = 1; $i -le $ClientCount; $i++) {
  $clientPrivate = (& $wg genkey).Trim()
  $clientPublic = ($clientPrivate | & $wg pubkey).Trim()
  $clientAddress = "10.88.0.$($i + 1)"
  $serverLines += '[Peer]'
  $serverLines += "PublicKey = $clientPublic"
  $serverLines += "AllowedIPs = $clientAddress/32"
  $serverLines += ''

  $clientLines = @(
    '[Interface]',
    "PrivateKey = $clientPrivate",
    "Address = $clientAddress/32",
    '',
    '[Peer]',
    "PublicKey = $serverPublic",
    "Endpoint = ${ServerEndpoint}:${ListenPort}",
    'AllowedIPs = 10.88.0.0/24',
    'PersistentKeepalive = 25'
  )
  $clientPath = Join-Path $Root "client$i.conf"
  Set-Content -Path $clientPath -Value ($clientLines -join "`r`n") -Encoding ASCII
  Protect-SecretFile $clientPath
  $clientPaths += $clientPath
  $clientPublicKeys += $clientPublic
}

Set-Content -Path $serverConf -Value ($serverLines -join "`r`n") -Encoding ASCII
Protect-SecretFile $serverConf

New-NetFirewallRule -Name 'WireGuard-51820' -DisplayName 'WireGuard UDP 51820' `
  -Direction Inbound -Protocol UDP -LocalPort $ListenPort -Action Allow -Force | Out-Null
New-NetFirewallRule -Name 'YebianSync-WG' -DisplayName 'Yebian sync from WireGuard' `
  -Direction Inbound -Protocol TCP -LocalPort 8787 -RemoteAddress '10.88.0.0/24' -Action Allow -Force | Out-Null

$serviceName = 'WireGuardTunnel$wg0'
if (Get-Service -Name $serviceName -ErrorAction SilentlyContinue) {
  & $wireguard /uninstalltunnelservice wg0 | Out-Null
  Start-Sleep -Seconds 1
}
& $wireguard /installtunnelservice $serverConf
Start-Sleep -Seconds 3
$service = Get-Service -Name $serviceName -ErrorAction SilentlyContinue
if (-not $service -or $service.Status -ne 'Running') {
  throw "WireGuard service $serviceName is not running."
}

$logLines = @(
  "generated_at=$(Get-Date -Format o)",
  "listen_port=$ListenPort",
  "server_public_key=$serverPublic",
  "client_count=$ClientCount"
)
for ($i = 0; $i -lt $clientPublicKeys.Count; $i++) {
  $logLines += "client$($i + 1)_public_key=$($clientPublicKeys[$i])"
  $logLines += "client$($i + 1)_config=$($clientPaths[$i])"
}
$logPath = Join-Path $Root 'wg-setup.log'
Set-Content -Path $logPath -Value ($logLines -join "`r`n") -Encoding ASCII
Protect-SecretFile $logPath

Write-Output "server_public_key=$serverPublic"
for ($i = 0; $i -lt $clientPublicKeys.Count; $i++) {
  Write-Output "client$($i + 1)_public_key=$($clientPublicKeys[$i])"
}
Write-Output "client_configs=$($clientPaths -join ',')"
Write-Output 'Copy each client config to its device, import it into WireGuard, then delete the copy from the server.'
