# 订阅自动制作：可靠性体系

> 状态：已实现并通过本地单测 + 真机迁移/健康面板验收（2026-11-14）。
> 目标不是“再修一次 ffmpeg 路径”，而是让**已经推送的视频不再因为一次失败、一次重启、一个缺失依赖而永久消失**。

## 1. 结论先行

1. **投递语义选 at-least-once，不追求 exactly-once。** 外部模型/ASR/B 站调用无法做分布式事务；正确做法是让发布幂等（稳定 `noteId`，重跑更新同一篇笔记），再用去重账本把 at-least-once 收敛成“业务上只完成一次”。
2. **发现与制作分离。** 每次检查先把所有未见视频写进 `production_seen_videos`（`pending`），再按额度/依赖/重试去制作。额度用完、Key 缺失、应用退出都只会**延迟**，不会丢掉视频。
3. **失败自动重试，不靠用户点。** `production_jobs` 持久化 `retry_count / next_retry_at / last_error_kind`；应用重启后继续按退避时间重试。transient 1 分钟起、24 小时封顶；依赖/登录类 10–15 分钟复检。
4. **依赖一次预检，不再每个视频各失败一次。** ffmpeg、AI Key、ASR Key、B 站登录、预算统一检查；缺依赖时暂停制作并在订阅页显示一个可操作的原因。ffmpeg 会自动从 PATH / WinGet / 常见安装目录发现，并修正在本机失效的旧路径。
5. **设备本地设置不再跨机同步。** `production.ffmpeg_path`、`production.auto.autostart` 是本机事实；之前把另一台机器的 ffmpeg 路径同步过来，正是 2026-11-14 11 个任务同时失败的直接原因。
6. **每日上限从 10 提到 50（一次性迁移）。** 实测 9 个订阅一天就有 13 条新视频，10 条/天会留下永久积压；50 条/天大约 ¥5/天（历史 15 个成功任务均值 ¥0.100/篇），真正的成本闸门仍是月度预算。
7. **健康面板给出“今天到底完成了没有”。** 今日完成/失败、待处理/重试中/积压、最近成功、下次重试、本机阻断原因；不再需要从 44 条任务里逐条找错误。

## 2. 事故复盘（实测）

时间：2026-11-14 01:01:28（本地）。

| 指标 | 数值 | 来源 |
| --- | --- | --- |
| 同一分钟失败的自动任务 | 11 | `production_jobs` |
| 失败耗时 | 约 11 秒内全部结束 | `created_at/finished_at` |
| 应用退出留下的 interrupted | 5 | `production_jobs` |
| 修复前 seen 状态 | `queued` 30 / `success` 0 | `production_seen_videos` |
| 修复前每日上限 | 10 | `production.auto.max_per_day` |
| 最后一次成功 | 2026-11-11 11:39:52 | `production_jobs` |
| 错误 | `找不到 ffmpeg（C:/Users/<另一台机器的用户>/.../ffmpeg.exe）` | 任务错误 |

根因不是模型、不是 B 站风控，而是四个结构问题叠加：

1. `production.ffmpeg_path` 被云同步从另一台机器覆盖，路径在本机不存在；
2. `seen` 只记录“已排任务”，不反写任务成败，失败后同一视频永远不会再被选中；
3. 失败/中断没有自动重试入口；
4. 没有依赖预检，11 个视频各自走到 ffmpeg 才失败，制造了 11 条噪声。

## 3. 调研：可靠队列的公共模式

### 3.1 at-least-once + 幂等

Temporal 的 Activity 默认是 at-least-once：Worker 成功执行但在上报前崩溃，Activity 会重试；因此官方要求 Activity 幂等，并把幂等键绑定到业务意图，而不是单次请求。[Temporal Job queues](https://docs.temporal.io/evaluate/features/job-queue)、[Activity definition](https://docs.temporal.io/encyclopedia/activities/activity-definition)、[Error handling](https://docs.temporal.io/develop/python/best-practices/error-handling)

本项目的对应做法：

- 幂等键 = `vault + mid + bvid + template`；`noteId` 由它稳定生成；
- 发布使用 Artifact v2 outbox，重复发布更新同一篇笔记，保留历史版本；
- “已发布但没来得及标成功”的崩溃窗口，由重试前的 `registry_for_note` 检查兜底，直接标成功，不重新调用模型。

### 3.2 持久队列 + 可见性超时

SQS 的 visibility timeout 和常见 SQLite 队列实现都把任务放在表里，用 `status + lease/next_attempt_at` 表示“谁在跑、什么时候可以再跑”，进程崩溃后由 reclaim 扫描恢复。[SQS visibility timeout](https://docs.aws.amazon.com/AWSSimpleQueueService/latest/SQSDeveloperGuide/sqs-visibility-timeout.html)

本项目的对应做法：

- 不把重试计时器放在内存里；`retry_count / next_retry_at` 写在 SQLite；
- 单 worker 信号量（`Semaphore(1)`）+ 串行检查锁，避免同一视频并发建任务；
- 启动时 `recover_interrupted()` 把 `running/queued` 标成 `interrupted`，维护循环按账本重试；
- SQLite WAL + busy timeout，读写不互相阻塞。[SQLite WAL](https://sqlite.org/wal.html)

### 3.3 指数退避 + jitter

AWS Builders' Library 的建议：只重试幂等/可重试错误；用 capped exponential backoff；加 jitter 避免重试风暴；把 timeout、retry、circuit breaker 分层。[Timeouts, retries and backoff with jitter](https://aws.amazon.com/builders-library/timeouts-retries-and-backoff-with-jitter/)、[Exponential Backoff and Jitter](https://aws.amazon.com/blogs/architecture/exponential-backoff-and-jitter/)

本项目的对应做法：

- transient：1m → 5m → 15m → 1h → 3h → 6h → 12h → 24h，±20% jitter；
- dependency/auth：10–15 分钟复检（修 Key/路径通常只需要下一次 tick）；
- risk（412/-352）：2 小时；订阅检查层仍是 412 单独 6 小时；
- permanent（无分P、文件不存在、模板不支持）：6 小时后再试，避免热循环，但不永久放弃。

### 3.4 依赖预检 / 熔断

Circuit breaker 的核心是“依赖坏了就不要每个请求都撞一次”；half-open 只放一个探针，成功再恢复。[CircuitBreaker](https://martinfowler.com/bliki/CircuitBreaker.html)

本项目的对应做法：

- 每次 drain/retry 前做一次 `processing_blockers()`：ffmpeg、AI Key、ASR Key、B 站登录、预算；
- 有阻断就不建任务、不重试，只在健康面板显示一条原因；
- 依赖恢复后下一个 maintenance tick 自动继续，不需要逐个任务点重试；
- B 站 412 仍是订阅层的熔断，避免把账号打进风控。

### 3.5 Outbox / 幂等消费者

发布不是一个纯数据库事务：要写 outbox、写文件、更新索引。采用 outbox + 幂等消费者，把“生成”和“入库”解耦，崩溃后可以安全重放。[Transactional outbox](https://microservices.io/patterns/data/transactional-outbox.html)、[Idempotent consumer](https://microservices.io/patterns/communication-style/idempotent-consumer.html)

本项目已有的 Artifact v2 outbox 正好承担这一层；本次只补“谁在什么条件下重放”。

## 4. 方案

### 4.1 两层账本

```text
发现层 production_seen_videos
  baseline / skipped_filter / skipped_existing   -> 终态，不再处理
  pending                                        -> 已发现，等额度/依赖
  queued                                         -> 有 job 在排队/运行
  success                                        -> 笔记已入库
  failed                                         -> 可重试失败，等 next_attempt_at
  blocked                                        -> 依赖/登录/风控，等依赖恢复
  cancelled                                      -> 用户取消/订阅删除

执行层 production_jobs
  auto / retry_count / next_retry_at / last_error_kind
```

关键点：**发现只写账本，制作才建 job**。这样即使 AI Key 缺失、今日额度用完、应用正在退出，视频仍然在 `pending`，不会因为第一页翻过去而消失。

### 4.2 一次 maintenance tick 做什么

应用启动 8 秒后、以及之后每 60 秒：

1. `reconcile_seen(vault)`：把 job 的最终状态反写到 seen；处理“queued 但 job 已失败/已删除/已完成”的崩溃残留；
2. `processing_blockers()`：依赖不就绪 → 只记录，不建任务、不重试；
3. `retry_due_locked()`：取到期失败任务，最多 2 个/轮；能续跑就续跑，否则全量重跑；
4. `drain_locked()`：把 `pending/failed/blocked` 中到期的视频按每日额度转成 job；
5. `run_due()`：串行检查到期订阅（每 UP 至少 2 秒间隔），发现新视频写入账本，再 drain 一次。

### 4.3 失败分类与动作

| 分类 | 典型错误 | seen 状态 | 下次尝试 | 是否自动 |
| --- | --- | --- | --- | --- |
| transient | 超时、连接失败、429/5xx、ASR 截断、空回复 | failed | 1m → 24h | 是 |
| dependency | 找不到 ffmpeg、缺 AI/ASR Key、预算用完 | blocked | 10m | 是 |
| auth | B 站登录过期、401/403、Key 无效 | blocked | 15m | 是 |
| risk | 412、-352、风控 | blocked | 2h | 是 |
| permanent | 无分P、文件不存在、模板不支持 | failed | 6h | 是（慢速） |
| cancelled | 用户取消 | cancelled | — | 否 |

### 4.4 每日额度与积压

- `production.auto.max_per_day` 统计**当天创建的 auto job 数**，不是 seen 行数；失败重试不重复占额度。
- 额度用完只影响“今天还能建几个新 job”；`pending/failed/blocked` 保留，下一天/下一轮继续。
- 2026-11-14 一次性把旧默认 10 迁移到 50；迁移标记 `production.auto.daily_cap_v2` 是本机设置，用户之后手动改回 10 不会被再次覆盖。
- 月度预算仍是硬闸门：`pause_on_budget=1` 且花费 ≥ 预算时，不建任务、不重试，积压保留。

### 4.5 ffmpeg 自愈

解析顺序：

1. 设置里的 `production.ffmpeg_path`；
2. `FFMPEG_PATH`；
3. `PATH` 里的 `ffmpeg`；
4. Windows 常见位置：WinGet `Gyan.FFmpeg*`、`C:\ffmpeg`、Chocolatey、Scoop、`Program Files`。

每个候选都执行 `ffmpeg -version` 校验；配置路径失效但找到可用路径时，**自动把设置改成本机可用值**。媒体函数每次执行前也会重新解析，防止任务快照里的旧路径继续生效。

### 4.6 幂等与崩溃窗口

- 任务 `noteId` 稳定；重复发布更新同一篇笔记；
- 重试前先查 registry：笔记已存在 → 直接把 job/seen 标成功，不重新调用模型；
- `seen` 先写 `queued` 再启动 job；启动失败写 `blocked/failed` 并保留 `job_id=''` 的退避，`reconcile_seen` 不会把它重置成“立即重试”；
- `pending_videos` 排除 `job_id` 仍指向 `queued/running/success/error/interrupted` 的行，避免 drain 与 retry worker 对同一视频各建一个 job。

### 4.7 可观测性

`production_auto_health` 返回：

```text
enabled / ready / discoveryReady
blockers[]: { kind, message, action }
ffmpegPath
pending / queued / retrying / blocked
todayDone / todayFailed / lastSuccessAt / nextRetryAt
```

订阅页顶部显示“自动制作健康（本机）”：今日完成/失败、待处理/积压、重试中、依赖阻断、本机依赖与 ffmpeg 路径。任务详情里显示“已自动安排重试：第 N 次失败，预计 …”。

## 5. 实现位置

| 文件 | 改动 |
| --- | --- |
| `src-tauri/src/production/retry.rs` | 新增错误分类、退避策略、seen 状态映射；单测 |
| `src-tauri/src/production/media.rs` | ffmpeg 解析/自愈；`prepare_audio`、`prepare_asr_parts` 用解析结果 |
| `src-tauri/src/production/pipeline.rs` | 每次运行前解析 ffmpeg，覆盖旧任务快照；步骤为空时从模板补建 |
| `src-tauri/src/production/store.rs` | job/seen 新列、迁移、reconcile、pending、retry schedule、stats、`ensure_steps` |
| `src-tauri/src/production/subscriptions.rs` | 发现/制作分离、drain、maintenance、自动重试 |
| `src-tauri/src/production/mod.rs` | `AutoHealth`、`processing_blockers`、`auto_retry`、完成回调 |
| `src-tauri/src/lib.rs` | `production_auto_health` 命令；maintenance 调度 |
| `src-tauri/src/sync/registry.rs` | `ffmpeg_path` / `autostart` / 迁移标记不跨机同步 |
| `src-tauri/src/sync/engine.rs` | 设备本地设置不再从旧 shadow 生成删除 op；启动时清理 stale shadow |
| `src/components/production/ProductionSubscriptions.vue` | 健康面板、状态文案 |
| `src/components/production/ProductionTaskDialog.vue` | 自动重试提示 |

## 6. 验证

### 6.1 单测 / 构建（实测）

- `cargo test --lib`：**197 passed / 7 ignored**（新增：错误分类与退避、每日上限一次性迁移、重试计划跨重启、seen 失败/成功 reconcile、pending 退避、ffmpeg 回退、设备本地设置）。
- `npm run build`：通过（1687 modules；构建时间见日志）。
- `npm run tauri build -- --no-bundle`：release `yebian.exe` 构建通过（首次 5m08s，修黑窗 3m13s，补步骤 3m14s）。完整安装包 bundling 在下载 `nsis-3.13.zip` 时 GitHub 超时，未生成新安装包；这是网络问题，不是编译失败。
- `cargo check --lib`：通过。

### 6.2 真机迁移（实测）

用现有用户库启动新代码后：

| 项 | 结果 |
| --- | --- |
| `production.auto.max_per_day` | 10 → 50 |
| `production.ffmpeg_path` | 另一台机器的路径 → `ffmpeg`（PATH） |
| `production_jobs.auto` | 30 个 auto / 14 个 manual |
| seen 状态 | failed 16 / success 14 / pending 3 / baseline 447 |
| 健康面板 | `ready=false`，blockers=`ai, asr`，retrying=16，pending=3，backlog=19 |
| 健康面板 UI | 订阅页显示“自动制作健康（本机）· 依赖未就绪，已暂停自动制作”，列出两个 Key 缺失及操作入口 |

这说明：

- 迁移/回填/账本对齐在真实库上生效；
- 依赖缺失时不再创建新任务，也不会继续制造 11 条失败；
- 旧失败任务进入 `retrying`，依赖恢复后由 retry worker 接手；
- **原先“还有 3 条留到明天”的王山水视频在后续检查中变成 3 条 `pending`，积压从 16 变 19**：发现不再依赖当日额度，额度只延迟制作，不丢记录；同时 error 仍为 11、success 仍为 28，没有新增失败。

### 6.3 真实 Key 自动重试与控制台窗口（实测）

本机随后 `hasAiKey=true`、`hasAsrKey=true`（Key 按设计不参与云同步）：

| 时间 | 结果 |
| --- | --- |
| 08:05 | health `ready=true`，16 error + 3 interrupted 进入 retry |
| 08:08 | success 28→29 |
| 08:09 | success 30 |
| 08:13 | success 31，error 16→1，queued 14 + running 1 |
| 08:16 | success 32，error 16→0，todayDone 4，todayFailed 0；剩余 15 条 queued/running 继续串行处理；全程没有点任何手动重试 |

原先 3 条 `pending` 已经转成任务，至少 1 条完成。剩余队列继续由 maintenance 串行处理。

**控制台黑窗**：维护循环每 60 秒调用 `ffmpeg -version` 做依赖校验，Windows GUI 子进程默认会分配可见控制台，表现为“隔一会弹一个命令行”。修复：

- `StdCommand` 的 `-version` 校验和两处 `tokio::process::Command` ffmpeg 子进程都加 `CREATE_NO_WINDOW`；
- ffmpeg 解析结果加 **5 分钟缓存**，同一组配置不再每个 tick 都执行 `-version`；
- 运行中 30 秒采样：可见 `ffmpeg / cmd / conhost` 窗口数为 **0**。

### 6.4 缺步骤的同步/旧任务（实测）

截图里的 6 条 `模型没有返回笔记正文` 并不是模型不写正文，而是这些任务来自 11-10 的旧记录/跨设备同步：`production_jobs` 同步，但 `production_steps` 按设计不同步，所以本机任务没有步骤。pipeline 用 `find_step` 找不到任何步骤，整段生成被跳过，约 100ms 就报“模型没有返回笔记正文”，然后按 transient 反复重试。

修复：`pipeline::run` 启动时如果 `job.steps` 为空，调用 `store.ensure_steps(template_id, source)` 从模板补建步骤，再执行。实测：

- 6 条任务原先 `steps=0`，重试 `retry_count=2`；
- 修复后「在汉武帝和朱元璋手下做官」补建 5 个步骤，日志出现“补建缺失的任务步骤（任务来自同步/旧记录）”，ASR 3 段 702 字、校对、生成完成并 success；
- 同一时刻 `success 45 / error 0 / queued 1 + running 1`，`todayDone 17 / todayFailed 0`；6 条中 4 条已 success，剩余 2 条继续串行。

### 6.5 联合投稿作者（实测）

《世界新声场》是联合投稿：订阅的是「肝帝董佳宁」，但 B 站视频 `owner/author` 返回「江西卫视」。旧流程直接用 `preview.author`，所以笔记 front-matter 的 `作者` 和 UP 标签都写成了江西卫视。

修复：`pipeline::resolve_source` 对 `options.subscription_id` 非空的自动任务，先用订阅记录里的 `name` 覆盖视频 owner；手动 B 站任务保持真实 owner。已修正这一篇的 front-matter 和 `production_jobs.source_json`，索引实测：

- `notes.up = 肝帝董佳宁`
- `tags = ["UP主/肝帝董佳宁"]`

新增 `subscription_author_overrides_joint_video_owner` 单测；`cargo test --lib` 197 passed / 7 ignored。

### 6.6 没有验证的

- **没有等剩余队列全部跑完。** 已实测队列从 19 条失败/中断收敛到 success 45 / error 0 / queued 1 + running 1；剩余由队列继续，不再需要人工介入。
- **缺步骤只在整个步骤表为空时补建。** 如果只是部分步骤行丢失，目前不会补；模板不存在的旧任务会报错。
- **没有双设备同时运行的压力测试。** seen/job 会同步，两台同时跑可能重复处理同一视频；目前默认一次只让一台机器执行制作。
- **没有验证应用完全退出时仍能检查。** 进程内调度器不注册 Windows 服务；需要应用运行，建议开启“关闭到托盘”或开机启动。
- **只读 UP 空间第一页 50 条。** 如果离线期间新增超过 50 条，超出部分仍可能漏；当前没有分页追赶。
- **预算暂停时当日不会完成。** 积压保留，但不是“无限制花钱也要完成”。

## 7. 使用与运维

1. 保持应用运行；如果要关窗口后继续，开启「设置 → 自动检查 → 关闭到托盘」，需要开机就跑再开「开机启动」。
2. 每台设备各填一次 AI/ASR Key；这是密钥不跨设备同步的刻意选择。填好后无需逐条重试，下一分钟自动继续。
3. 订阅页顶部健康面板是唯一需要看的地方：
   - 绿色“依赖就绪” = 正常；
   - 黄色“依赖未就绪” = 按 blocker 的 action 处理，积压不会丢；
   - “重试中 N” = 已有失败任务在退避队列里，不需要手动点。
4. 如果某天视频特别多：每日上限 50；超过部分会留到第二天，健康面板显示积压。预算优先于“当天全部完成”。
5. 需要看单个视频：订阅 → 最近视频，状态列会显示 `待处理 / 等待重试 / 等待依赖 / 已完成`。

## 8. 诚实清单

- **本机没有 Key**：这是本次交付最大的未验证项。代码路径、账本、退避、健康面板都实测了；真实 Key 下的“失败→自动重试→成功入库”没有跑。
- **自动重试成功路径**由 store 单测（`retry_schedule_survives_reopen`、`seen_ledger_reconciles_failed_job_and_marks_success`）和 retry 单测覆盖，不是端到端。
- **每日上限 50 是一次性产品决策**：旧默认 10 被迁移为 50；如果用户原本刻意设 10，也会被改一次（之后改回不会覆盖）。依据是 2026-11-14 的 9 订阅/13 条新视频/3 条积压实测。
- **ffmpeg 解析依赖本机文件系统**；WinGet 目录扫描深度 5，极端自定义安装位置可能找不到，会显示 blocker，不会静默成功。
- **多设备**：`production_jobs` / `production_seen_videos` 同步，两个设备同时运行有重复处理风险；本次没有加设备租约。
- **同步不包含密钥**：第二台设备健康面板会显示 AI/ASR blocker；如果另一台设备在跑，任务会由那台完成并通过同步回传笔记。
- **没有系统服务**：应用不运行就不检查；不伪装成“永远在线”。
- **B 站接口非官方**：412/-352 只能退避；本次没有做验证码/风控绕过。
- **UI 只在当前 880×590 的桌面窗口做了目视检查**，没有做 1440/1024 的完整几何审计；健康面板在 ≤760px 变为两列。

## 9. 参考

- AWS Builders' Library, *Timeouts, retries and backoff with jitter*: https://aws.amazon.com/builders-library/timeouts-retries-and-backoff-with-jitter/
- AWS Architecture Blog, *Exponential Backoff and Jitter*: https://aws.amazon.com/blogs/architecture/exponential-backoff-and-jitter/
- Temporal, *Job queues*: https://docs.temporal.io/evaluate/features/job-queue
- Temporal, *Activity definition*: https://docs.temporal.io/encyclopedia/activities/activity-definition
- Temporal, *Error handling*: https://docs.temporal.io/develop/python/best-practices/error-handling
- AWS SQS, *Visibility timeout*: https://docs.aws.amazon.com/AWSSimpleQueueService/latest/SQSDeveloperGuide/sqs-visibility-timeout.html
- SQLite, *Write-Ahead Logging*: https://sqlite.org/wal.html
- Martin Fowler, *CircuitBreaker*: https://martinfowler.com/bliki/CircuitBreaker.html
- microservices.io, *Transactional outbox*: https://microservices.io/patterns/data/transactional-outbox.html
- microservices.io, *Idempotent consumer*: https://microservices.io/patterns/communication-style/idempotent-consumer.html
