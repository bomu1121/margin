# UP 主更新自动制作：调研与实现方案

> 状态：**调研完成，尚未实现**（2026-11-07）。
> 需求：选定 UP 主；UP 发新视频时自动创建制作任务；笔记进入笔记库；最终目录仍由用户手动决定。

## 1. 结论先行

1. **不用 RSSHub/外部服务器**。桌面端直接调 B 站空间投稿接口，走现有本地 Rust 引擎；离线本地数据不离开机器。
2. **必须前置 WBI 签名客户端**。旧的 `x/space/arc/search` 已不可用；当前代码只有公开视频详情/收藏夹接口，没有 UP 投稿列表能力。
3. **自动检查放在应用进程内**，用 Rust Tokio 定时器；不引入 Windows 服务。应用必须保持运行；要真正“开机后自动”，增加**托盘 + 开机启动（opt-in）**，而不是假装后台常驻。
4. **自动任务只进「待整理」暂存目录**，不替用户决定最终文件夹。默认落在 `ScribeFlow/待整理/`，用户通过现有侧栏移动；移动后笔记 id/标注保持。
5. **先建立基线，只处理订阅之后的新视频**。首次订阅不自动补历史；可选“补最近 0/3/5/10 条”，默认 0。
6. **B 站请求必须克制**：默认 30 分钟/UP，串行、每次间隔 2 秒、每轮最多 3 个任务；-101 暂停订阅，-352/412 进入长退避，不自动重试风暴。

## 2. 当前实现审计

现有能力：

- `production/clients.rs` 已有公开视频详情、音频地址/下载、二维码登录、收藏夹、稍后再看、历史；**没有 WBI 签名，也没有 UP 投稿列表**。
- `production_start` 已能把一个 B 视频变成任务；`publish` 固定写 `ScribeFlow/`，没有暂存目录概念。
- `production_jobs` 只有手动任务；没有订阅/已见视频表。
- 桌面端没有托盘、关闭到托盘、开机启动；当前关闭窗口即退出。
- 已验证：81/81 Rust 测试、6.75 MiB 包；已有 B 站登录 Cookie、公开视频预览、音频链路。

因此这个需求不是单一 UI 功能，前置依赖是：

1. B 站 WBI 签名 + 投稿列表/用户信息；
2. 订阅与已见视频的持久化、基线、去重；
3. 自动任务与暂存区发布；
4. 进程内调度；
5. 托盘/开机启动/通知（要真正自动才需要）。

## 3. B 站接口调研

### 3.1 投稿列表

当前可用接口：

```text
GET https://api.bilibili.com/x/space/wbi/arc/search
  ?mid=<UID>&pn=1&ps=50&order=pubdate
```

旧 `/x/space/arc/search` 已废弃（返回 `-400`）。该 WBI 接口必须带 `w_rid` + `wts`，否则 412 或 `-352`。响应关键字段：

```text
data.list.vlist[]: bvid, title, author, mid, created, length, pic, aid
data.page: pn, ps, count
data.is_risk
```

`created` 是发布时间戳；用 `bvid` 去重，不靠标题或发布时间字符串。

### 3.2 WBI 签名

流程：

1. `GET /x/web-interface/nav` 取 `data.wbi_img.img_url` / `sub_url` 的文件名作为 `img_key` / `sub_key`；
2. 拼成 64 字符，按固定 64 项重排表取前 32 位 `mixin_key`；
3. 参数加 `wts`，按 key 排序，URL encode；
4. `w_rid = md5(query + mixin_key)`。

官方文档给出可验证的测试向量：

```text
img_key=7cd084941338484aae1ad9425b84077c
sub_key=4932caff0ff746eab6f01bf08b70ac45
params={foo:114, bar:514, baz:1919810}
wts=1684805578
expected=bar=514&baz=1919810&foo=114&wts=1684805578
         &w_rid=bb97e15f28edf445a0e4420d36f0157e
```

实现必须先用这个向量做单测；不能只靠“请求碰巧成功”。

### 3.3 Cookie、buvid 与风控

- 需要登录 Cookie（`SESSDATA` / `bili_jct` / `DedeUserID`）；未登录或过期返回 `-101`。
- 空间接口通常还要求浏览器指纹类 Cookie（`buvid3`）；登录后从 `/x/frontend/finger/spi` 获取 `buvid3`/`buvid4` 并加入请求 Cookie。
- 真实测试记录显示：1.2–2.5 秒间隔连续约 8 次请求就可能触发 412；`-352` 是单次风控，412 是 IP 级拦截。
- 因此：默认 30 分钟轮询，串行，UP 间 2 秒，失败不连续重试；412 暂停整个 B 站轮询 6 小时，提示用户；不伪装/绕过验证码。

## 4. 数据模型

新增到 `yebian.sqlite`：

```sql
CREATE TABLE production_subscriptions (
  id TEXT PRIMARY KEY,
  vault_path TEXT NOT NULL,
  mid INTEGER NOT NULL,
  name TEXT NOT NULL,
  avatar TEXT NOT NULL DEFAULT '',
  enabled INTEGER NOT NULL DEFAULT 1,
  interval_minutes INTEGER NOT NULL DEFAULT 30,
  template_id TEXT NOT NULL DEFAULT 'video-note',
  prompt TEXT NOT NULL DEFAULT '速览版',
  filters_json TEXT NOT NULL DEFAULT '{}',
  baseline_at INTEGER NOT NULL DEFAULT 0,
  last_checked_at INTEGER NOT NULL DEFAULT 0,
  next_check_at INTEGER NOT NULL DEFAULT 0,
  last_error TEXT NOT NULL DEFAULT '',
  consecutive_failures INTEGER NOT NULL DEFAULT 0,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  UNIQUE(vault_path, mid)
);

CREATE TABLE production_seen_videos (
  vault_path TEXT NOT NULL,
  mid INTEGER NOT NULL,
  bvid TEXT NOT NULL,
  pubdate INTEGER NOT NULL DEFAULT 0,
  status TEXT NOT NULL, -- baseline/queued/running/success/failed/skipped_existing/skipped_filter
  job_id TEXT NOT NULL DEFAULT '',
  note_id TEXT NOT NULL DEFAULT '',
  first_seen_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  PRIMARY KEY(vault_path, mid, bvid)
);
```

- 订阅按 vault 隔离；切库只显示/调度当前库的订阅。
- `seen` 先插入再启动任务，唯一键防重复；失败不会每秒重新建任务。
- 自动任务复用 `production_jobs`；`JobOptions` 增加 `auto` / `subscriptionId` 标记，不新增任务表。
- `publish` 增加暂存目录：自动任务写 `ScribeFlow/待整理/`，手动任务仍写 `ScribeFlow/`。

## 5. 调度与任务流

### 5.1 订阅

- 输入：UP 空间 URL 或 UID；通过 WBI 用户信息接口取 name/avatar。
- 新建订阅先取第一页（50 条）建立 baseline：全部写 `seen(status=baseline)`，不建任务。
- 可选“补最近 N 条”：N=0/3/5/10，默认 0；只对选择 N 的最近视频建任务。
- 每 UP 配置：启用、间隔 30/60/120 分钟、模板、提示词、时长/标题过滤。
- `立即检查` 有 60 秒冷却；返回本次新增/跳过/错误。

### 5.2 每轮检查

1. 只处理 `enabled=1` 且 `next_check_at<=now` 的订阅，按顺序执行；
2. 拉第一页（`ps=50`，`order=pubdate`）；如果全是未见且数量接近 50，最多再拉第二页；
3. 过滤 `seen`、已存在 note（稳定 noteId/registry）、过滤规则；
4. 每轮最多 3 个新任务，插入 `seen(status=queued)` 后调用内部 `production_start`；
5. 任务由现有 single gate 串行执行，自动任务不并行；
6. 更新 `last_checked_at` / `next_check_at`（加 0–180 秒 jitter）；
7. 每天每订阅最多 10 个自动任务；超过则跳过并记录。

### 5.3 失败语义

| 情况 | 行为 |
| --- | --- |
| Cookie 过期/-101 | 暂停该订阅，状态“需要重新登录”，通知一次 |
| -352 | 该订阅退避 2 小时；不立即重试 |
| HTTP 412 | 暂停所有 B 站轮询 6 小时，提示 IP 风控 |
| 网络/超时 | 下个周期再试 |
| 视频已有笔记 | 标记 `skipped_existing`，不覆盖 |
| 自动任务失败 | 标记 failed，不自动重建；用户在任务页手动重试 |
| 预算超限 | 暂停自动任务并通知；不影响手动任务 |

## 6. 暂存区与正式位置

自动任务生成的笔记：

```text
ScribeFlow/待整理/<标题>.md
```

- front matter 增加标签 `制作/待整理`，任务卡片显示“自动”；
- 用户通过现有侧栏拖拽/移动到正式目录；
- 移动前已固化 note id，标注和 `[[链接]]` 跟着走；
- 不自动决定最终目录，也不在用户移动后再次覆盖；
- 已存在同一 noteId 的笔记时不创建自动任务，避免污染用户手写/已整理的笔记。

## 7. 后台运行

自动检查需要应用进程活着。方案：

- Rust `tokio` 后台循环，启动后每 60 秒检查到期订阅；不阻塞 UI。
- 系统托盘：打开、立即检查、退出；可选“关闭窗口时最小化到托盘”。
- 开机启动：`tauri-plugin-autostart`（HKCU Run），带 `--minimized`，启动后隐藏到托盘。
- 默认**不强制**开机启动；订阅页明确显示“后台检查：开/关”，关闭窗口即退出。
- 不采用 Windows 服务/Task Scheduler：会多一套生命周期和权限，且仍要启动 Rust 引擎/WebView；先用进程内托盘方案。

## 8. UI

制作台新增「订阅」页：

- 添加：UP 空间链接/UID；
- 列表：头像、名字、UID、启用、间隔、下次检查、最近检查、错误；
- 单项：编辑模板/提示词、过滤、立即检查、删除；
- 新建时选择“仅订阅之后 / 补最近 0/3/5/10 条”；
- 顶部：后台检查状态、开机启动/关闭到托盘开关、B 站登录状态；
- 任务页：自动任务标签；任务详情仍可打开笔记；
- 侧栏：`ScribeFlow/待整理` 直接可见；不新增独立收件箱页面，先复用侧栏。

## 9. 验收

### 单元/集成

- WBI 官方测试向量精确通过；
- WBI 参数排序、URL encode、`wts` 过期处理；
- 订阅唯一键/基线/seen 去重；
- 已有 noteId 不建任务；
- 每轮上限/每日上限/预算暂停；
- -101/-352/412/网络错误的状态流转；
- 自动任务写入 `ScribeFlow/待整理`，手动任务仍写 `ScribeFlow`。

### 真机

- 用临时 vault + mock B 站服务（或受控测试 UID）验证：baseline 不建任务、下一条新视频建 1 个任务、重复检查不重复建；
- 无登录时显示“需要登录”，不空跑；
- 关闭窗口到托盘后定时器仍运行；退出托盘后停止；
- 1440/1040、明暗两组文字审计 0 违规；
- 不以真实 UP 高频轮询做测试，避免触发 412；真实接口只做一次受控拉取。

## 10. 前置依赖与顺序

1. **WBI 客户端**（先做，带官方向量测试；可被后续 B 站功能复用）。
2. **订阅表 + CRUD + 手动检查**（应用打开时可用）。
3. **自动调度 + 暂存区 + 去重/预算**。
4. **托盘/开机启动/通知**。
5. **UI 与端到端验收**。

## 11. 诚实边界

- B 站接口非官方，WBI/风控可能变化；412 无法在应用内解决，只能暂停/等。
- 应用没运行就不会检查；开机启动/Tray 需要用户开启，且 Windows autostart 有系统层差异。
- 自动任务会真实消耗模型 token；预算检查是按已有 usage 估算，不是预扣费，任务仍可能超预算。
- 只处理“投稿视频”；动态、直播、充电专属/付费视频不在首版。
- 首次订阅默认不补历史，避免一次生成大量旧视频；补最近 N 条需要用户显式选择。
- 自动任务失败不自动重试，避免重复计费和刷任务；用户手动重试。
- 暂存区固定为 `ScribeFlow/待整理`；用户移动后任务卡片里的旧路径可能不是最终路径，但 noteId/标注保持。
- 没有跨设备/云端调度；所有 Cookie、订阅、seen 都在本机。

## 12. 参考

- B 站 WBI 签名与官方测试向量：
  <https://github.com/SocialSisterYi/bilibili-API-collect/blob/master/docs/misc/sign/wbi.md>
- B 站空间投稿接口/风控行为调研（DeepWiki 摘要）：
  <https://deepwiki.com/SocialSisterYi/bilibili-API-collect/2.1-api-signatures>
- B 站 412/-352 风控记录：
  <https://errors.standardbeagle.com/DIYgod/RSSHub/got-error-code-data-code-while-fetching-data-893b67/>
- Tauri 2 系统托盘：
  <https://v2.tauri.app/learn/system-tray/>
- Tauri 2 开机启动插件：
  <https://v2.tauri.app/plugin/autostart/>
