# 同步实际状态（2026-11-24）

> 这不是设计稿，是**当前部署的事实**。新会话/新机器先读这里，再看 `sync-server/deploy/README.md`。
> **不要**把 token、WireGuard 私钥、`clientN.conf` 写进这个文件或 Git。

## 拓扑

- 服务端：Windows Server `119.91.210.85`，`C:\YebianSync\yebian-sync-server.exe`，数据 `C:\YebianSync\data\`（SQLite + blobs）。
- 传输：**WireGuard**，服务端 `10.88.0.1:8787`，网段 `10.88.0.0/24`。
- 旧的 SSH 隧道（本机 `18787` → 服务器 `8787`）**2026-11-16 已停用**；Startup 里的 `YebianSyncTunnel.vbs` 被改名成 `.disabled-2026-11-16`。
- 应用云同步地址应为 `http://10.88.0.1:8787`，**不是** `127.0.0.1` / 旧 `18787`。
- 应用自动同步开启，间隔 1 分钟；不需要每天手动点。

## 设备

| 设备 | 传输 | 应用地址 | 状态 |
| --- | --- | --- | --- |
| 家里电脑 | WireGuard 客户端 | `http://10.88.0.1:8787` | 正常；服务器上 `10.88.0.2` 有近期握手 |
| 办公室电脑 | SSH 隧道（Startup VBS，`18787` → 服务器 `127.0.0.1:8787`） | `http://127.0.0.1:18787` | **2026-11-24 已恢复**；游标 9194，`last_error` 空 |

## 客户端配置在哪

- 服务端 `C:\YebianSync\` 生成 `wg0.conf`、`client1.conf`、`client2.conf`；客户端配置拷走后应从服务器删除。
- **每台设备必须用自己的 client 配置**，不要复用；两个 peer 用同一个 `10.88.0.x` 会冲突。
- 不要为了加设备直接 `-Rotate`：那会轮换所有客户端密钥，家里那台会断。先确认脚本是否支持只加 peer；不支持就手动在 `wg0.conf` 加一个 `[Peer]` 并重启 `WireGuardTunnel$wg0`。
- 私钥只存在设备上；本文件不记录。

## 办公室这台恢复步骤

1. 从家里那台或服务器 `C:\YebianSync\` 拿办公室对应的 `clientN.conf`（看 `Address = 10.88.0.x` 判断是哪台）。
2. 安装 WireGuard for Windows，导入配置，连接。
3. 验证：
   ```bash
   curl http://10.88.0.1:8787/v1/health
   ```
4. 应用「设置 → 云同步」地址改为 `http://10.88.0.1:8787`；vault id / token 不动，保存。
5. 等 1 分钟自动同步；数据应回到这台机器。

## 不要做

- 不要在两台电脑上各跑一个 sync-server。
- 不要新建 vault / 重置 cursor。
- 不要用 `127.0.0.1` 当另一台电脑的同步地址。
- 不要把 `clientN.conf`、私钥、token 提交到 Git。

## 应用侧体检与自愈（P200，2026-11-24）

- 新增 `sync_health` 命令：GET `<服务器地址>/v1/health`，2 秒超时，返回可达性、延迟、HTTP 状态、错误和地址类型（loopback / tailscale / private / public）。
- 设置 → 云同步顶部显示「服务器可达 / 不可达」、当前地址、延迟；loopback 会明确写“只指向当前这台电脑”，旧 `18787` 会标“已停用的旧 SSH 隧道端口”。
- 后台同步在每次要推/拉之前，如果配置的是 **loopback 且探针失败**，会尝试 `http://10.88.0.1:8787`；通了就自动改写地址（token / vault id 不动），不需要手点。只自愈 loopback，Tailscale/公网地址临时离线不会被替换。
- 设置页地址输入只在第一次加载时回填，10 秒一次的状态刷新不会再覆盖正在编辑的输入框。
- 实测：`cargo test --lib sync::health` 7 passed（探针打真实本地 HTTP 服务、地址分类、报告字段、死 loopback 自动迁移且保留 token/vault id）；`npm run build` 通过。
- 限制：传输没连上（没装 WireGuard/Tailscale、没有 client 配置）时，应用代码无法把数据送过去；这台机器仍需先恢复传输。

## 方案取舍（2026-11-24）

- **数据层保持 server-authoritative op log**。PowerSync / ElectricSQL / Zero 这些主流本地优先引擎都收敛到“中心服务端定序 + 本地 SQLite 物化”这一路线；Syncthing 只同步文件，不覆盖 `annotations`、`facts`、`production_jobs` 这些表，不能用。
- **传输层长期换 Tailscale**：服务器和两台电脑登录同一账号即可，地址形如 `http://100.x.y.z:8787`；自动 NAT 穿透，直连失败时走 DERP 中继，不用每台机器拷 `clientN.conf`。这是当前“每台设备一份 WireGuard 配置”问题的根解。
- **Cloudflare Tunnel 是备选**：`cloudflared` 只向外建连，不需要公网端口；但需要域名和公开入口，暴露面比 tailnet 大，不适合当前的单用户私有库。
- 应用侧代码不依赖具体传输：换成 Tailscale 后只改设置里的地址即可，探针/自愈/同步引擎都不动。
- 参考：
  - Tailscale NAT 穿透与 DERP：<https://tailscale.com/blog/how-nat-traversal-works>
  - Cloudflare Tunnel（outbound-only）：<https://developers.cloudflare.com/cloudflare-one/connections/connect-networks/>

## 2026-11-24 以家里那台为准（P202）

- **谁覆盖了谁**：家里设备 `fa546746-8c05-42ad-83d6-1f73f7b910df` 最后一条 op 是 `seq 8350`（HLC `1795452857688`，约 00:54）；办公室设备 `c2e900da-987a-43e9-b1ed-8373ff52b481` 在 12:01 追加了 `seq 8351–9194` 共 844 条，其中文件 upsert 78 条。办公室离线 13 天的 pending 在推的时候拿到**新的 HLC**，所以压过了家里凌晨的版本。
- **16 篇被覆盖**：家里在 `seq 7625–7696` 对 16 个路径有不同 hash 的 upsert；办公室的旧稿把它们覆盖了。已从服务器 blob 拉回家里的版本、逐篇校验 SHA-256、写回 vault；应用自动推送 `seq 9195–9210`，服务端 head `9210`，`last_error` 空。办公室旧稿备份在 `%TEMP%\yebian-vault-backup-2026-11-24T04-39-15-051Z`（16 篇，256K）。
- **62 篇办公室-only**：这些路径在家里 `seq <= 8350` 的历史里从未出现，不是“覆盖”；P203 已按用户确认删除，清单在 vault 根目录 `同步基线-办公室删除清单-2026-11-24.md`。
- **表数据仍是混合**：办公室 844 条里还有 `production_seen_videos` 521、`production_jobs` 148、`note_new` 40、`facts` 10 等；P203 已按基线回滚/删除，数字见下一节。
- **完整以家里为准**：在家里那台运行 `npm run sync:force-push`（`tools/force-push.cjs`）。它把 `sync_shadow` 全部标成待推、把 `deleted=0`，下一次同步会重推本机所有行/文件，并对本机已删除的路径重发 delete，从而覆盖办公室的 12:01 版本并清掉复活路径。工具需要先在设备上（提交推送或手工复制）。
- **注意**：如果家里那台已经启动并拉过办公室的 ops，旧路径可能已被重新创建；先确认/清理这些路径，再跑 force-push，否则它们会被当成“本机现有文件”再推上去。应用同步顺序是先 push 再 pull，所以只要 force-push 后启动，家里的 delete 会先到服务器。
- **限制**：没有家里那台的实际访问；16 篇的“家里版本”来自服务器上家里最后一次同步的 blob，家里在 00:54 之后改过但没同步的内容不在里面。

## 2026-11-24 基线快照（P203）

- **基线**：`seq 8350`，即家里设备最后一次同步的位置（约 00:54）。这是双方共同的“最初版本”。
- **做法**：新增 `tools/push-baseline.cjs`，以合成设备把“当前状态 → 基线”的差异作为普通 op 推给服务器；应用按正常 pull 应用，不改本地 DB，不做文件手术。合成 HLC 取服务器上最大 HLC + 1ms，能压过办公室 12:01 的旧 pending，但会输给家里之后新推的本地修改。
- **结果**：推送 355 条（121 upsert / 234 delete）；服务端 head `9210 → 9565`，应用随后把清单文件推成 `9566`；办公室游标 `9566`，`last_error` 空。
- **删除（办公室-only）**：62 个文件（`ScribeFlow/` 3 + `待整理/` 59）、104 个任务、21 条 `note_new`、10 条事实（连带 checks/evidence/relations/mentions/sources）、1 条 seen video、1 个自定义提示词及其版本。
- **恢复为家里版本**：19 个任务、73 条 seen video、9 个订阅、1 条标注、19 条 `note_new`。
- **当前**：vault `.md` 从 182 → **121**（基线 120 + 清单 1）；`note_new` 50、`facts` 100、`fact_sources` 92、`production_jobs` 187、`production_seen_videos` 520、`production_subscriptions` 9。
- **清单**：vault 根目录 `同步基线-办公室删除清单-2026-11-24.md`，列出 62 个删除文件、104 个删除任务标题、21 条删除笔记记录、10 条删除事实，便于后续核对遗漏。清单本身在基线之外；再跑 `sync:push-baseline` 会删掉它。
- **家里那台**：下次启动/同步时会拉这 356 条 op，收敛到同一基线；它在 00:54 之后没同步的本地修改会先 push（push 先于 pull），HLC 比合成 HLC 新，因此不会被基线覆盖。
- **限制**：基线只覆盖家里最后一次同步到服务器的内容；00:54 之后家里本地未同步的改动不在这里。合成设备会出现在 op log 里。`production_jobs.config_json` 含家里 ffmpeg 路径，历史任务不重跑时不影响。

## 2026-11-24 恢复记录

- **防火墙**：腾讯云轻量服务器「Windows Server-AF6t」（`lhins-mp0a9rl7`，广州，`119.91.210.85`）新增 `TCP:22`，来源 `60.186.85.254/32`（办公室当前公网 IP），备注 `SSH deploy zcode`。原有 22 规则只放行了家里/历史 IP，办公室不在列表里。
- **传输**：恢复 Startup 里的 `YebianSyncTunnel.vbs`（之前改名 `.disabled-2026-11-16`），内容为隐藏运行 `ssh -N -L 18787:127.0.0.1:8787 -i ~/.ssh/yebian_sync_deploy Administrator@119.91.210.85`，断线 5 秒后重连；开机登录自启，不需要手动点。
- **服务端**：同步服务任务 `YebianSync` 仍绑 `0.0.0.0:8787`，但云防火墙未放行 8787，公网不可达；22/80/ICMP/3389/51820 是放行项。新二进制把 axum 默认 2MB 请求体上限提高到 64MB（离线设备一次推全部 pending 会超过 2MB，原报 `push failed: HTTP 413`）；部署后大小 `3,430,912 B`。
- **实测（办公室这台）**：隧道本地 `/v1/health` = `{"ok":true}`；同步游标 `2597 → 9194`，`last_error` 空，`last_sync_at` 更新到当天；`note_new` 40→71、`facts` 69→110、`fact_sources` 64→99、`production_jobs` 138→291、`history_search_candidates` 360→399；vault 内 `.md` 182 篇，82 篇带隐藏标记。
- **限制**：办公室公网 IP 会变；变了之后 SSH 规则失效，隧道会重连失败，需要更新安全组来源。WireGuard 的 `UDP:51820` 是对所有 IPv4 放行的，`C:\YebianSync\client2.conf`（`10.88.0.3`）还在服务器上，可作为不依赖办公室 IP 的备选传输；但本机没装 WireGuard，本次没有切换。
- **安全**：服务器 `3389/tcp` 对所有 IPv4 放行，且 Administrator 密码曾出现在旧会话记录里；建议限制 RDP 来源并轮换密码。服务器上的 `client1.conf` / `client2.conf` 仍保留，设备都装好后应删除或转存到密码管理器。
