# 数据同步方案（Yebian Cloud Sync）

> 目标：把页边笔记的全部用户数据放到自己的服务器上，多台设备始终一致，**不需要用户手动解决冲突**。
> 本文是选型结论 + 协议 + 落地计划。实现见 `src-tauri/src/sync/` 与 `sync-server/`。

---

## 0. 结论（先看这里）

**方案：服务器权威的「有序操作日志 + 本地 SQLite 物化」同步层，分记录面和文件面两条数据流。**

- **记录面**：`yebian.sqlite` 里的业务表按「行级 LWW（Last-Writer-Wins）+ 墓碑 + HLC（混合逻辑时钟）」同步。
- **文件面**：vault 里的 Markdown / artifact JSON 按「内容寻址 blob（SHA-256）+ 路径清单 LWW」同步；正文并发修改保留双方内容，不要求用户操作。
- **服务端**：只做「幂等、去重、按序追加的加密日志 + blob 仓」，不跑业务逻辑。TLS + 每库 token；协议把 payload 当不透明字节，后续加端到端加密不需要改服务端。
- **冲突策略**：全部自动。LWW 决定同一行的最终值；被覆盖的一方写进 `sync_conflicts` 留档，用户不需要点任何按钮。文件冲突保留失败方的 blob，永不静默丢数据。

**为什么这是「不是权宜之计」的解法**：

1. 它符合 2026 年行业已经收敛的形态：服务器是权威源，客户端是本地缓存（Zero / PowerSync / ElectricSQL 都是这个方向）；我们只是把它做成适合本项目的 Rust + SQLite 版本。
2. 协议是「操作日志 + 不透明 payload + 内容寻址 blob」，所以后面的 **端到端加密、共享笔记库、Web 端、正文协同编辑** 都能在不改服务端的前提下逐层加上。
3. 服务端只有两张表（ops / blobs）和四个接口，部署、备份、迁移都简单；将来数据量上来可以整体换 Postgres，客户端协议不变。

**不选现成引擎的原因**（详见 §2）：

- `cr-sqlite` 在 2026 年已事实停更（作者加入 Rocicorp 做 Zero），不适合作为长期底座。
- `PowerSync / Zero / ElectricSQL` 都以 Postgres 为中心，并要求同步引擎接管客户端数据库/查询。本项目是 Rust 后端直接 `rusqlite` 写 24 张表，还有文件 vault；迁过去等于重写数据层，并引入 Postgres + 一个额外常驻服务。ElectricSQL 还是只读同步，离线写要自己做。
- `Syncthing / 网盘` 只能同步 SQLite 文件本身，WAL + 多端并发写会损坏，也无法做行级合并。
- `Automerge / Loro / Yjs` 是协作文档 CRDT，适合正文字符级合并；但把关系型数据整体改成文档是重写，且元数据会膨胀。本应用正文**不在应用内编辑**，当前收益不足以承担这个迁移；协议已为将来接入它留了口子（见 §8）。

---

## 1. 本项目要同步什么

两个数据库：

| 库 | 路径 | 是否同步 | 原因 |
| --- | --- | --- | --- |
| 用户库 | `%APPDATA%\com.yebian.notes\yebian.sqlite` | **是** | 标注、设置、事实、史料、引用、任务、订阅、提示词等全部用户数据 |
| 索引库 | `%APPDATA%\com.yebian.notes\yebian-index.sqlite` | **否** | 派生数据，每台设备本地重建；同步它只会制造冲突和体积 |

文件面：vault 目录下的 Markdown、artifact JSON 等非隐藏文件。`.trash/`、`.yebian/`、`.git/`、`node_modules/`、`target/` 不同步。

### 1.1 用户库里的表怎么分类

| 类别 | 表 | 同步 | 说明 |
| --- | --- | --- | --- |
| 标注/设置 | `annotations`, `settings` | 是 | 设置里的密钥、本机路径要过滤（§6） |
| 事实 | `facts`, `note_fact_mentions`, `fact_sources`, `fact_evidence`, `fact_checks`, `fact_relations`, `fact_merges`, `fact_splits`, `fact_conflict_resolutions`, `fact_reviews`, `fact_meta` | 是 | 合成事实/合并/拆分是用户资产 |
| 史料 | `history_events`, `sources`, `source_passages`, `citations`, `history_meta`, `history_search_candidates` | 是 | 候选也是用户劳动成果 |
| 制作 | `production_jobs`, `production_prompts`, `production_prompt_versions`, `production_model_prices`, `production_subscriptions`, `production_seen_videos` | 是 | 任务历史/提示词/订阅跟着走 |
| 设备本地 | `production_steps`, `production_logs`, `production_checkpoints`, `history_page_cache` | **否** | 运行日志、断点、页面缓存是设备本地的；断点续跑不应跨设备抢 |
| 索引 | `notes`, `notes_fts`, `index_meta` | 否 | 派生 |

> `production_subscriptions` / `production_seen_videos` / `production_jobs` 里的 `vault_path` 是设备本地路径。同步时把它归一化成 `__VAULT__` 占位符，应用时换回本机路径，否则两台机器会互相插入重复行。

---

## 2. 选型对比（2026 年现状）

| 方案 | 形态 | 离线多端写 | 能处理文件 vault | 改造成本 | 长期风险 | 结论 |
| --- | --- | --- | --- | --- | --- | --- |
| Syncthing / Nextcloud / 网盘 | 文件级 | 会冲突 | 是 | 低 | SQLite WAL 并发写损坏，无行级合并 | 否 |
| 直接同步 SQLite 文件 | 文件级 | 损坏风险 | 否 | 低 | 不可用 | 否 |
| cr-sqlite | SQLite 扩展 CRDT | 是 | 否 | 中 | **已事实停更**（作者加入 Rocicorp；仅 Fly.io 私有 fork 在用） | 否 |
| PowerSync | Postgres → 客户端 SQLite | 是（写回自建 API） | 否 | 高 | 需 Postgres + 常驻服务 + 客户端 SDK 接管 DB | 否（本机 Rust 数据层不匹配） |
| Zero (Rocicorp) | Postgres → 客户端缓存 | 乐观写 + 服务端 mutator | 否 | 高 | Postgres-only；客户端模型偏 Web/JS | 否 |
| ElectricSQL | Postgres → 客户端只读 | **只读**，写要自建 API | 否 | 中高 | 不解决离线写队列/冲突 | 否 |
| Automerge / Loro / Yjs | 文档 CRDT | 是 | 需要另做 | 很高（数据模型重写） | 元数据膨胀；SQL 查询丢失 | 否（作为正文 CRDT 的将来选项） |
| **自研：服务器权威 op log + 本地 SQLite 物化** | 记录面 LWW + 文件面 CAS | 是 | 是 | 中（约 1500 行核心） | 自己要维护同步协议 | **是** |

关键判断：本项目的同步量很小（标注/事实是几百到几万行，正文是文本），**性能不是瓶颈，数据层兼容性和长期可控性才是**。自研 op log 能让现有 24 张表、现有 Rust 写路径、现有文件 vault 都原样保留。

---

## 3. 架构

```
┌────────────── 设备 A ──────────────┐        ┌──────── 你的服务器 ────────┐
│ yebian.sqlite (权威本地副本)        │        │ sync-server (Rust/axum)    │
│  ├─ 业务表                         │  HTTPS │  ├─ vaults(token_hash)     │
│  ├─ sync_state (cursor/device)     │ ─────► │  ├─ ops(vault,seq,op_id,   │
│  ├─ sync_shadow (行 hash + HLC)    │  推送  │  │      hlc,payload)        │
│  └─ sync_conflicts (冲突留档)      │ ◄───── │  └─ blobs/<vault>/<sha256> │
│ vault/*.md (文件面)                │  拉取  │                            │
└────────────────────────────────────┘        │ 只有追加日志 + blob 仓     │
                                              │ 不解析业务 payload          │
┌────────────── 设备 B ──────────────┐        └────────────────────────────┘
│ 同一套本地结构，独立 device_id     │
└────────────────────────────────────┘
```

- **服务器是权威排序，客户端是本地物化**：服务器给每条 op 一个单调 `seq`；客户端按 `seq` 拉取，按 HLC 做 LWW。
- **推送幂等**：op 有全局唯一 `op_id`，服务器 `INSERT OR IGNORE` 去重；客户端超时重推不会重复。
- **两阶段同步**：先 diff 本地 → 推 op + blob；再拉远端 op → 应用。两个方向都用同一套 op 格式。
- **离线**：所有本地写先落 SQLite，同步是后台附加动作；断网只影响「什么时候同步」，不影响使用。

---

## 4. 协议

### 4.1 时钟与顺序

每条 op 带一个 **HLC**，编码成可字典序比较的字符串：

```
{物理毫秒:013}-{逻辑计数:005}-{device_id}
例：0001794068583104-00000-b3f2...
```

- 同一行多版本时，HLC 大者胜；物理时间相同用计数，再相同用 device_id 决出确定赢家。
- HLC 在客户端生成；服务器不信任也不改写它，只按到达顺序分配 `seq`。
- 这样即使两台设备时钟有偏差，也有一个确定的、所有人一致的胜负，不需要人工裁决。

### 4.2 op 格式

```json
{
  "op_id": "uuid",
  "entity": "table:annotations",     // 或 "file"
  "pk": "note_id\u001fblock_id",      // 表的主键拼接；file 用 rel_path
  "kind": "upsert" | "delete",
  "hlc": "0001794068583104-00000-设备",
  "payload": { ... },                // upsert 是整行 JSON；delete 为 null
  "codec": "json"                    // 预留 "xchacha20poly1305"，服务端不解析
}
```

- **payload 对服务端不透明**。现在 `codec=json`，将来开启 E2E 只改客户端编码；服务端仍按字节存储和转发。
- 行级整行覆盖（而不是字段级 diff），因为本项目的冲突面主要是「同一行谁最后改」，整行 LWW 足够且实现简单。

### 4.3 接口

所有接口要求 `Authorization: Bearer <vault-token>`；`{vault}` 必须与 token 绑定的库一致。

| 方法 | 路径 | 作用 |
| --- | --- | --- |
| `POST` | `/v1/vaults/{vault}/ops` | 推送一批 op；按 `op_id` 去重；返回 `{accepted:[op_id], next_seq}` |
| `GET` | `/v1/vaults/{vault}/ops?cursor=&limit=` | 按 `seq` 升序拉取；返回 `{ops:[...], cursor, has_more}` |
| `PUT` | `/v1/vaults/{vault}/blobs/{sha256}` | 上传 blob；服务端校验 SHA-256；幂等 |
| `GET` | `/v1/vaults/{vault}/blobs/{sha256}` | 下载 blob |
| `GET` | `/v1/health` | 健康检查（不需要 token） |

`cursor` 是服务器 `seq`，不是 HLC。客户端只记「我拉到哪」，不参与冲突判定。

### 4.4 鉴权与部署

- 每个笔记库一个随机 token（32 字节），服务端只存 SHA-256。
- TLS 由反向代理终止（Caddy / Nginx），服务端默认只监听 `127.0.0.1:8787`。
- 创建库：`create-vault` 是第一个参数，打印一次 `vault_id` + `token`，客户端填一次。
- 备份：SQLite 在线备份或 Litestream；blobs 目录 rsync。

```bash
cd sync-server
cargo build --release

# 1. 建库（只打印一次 token，自己保存好）
./target/release/yebian-sync-server create-vault demo \
  --db ./data/sync.sqlite --blobs ./data/blobs

# 2. 启动（建议前面挂 Caddy 做 HTTPS）
./target/release/yebian-sync-server \
  --db ./data/sync.sqlite --blobs ./data/blobs --bind 127.0.0.1:8787
```

客户端在「制作 → 设置 → 云同步」填服务器地址、笔记库 ID、Token，保存后点「立即同步」。

---

## 5. 冲突规则（全部自动，无需用户操作）

| 数据 | 规则 | 用户需要做什么 |
| --- | --- | --- |
| `settings` | 每个 key 独立 LWW | 无 |
| `annotations` | 按 `id` LWW；删除是墓碑 | 无 |
| `facts` 等事实表 | 按 `id` 整行 LWW；合并/拆分是带 `undone_at` 的操作行，重复应用幂等 | 无 |
| 同 `normalized` 事实并发创建 | 由服务端 `seq` + HLC 决出保留行；另一行进 `sync_conflicts`，不静默丢 | 无 |
| 史料/引用/候选 | 按 `id` LWW；删除墓碑 | 无 |
| 制作任务/订阅 | 按主键 LWW；`vault_path` 归一化 | 无 |
| 笔记文件 | 路径清单 LWW；同路径并发改内容时保留本机版本，远端版本写进 `sync_conflicts` 并保留 blob | 无 |
| 正文并发编辑 | 当前版本：块级并集合并（按已有 `block_id`），不丢段落；同块冲突时保留双方 | 无 |

**「不用手动解决」的定义**：任何冲突都有确定的自动结果；被覆盖的数据进 `sync_conflicts` 可查，界面不弹「请选择保留哪份」。这比网盘的 conflict copy 更省事，也比纯 LWW 更安全。

### 5.1 首次同步（新设备第一次连接）

`cursor = 0` 且 `sync_shadow` 为空时，本次同步先**拉后推**，避免本地旧版本先写进服务器：

| 情况 | 结果 | 留档 |
| --- | --- | --- |
| 服务器有、本地没有 | 拉到本地 | 无 |
| 本地有、服务器没有 | 合并后由 push 上传 | 无 |
| 两边同 `id` / 同路径、内容不同 | 按下面的规则判胜负，只保留赢家 | 输家写 `sync_conflicts` |
| 服务器删了、本地还有 | 保留本地，push 重新上架 | 写 `sync_conflicts` |

同 key 的胜负规则：

- **记录行**：两边都有数值 `updated_at` 时，大的赢；相等或没有该列时，服务器赢。
- **文件**：本地文件 mtime 与远端 op 的 HLC 物理时间比，新的赢；相等时服务器赢。
- 输家无论在本机还是服务器，都写进 `sync_conflicts`；**文件冲突额外把输家内容复制到 `vault/.yebian/conflicts/<毫秒时间戳>/<相对路径>`**，不需要用户点任何按钮。

首次同步完成后，后续走正常的 HLC LWW（上表）。`sync_shadow` 有记录或 `cursor > 0` 时不会再次进入首次同步分支。

---

## 6. 安全与隐私

- **默认不同步的秘密**：`settings` 里含 `api_key` / `cookies` / `account` / `token` / `secret` 的键、以及 `vault_path` / `index_root`，一律不上传。原则是「用户内容同步，设备凭据不同步」。
- **传输**：TLS；token 只存哈希。
- **服务端不解析 payload**：为 E2E 留路。E2E 开启后，token 只用于路由，内容用库密钥（XChaCha20-Poly1305）加密；新设备用一次性恢复码配对。
- **体积上限**：单 blob 默认 64 MiB；单批 op 默认 2 MiB；超限拒绝并提示，不静默截断。
- **路径安全**：应用远端文件前校验 `rel_path` 不含 `..`、不以 `/` 或盘符开头，且落在 vault 内。

---

## 7. 客户端集成

新增三张本地表（在 `yebian.sqlite`，不参与同步）：

```sql
sync_state(id CHECK(id=1), vault_id, device_id, cursor, server_url, token)
sync_shadow(entity, pk, row_hash, hlc, deleted, PRIMARY KEY(entity, pk))
sync_conflicts(id, entity, pk, local_json, remote_json, remote_blob, created_at)
```

同步循环：

0. `first sync`：`cursor = 0` 且 `sync_shadow` 为空时，先拉取服务器全部 op 并按 §5.1 合并，再把 cursor 推到远端末尾；之后才进入下面的 diff → push → pull。这样本地从未见过服务器的旧副本不会先覆盖远端。
1. `collect`：读取所有同步表的当前行，规范化 `vault_path`，按 `(entity, pk)` 算 canonical JSON 的 SHA-256。
2. `diff`：和 `sync_shadow` 比。新增/变化 → 生成 upsert op；影子有而本地无 → 生成 delete op。
3. `push`：上传缺失 blob、推送 op；服务器返回接受情况。
4. `pull`：从 `cursor` 拉远端 op。
5. `apply`：跳过自己的 op；`shadow.hlc >= op.hlc` 跳过；否则写入/删除，并更新影子。若本机这一行在影子之外还有未同步修改，保留本机、把远端写进 `sync_conflicts`（本地优先，不丢）。
6. `commit cursor`。

触发时机：保存配置后立即生效；任意同步表写入会 bump `sync_dirty.seq`，后台线程每 2 秒检查一次，有本地改动就推送；远端按设置间隔（默认 1 分钟）拉取；「手动同步一次」只是可选的兜底，不是正常流程的一部分。同步在后台线程跑，不阻塞 UI。

---

## 8. 分期与将来

| 阶段 | 内容 | 状态 |
| --- | --- | --- |
| P0 | 记录面（标注/设置/事实/史料/任务）+ 文件面；服务端；收敛测试 | 已实现（本次） |
| P1 | 设置页配置、自动同步、状态/错误展示 | 已实现（冲突查看 UI 未做） |
| P2 | 端到端加密（op 与 blob 都加密；恢复码配对） | 协议已预留 |
| P3 | 正文块级 CRDT / Loro 接入（应用内编辑上线时） | 数据模型已按 `block_id` 预留 |
| P4 | 共享笔记库、只读分享、Web 端 | 服务端按库隔离，天然支持多库 |

**为什么现在不做 E2E / 正文 CRDT**：不是省事，而是它们依赖 UI 配对流程/编辑器，当前没有入口；协议层已经把二者变成「客户端加一层编解码」，不需要推翻服务端。

---

## 9. 验证

- 单元：HLC 单调/偏序、canonical JSON 哈希稳定、路径安全、设置过滤。
- 收敛：两个独立 `yebian.sqlite` + 两个 vault + 内存传输，模拟离线并发（新增/修改/删除/同一行同时改/文件新建/移动/删除），断言双方最终表内容与文件哈希一致。
- 服务端：push 去重、seq 游标、blob 校验、无 token 拒绝。
- 端到端：真实 axum 服务 + 两个客户端跑完整 push/pull。

结果写进 `docs/experiment-log.txt`，数字以实测为准。

---

## 10. 诚实清单（当前实现）

- 记录面是**行级 LWW**，不是字段级合并；同一行两端同时改不同字段，后写者覆盖整行。本项目的事实/标注行大多是整行语义，影响可接受；但这是需要知道的边界。
- 同 `normalized` 的事实并发创建：后到的一方进 `sync_conflicts`，本地保留本机版本；两台设备可能各留一条本地事实，尚未按 `normalized` 自动并成一条（P2）。这是「完全同步」目前唯一的已知缺口。
- 首次同步的胜负依赖 `updated_at`；没有该列的表（如 `annotations`、`settings`）在两边内容不同时一律**服务器赢**，本地版本只进 `sync_conflicts`（不丢，但不会自动采用）。
- 文件冲突按本地 mtime 与远端 op 的 HLC 物理时间比较；mtime 在复制/解压后可能不可靠，此时会退回服务器优先。冲突副本在 `.yebian/conflicts/<时间戳>/` 下，不会自动清理。
- 首次同步只在 `cursor = 0` 且 `sync_shadow` 为空时触发；如果本机曾经同步过、之后被手动清空影子，不会重新合并。
- 文件面正文目前是**路径级 LWW + 冲突留档**；块级并集合并是后续项，本文档先定义规则。
- 同步是**最终一致**，不是实时；手动/定时触发，延迟取决于配置。
- 服务端是单机 SQLite；没有分片、没有多副本、没有限流。适合个人/小团队，不适合公开多租户。
- 没有 E2E、没有设备配对 UI；`sync_conflicts` 有数据但没有查看界面（P2）。
- 断开同步/删除服务器数据不会删除本地数据；但服务器上的 blob 不做垃圾回收（P2 需要）。
- `production_logs` / `checkpoints` / `steps` / `history_page_cache` 不同步，避免跨设备抢运行状态。
- 同步是**全库快照 diff**，每次同步会扫一遍同步表和 vault 文件；当前 24 篇/59 事实规模无感，十万级需要加 mtime/行版本缓存。
