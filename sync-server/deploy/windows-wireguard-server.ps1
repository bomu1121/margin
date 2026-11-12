$ErrorActionPreference = 'Stop'
$root = 'C:\YebianSync'
$wg = 'C:\Program Files\WireGuard\wg.exe'
Start-Transcript -Path "$root\wg-setup.log" -Force | Out-Null

$serverPriv = (& $wg genkey).Trim()
$serverPub = ($serverPriv | & $wg pubkey).Trim()
$c1Priv = (& $wg genkey).Trim()
$c1Pub = ($c1Priv | & $wg pubkey).Trim()
$c2Priv = (& $wg genkey).Trim()
$c2Pub = ($c2Priv | & $wg pubkey).Trim()

$serverConf = @"
[Interface]
Address = 10.88.0.1/24
ListenPort = 51820
PrivateKey = $serverPriv

[Peer]
PublicKey = $c1Pub
AllowedIPs = 10.88.0.2/32

[Peer]
PublicKey = $c2Pub
AllowedIPs = 10.88.0.3/32
"@
Set-Content -Path "$root\wg0.conf" -Value $serverConf -Encoding ASCII

$c1 = @"
[Interface]
PrivateKey = $c1Priv
Address = 10.88.0.2/32

[Peer]
PublicKey = $serverPub
Endpoint = 119.91.210.85:51820
AllowedIPs = 10.88.0.0/24
PersistentKeepalive = 25
"@
Set-Content -Path "$root\client1.conf" -Value $c1 -Encoding ASCII

$c2 = @"
[Interface]
PrivateKey = $c2Priv
Address = 10.88.0.3/32

[Peer]
PublicKey = $serverPub
Endpoint = 119.91.210.85:51820
AllowedIPs = 10.88.0.0/24
PersistentKeepalive = 25
"@
Set-Content -Path "$root\client2.conf" -Value $c2 -Encoding ASCII

New-NetFirewallRule -Name 'WireGuard-51820' -DisplayName 'WireGuard UDP 51820' `
  -Direction Inbound -Protocol UDP -LocalPort 51820 -Action Allow -ErrorAction SilentlyContinue | Out-Null
New-NetFirewallRule -Name 'YebianSync-WG' -DisplayName 'Yebian sync from WireGuard' `
  -Direction Inbound -Protocol TCP -LocalPort 8787 -RemoteAddress 10.88.0.0/24 -Action Allow -ErrorAction SilentlyContinue | Out-Null

& 'C:\Program Files\WireGuard\wireguard.exe' /installtunnelservice "$root\wg0.conf"
Start-Sleep -Seconds 3

Get-Service 'WireGuardTunnel$wg0' | Select-Object Name, Status | Format-List
Write-Output "server_pub=$serverPub"
Write-Output "client1_pub=$c1Pub"
Write-Output "client2_pub=$c2Pub"
Stop-Transcript | Out-Null
