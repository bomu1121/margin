# UP 主更新自动制作：调研与实现方案

> 状态：**已实现并通过本地 mock 端到端验收**（2026-11-07）。
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
- 已有：101/101 Rust 测试（订阅实现后）、7.56 MiB 包；已有 B 站登录 Cookie、公开视频预览、音频链路。

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

网上常见一组“官方测试向量”：

```text
img_key=7cd084941338484aae1ad9425b84077c
sub_key=4932caff0ff746eab6f01bf08b70ac45
params={foo:114, bar:514, baz:1919810}
wts=1684805578
expected=bar=514&baz=1919810&foo=114&wts=1684805578
         &w_rid=bb97e15f28edf445a0e4420d36f0157e
```

**实测这组 `w_rid` 与它自己给出的参数、重排表不自洽**：按公开算法算出的 mixin key 是
`ea1db124af3c7062474693fa704f4ff8`，MD5 是 `c9864ec9f22928db4a94a49ae7986c59`。
因此实现没有把这个 `bb97...` 当作断言；改用两层验证：

1. 单测锁定公开的 mixin key 推导结果，以及独立 Python/Node 实现算出的 query/MD5；
2. 用同一份 Rust 代码对真实 `/x/space/wbi/arc/search?mid=2` 发起一次请求，返回 `code=0` 且拿到视频列表。
   这是算法可用的直接证据；没有为了“通过测试”去改算法迁就一条对不上的期望值。

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
  last_message TEXT NOT NULL DEFAULT '',
  consecutive_failures INTEGER NOT NULL DEFAULT 0,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  UNIQUE(vault_path, mid)
);

CREATE TABLE production_seen_videos (
  vault_path TEXT NOT NULL,
  mid INTEGER NOT NULL,
  bvid TEXT NOT NULL,
  title TEXT NOT NULL DEFAULT '',
  pubdate INTEGER NOT NULL DEFAULT 0,
  duration INTEGER NOT NULL DEFAULT 0,
  cover TEXT NOT NULL DEFAULT '',
  status TEXT NOT NULL, -- baseline/queued/failed/skipped_existing/skipped_filter
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

- 输入：UP 空间 URL、m.bilibili.com/space 链接、分享文案或 UID；粘贴到可识别内容后 380ms 自动读取，按钮保留为重试；通过公开 card 接口取 name/avatar/fans。
- 新建订阅先取第一页（50 条）建立 baseline：全部写 `seen(status=baseline)`，不建任务。
- 可选“补最近 N 条”：N=0/3/5/10，默认 0；只对选择 N 的最近视频建任务。
- 每 UP 配置：启用、间隔 15/30/60/120/360/720 分钟、模板、提示词、时长/标题过滤。
- `立即检查` 与自动检查共用一把串行锁；手动检查仍受风控退避和预算闸门约束，不会绕过它们。

### 5.2 每轮检查

1. 只处理 `enabled=1` 且 `next_check_at<=now` 的订阅，按顺序执行；
2. 拉第一页（`ps=50`，`order=pubdate`）；只处理第一页，避免为了找旧视频继续翻页；
3. 过滤 `seen`、已存在 note（稳定 noteId/registry）、过滤规则；
4. 每轮最多 `max_per_check`（默认 3）个新任务，插入 `seen(status=queued)` 后调用内部 `production_start`；
5. 任务由现有 single gate 串行执行，自动任务不并行；
6. 更新 `last_checked_at` / `next_check_at`（加 0–180 秒 jitter）；
7. 每天最多 `max_per_day`（默认 10）个自动任务；超过则跳过并记录在 `last_message`。

### 5.3 失败语义

| 情况 | 行为 |
| --- | --- |
| Cookie 过期/-101 | 错误写入订阅，`next_check_at` 退避 6 小时；重新登录成功后自动清掉登录类错误并提前重试 |
| -352 | 该订阅退避 2 小时；不立即重试 |
| HTTP 412 | 暂停所有 B 站轮询 6 小时，提示 IP 风控 |
| 网络/超时 | 指数退避，最长 6 小时 |
| 视频已有笔记 | 标记 `skipped_existing`，不覆盖 |
| 自动任务启动失败 | 标记 failed，不自动重建；用户在任务页手动重试 |
| 自动任务运行失败 | 任务页保留错误与检查点；不自动重试，避免重复计费 |
| 预算超限 | 暂停自动检查；手动检查也被拦住，直到调整预算或关闭闸门 |

## 6. 暂存区与正式位置

自动任务生成的笔记：

```text
ScribeFlow/待整理/<标题>.md
```

- front matter **不写**标签；自动身份看 `production_jobs.auto` / `subscription_id`，任务列表和详情显示“自动 / UP 自动”；
- 用户通过现有侧栏移动笔记到正式目录；
- 移动前已固化 note id，标注和 `[[链接]]` 跟着走；
- 不自动决定最终目录，也不在用户移动后再次覆盖；
- 已存在同一 noteId 的笔记时不创建自动任务，避免污染用户手写/已整理的笔记。

## 7. 后台运行

自动检查需要应用进程活着。方案：

- Rust `tokio` 后台循环，启动后每 60 秒检查到期订阅；不阻塞 UI。
- 系统托盘：打开、退出；左键托盘图标恢复主窗口。`TrayIcon` 句柄由 AppState 持有，避免 build 后 drop 导致图标消失。
- 关闭窗口到托盘：opt-in；开启后 `CloseRequested` 被拦截并隐藏窗口，进程与调度器继续运行。
- 开机启动：`tauri-plugin-autostart`（HKCU Run），带 `--hidden`；只有同时开启“关闭窗口时留在托盘”时，启动后才隐藏主窗口。
- 默认**不强制**开机启动；订阅页/设置页显示“自动检查：开/关”；关闭窗口默认仍退出。
- 不采用 Windows 服务/Task Scheduler：会多一套生命周期和权限，且仍要启动 Rust 引擎/WebView；先用进程内托盘方案。

## 8. UI

制作台新增「订阅」页：

- 添加默认从「我的关注」选：每页 50 条、本地按名字/UID 过滤、可加载更多；不会自动关注或改关注关系。
- 第二个入口「视频链接」：粘贴 BV/av/b23，先显示视频标题/封面/作者，再订阅该作者；`view.owner.mid` 由后端返回。
- 第三个入口「UID / 空间链接」保留为精确/兜底：粘贴后自动读取，成功后把长链接收成 UID；资料卡显示头像/粉丝数；识别失败就地提示。
- 选中任一入口后隐藏入口标签，显示资料卡 + 间隔/模板/提示词/首次检查；「换一个」回到入口选择。
- 列表：头像、名字、UID、启用、间隔、最近视频、下次检查、最近检查、错误/摘要；
- 单项：开关、编辑模板/提示词/过滤、立即检查、最近视频、取消订阅（内联确认）；
- 新建时选择“仅订阅之后 / 补最近 0/3/5/10 条”；补跑会明确提示成本与每日上限；
- 顶部：自动检查总开关、订阅数/自动任务/下次检查、B 站登录状态、预算暂停提示；
- 设置页「自动检查」：总开关、默认间隔、每次/每天上限、预算闸门、关闭到托盘、开机启动；
- 任务页：自动筛选与自动标签；任务详情显示“UP 自动”；
- 侧栏：`ScribeFlow/待整理` 直接可见；不新增独立收件箱页面，先复用侧栏。

## 9. 验收

### 单元/集成

- WBI 重排表、签名 query、参数清洗、UID 解析、风控退避时长：Rust 单测；
- 订阅唯一键/基线/seen 去重/统计/删除：Rust store 单测；
- 自动任务写入 `ScribeFlow/待整理`、手动任务仍写 `ScribeFlow`：publish 单测；
- `cargo test`：102 passed / 3 ignored（2026-11-07）。

### 真机

- 真实空间接口 `/x/space/wbi/arc/search?mid=2` 一次受控请求：`code=0`，返回 50 条、页面总数 43（当前实际可返回）；
- 关注入口：真实账号关注总数 284，首屏返回 50 条；选择「肝帝董佳宁」后补公开资料并显示 172 万粉丝；
- 视频入口：`BV1ujNV6qEXg` 解析出「帮不帮？/碧诗」，点「订阅这个 UP」后显示 UID 2 / 144 万粉丝；没有提交订阅；
- 真实订阅添加：首次基线 43 条 `seen=baseline`，没有创建任务；
- 显式补跑 1 条：创建 1 个 auto 任务（`options.auto=true`），mock AI/ASR 完成，笔记写入
  `ScribeFlow/待整理/验证笔记.md`，front matter 含来源链接，**不含** `自动制作` 标签（2026-11-09 移除）；
- 再次立即检查：`newTasks=0`，消息“没有新视频”，不会重复建任务；
- 关闭窗口到托盘：`WM_CLOSE` 后进程仍存活、主窗口 `visible=False`；再次启动单实例后窗口恢复；
- 开机启动插件：`is_enabled` false → enable true → disable false；
- 1440 与 1024×643 内容区：订阅页、设置页明/暗 audit 均 0 <12px、0 低对比；
- 自动化环境：临时 vault + 本地 mock AI/ASR；测试结束后恢复原 SQLite 备份与 sample-vault。
- 没有用真实 UP 高频轮询做测试；没有用真实模型 key 跑自动任务。

## 10. 前置依赖与顺序

1. **WBI 客户端**（先做，带官方向量测试；可被后续 B 站功能复用）。
2. **订阅表 + CRUD + 手动检查**（应用打开时可用）。
3. **自动调度 + 暂存区 + 去重/预算**。
4. **托盘/开机启动/通知**。
5. **UI 与端到端验收**。

## 11. 诚实边界

- B 站接口非官方，WBI/风控可能变化；412 无法在应用内解决，只能暂停/等。真实接口只做了一次受控拉取，长期稳定性未验证。
- 应用没运行就不会检查；开机启动/Tray 需要用户开启，且 Windows autostart 有系统层差异。托盘菜单只有“打开/退出”，没有“立即检查”。
- 自动检查总开关关闭后，已创建的任务仍会继续；暂停订阅也不取消任务。
- 自动任务会真实消耗模型 token；预算检查是按已有 usage 估算，不是预扣费，任务仍可能超预算。
- 只处理“投稿视频”；动态、直播、充电专属/付费视频不在首版。只读第一页 50 条，不翻页补旧视频。
- 首次订阅默认不补历史；补最近 N 条受每日上限约束，没轮到的会记为 baseline 并明确提示，不会以后偷偷补跑。
- 自动任务启动失败/运行失败都不自动重试；用户手动重试。应用在任务中途退出会留下 interrupted 任务，需要手动继续。
- 暂存区固定为 `ScribeFlow/待整理`；用户移动后任务卡片里显示的历史路径可能不是最终路径，但 noteId/标注保持。
- 去重按稳定 noteId + registry；如果用户手写过一篇同 URL 但不同 noteId 的笔记，自动任务仍可能生成第二篇（首版未按“原始链接”反查全库）。
- `seen` 状态只到 `queued/failed/skipped_*`；任务实际成败以 `production_jobs` 为准，不反写 seen。
- WBI 的公开 `bb97...` 向量与参数不自洽，因此没有把它当断言；改为 mixin key + 独立实现 + 真实请求三层验证。
- 手动“立即检查”与自动检查共用串行锁，但没有单独的 60 秒/UP 冷却；连续点击会被 busy 锁挡住，间隔过短仍可能增加风控概率。
- 关注入口只读已加载页面并在本地过滤，没有全量名字搜索；关注列表接口仍需登录 Cookie，并可能受风控影响，失败时会提示改用视频链接/UID。
- 视频反查只支持公开视频；b23 短链沿用现有预览的跳转解析；私有/付费视频可能拿不到作者 mid。
- 选择关注/视频作者后会补一次公开资料请求以拿粉丝数；失败时保留列表里的名字/头像，不阻塞添加。
- 没有跨设备/云端调度；所有 Cookie、订阅、seen 都在本机。
- 截图：`production-24-subscriptions-empty.png`、`production-25-subscriptions-add.png`、`production-26-subscriptions-card.png`、`production-27-subscriptions-settings.png`、`production-28-subscriptions-1040.png`、`production-29-task-auto.png`、`production-30-subscriptions-link.png`、`production-31-subscriptions-video.png`。

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
