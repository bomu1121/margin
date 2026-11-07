# 页边笔记制作台（原生实现）

> 状态：**2026-11-06 已落地**。制作台不再跳转 ScribeFlow Web 平台，执行、任务、模板、登录、入库都在页边笔记进程内完成。
> 旧的 ScribeFlow Artifact v2 / outbox 协议继续作为入库通道；`docs/SCRIBEFLOW.md` 里关于 outbox 的部分仍然有效，平台入口不再使用。
> 后续琐碎需求与优先级见 [`PRODUCTION_BACKLOG.md`](PRODUCTION_BACKLOG.md)；模型接入/计价改造见 [`MODEL_PROVIDER_DESIGN.md`](MODEL_PROVIDER_DESIGN.md)。

## 1. 一句话架构

```text
Vue 制作台（页边笔记主窗口）
  └─ Tauri command / event
       └─ Rust ProductionService（进程内）
            ├─ SQLite：任务、步骤、日志、自定义提示词
            ├─ 来源：B站链接 / B站收藏 / 本地音视频 / 粘贴文稿
            ├─ 处理：转写 / 校对 / 加工 / 分章 / 合并 / 导图 / 出题 / 攻略 / 溯源
            ├─ AI/ASR/Search：reqwest 直连 OpenAI 兼容接口
            ├─ B站：公开链接、扫码登录、收藏夹/稍后再看/历史
            ├─ 媒体：ffmpeg 在本机提取音频
            └─ 入库：写 .yebian/outbox → outbox::drain → 笔记 + 索引 + 注册表
```

**没有 Node sidecar，没有常驻 HTTP 服务，不占端口。** 任务在 Tauri 的 async runtime 里执行，进度通过 `production-update` 事件推给界面。

## 2. 模块

| 文件 | 职责 |
| --- | --- |
| `src-tauri/src/production/mod.rs` | 服务、任务模型、命令入口、B站/设置转发 |
| `production/store.rs` | SQLite 表、任务 CRUD、设置、提示词、恢复 |
| `production/templates.rs` | 10 个内置模板与步骤 |
| `production/prompts.rs` | 11 个内置提示词、校对/合并/分章/导图/出题/溯源 system prompt |
| `production/providers.rs` | 13 个 provider 预设、模型发现、models.dev 价目、余额、CNY 换算 |
| `production/clients.rs` | OpenAI / Anthropic / Gemini chat、ASR multipart、B站 API/QR、联网检索 |
| `production/media.rs` | ffmpeg 音频提取、临时目录 |
| `production/pipeline.rs` | 一次任务的完整执行、进度、取消、重试 |
| `production/publish.rs` | Artifact v2 包、稳定 noteId/revision、相关笔记、outbox 入库 |

## 3. 功能

### 3.1 任务与模板

- 10 个模板：视频笔记、文稿笔记、长文分章、思维导图、练一练、阴阳师攻略、多风格对照、系列综述、速览与溯源、多路对照。
- 新建向导四步：来源 → 模板 → 选项 → 确认。
- 模板是固定链路，没有画布、没有工程文件夹、没有节点编辑器。
- 任务状态：排队中 / 进行中 / 已完成 / 失败 / 已取消 / 已中断。
- 取消会中止在途 HTTP、ffmpeg 和下载；重试会**重新执行整条模板**，保留步骤日志。
- 应用重启会把遗留的 running/queued 标记为 interrupted。

### 3.2 来源

- **B站链接**：BV / av / b23（b23 跟随短链跳转）；粘贴后 520ms 防抖自动解析，修改链接立即清空旧预览/自动标题；非法链接明确提示，解析失败可重试，15s 超时；支持分P/合集多选，全选/反选/清空，未选分P 时不能下一步。
- **B站收藏**：扫码登录后读取收藏夹、稍后再看、观看历史；收藏夹首屏约 100 条（5 页 × 20），可「加载更多」；支持全选已加载/清空，一次最多选 10 个视频；观看历史按 cursor 翻页，最多 100 条。
- **多视频拆分**：视频笔记/长文/导图/练一练/对照/溯源等模板选择多个视频时，每个视频创建一个独立任务、独立笔记；「系列综述」保留合并成一篇的语义。
- **本地音视频**：通过系统文件选择器选择；音频/视频由本机 ffmpeg 转 16k 单声道 wav。
- **粘贴文稿**：直接进入校对和加工。
- B站公开视频不需要登录；收藏夹/稍后再看/历史需要登录。收藏夹实测首屏 98 条、点「加载更多」到 117 条；观看历史实测 100 条，带封面和 BV 号。

### 3.3 处理

- ASR：MiMo 走 `/chat/completions`，音频转 base64 data URL，请求头 `api-key`；OpenAI 兼容仍走 `/audio/transcriptions` multipart。
- MiMo 单次上限 10MB（base64）；长音频在 `media::prepare_audio` 自动转 48kbps 单声道 MP3，原始音频超过约 7MB 或格式不支持时也会先转码。
- AI：三种协议——OpenAI 兼容 `/chat/completions`、Anthropic `/messages`、Gemini `generateContent`；失败按设置重试。
- 断点续跑：失败/中断/取消后按单元检查点继续；支持 ASR 分段、校对分段、加工分段、B站/收藏夹 下载和发布幂等；任务详情显示可复用段数，提供「继续制作 / 重跑全部」。
- 模型接入：选 provider → 填 Key → `/models` 拉账号可用模型；失败回退 models.dev 目录。价格按目录美元价 × 设置汇率折算 CNY；目录没收录时可选填手动 CNY 价目（手动 > 目录 > 不估算）。模型列表支持推荐/上下文/输入价/输出价排序、模型 ID 复制。
- usage 归一化：未缓存输入 / 缓存读 / 缓存写 / 输出 / 推理 / 音频；有目录价才算金额，没有就只记 token。
- 校对：按 6000 字切块，逐块校对后拼接。
- 加工：按 12000 字切块；长文稿先分块生成再合并。
- 分章：按粒度/最大章节数生成 `##` 章节。
- 合并：多份产物合成一篇。
- **速览版**：SF 的两步配方已恢复——第一步用完整 `PROMPT_SULAN` 生成「速览 / 5-9 个判断句小节 / 速查」；第二步单独调用补「弦外之音」，机械插到 `## 速查` 前。原生阅读器把 SF 的 Obsidian callout 适配成 `##` 小节和 `**条目**（类别）：说明`，不再输出 `[!summary]` 字面量。
  > **速览版是产品样板，不能简化成一条风格句。** 修改前先对照 `packages/shared/src/prompt.ts` 的 `PROMPT_SULAN` / `SLOT_XIANYIN`；至少保留四件事：速览 4-6 条、正文 5-9 个判断句小节、速查按类别分组、单独的弦外之音。
- 导图：Markdown 缩进大纲，结果页「思维导图」。
- 练一练：JSON 题目，结果页本地判分。
- 攻略：阴阳师专用 prompt。
- 溯源：调用联网检索（智谱 / Tavily），再让模型逐条核查；结果页「溯源」。
- 多风格/多路：同一素材生成两版，结果页「对照」。
- 属性抽取：人物/事件/时期/公司/概念等写入 front matter；相关笔记按当前配置的全部字段匹配。

### 3.4 入库

- 稳定 `noteId`：`yn-<sha1(template + source_key)>`；同来源同模板重跑更新同一篇，revision 递增。
- 包目录：`.yebian/outbox/<noteId>/<revision>/`（`note.md`、`artifact.json`、`READY`）。
- 页边笔记 `outbox::drain` 负责最终目录、文件名、front matter、索引和标注；制作引擎不直接写用户笔记目录。
- 入参：标题、类型、来源、作者、原始链接、发布日期、创建日期、tags、属性。
- **手改冲突（首版）**：重跑前把当前笔记正文与最新 outbox 生成稿比较；不一致时弹「保留两份 / 覆盖并备份 / 取消」。
- **历史版本**：任务详情「版本」列出 outbox 生成稿和 `.yebian/versions` 手改备份；可预览、恢复。恢复前会自动把当前稿再存一份手改备份，恢复后更新索引。
  - 保留两份：原笔记和标注不动，新生成稿用新 noteId 另存；
  - 覆盖并备份：先把当前笔记复制到 `.yebian/versions/<noteId>/manual-<时间>.md`，再覆盖；标注可能失效；
  - 取消：不创建任务。
- 入库后调用 index `delta_wait`，笔记页立即可读。
- 相关笔记：按当前配置的全部字段匹配库内旧笔记，追加 `## 相关笔记` 的 `[[链接]]`；跳过当前笔记和 `ScribeFlow/` 自身产物。

### 3.5 设置

- AI：13 个 provider 预设；「获取模型」只拉列表，「测试生成」会发一条最小 chat 验证 Key/模型/网络；模型搜索；参考价（CNY 主、USD 副）自动带出；余额（DeepSeek/Moonshot/OpenRouter）；本月用量与预算提醒；高级里可改接口地址、协议（自定义）和汇率。
- AI Key 按 provider 分开存；切换 provider 不会把上一家的 key 发给下一家。
- ASR：engine/baseUrl/model/key、每分钟单价。
- 检索：渠道/key/每次结果数。
- 制作：默认模板、默认提示词、重试次数、ffmpeg 路径、通知/提示音。
- B站：扫码登录/退出、收藏夹同步。
- 提示词：内置 11 个 + 自定义（新建/编辑/删除/设为默认）；每次保存生成版本，版本页可看当前草稿 vs 任意版本的逐行 diff、恢复旧版（恢复会生成新版本）；同名自定义会提示覆盖内置。
- 标签与属性：默认抽取字段。
- 数据：任务数、临时音频、outbox 体积；清理临时音频/已完成任务。

### 3.6 可读性

- 制作台所有信息文字统一到 **12px 下限**，表单控件 13px；共替换 123 处 8.5–11.5px 的声明。
- 同步放大按钮最小高度、向导步骤圆点、步骤状态圆点，避免“字变大但点击区/图标没变”。
- 任务表在 1180px 以下把「来源」列从 100px 调到 118px，`B站 · BV...` 不再被截断；1040px 实测无横向溢出。
- 金色徽标改用深金色 `#7a5008`，暗色导航计数改用 `--ink-2`，消除对比度不足。
- `tools/audit-text.mjs`（12px / 4.5:1）实测：home 171 节点、settings 76–130、tasks 161、task detail 235、wizard 180（历史列表 536）、Bili 登录 89；明/暗均为 0 个 <12px、0 个低对比。改动前 home 是 144/178 个 <12px。

## 4. 本地数据

用户数据库 `%APPDATA%/com.yebian.notes/yebian.sqlite` 新增：

| 表 | 用途 |
| --- | --- |
| `production_jobs` | 任务、来源/选项 JSON、状态、进度、结果、usage |
| `production_steps` | 每一步的状态与详情 |
| `production_logs` | 运行日志（每条截断 8000 字符） |
| `production_prompts` | 自定义提示词 |
| `production_prompt_versions` | 提示词版本快照 |
| `production_model_prices` | 手动价目（provider + model） |
| `production_checkpoints` | 单元级检查点（payload + usage） | | 单元级检查点（payload + usage） |

`production_jobs` 还保存 `vault_path`、`config_json`（非密钥执行配置快照）和 `checkpoint_at`。
设置仍存 `settings` 表，键前缀 `production.`。AI usage 聚合在 `production_jobs.usage_json`，
没有单独的 `production_usage` 明细表；月度汇总按 `created_at` 过滤本月任务。

AI key 按 provider 分开存：`production.ai.api_key.<provider>`。旧键 `production.ai.api_key`
在打开数据库时迁移到当前 provider 后删除。AI/ASR/检索 key 目前**明文存本机 SQLite**，
与旧 ScribeFlow 一致；商业化前需要迁移到系统凭据存储/KMS。

## 5. 验收（已跑）

```bash
cd src-tauri && cargo test --lib
# 80 passed, 0 failed, 2 ignored

# 需要网络的 models.dev 实时目录测试（手动跑）
cd src-tauri && cargo test --lib live_catalog -- --ignored --nocapture
# catalog presets=11 models=636 deepseek input=¥4.752/M output=¥14.256/M

npm run build
# vue-tsc + vite build 通过
```

手工端到端（本机，真实文件系统；视频链路最近一次用真实 MiMo ASR + DeepSeek）：

- 文本：`text → 校对 → 加工 → 入库`；写 `ScribeFlow/联调笔记.md`，front matter 含 `id/标题/类型/来源/tags/人物/事件`；同 source 重跑 revision 1 → 2，仍是一篇。
- B站：真实公开视频 `BV1DTWw6UE1y`（18:42 / 971s）→ 下载音轨 → ffmpeg → mock ASR → mock 模型 → `ScribeFlow/B站联调笔记.md`；front matter 的 `作者/原始链接/发布日期` 来自 B站；`source.videos` 持久化。
- **B站真实链路（2026-11-06）**：`BV1J4486nEEe`（18:42 / 971s）→ 下载音轨 → 48kbps MP3 → 按 10 分钟切成 2 段 → MiMo 转写 4,438 字 → DeepSeek v4 flash 加工 → `ScribeFlow/美国最贵选举结束 特朗普输麻了吗？.md`（5,167 B）。第二次运行在修复成本写入后完成：audioSeconds 987；prompt 1,385（含缓存 128）/ completion 2,912 / reasoning 2,143 tokens；estimatedCost **¥0.01394**；约 2 分钟。第一次运行（12:04）的 estimatedCost 因写入遗漏记成 0，已修，旧任务不回填。
- 思维导图：生成 `result.mindmap` 并入库。
- UI：从「新建制作 → 粘贴文稿 → 文稿笔记 → 开始制作」走通，任务详情显示结果、步骤、用量、打开笔记；B站链接解析后显示封面，便于确认选对视频。
- 提示词版本实测：自定义「版本测试」v1 13 字 → v2 24 字 → 恢复 v1 生成 v3；diff 标出 1 行新增/1 行删除；删除提示词会连同 3 个版本一起删除。
- 同名覆盖提示实测：临时建一个同名「速览版」自定义，编辑器出现覆盖警告，随后删除；截图 `production-14-prompt-shadow.png`。
- 恢复提示实测：恢复 v1 后自动回到编辑页，表单上方直接显示「已恢复为 v1，并保存为一个新版本。」；版本数变 3。该状态明/暗 audit 132 个文本节点，0 个 <12px / 0 个低对比；截图 `production-15-prompt-restore-notice.png`。
- 模型连接测试：DeepSeek key 实测「连接成功 · 967 ms · deepseek-v4-flash · 回复 OK · 48 tokens」；换错误 Key 返回「Key 无效，或这个 Key 没有该模型权限」。
- 手改冲突真机走查：给生成笔记追加一段手写内容后重跑，检测到「当前 1,889 字 / 上次生成 1,864 字 / revision 1」；选保留两份后原笔记和手写段仍在，新生成稿使用新 noteId；选覆盖并备份后 `.yebian/versions/<noteId>/manual-*.md` 保留 2,170 字手改稿，原笔记被新稿覆盖。
- E-01 可重复验收（2026-11-07）：`npm run verify:resume` 18/18、`npm run verify:cleanup` 6/6；临时 vault/mock，测试后任务列表恢复；脚本先检查切换库后任务列表为空，避免旧构建误删真实任务。
- 清理与续跑真机（2026-11-07）：数据页把「清理已完成任务」和「清理失败/中断任务」分开；后者需确认，提示会删除续跑检查点。`clear_temp` 返回清除/保留字节，运行中和有检查点的任务不会被删；数据页明/暗 93 节点 0 个 <12px / 0 低对比。
- 相关笔记字段真机（2026-11-07）：临时库写入 `公司: 字节跳动` 的旧笔记；新任务字段配置「公司/概念」，mock 模型返回 `{"公司":["字节跳动"],"概念":["Agent"]}`；生成笔记 front matter 带公司/概念，并追加 `## 相关笔记` / `[[旧公司]]`。
- 断点续跑真机（2026-11-07）：文本任务第 2 次调用注入 500，失败时 usage 1M/1M、¥40；继续后 mock 只多 1 次成功调用，最终 2M/2M、¥80（旧行为是 4 次请求/3 次成功/¥120）。「重跑全部」清空检查点，同场景共 4 次请求。ASR 两段任务第 2 段失败，继续只重发第 2 段（mock ASR 3 次、chat 2 次）。UI 明/暗 222 节点 0 个 <12px / 0 低对比。
- P1-04/P1-07 真机走查：任务详情 1.5s 轮询，长文本任务从 25% 自动变 success；重命名只改任务标题，删除任务后已入库笔记仍在；筛选显示「全部 8 / 已完成 6 / 失败 2」。
- P1-01/02/03 真机走查：假 BV 提示「视频不存在或链接无效」并可重试；收藏夹全选 98 → 清空 0；31 个分P 的课程视频全选/反选/清空正常，0 个时下一步禁用。
- 速览版真实视频（2026-11-06，`run_be4f51230e0d4b3785133fdcc2f2414a`）：转写 4,438 字 → 校对 4,433 字 → 完整速览版 3,051 字，正文 3,133 字；包含 `## 速览`、7 个判断句小节、`## 速查`（多组条目）和 `## 弦外之音`。note 4,024 字符（10,670 B）；usage 7,708 / 5,789 / 3,904 tokens，estimatedCost ¥0.0333（当时的校对/抽取 usage 漏计，见 §6）。日志可在「任务 → 步骤 → 日志」查看。
- P0-01/P0-02 真机走查（2026-11-06）：收藏夹 49 个、稍后 20 条、历史 100 条，封面/UP 主齐全；
  历史多选 2 个后建任务，`source.videos=2`，取消后 note 为空。失效 Cookie 的 UI 提示为“B站登录可能已过期，请重新扫码”。
  运行中取消 → cancelled、无 note；杀进程重启 → interrupted、无 note；ffmpeg 失败重试 → success，仍是同一 noteId。
  空收藏夹「1111」实测 0 行、显示「这个收藏夹没有视频」；「系列综述」多视频仍按原语义合并一篇。
- ASR Key 轮换：用 0.5s 静音直连 MiMo `/chat/completions`，HTTP 200，返回「嗯。」；旧 Key 已不再使用。
- 模型设置：真机打开「设置 → AI 模型」，无 Key 时显示 models.dev 目录里的 4 个 DeepSeek 模型；
  `deepseek-v4-pro` 自动显示 ¥4.75 / ¥14.26；切换 OpenAI 显示 51 个模型；切换 Ollama 明确显示本地未启动；
  无 Key 点「查询余额」在按钮下方就地报「还没有配置 API Key」，不是顶部不可见的全局错误。
- 视觉：设置页在 1440×900 与 1040×760 下无横向溢出；1040 宽时右侧「制作引擎」栏自动隐藏。
- 截图：`shots/production-01-home.png`、`production-02-wizard.png`、`production-03-account.png`、
  `production-04-cover.png`、`production-05-settings.png`、`production-06-settings-usage.png`、
  `production-13-prompt-versions.png`、`production-14-prompt-shadow.png`、`production-15-prompt-restore-notice.png`、
  `production-16-model-sort-copy.png`、`production-17-manual-price.png`、`production-18-model-pinned.png`、`production-19-resume-failed.png`、`production-20-resume-success.png`、`production-21-data-cleanup.png`。

实测数字：

- 安装包 **7,070,617 B（6.74 MiB）**；`yebian.exe` **15,913,984 B（15.15 MiB）**。
- models.dev 目录：11 个有目录的 provider、636 个模型；DeepSeek V4 Pro 输入 ¥4.752 / 输出 ¥14.256
  每百万 token（USD 0.66 / 1.98 × 7.2）。
- 没有新增常驻进程/端口；价格目录只在 `%APPDATA%/com.yebian.notes/production/models-dev-catalog.json`
  缓存 24h。

## 6. 诚实边界

- **B站列表没有逐条打开验收**：真实账号已验收藏夹 49 条、稍后再看 20 条、历史 100 条，多选建任务和失效 Cookie 路径也走过；没有逐条播放/转写列表里的每个视频。
- **联网溯源没有真实检索验收**：智谱/Tavily 请求已实现，但没有 key；这条模板在未配置检索时会明确失败。
- **模型接入只实测了 DeepSeek 的真实对话**：models.dev 目录、provider 切换、价格显示、无 Key 报错、DeepSeek 连接测试都真机走过；Anthropic/Gemini/OpenRouter 的 `/models`、chat、余额，以及 OpenRouter 的 `usage.cost` 没有有效 key 验收。
- **断网缓存回退没有实测**：24h 缓存 + stale 回退代码存在，但没有拔网线/改 hosts 验证。
- **任务详情 usage 的真实数据仍有限**：`run_be4f…` 的 7,708 / 5,789 / 3,904 和 ¥0.0333 是漏计校对/抽取前的下限；2026-11-07 后 mock 任务 2M/2M ¥80、3M/3M ¥120 验证了修正后的口径，其他 provider 没有真实账单核对。
- **ASR 仍是手填每分钟单价**：P0-06 只移除 AI 模型单价；ASR 自动计价在 P2-09。
- **MiMo 已用真实 18 分钟视频跑通**：先按 10 分钟分段再转写；超长音频仍受单段 10MB 限制，分段后每段 48kbps MP3 约 3–4MB。
- **速览版已恢复 SF 两步配方**，但原文排版从 Obsidian callout 改成了原生 `##` 小节；旧 SF 笔记里的 `> [!summary]` 目前仍不会自动渲染成盒子。
- **ASR 单价仍是 0**：任务里的 audioSeconds 987 只记录时长，没有计入金额；估算金额目前只含 AI token。
- **ffmpeg 解析**：这台机器曾出现一次 ffmpeg 退出但 stderr 为空；已改为按字节读取并 lossy 解码 stderr、错误里带 exit code。最终验收把「设置 → 制作 → ffmpeg 路径」显式设为 WinGet 的绝对路径。
- **手动价目是用户填的估算值**：不是服务商账单，不随模型目录更新；只按 provider + model 精确匹配，不做日期后缀归一。
- **手动价目没有真实服务商账单核验**：用 DeepSeek provider + 未收录模型 + 本地 mock 验证了 `手动 > 目录` 和 ¥80/¥120 的精确成本；自定义 provider 的 UI 保存/清除已走，但没有用它的真实 key 跑任务。
- **模型排序是前端排序**：只作用于已拉取的模型列表，先过滤再排序再截前 80；不是服务商推荐排序。
- **复制模型 ID 依赖 Tauri clipboard 插件**：不再触发 WebView2 的剪贴板权限框；安装包因此增加约 0.12 MiB。
- **token 统计已修正但旧任务不回填**：校对/合并/属性抽取的调用之前漏计；2026-11-07 后新任务计入，旧任务（包括 `run_be4f…`）的金额是下限。
- **汇率是用户设置值**：默认 7.2，不抓实时汇率；界面标明汇率和「估算」。
- **阶梯价/峰谷价未实现**：按目录基础档估算；DeepSeek 峰谷价在 provider 说明里标注。
- **练一练**依赖模型返回 JSON；解析器兼容数组和 `{items:[]}`，但只用 mock 文本验证过流程，没有用真实模型验证题目质量。
- **「系列综述」仍合并**：其他视频模板多输入会拆成每个视频一个任务、一篇笔记；系列综述保留合并语义，分篇与综述的边界还需要产品确认。
- **提示词版本只覆盖自定义提示词**：内置提示词在代码里，不能在界面改也不能建版本；每次保存自定义提示词才生成版本，内容相同不重复生成。
- **提示词 diff 是行级 LCS**：只能看当前草稿和某一个历史版本的差异，不能选两个历史版本互比，也不能按 hunk 合并；恢复是整段替换。
- **同名自定义只做了提示**：自定义与内置同名时警告会覆盖内置；两个自定义同名时按数据库顺序取第一个，没有唯一性约束。
- **恢复提示词没有跑真实生成**：版本恢复、diff、级联删除都实测过，但没有用恢复后的提示词跑一次真实模型任务来验收输出。
- **相关笔记是精确字符串匹配**：不做别名/模糊/语义相似度；字段名仍来自设置里的五项，不支持任意自定义键；不处理手动移动/删除后的注册表修正（SF-005）。
- **通知**使用 Web Notification + WebAudio；没有在 Windows 通知中心做系统级验收。
- **任务重命名只改任务标题**；不改已入库笔记标题，也不改文件名。
- **断点续跑已实现首版**：单元级检查点、继续/重跑、发布认领；实测见 [`RESUME_DESIGN.md`](RESUME_DESIGN.md)。
- **断点续跑不做跨设备/跨库**：任务绑定创建时 vault；旧任务启动时绑定当前 vault，之后不能跨库继续/清理。
- **E-01 首轮误删任务记录**：首版验证脚本在 vault 过滤修复前清空了 sample-vault 的 10 条任务记录；笔记文件未动，但任务历史无法恢复。修复后脚本有“切换临时库后任务列表必须为空”的前置断言。
- **检查点不是服务商账单**：请求已发出但响应丢失仍可能重复计费；旧任务不补账。
- **应用重启后的 resume 只做了状态/存储单测**：现有 interrupted 机制复用，未单独跑“杀进程→重启→继续”的端到端。
- **没有云端/多租户**；所有密钥、任务、媒体都在本机。
- **安装/卸载验收没有在 6.74 MiB 构建上重跑**；上一轮验收是 11-04 的 5.29 MiB 构建。
- **B站公开接口可能变更**；失败会显示接口错误，不会静默产出空笔记。
- **观看历史固定取最近 100 条**（5 页 × 20），没有「加载更多」；更早记录只能在B站客户端查看。
- **收藏夹首屏只取约 100 条**，之后每次「加载更多」20 条；1051 条不会一次拉完。
- **冲突备份**已可在任务详情「版本」浏览和恢复；旧笔记如果没有 outbox 生成稿，无法判断是否手改，会按无冲突处理。
- **模型连接测试只实测了 DeepSeek**：Anthropic、Gemini、OpenRouter 的同一入口代码已接，但没有对应 key；20s 超时路径未故意制造挂起请求。
- **笔记版本恢复不是三方合并**：恢复是整篇替换，不会逐段合并；恢复旧版后标注可能失效。
- **ffmpeg 是外部依赖**：B站音轨和本地视频需要；未随安装包内置（用户机器已安装，测试通过）。

## 7. 旧 ScribeFlow 的位置

- ScribeFlow Web 平台不再作为产品入口；`/studio` 现在渲染 `ProductionView.vue`。
- 旧 `open_scribeflow` 命令、`StudioSidebar.vue`、`StudioView.vue` 仍留在仓库作为兼容/迁移参考，但主界面不再引用。
- 旧 outbox 包仍会被 `outbox::drain` 处理；Artifact v2 注册表继续供笔记页「来源与制作」使用。
- 旧的 `docs/SCRIBEFLOW.md` 保留为历史与协议文档；新的实现以本文为准。
