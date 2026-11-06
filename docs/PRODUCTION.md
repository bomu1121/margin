# 页边笔记制作台（原生实现）

> 状态：**2026-11-06 已落地**。制作台不再跳转 ScribeFlow Web 平台，执行、任务、模板、登录、入库都在页边笔记进程内完成。
> 旧的 ScribeFlow Artifact v2 / outbox 协议继续作为入库通道；`docs/SCRIBEFLOW.md` 里关于 outbox 的部分仍然有效，平台入口不再使用。

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
| `production/clients.rs` | OpenAI 兼容 chat、ASR multipart、B站 API/QR、联网检索 |
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

- **B站链接**：BV / av / b23（b23 跟随短链跳转）；支持分P/合集多选。
- **B站收藏**：扫码登录后读取收藏夹、稍后再看、观看历史，多选视频作为一个任务。
- **本地音视频**：通过系统文件选择器选择；音频/视频由本机 ffmpeg 转 16k 单声道 wav。
- **粘贴文稿**：直接进入校对和加工。
- B站公开视频不需要登录；收藏夹/稍后再看/历史需要登录。

### 3.3 处理

- ASR：MiMo / OpenAI 兼容 `/audio/transcriptions`。
- AI：OpenAI 兼容 `/chat/completions`，失败按设置重试。
- 校对：按 6000 字切块，逐块校对后拼接。
- 加工：按 12000 字切块；长文稿先分块生成再合并。
- 分章：按粒度/最大章节数生成 `##` 章节。
- 合并：多份产物合成一篇。
- 导图：Markdown 缩进大纲，结果页「思维导图」。
- 练一练：JSON 题目，结果页本地判分。
- 攻略：阴阳师专用 prompt。
- 溯源：调用联网检索（智谱 / Tavily），再让模型逐条核查；结果页「溯源」。
- 多风格/多路：同一素材生成两版，结果页「对照」。
- 属性抽取：人物/事件/时期等写入 front matter；相关笔记按这些字段匹配。

### 3.4 入库

- 稳定 `noteId`：`yn-<sha1(template + source_key)>`；同来源同模板重跑更新同一篇，revision 递增。
- 包目录：`.yebian/outbox/<noteId>/<revision>/`（`note.md`、`artifact.json`、`READY`）。
- 页边笔记 `outbox::drain` 负责最终目录、文件名、front matter、索引和标注；制作引擎不直接写用户笔记目录。
- 入参：标题、类型、来源、作者、原始链接、发布日期、创建日期、tags、属性。
- 入库后调用 index `delta_wait`，笔记页立即可读。
- 相关笔记：按人物/事件/时期匹配库内旧笔记，追加 `## 相关笔记` 的 `[[链接]]`；跳过 `ScribeFlow/` 自身产物。

### 3.5 设置

- AI：provider/baseUrl/model/key、输入/输出单价、币种。
- ASR：engine/baseUrl/model/key、每分钟单价。
- 检索：渠道/key/每次结果数。
- 制作：默认模板、默认提示词、重试次数、ffmpeg 路径、通知/提示音。
- B站：扫码登录/退出、收藏夹同步。
- 提示词：内置 11 个 + 自定义（新建/编辑/删除/设为默认）。
- 标签与属性：默认抽取字段。
- 数据：任务数、临时音频、outbox 体积；清理临时音频/已完成任务。

## 4. 本地数据

用户数据库 `%APPDATA%/com.yebian.notes/yebian.sqlite` 新增：

| 表 | 用途 |
| --- | --- |
| `production_jobs` | 任务、来源/选项 JSON、状态、进度、结果、usage |
| `production_steps` | 每一步的状态与详情 |
| `production_logs` | 运行日志（每条截断 8000 字符） |
| `production_prompts` | 自定义提示词 |

设置仍存 `settings` 表，键前缀 `production.`。AI/ASR/检索 key 目前**明文存本机 SQLite**，与旧 ScribeFlow 一致；商业化前需要迁移到系统凭据存储/KMS。

## 5. 验收（已跑）

```bash
cd src-tauri && cargo test --lib
# 40 passed, 0 failed, 1 ignored

npm run build
# vue-tsc + vite build 通过
```

手工端到端（本机，mock AI/ASR，真实文件系统）：

- 文本：`text → 校对 → 加工 → 入库`；写 `ScribeFlow/联调笔记.md`，front matter 含 `id/标题/类型/来源/tags/人物/事件`；同 source 重跑 revision 1 → 2，仍是一篇。
- B站：真实公开视频 `BV1DTWw6UE1y`（18:42 / 971s）→ 下载音轨 → ffmpeg → mock ASR → mock 模型 → `ScribeFlow/B站联调笔记.md`；front matter 的 `作者/原始链接/发布日期` 来自 B站；`source.videos` 持久化。
- 思维导图：生成 `result.mindmap` 并入库。
- UI：从「新建制作 → 粘贴文稿 → 文稿笔记 → 开始制作」走通，任务详情显示结果、步骤、用量、打开笔记；B站链接解析后显示封面，便于确认选对视频。
- 截图：`shots/production-01-home.png`、`shots/production-02-wizard.png`、`shots/production-03-account.png`、`shots/production-04-cover.png`。

实测数字：

- 安装包 **6,732,939 B（6.42 MiB）**；`yebian.exe` **14,734,336 B（14.05 MiB）**。
- 比上一版 5.49 MiB 安装包增加约 0.93 MiB（reqwest/rustls/tokio/qrcode 前端依赖）。
- 没有新增常驻进程/端口；空闲时只多 SQLite 表与 Tauri async runtime 的任务。

## 6. 诚实边界

- **B站登录成功后的收藏夹/稍后再看/历史没有做完整验收**：真实账号扫码成功、用户名/UID/头像已显示；成功回调 Cookie 已改为优先读 `Set-Cookie`；收藏列表/多选/登录过期路径仍待真实账号逐项走查。
- **联网溯源没有真实检索验收**：智谱/Tavily 请求已实现，但没有 key；这条模板在未配置检索时会明确失败。
- **练一练**依赖模型返回 JSON；解析器兼容数组和 `{items:[]}`，但只用 mock 文本验证过流程，没有用真实模型验证题目质量。
- **分篇（每个输入一篇）没有暴露到界面**：当前多输入会合并成一篇；`JobOptions.split` 仍是预留字段，不要在文档里宣称已支持。
- **自定义提示词**可用，但没有版本管理/对比；提示词名称重复时自定义会覆盖同名内置风格。
- **相关笔记**只匹配 `人物`/`事件`/`时期`，其他抽取字段不参与匹配；不处理手动移动/删除后的注册表修正（SF-005）。
- **通知**使用 Web Notification + WebAudio；没有在 Windows 通知中心做系统级验收。
- **没有任务重命名 UI**；只有删除记录（不删除已入库笔记）。
- **没有手动修改冲突检测**：重跑会覆盖生成笔记，标注锚点可能失效。
- **没有云端/多租户**；所有密钥、任务、媒体都在本机。
- **安装/卸载验收没有在 6.42 MiB 构建上重跑**；上一轮验收是 11-04 的 5.29 MiB 构建。
- **B站公开接口可能变更**；失败会显示接口错误，不会静默产出空笔记。
- **ffmpeg 是外部依赖**：B站音轨和本地视频需要；未随安装包内置（用户机器已安装，测试通过）。

## 7. 旧 ScribeFlow 的位置

- ScribeFlow Web 平台不再作为产品入口；`/studio` 现在渲染 `ProductionView.vue`。
- 旧 `open_scribeflow` 命令、`StudioSidebar.vue`、`StudioView.vue` 仍留在仓库作为兼容/迁移参考，但主界面不再引用。
- 旧 outbox 包仍会被 `outbox::drain` 处理；Artifact v2 注册表继续供笔记页「来源与制作」使用。
- 旧的 `docs/SCRIBEFLOW.md` 保留为历史与协议文档；新的实现以本文为准。
