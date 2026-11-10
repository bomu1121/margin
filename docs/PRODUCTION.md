# 页边笔记制作台（原生实现）

> 状态：**2026-11-07 已落地**（含 UP 订阅自动制作）。制作台不再跳转 ScribeFlow Web 平台，执行、任务、模板、登录、入库都在页边笔记进程内完成。
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
            ├─ B站：公开链接、扫码登录、收藏夹/稍后再看/历史、WBI 空间投稿
            ├─ 订阅：基线/seen 去重、进程内调度、预算/风控退避
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
| `production/subscriptions.rs` | WBI 空间投稿、订阅基线/去重、自动调度、预算/风控退避 |

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
- 溯源：调用联网检索（DeepSeek Search / 智谱 / Tavily），先抽可核查主张，再按主张多查询、排序、分批核查；结果页「溯源」可点来源。DeepSeek Search 与 DeepSeek 模型共用一个平台 Key，检索 Key 可留空复用模型 Key。设计与实测见 [`TRACE_SEARCH_DESIGN.md`](TRACE_SEARCH_DESIGN.md)。
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
- 相关笔记：按当前配置的全部字段匹配库内旧笔记，追加 `## 相关笔记` 的 `[[链接]]`；跳过当前笔记、旧 `ScribeFlow/` 产物和新的 `待整理/` 暂存产物。

### 3.5 设置

- AI：13 个 provider 预设；「获取模型」只拉列表，「测试生成」会发一条最小 chat 验证 Key/模型/网络；模型搜索；参考价（CNY 主、USD 副）自动带出；余额（DeepSeek/Moonshot/OpenRouter）；本月用量与预算提醒；高级里可改接口地址、协议（自定义）和汇率。
- AI Key 按 provider 分开存；切换 provider 不会把上一家的 key 发给下一家。
- ASR：engine/baseUrl/model/key、每分钟单价。
- 检索：渠道（DeepSeek Search / 智谱 / Tavily）/key/每次结果数；DeepSeek Search 的 Key 留空时复用 DeepSeek 模型 Key。
- 制作：默认模板、默认提示词、重试次数、ffmpeg 路径、通知/提示音。
- B站：扫码登录/退出、收藏夹同步。
- 提示词：内置 11 个 + 自定义（新建/编辑/删除/设为默认）；每次保存生成版本，版本页可看当前草稿 vs 任意版本的逐行 diff、恢复旧版（恢复会生成新版本）；同名自定义会提示覆盖内置。
- 标签与属性：默认抽取字段。
- 数据：任务数、临时音频、outbox 体积；清理临时音频/已完成任务。
- 自动检查：总开关、默认检查间隔、每次/每天上限、预算闸门、关闭到托盘、开机自启；跳转管理 UP 订阅。

### 3.6 可读性

- 制作台所有信息文字统一到 **12px 下限**，表单控件 13px；共替换 123 处 8.5–11.5px 的声明。
- 同步放大按钮最小高度、向导步骤圆点、步骤状态圆点，避免“字变大但点击区/图标没变”。
- 任务表在 1180px 以下把「来源」列从 100px 调到 118px，`B站 · BV...` 不再被截断；1040px 实测无横向溢出。
- 金色徽标改用深金色 `#7a5008`，暗色导航计数改用 `--ink-2`，消除对比度不足。
- `tools/audit-text.mjs`（12px / 4.5:1）实测：home 171 节点、settings 76–130、tasks 161、task detail 235、wizard 180（历史列表 536）、Bili 登录 89；明/暗均为 0 个 <12px、0 个低对比。改动前 home 是 144/178 个 <12px。

### 3.7 UP 主更新自动制作

> 设计、接口调研和诚实边界见 [`SUBSCRIPTION_DESIGN.md`](SUBSCRIPTION_DESIGN.md)。

- 入口：制作台侧栏「订阅」或首页「UP 订阅」。默认从「我的关注」选（50 条/页、本地过滤、加载更多）；也可以粘贴视频链接反查作者，或用 UID / space / m.bilibili 空间链接兜底。选中后显示资料卡和制作选项；「换一个」重新选。
- 首次检查建立基线：当前第一页（最多 50 条）全部记为 `baseline`，不会把旧视频一次性跑一遍；可选补最近 3/5/10 条（后端也接受 1–10）。
- 自动任务：使用现有视频笔记链路，`options.auto=true`、`options.subscriptionId`；**不再写 `自动制作` 标签**（2026-11-09 移除，自动身份看任务字段）。结果先写 `待整理/`，用户从侧栏移动到正式目录，noteId/标注保持。
- 去重：`vault + mid + bvid` 唯一；已有 registry 且文件存在时标 `skipped_existing`；被筛选标 `skipped_filter`；同一视频不会重复创建任务。
- 调度：应用内 Tokio 每 60 秒检查到期订阅；串行、UP 间至少 2 秒、每次最多 `maxPerCheck`（默认 3）、每天最多 `maxPerDay`（默认 10）；间隔加 0–180 秒 jitter。
- 风控：WBI 签名 + `buvid3/buvid4`；-101 退避 6 小时，-352 退避 2 小时，HTTP 412 全局暂停 6 小时；登录恢复后清掉登录类错误并提前重试。
- 瞬时网络错误：space/WBI/buvid/followings/login 请求最多重试 3 次（250/500ms），只重试 timeout/connect/request，不重试 412/-352；错误显示为“连接失败（DNS/代理/TLS）/请求超时”，不再把完整 URL 贴在卡片上。首次传输失败退避 2 分钟，之后 4/8/16/32/60 分钟；风控/登录冷却不变。
- 预算：自动任务和手动检查共用预算闸门；本月估算费用 ≥ 预算且开启「预算用完时暂停」时，两处都停止。
- 后台：托盘「打开/退出」；设置页可开启「关闭窗口时留在托盘」和「开机自动启动（静默到托盘）」；默认关闭，应用关闭即退出。
- 任务页：新增「自动」筛选；任务行和详情显示「自动 / UP 自动」，失败/中断仍按现有任务页处理。

### 3.8 事实与笔记关联（Phase 1/2/3/4/5）

> 设计、数据契约和未完成项见 [`FACTS_DESIGN.md`](FACTS_DESIGN.md)；史料/原文检索方案（事件为后端实验）见 [`HISTORY_SOURCES_DESIGN.md`](HISTORY_SOURCES_DESIGN.md)。

- 溯源主张不再只写进笔记末尾文本：每条 claim 落 `facts`，笔记通过 `note_fact_mentions`（带 `run_id`、quote）与事实关联。
- 事实核验后写 `fact_sources` / `fact_evidence` / `fact_checks`；状态为 `candidate / verified / disputed / unsupported`。
- 笔记右栏改为「事实 / 标注 / 关联 / 信息」四个标签页：事实列表只做索引，按需关注排序；标注点正文「+」自动切 tab；来源/文件收进信息 tab。
- 顶栏「事实」库：列表只做索引（状态、全文、笔记/证据/关联计数），点整行进入事实详情；引用笔记、证据、关系、核验历史都在详情页。
- 事实详情页：像笔记一样承载引用它的多篇笔记、每个 run 的 quote、证据、相关/冲突、核验历史、重新核验和合并重复。
- 溯源抽取后先做一次模型归并：新 claim 命中已有事实就只加 mention、不新建事实、不重复检索；模型失败降级为词面 `same_as`，再降级为新事实。抽取 prompt 要求同句/同段子断言合并。
- 旧 `result_json.trace[]` 在启动时一次性回填每篇笔记**最新一次** trace；当前真实笔记回填 10 条事实（4 条有证据、6 条存在争议）。
- 事实关系：`fact_relations` 保存候选（CJK bigram Dice ≥ 0.34，最多 3 条，正向+反向读取）。≥0.62 且数字一致记 `same_as`；≥0.55 且两边数字集合不同记 `contradicts`；其余记 `related`。三者都是候选，不自动改状态。
- 人工合并：事实库「合并重复…」只对 `same_as` 候选开放（阈值 0.62），必须用户确认；来源、证据、笔记提及和核验历史转移到保留事实，被合并事实删除。快照含双方合并前状态、被移动记录 id；「合并记录」可撤销，旧版快照只能查看。
- 冲突：事实库「只看冲突」筛选 `contradicts`；当前 16 条事实没有真实数值冲突，单元测试覆盖 400 万 vs 300 万。
- 图谱：默认显示笔记—事实；来源层默认关，打开后来源是灰点；点事实/来源可看全文、状态、关联笔记、URL 和「在事实库打开」。
- 任务详情「溯源」读事实库当前状态；事实详情页是新的主要阅读入口。
- 任务详情「溯源」不再读旧任务快照，按 `noteId + runId` 读事实库当前状态。证据引文里的 `<sup>` 脚注和 HTML 实体在读取时清理。
- 新生成笔记不再追加 `## 溯源` 长文；旧笔记已有文本保留。自动语义合并（同义改写）仍未做。

### 3.9 独立史料检索系统（领域分区 + 候选池 + 只读索引）

> 完整调研与实现记录见 [`HISTORY_SEARCH_DESIGN.md`](HISTORY_SEARCH_DESIGN.md)；数据/引用模型见 [`HISTORY_SOURCES_DESIGN.md`](HISTORY_SOURCES_DESIGN.md)。

- 史料是单独实体：`sources` / `source_passages` / `citations`；事实只引用 passage，不把书名/卷页塞进事实文本。
- 事实增加 `domain`（`history` / `general`）：主张抽取时由模型标注；已有 citation 的旧事实由 `domain_version=1` 迁移标成 `history`。事实库按「历史事实 / 其他事实」分区，只有历史事实显示「史料」区；事实详情可人工纠正归类。
- **独立史料检索**：查询规划输出 `work/section/queries`；多源发现走 ctext 官方 API（匿名书目，Key 后可全文）、Wikisource、Internet Archive；NLC/一史馆/HathiTrust 只做候选链接。候选按 authority 分层，只有 primary/transcription/ocr 且抓到全文才自动写 citation；link_only/二手/无 Key 只进候选池。
- **ctext 边界**：网页抓取被明确禁止且会返回故意损坏的数据；只走 `https://api.ctext.org`，没有 API Key 时只保留候选。设置页新增 ctext API Key（可选）。
- 抓全文后按句切 passage；确定性最长公共子串 + 模型挑句，quote 必须是 passage 的逐字子串；失败/抓不到全文不写 citation。
- `history_search_candidates` 记录候选 provider/authority/status/reason；`history_page_cache` 缓存抓取文本 7 天；事实详情显示候选，只有「已引用」算史料。
- **候选补录**：候选行「补录原文」→ 打开来源页复制正文 → 粘贴到事实详情 → 程序切 passage、确定性/模型挑句、逐字子串校验 → 写 citation 或拒绝。ctext 无 Key、Wikisource/IA 直连失败时，这是最后一步，不是 14 字段手填。
- 历史事件后端表保留为实验，当前 UI 不暴露事件页/关联；主流程只做事实 → 史料引文。
- 旧 `fact_sources`/`fact_evidence` 是网页事实核查证据，不迁入史料库；`source_version=3` 清理已有 web 副本，旧证据留在事实「网络证据」里。
- 顶栏保留「史料」；史料库和史料详情改为**只读索引**，移除「新增史料 / 找史料 / 加原文段落 / 引用到事实」手工主入口；保留轻量「补录离线史料」。
- **live 自动 citation 未命中**：本机 Wikisource/IA 直连超时，ctext 无 Key；严嵩事实规划到 `明史·卷三百八·奸臣传·严嵩`，拿到 7 条候选，自动引用 0 条，citation 仍只有早前人工《明史》一条。

### 3.10 事实整理工作台（M1 + 拆分 M2）

> 设计与边界见 [`FACT_CURATION_DESIGN.md`](FACT_CURATION_DESIGN.md)；拆分的迁移矩阵与撤销见 [`FACT_SPLIT_DESIGN.md`](FACT_SPLIT_DESIGN.md)。

- 入口：事实库 →「整理事实」→ `/facts/curate`。
- 重复候选：已有 `same_as` 成对展示，选保留项合并；复用 merge/undo。
- 冲突：已有 `contradicts` 成对展示，记录 `needs_research/different_scope/keep_both/one_side_wrong` + 备注；不改事实状态。
- 复合事实：≥2 个逗号/分号 + 长度≥30 + 有 related 事实；显示相关原子事实，可标记 `needs_split`/`reviewed`。
- 无来源：无网页证据且无 citation；可标 `reviewed`。
- **拆分 M2**：事实详情「拆分…」或复合卡「拆分」；2–8 条子事实，normalized 相同的已有事实复用；复制 mention，证据/引用留在父事实；父事实 `status='split'`、review 置 `reviewed`；`fact_splits` 记录快照，子事实无新证据时可撤销。**阅读右栏保留父事实原句，子事实只在事实详情/事实库展开**，避免把阅读拆成碎片。
- 新表：`fact_splits`、`fact_conflict_resolutions`、`fact_reviews`。
- **不做自动拆句/自动证据映射/归档/语义聚类/批量操作**；拆分子事实默认无来源，需要逐条核验。
- 17 条基线时：0 重复 / 0 冲突 / 2 复合 / 1 无来源 / 17 未整理；真机验证待拆分、已看过、临时冲突记录，测试数据已清理。
- 当前 59 条事实（43 历史 + 16 通用）：0 重复 / 0 冲突 / 2 复合 / 4 无来源 / 59 未整理；新增历史事实尚未逐条整理。
- 拆分真机（2026-11-09）：真实「小布什家族」复合事实拆成 4 条（2 条复用 + 2 条新建）；父事实 `split`、网页证据仍在父事实、review 置 `reviewed`；撤销后父事实恢复 `disputed`、新建子事实删除、已有子事实保留、复制 mention 删除，库回到 59 事实 / 63 mentions / 0 reviews / 0 active splits。测试产生的 split 记录已清理。

### 3.11 历史链路质量基线

> 真实语料、指标与失败项见 [`HISTORY_EVAL.md`](HISTORY_EVAL.md)。

- 7 条 UP `王山水读通鉴` 视频 + 已有嘉靖笔记：8 篇历史笔记 / 43 条历史事实；
- 7 个新任务全部 success：1,730s、455,810 tokens、¥1.0705；
- 42 条新事实 42/42 标为 history；前 34 条 work/section 人工核对 34/34 未见明显错书，长笔记 8 条实际检索的 top1 书目与时代相符；
- 库内 43/43 有候选（含 12 条评测补搜 + 1 条长笔记手动补搜）；41/43 top1 是 ctext 书目，2 个缺口（苏轼文集、江表传）只有外链；
- 网页证据 39/43；自动 citation 0，全库仍只有人工《明史》1 条；
- 自动 trace 覆盖 30/43（batch limit 3/6/8）；长笔记自动史料入库 8/9，第 9 条手动补搜 4 条候选；
- 13 分钟 9 主张长笔记：前两次在 66% 停留 >15 分钟取消；加 `verify_claims` 每批 90s 超时 + PDF 4MB/20s 提取上限后，第三次 **383s 成功、100%**，核查批次 60s/37s/9s，未触发超时；
- 结论：规划/候选可用，full text 和自动覆盖（8 条上限）仍是瓶颈；补录成功率只在新样本外验过（嘉靖）。

### 3.12 待补录候选队列

> 设计见 [`SOURCE_QUEUE_DESIGN.md`](SOURCE_QUEUE_DESIGN.md)。

- 入口：史料库 →「待补录」→ `/sources/pending`。
- 按事实聚合 `history_search_candidates` 中 `status='candidate'` 的候选；显示 provider/title/authority/reason；
- 「打开来源」走系统浏览器；「补录原文」与事实详情共用 `CandidatePasteDialog`；
- 成功写 citation、候选标 cited；失败标 rejected、0 citation；
- 当前队列 360 条候选 / 43 条事实 / 0 条已引用；仍需人工打开页面复制原文，不自动抓 ctext 网页/Wikisource/IA；没有候选的事实不会进队列，需要先在事实详情手动「自动找史料」。

### 3.13 新增页面 UI 整改（2026-11-09）

> 走查报告与前后对照见 [`UI_AUDIT.md`](UI_AUDIT.md)；最终截图在 `shots/audit/01..13`，整改前标注图在 `shots/audit/annotated/`。

已改：

- **史料库**：删除「补录离线史料」；tab 加 count；补“史料从哪来”三步和队列 CTA。
- **待补录队列**：事实组默认只展开第一组；组头显示候选数 + provider 分布；候选行压缩（重复 reason 只显示一次）；补录改主按钮；加 provider/authority 筛选和全部折叠；统计改“已补录候选（不含手工史料）”。
- **事实详情**：冲突面板区分“数字不一致”和“文字表述不一致”；拆分只在复合事实出现；空 section 收起；候选升级为独立 section；加 sticky 锚点栏。
- **事实库**：1040 动作不再孤行；范围/状态分组标签；状态“全部”改“不限”；行内只留差异 meta。
- **事实整理**：统计卡 4 个可行动项 + 可选“未标记”；0 重复/冲突/复合/无来源收成一行；复合卡「拆分」主按钮 + 「⋯」菜单。

真机复检：队列折叠 12→0→12；史料库无手工入口、tab 1/1/0/0/0/0；严嵩无拆分、小布什有拆分；定性冲突不再显示空数字行；拆分/撤销回归后库回到 59 事实 / 63 mentions / 0 reviews / 0 active splits。`cargo test --lib` 166 passed / 7 ignored；包 **8,722,874 B（8.32 MiB）** / exe **21,914,112 B（20.90 MiB）**。

未做（P2）：子事实“来自拆分”父链接；笔记右栏父事实“4 条子事实”badge；拆分弹窗从句补主语；事实库父子组；900 宽/125% 缩放/暗色模式复检。

### 3.14 左右栏与浮层滚动条（2026-11-09）

> 设计与边界见 [`PANEL_INTERACTION.md`](PANEL_INTERACTION.md)。

- 左栏 180–440px 可拖；顶栏开关收起/展开；双击复位；键盘 ←/→ 16px、Home 复位；持久化 `yebian.shell.sidebar.width/collapsed`。
- 右栏 300–560px 可拖；标题「✕」收起为 28px 边缘条；双击复位；持久化 `yebian.rail.width/collapsed`。
- 滚动条改为全局浮层 thumb：靠近右缘 90ms 显示，900ms 无操作 180ms 淡出，快速划过不触发；thumb 可拖。只对纯纵向滚动容器隐藏原生滚动条。
- 修复：左栏收起时主区显式放第 2 列，避免 0 宽列把正文吃掉；`yebian.sidebar.collapsed` 继续归 NoteList，不覆盖文件夹折叠状态。
- 真机：左栏 240→300、右栏 380→430、左右收起/展开、滚动条 hover/淡出/快速划过、thumb 拖动 0→120、1040 顶栏不溢出。包 **8,722,874 B（8.32 MiB）** / exe **21,914,112 B（20.90 MiB）**。
- 未验：真实触屏拖拽、横向滚动自定义、900 宽/缩放、所有页面在 1040 的新栏宽组合。

### 3.15 笔记阅读页排版整改（2026-11-09）

> 走查报告与前后对照见 [`NOTE_READING_AUDIT.md`](NOTE_READING_AUDIT.md)；改前 `shots/audit/14,15`，改后 `shots/audit/40,41,42,43`。

- **标题**：按 `：:，,、；;` 切成语义块，每块 `nowrap`，不再出现「机/会」单字孤行；`clamp(30px,2.7vw,39px)`，正文容器 ≤760px 时 28px。
- **署名行**：原 `.meta`（作者/日期/完整 run id/字数）和 `.prov-strip`（ScribeFlow badge/run id/prompt/时间/按钮）合并为一行 `字数 · 原视频`；删除日期、UP 名、重复 run id、badge、prompt 和可见生成时间。日期、UP、发布/运行/生成信息收进右栏「信息」。三档高度 20px、单行、横向溢出 0。
- **列表**：侧栏收藏/树改成 30px 单行，每篇只有标题（标注数行尾 badge，虚拟滚动行高 48→30）；标签台结果、事实详情笔记组不再拼 `UP · 日期`；日期/UP 收进「信息」，图谱仍把 UP 当关系属性。
- **对齐**：笔记行 lead 16→19px（等于 14px×1.35 的标题行框），图标 SVG 14→16px 并上移 1px，星标上移 1px；图标/文字行框中心 DOM 均为 215.9px，侧栏头部标题与三个按钮中心均为 79px。
- **速览**：`.takes ol` 恒为单栏，窄页不再变报纸双栏。
- **正文**：`.doc` 上限 860px；≤760px 容器正文 17px / 行高 1.9。
- **栏宽**：保留用户持久化偏好，但 ≤1180 视口有效值压到左 280 / 右 330；回宽屏恢复。
- DOM 实测（打包窗口 + CDP）：1865 正文 860 / 标题 39px；1440 正文 622 / 标题 28px；1040 左 280 / 右 330 / 正文 430 / 标题 28px。三档标题均 2 个视觉行，署名均 1 行；侧栏 30px 单行、不再出现 UP 和日期。
- 回归：`cargo test --lib` 166 passed / 7 ignored；`npm run tauri build` 通过；包 **8,722,874 B（8.32 MiB）** / exe **21,914,112 B（20.90 MiB）**。
- 未验：900 宽、125%/150% 缩放、暗色、真实触屏；只测一篇 2,461 字中文笔记；标题分块是标点启发式；原视频用系统浏览器打开，不是内嵌播放。

### 3.16 侧栏搜索与结果面板（2026-11-09）

> 设计、交互与边界见 [`SEARCH_DESIGN.md`](SEARCH_DESIGN.md)。

- **单一入口**：删除顶栏 `.search-wrap`；侧栏 `筛选笔记…` 改为 `搜索笔记…`，成为唯一搜索入口；`Ctrl+K` 派发 `yb-focus-search` 聚焦侧栏，`Esc` 清空恢复树。
- **结果面板**：140ms 防抖调用 `api.searchNotes`；有 query 时隐藏收藏/树，显示 `N 篇笔记` + 标题 + 路径；标题/路径不含 query 时标 `正文命中`；点击结果打开笔记并保留 query。
- **样式修复**：搜索图标显式 `fill:none`（黑点来源）；focus 去掉 border+outline 双环，改为 1px 蓝边 + `0 0 0 2px var(--blue-soft)`；清除按钮 24×24。
- **窄窗标题**：1040 下长标题块 `min-width:0; white-space:normal`，块内换行，不再被右栏裁切。
- 实测：搜索「特朗普」6 篇；`search-wrap` 数量 0；图标 `fill:none`；focus `outline:none`；`Ctrl+K` 聚焦 `.side-filter`；`Esc` 清空后树恢复；点击结果标题切换、query 保留。
- 回归：`cargo test --lib` 166 passed / 7 ignored；包 **8,722,874 B（8.32 MiB）** / exe **21,914,112 B（20.90 MiB）**。
- 未做：正文片段/高亮、正则/标签过滤语法、搜索历史；只测 24 篇；未复检暗色/125%/触屏。

### 3.17 标注改为选区驱动（M1，2026-11-10）

> 交互调研与三个 demo 见 [`ANNOTATION_REDESIGN.md`](ANNOTATION_REDESIGN.md)。

- 删除 `NoteBlock` 每段的 `+` 和 `@annotate`；标注入口只剩“拖选正文”。
- 选区支持跨块；右栏「标注」出现临时卡片：引文、3 色、标记/存疑/待办、单行输入；`Enter` 保存，`Esc` 取消。
- SQLite `annotations` 增 `segments_json`（blockId + UTF-16 start/end）和 `color`；旧标注为空时按整块回退；`relocate_annotations` 同步 segment blockId。
- 正文高亮用 `<mark class="anno-hl" data-anno>` 精确包裹；点高亮选中卡片，点卡片定位原文；打开有标注的笔记自动切到「标注」。
- 页边卡片目前在右栏按正文顺序排列，**没有排到段落同高**；这是 M2 的布局项。
- 回归：`cargo test --lib` 166 passed / 7 ignored；前端 build / tauri build 通过；包 **8,722,874 B（8.32 MiB）** / exe **21,914,112 B（20.90 MiB）**。
- 未做：hover 预览、键盘 H/N、移动端选区、颜色命名、删除撤销、跨段卡片对齐。

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
| `production_checkpoints` | 单元级检查点（payload + usage） |
| `production_subscriptions` | UP 订阅：mid、间隔、模板/提示词、过滤、基线/下次检查/错误 |
| `production_seen_videos` | 已见视频：vault+mid+bvid 唯一，状态/任务/笔记、最近视频展示 |

`production_jobs` 还保存 `vault_path`、`config_json`（非密钥执行配置快照）和 `checkpoint_at`。
设置仍存 `settings` 表，键前缀 `production.`。AI usage 聚合在 `production_jobs.usage_json`，
没有单独的 `production_usage` 明细表；月度汇总按 `created_at` 过滤本月任务。

AI key 按 provider 分开存：`production.ai.api_key.<provider>`。旧键 `production.ai.api_key`
在打开数据库时迁移到当前 provider 后删除。AI/ASR/检索 key 目前**明文存本机 SQLite**，
与旧 ScribeFlow 一致；商业化前需要迁移到系统凭据存储/KMS。

## 5. 验收（已跑）

```bash
cd src-tauri && cargo test --lib
# 164 passed, 0 failed, 7 ignored

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
- 标注重定位真机（2026-11-07）：临时笔记加入一条 `old-block` 标注，打开后显示「锚点失效 1 条」；点「尝试重新定位」后恢复为当前块 `bb36dd871`，提示「已重新定位 1 条；0 条仍需手动处理。」；明/暗 45 个文本节点、1040 下 46 个，均 0 个 <12px / 0 低对比。
- DeepSeek Search 全链路（2026-11-07，多查询 + 正文抓取）：`BV1fm476EEgh`（927s）走「速览与溯源」六步成功，笔记 `ScribeFlow/民主党大胜中期选举，然后呢？.md`；10 条可核查主张、**4 条外部可印证 / 6 条存在争议 / 0 条未找到**；20 次检索全成功、91 条去重候选；抓取 12 次/成功 6 次（402/403/网络失败）；usage 50,740/36,919、音频 927s、估算 ¥0.21388。截图里的“1180 万儿童”已抓到 ASPE PDF，但视频说实际领取 120 万、ASPE 原文是 180 万，所以是“存在争议”。旧单查询版是 18 条中 1 条可印证。第一次 120 秒下载超时（2.31MB）已修：下载客户端 900s、选最低码率音轨。另有 MiMo 转写重复段落 4 次的源问题。
- 事实库 Phase 1 真机（2026-11-07）：最新 trace 回填 10 条事实（4 verified / 6 disputed）；笔记右侧事实栏显示 10 条、来源域名与支持/冲突、来源链接；点「定位」成功高亮正文块；旧历史运行造成的 59 条堆积已按 `run_id` 改为只取每篇笔记最新 trace。
- 事实关系真机（2026-11-07）：10 条事实/45 对，只写入 1 对相关（小布什两场战争，0.377）；笔记侧栏「相关」显示 2 条，点击跳到事实库并高亮目标卡。共享“120 亿美元”的两条事实（0.273）没有被误连。修正“后创建且 ID 更大的事实不建关系”的方向 bug 后，真实库重建为 22 行双向关系（11 对）。
- 事实重跑与合并真机（2026-11-08）：`BV1fm476EEgh` 第二次跑 `run_6f01...`，290 秒、¥0.166608，10 条新事实（5 verified / 4 disputed / 1 unsupported）；由于当时还是旧构建，正文 8,492 B 仍含 `## 溯源`。移除追加后短文本 trace 4,043 B 无该章节；视频链路未在最终构建上重跑。
- 事实合并/撤销真机（2026-11-08）：合并弹窗默认最高相似；确认后来源/证据/提及/核验历史迁移，被合并事实消失；「合并记录」可直接撤销，撤销后源事实恢复、保留事实回到合并前状态。真实库合并 5 对（0.65–0.94）后从 21 条降到 16 条（7 verified / 8 disputed / 1 unsupported），保留 6 条记录（5 生效、1 已撤销）。
- 图谱真机（2026-11-08）：13 篇笔记、16 条事实、19 个来源，97 节点 / 95 条边；事实层默认开、来源层默认关；事实节点按状态着色，点节点可看事实全文/提及笔记/来源，来源节点可打开 URL。
- 事实详情页真机（2026-11-08）：列表 16 行只做索引；点第二行进入 `/facts/<id>`，1 篇笔记、2 条证据、核验历史都在详情；列表不再有「打开笔记」主按钮。
- 笔记右栏真机（2026-11-08）：事实 tab 10 行（5 有证据 / 5 需关注），按争议/未找到优先排序；标注 tab 空状态；关联 tab 局部图；信息 tab 来源 + 文件。正文点「+」自动切到标注 tab，textarea 获得焦点。
- 历史史料真机（2026-11-08，独立检索版）：17 条事实 = 1 条历史事实 + 16 条通用事实；1 份《明史》史料、1 段原文、1 条引用；事件表 0；史料库/史料详情只读 + 轻量离线补录；事实详情「自动找史料」规划到 `明史·卷三百八·奸臣传·严嵩`，拿到 7 条候选（ctext 明史/HathiTrust/NLC/ctext 台湾文献/明史演义/明史纪事本末/一史馆），Wikisource 直连失败、ctext 无 Key，自动引用 0 条，citation 仍为 1；候选全部进候选池并显示 reason。**候选补录真机**：ctext《明史》候选粘贴原文后写入 1 条 citation（1→2），随后移除 citation、删测试 source/passage、候选恢复 candidate，库回到 1/1/1；「归类」general ↔ history 切换、引用移除、离线补录均真机通过。
- ctext live（2026-11-08）：`api.ctext.org/searchtexts?title=明史` 返回书目（含 `明史` / `ctp:wb410835`）；`gettext` 无 Key 返回 `ERR_REQUIRES_AUTHENTICATION`；网页抓取明确禁止，代码不抓网页。
- 事实整理真机（2026-11-09）：`/facts/curate` 显示 0 重复 / 0 冲突 / 2 复合 / 1 无来源 / 17 未整理；两条复合事实都能看到相关原子事实；标记待拆分后未整理 17→16，标记已看过成功；临时插入 1 条 contradicts 关系后冲突区显示 1 对，选择「口径不同」+备注写入 resolution；测试后删除临时关系/resolution/reviews，库回到 sources=1/citations=1、reviews=0、conflict_resolutions=0。
- 真实历史笔记验证（2026-11-08）：`yn-2fe...`《嘉靖二十年不上朝，为什么还能掌控明朝？》；事实「严嵩无他才略，唯一意媚上，窃权罔利。」；史料《明史》卷三百八《严嵩传》中华书局点校本，原文同句。笔记右栏显示「有史料 / 历史 / 1 条史料」；事实库显示「历史事实 1 / 其他事实 16」；通用事实详情没有「史料」区；历史事实详情显示「史料」，网页证据为空时提示以史料为准。截图见 `shots/domain/`（01–10）。
- 历史质量基线（2026-11-09）：UP 431055518 的 7 条视频 + 嘉靖笔记，8 篇历史笔记 / 43 条历史事实；7 个新任务 1,730s、455,810 tokens、¥1.0705；42/42 新事实 history；前 34 条规划 34/34 人工核对无明显错书；库内候选 43/43（含 12 条评测补搜 + 1 条长笔记手动补搜），ctext top1 41/43；网页证据 39/43；自动 citation 0；自动 trace 覆盖 30/43（limit 8）。长笔记修复后 383s 成功、100%，但自动史料入库 8/9。详见 [`HISTORY_EVAL.md`](HISTORY_EVAL.md)。
- 事实返回真机（2026-11-08）：笔记右栏点事实 -> `/facts/<id>?from=note&noteId=...`，返回按钮显示「返回笔记」；返回 `/note/<id>?fact=<id>` 后自动切事实 tab、滚到原事实并高亮。从事实库进入则显示「返回事实库」，返回 `/facts`。
- 语义归并真机（2026-11-08）：`run_a9e4...` 20 秒、¥0.01039284；模型把“美国每年给低收入家庭孩子的托育补助大约是120亿美元”归并到已有 `e94b4a`，事实总数保持 16，日志含归并理由；测试笔记和 mention 已清理。
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
- UP 订阅真机（2026-11-07）：真实 UID 2 一次受控空间请求 `code=0`；添加订阅基线 43 条、0 任务；补跑 1 条创建 auto 任务并成功，notes 路径 `待整理/验证笔记.md`；再次立即检查 `newTasks=0`。mock 用量 4M/4M，音频 11s。关闭窗口到托盘后进程存活、主窗口隐藏；单实例可恢复。截图 `production-24..29`。
- UP 空间瞬时失败真机（2026-11-09）：`寒清关下说书人`（mid 27199932）一次空间请求报 `error sending request`，卡片显示完整 URL，首次退避 60 分钟；同库其他 6 个 UP 正常。手动立即检查成功，说明是瞬时传输失败。最终构建改为空间/WBI/buvid/followings/login 最多 3 次重试（250/500ms）、错误分类显示、首次传输失败退避 2 分钟；复检该订阅返回“没有新视频”、`last_error` 清空、`consecutive_failures=0`、下次约 33 分钟后。截图 `shots/subscriptions/01-recovered.png`。
- 视觉：设置页在 1440×900 与 1040×760 下无横向溢出；1040 宽时右侧「制作引擎」栏自动隐藏。
- 截图：`shots/production-01-home.png`、`production-02-wizard.png`、`production-03-account.png`、
  `production-04-cover.png`、`production-05-settings.png`、`production-06-settings-usage.png`、
  `production-13-prompt-versions.png`、`production-14-prompt-shadow.png`、`production-15-prompt-restore-notice.png`、
  `production-16-model-sort-copy.png`、`production-17-manual-price.png`、`production-18-model-pinned.png`、`production-19-resume-failed.png`、`production-20-resume-success.png`、`production-21-data-cleanup.png`、`production-22-annotation-orphan.png`、`production-23-annotation-relocated.png`、`production-24-subscriptions-empty.png`、`production-25-subscriptions-add.png`、`production-26-subscriptions-card.png`、`production-27-subscriptions-settings.png`、`production-28-subscriptions-1040.png`、`production-29-task-auto.png`、`production-30-subscriptions-link.png`、`production-31-subscriptions-video.png`、`production-32-trace-multi-query.png`、`production-33-note-facts.png`、`production-34-facts-library.png`、`production-35-facts-related.png`、`production-36-facts-related-card.png`、`production-37-note-facts-related.png`、`production-38-facts-focus-jump.png`、`production-39-facts-related-final.png`、`production-40-fact-merge-dialog.png`、`production-41-fact-merged.png`、`production-42-facts-merged-library.png`、`production-43-note-after-merge.png`、`production-44-facts-final.png`、`production-50-facts-conflict-merge-ready.png`、`production-51-merge-history.png`、`production-52-merge-undone.png`、`production-55-merge-history-6.png`、`production-56-graph-facts-64pass.png`、`production-57-graph-sources-dots.png`、`production-58-facts-final-16.png`、`production-59-note-facts-final.png`、`production-60-task-trace-tab.png`、`production-61-wizard-trace-options.png`、`production-62-production-settings.png`、`production-63-search-settings.png`、`production-64-graph-fact-panel.png`、`production-65-graph-source-panel.png`、`production-66-facts-final-clean.png`、`production-68-task-trace-current-facts.png`、`production-69-graph-final.png`、`production-70-facts-index.png`、`production-71-fact-detail.png`、`production-72-facts-index-final.png`、`production-73-fact-detail-final.png`、`production-74-fact-detail-multi-evidence.png`、`production-75-facts-index-final2.png`、`production-76-fact-detail-final2.png`、`production-77-fact-conflict-explained.png`、`production-78-fact-conflict-explained-final.png`、`production-79-note-rail-facts.png`、`production-80-note-rail-notes.png`、`production-81-note-rail-links.png`、`production-82-note-rail-info.png`、`production-83-note-rail-final.png`、`production-84-note-return-highlight.png`、`production-124-ming-note-simplified.png`、`production-125-ming-fact-simplified.png`、`production-126-ming-source-simplified.png`、`production-127-sources-library-simplified.png`、`production-106-source-search-final.png`；历史功能成组截图见 [`shots/history/`](../shots/history/README.md)；事实领域分区与史料只读验收见 [`shots/domain/`](../shots/domain/README.md)；右栏四 tab 的成组截图在 [`shots/note-rail/`](../shots/note-rail/README.md)；按流程整理的 15 张在 [`shots/facts-flow/`](../shots/facts-flow/README.md)；待补录队列与长笔记第 9 条补搜见 `shots/queue/04-pending-queue-360.png`、`05-paste-from-queue-360.png`、`06-source-library-pending-360.png`、`07-ninth-fact-candidates.png`；59 条事实的整理概览见 `shots/curation/08-overview-59-facts.png`；复合事实拆分入口见 `shots/curation/09-composite-split-action.png`，拆分弹窗/结果/撤销/事实库筛选见 `shots/split/01..04`。

实测数字：

- 安装包 **8,722,874 B（8.32 MiB）**；`yebian.exe` **21,914,112 B（20.90 MiB）**。
- models.dev 目录：11 个有目录的 provider、636 个模型；DeepSeek V4 Pro 输入 ¥4.752 / 输出 ¥14.256
  每百万 token（USD 0.66 / 1.98 × 7.2）。
- 没有新增常驻进程/端口；价格目录只在 `%APPDATA%/com.yebian.notes/production/models-dev-catalog.json`
  缓存 24h。

## 6. 诚实边界

- **UP 订阅仍依赖应用进程**：自动检查只在应用运行时发生；托盘/开机自启需用户手动开启，托盘菜单没有“立即检查”。B 站 WBI/风控是外部变量，真实请求只做了受控单次验证。
- **UP 订阅自动任务不自动重试**：启动失败/运行失败都会留在任务页；应用中途退出会变成 interrupted，需要手动继续；手动“立即检查”没有单独 60 秒/UP 冷却。
- **UP 订阅只读第一页 50 条、不反查手写同 URL 笔记**：两个检查之间超过 50 条新投稿可能只处理第一页；用户若手写过同 URL 但不同 noteId 的笔记，仍可能生成第二篇。
- **关注入口是本地过滤**：每页 50 条，搜索只覆盖已加载的关注，不是全量搜索；关注接口需要登录，可能受 B 站风控影响。视频反查只保证公开视频能拿到作者 mid。
- **B站列表没有逐条打开验收**：真实账号已验收藏夹 49 条、稍后再看 20 条、历史 100 条，多选建任务和失效 Cookie 路径也走过；没有逐条播放/转写列表里的每个视频。
- **联网溯源只验收了 DeepSeek 的检索端点**：用真实 DeepSeek Key 请求 `/v1/tools/search` 返回 200/3 条结果；解析器单测覆盖 summary/snippet，但没有跑完整溯源任务，也没有用智谱/Tavily 的真实 key 验收。
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
- **标注重定位是文本相似度匹配**：只处理同一 noteId 的覆盖/重生成；保留两份不会把标注复制到新笔记；短句、相近候选、大幅改写会留在失效区；不删除、不静默改挂。
- **断点续跑不做跨设备/跨库**：任务绑定创建时 vault；旧任务启动时绑定当前 vault，之后不能跨库继续/清理。
- **E-01 首轮误删任务记录**：首版验证脚本在 vault 过滤修复前清空了 sample-vault 的 10 条任务记录；笔记文件未动，但任务历史无法恢复。修复后脚本有“切换临时库后任务列表必须为空”的前置断言。
- **检查点不是服务商账单**：请求已发出但响应丢失仍可能重复计费；旧任务不补账。
- **应用重启后的 resume 只做了状态/存储单测**：现有 interrupted 机制复用，未单独跑“杀进程→重启→继续”的端到端。
- **没有云端/多租户**；所有密钥、任务、媒体都在本机。
- **安装/卸载已在较早的同数据层包验收；8.32 MiB 最终包只做了覆盖安装并启动**：覆盖后 13 篇笔记、16 条事实、6 条合并记录、B站账号和订阅仍在；静默卸载/重装验收仍来自较早的 7.56 MiB 包，不是这个最终包。没有重测没有 WebView2 的干净机器，也没有勾选卸载时删除应用数据。
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
