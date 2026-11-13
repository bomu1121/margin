# 部署到自己的服务器

服务端只有一个 Rust 二进制 + 一个 SQLite 文件 + 一个 blob 目录。下面是三条路径，按你的服务器情况选一条。

## 0. 通用步骤（三条路径都要做）

1. 上传仓库（在开发机上）：
   ```bash
   scp -r sync-server user@SERVER_IP:/tmp/yebian-sync-src
   ```
2. 在服务器上安装：
   ```bash
   sudo bash /tmp/yebian-sync-src/sync-server/deploy/install.sh /tmp/yebian-sync-src
   ```
   安装脚本会自动：装编译依赖、装 Rust（如果没有）、编译 release、建 `yebian-sync` 系统用户、写 systemd 服务、启动。
3. 建库并保存 token（只打印一次）：
   ```bash
   sudo -u yebian-sync /usr/local/bin/yebian-sync-server create-vault default \
     --db /var/lib/yebian-sync/sync.sqlite \
     --blobs /var/lib/yebian-sync/blobs
   ```
4. 腾讯云安全组：
   - 只放行 **22/tcp**（SSH，最好限制来源 IP）。
   - **不要**对公网放行 8787；服务默认只监听 127.0.0.1 或 Tailscale 地址。
   - 走域名 HTTPS 时再放行 80/443。
5. 备份：
   ```bash
   sudo bash /tmp/yebian-sync-src/sync-server/deploy/backup.sh
   ```
   建议加一条 cron（见脚本头部）。

## A. 域名 + Caddy（最接近正式产品）

需要：一个域名、服务器在境外/香港，或大陆服务器已完成 ICP 备案。

```bash
# 1) 先按 loopback 方式安装
BIND_OVERRIDE=127.0.0.1:8787 sudo -E bash /tmp/yebian-sync-src/sync-server/deploy/install.sh /tmp/yebian-sync-src

# 2) 安装 Caddy
sudo apt install -y caddy        # Debian/Ubuntu
# 或按 https://caddyserver.com/docs/install 安装

# 3) 配置反向代理
sudo cp /tmp/yebian-sync-src/sync-server/deploy/Caddyfile.example /etc/caddy/Caddyfile
sudo sed -i 's/sync.example.com/你的域名/' /etc/caddy/Caddyfile
sudo systemctl reload caddy
```

客户端填 `https://你的域名`。Caddy 自动申请 Let's Encrypt 证书，客户端不需要额外信任配置。

## B. 私有网络（没有域名 / 大陆服务器推荐）

服务只绑在私有网络地址上，公网完全看不到 8787。

### B1. Tailscale（最省事）

```bash
curl -fsSL https://tailscale.com/install.sh | sh
sudo tailscale up
```

然后重新跑安装脚本（它会自动把服务绑到 Tailscale IPv4）：

```bash
sudo bash /tmp/yebian-sync-src/sync-server/deploy/install.sh /tmp/yebian-sync-src
```

在每台客户端设备装 Tailscale 并登录同一账号，客户端填：

```
http://100.x.y.z:8787
```

`100.x.y.z` 是服务器的 Tailscale IP（`tailscale ip -4`）。

> Tailscale 的控制面在境外；大陆网络下可能需要代理或改用 B2。

### B2. WireGuard（完全自建）

在服务器和客户端之间建一个 WireGuard 网段（例如 `10.88.0.1/24`），服务绑到 `10.88.0.1:8787`：

```bash
BIND_OVERRIDE=10.88.0.1:8787 sudo -E bash /tmp/yebian-sync-src/sync-server/deploy/install.sh /tmp/yebian-sync-src
```

安全组只放行 `51820/udp`（WireGuard）和 22。客户端填 `http://10.88.0.1:8787`。

### B3. SSH 隧道（零额外服务）

每台客户端启动时跑：

```bash
ssh -N -L 8787:127.0.0.1:8787 user@SERVER_IP
```

客户端填 `http://127.0.0.1:8787`。隧道本身由 SSH 加密；缺点是每台设备要维持一条隧道。

## C. 公网 IP + 自签证书

不推荐：需要给客户端加自定义根证书（当前客户端还没做这个设置）。有域名就用 A，没有就用 B。

## D. Windows Server（本次实际使用）

Windows Server 不能跑 `install.sh`，用下面这套。服务仍然只监听 `127.0.0.1:8787`，客户端通过 SSH 隧道访问。

```powershell
# 1) 在开发机编译静态 CRT 的 Windows 版本
$env:RUSTFLAGS = "-C target-feature=+crt-static"
cargo build --release --manifest-path sync-server/Cargo.toml

# 2) 传到服务器
scp sync-server/target/release/yebian-sync-server.exe `
  Administrator@SERVER_IP:C:/YebianSync/yebian-sync-server.exe

# 3) 建库（token 只打印一次）
ssh Administrator@SERVER_IP `
  'C:\YebianSync\yebian-sync-server.exe create-vault default --db C:\YebianSync\data\sync.sqlite --blobs C:\YebianSync\data\blobs'

# 4) 注册开机任务（SYSTEM 运行）
scp sync-server/deploy/windows-install.ps1 Administrator@SERVER_IP:C:/YebianSync/
ssh Administrator@SERVER_IP `
  'powershell -NoProfile -ExecutionPolicy Bypass -File C:\YebianSync\windows-install.ps1'
```

客户端（开发机）建立隧道并设为登录自启：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File sync-server/deploy/windows-tunnel.ps1
```

它会在当前用户的 Startup 目录写一个隐藏运行的 VBS，断线每 5 秒重连。客户端「云同步」填：

- 服务器地址：`http://127.0.0.1:8787`
- 笔记库 ID / Token：第 3 步打印的值

服务器每日备份（停止服务 1 秒，复制 SQLite + WAL + blobs，保留 14 份）：

```powershell
scp sync-server/deploy/windows-backup.ps1 Administrator@SERVER_IP:C:/YebianSync/
ssh Administrator@SERVER_IP `
  'schtasks /Create /F /SC DAILY /ST 03:30 /TN YebianSyncBackup /TR "powershell -NoProfile -ExecutionPolicy Bypass -File C:\YebianSync\windows-backup.ps1" /RU SYSTEM /RL HIGHEST'
```

腾讯云轻量防火墙只需要给客户端出口 IP 放行 **22/tcp**；`8787` 不要对公网开放。

## E. Windows Server（WireGuard，推荐）

比 SSH 隧道更适合长期运行：服务端加入 WireGuard 网段，客户端直接访问 `10.88.0.1:8787`，不再依赖登录自启的 SSH 隧道。

前提：服务器和客户端安装 WireGuard for Windows；服务器已有 `C:\YebianSync\yebian-sync-server.exe`、`data/` 和 `YebianSync` 计划任务（先按 D 的 1–4 步完成）。

先把两个脚本传到服务器：

```powershell
scp sync-server/deploy/windows-wireguard-server.ps1 `
    sync-server/deploy/windows-wireguard-bind.ps1 `
    Administrator@SERVER_IP:C:/YebianSync/
```

### E1. 服务器生成配置并启动 WireGuard

```powershell
# 默认生成 server + 2 个客户端，服务监听 UDP 51820
powershell -NoProfile -ExecutionPolicy Bypass `
  -File C:\YebianSync\windows-wireguard-server.ps1

# 三台设备 / 换公网 IP 时：
# -ClientCount 3 -ServerEndpoint 新IP
```

脚本会：

- 生成 `wg0.conf`、`client1.conf`、`client2.conf`；
- 把私钥文件 ACL 收紧到 Administrators / SYSTEM；
- 创建 UDP 51820 和仅允许 `10.88.0.0/24` 访问 8787 的防火墙规则；
- 安装并启动 `WireGuardTunnel$wg0`。

**不要**在已有配置上直接重跑：默认会拒绝，避免旧客户端全部失效。确实要换密钥时显式加 `-Rotate`，脚本会先备份到 `C:\YebianSync\wg-backup-<时间>`。

### E2. 服务绑定到 WireGuard 地址

```powershell
powershell -NoProfile -ExecutionPolicy Bypass `
  -File C:\YebianSync\windows-wireguard-bind.ps1
```

默认把计划任务改成：

```text
--bind 10.88.0.1:8787
```

脚本会拒绝 `0.0.0.0`（除非显式 `-AllowPublicBind`），并在 `http://10.88.0.1:8787/v1/health` 上重试自检。

### E3. 客户端

1. 把服务器上的 `clientN.conf` 拷到对应设备，WireGuard 客户端导入；
2. 服务器上的 client 配置拷走后删除；
3. 页边笔记「云同步」服务器地址改为：

```text
http://10.88.0.1:8787
```

不是 `127.0.0.1`，也不是旧的 `18787`。

### E4. 云防火墙

腾讯云/安全组只放行：

- `51820/udp`，来源限客户端公网 IP；
- `22/tcp` 只给运维来源。

`8787` 仍然不要对公网开放；客户端通过 WireGuard 内网访问。

## 升级

```bash
scp -r sync-server user@SERVER_IP:/tmp/yebian-sync-src
sudo bash /tmp/yebian-sync-src/sync-server/deploy/install.sh /tmp/yebian-sync-src
```

数据在 `/var/lib/yebian-sync/`，升级不会动它。

## 排查

```bash
systemctl status yebian-sync
journalctl -u yebian-sync -n 100 --no-pager
curl -s http://127.0.0.1:8787/v1/health
```
