# ScribeFlow 制作台与 Artifact 协议

> 状态：**本地 Artifact v2 Outbox / ACK / 注册表与 Yebian 工作区模式已实现（2026-11-05）**；实时运行状态与云端仍未做。
> 这是后续所有 ScribeFlow 集成工作的唯一入口。找“还有什么能继续开发”时，先读本文件的状态矩阵和 `SF-xxx` 列表，不要从 README 的旧结论猜。

## 1. 目标边界

- **ScribeFlow 是可替换的生产者**：画布、节点、提示词、转写、运行记录留在它自己的 Web 应用里。
- **页边笔记是系统 of record**：Markdown、索引、标注、关系图、最终目录和 front matter 都归它。
- 两边不共享 SQLite，不把 Vue Flow 嵌进 Tauri。
- 新链路只通过 `.yebian/outbox` 的 Artifact 包交付；Obsidian 直写和相邻 sidecar 只保留为兼容层。

## 2. 当前实现（本地 v2）

### 2.1 目录与时序

```text
<vault>/
├── .yebian-vault                         # {"schema":"yebian-vault-v2",...}
├── .yebian/
│   ├── outbox/<artifact_id>/<revision>/
│   │   ├── note.md                       # 正文，不含 front matter
│   │   ├── artifact.json                 # Artifact v2 manifest
│   │   ├── READY                         # 最后写入；表示包完整
│   │   └── ACK                           # 页边笔记入库后写
│   └── artifacts/<artifact_id>.json      # 页边笔记的 Artifact 注册表
├── ScribeFlow/<标题>.md                  # 页边笔记最终写入的笔记
└── ...普通笔记
```

1. ScribeFlow 检测到目标库 marker 是 `yebian-vault-v2` 时，**不直接写 vault 的笔记目录**；它生成 `note.md` + `artifact.json`，最后写 `READY`，放入 `.yebian/outbox`。
2. 页边笔记启动、切库、刷新制作台时 drain：校验 schema/路径/大小，写 `ScribeFlow/<标题>.md`，写 front matter，更新索引，写 `.yebian/artifacts` 注册表，最后写 `ACK`。
3. 页边笔记没打开时，包留在 outbox；下次打开自动入库。
4. 同一 `note_id` 的重跑写入下一个 revision；只保留一篇笔记，注册表 revision 递增。
5. 旧版 `ScribeFlow/` 直写 + 相邻 sidecar 仍可读，但不再扩展。

### 2.2 Artifact v2 manifest

```json
{
  "schema": "yebian.artifact.v2",
  "artifactId": "sf-…",
  "noteId": "sf-…",
  "revision": 1,
  "status": "ready",
  "title": "中国社保的累退…",
  "noteType": "视频笔记",
  "tags": ["话题/AI"],
  "entities": { "人物": ["…"], "事件": ["…"], "时期": ["…"] },
  "source": { "kind": "B站", "label": "B站", "url": "https://…", "author": "…", "publishedAt": "2026-10-14" },
  "run": { "projectId": "…", "projectName": "社保专题", "runId": "run_…", "nodeId": "…", "prompt": "builtin.…", "model": "…" },
  "generatedAt": "2026-11-05T10:56:58.040Z",
  "usage": { "promptTokens": 0, "completionTokens": 0, "estimatedCost": 0, "currency": "CNY" },
  "assets": []
}
```

由页边笔记负责的映射：

| Artifact | 笔记 |
| --- | --- |
| `noteId` | front matter `id` |
| `title` | `标题` + H1 |
| `noteType` / `source.label` | `类型` / `来源` |
| `source.author/url/publishedAt` | `作者 / 原始链接 / 发布日期` |
| `generatedAt` | `创建日期` |
| `tags` | `tags`，只保留话题标签 |
| `entities` | `人物 / 事件 / 时期` |
| `run/usage/assets` | Artifact 注册表，不进正文 |

ScribeFlow 不再决定最终目录、文件名、YAML 引号或 `## 相关笔记`；这些归页边笔记。`来源/B站`、`UP主/...` 这类自动标签在 v2 里被过滤，不再写进笔记。

### 2.3 更新、重复与 ACK

- 包目录按 `artifact_id/revision` 唯一；`READY` 最后写，崩溃不会产生半成品。
- drain 成功写 note + 注册表 + `ACK`；重试看到 ACK 就跳过。
- 同一 `noteId` 的新 revision 覆盖同一路径；页边笔记按块哈希保留标注，正文变化会显示「锚点失效」。
- **当前限制**：用户手动改过生成笔记后，新 revision 仍会覆盖；冲突/版本 UI 是 SF-006。
- 旧 sidecar 的 `revision` 会作为 v2 起始 revision 的参考，避免从 1 重新计数。

## 3. 状态矩阵

| 能力 | 状态 | 证据/说明 |
| --- | --- | --- |
| 制作台路由与侧栏 | 已实现 | `StudioView.vue`、`StudioSidebar.vue` |
| Artifact v2 Outbox / READY | 已实现 | ScribeFlow `writeObsidianNote` 的 v2 分支 |
| drain / ACK / 注册表 | 已实现 | `src-tauri/src/outbox.rs`、`artifact.rs` |
| 笔记页来源条 | 已实现 | `NoteView.vue` 的 `.prov-strip` 与右侧「来源与制作」 |
| 本机连接状态 / 打开画布 | 已实现 | 探测 `127.0.0.1:8787`；本地开发优先开 5173 |
| 旧 Obsidian 直写 + sidecar | 已实现（legacy） | 只修 bug，不扩展 |
| 运行中任务列表/SSE | **未做** | SF-002；现在只有 ready 包，没有 running 进度 |
| ScribeFlow Yebian 工作区瘦身 | **已实现基础版** | SF-012；隐藏 Obsidian/坚果云/重打标/回补，节点面板/模板/运行详情重命名发布节点；设置始终有「页边笔记」入口，激活后侧栏显示徽标 |
| 云端 Artifact API/多租户 | **未做** | SF-003 |
| 笔记页打开原始链接 | **未做** | SF-004 |
| 移出 `ScribeFlow/` 后的中心关联 | 部分 | 注册表按 noteId 查找；移动后的路径更新与清理未做 |
| 重新生成/版本冲突 UI | **未做** | SF-006 |
| 真实 B 站全链路 | **未做** | SF-008 |

## 4. 未完成 backlog（按编号找）

- **SF-001（已完成，legacy）**：Obsidian 直写时稳定 `id` + 相邻 sidecar；仅保留兼容。
- **SF-011（已完成）**：Artifact v2 Outbox/READY/ACK/注册表；ScribeFlow v2 发布分支；页边笔记 drain。
- **SF-002 运行中状态**：ScribeFlow 运行开始时写 `status: running` 的占位包/状态文件，结束后更新；或页边笔记只读查询 `/api/runs`。验收：画布运行时制作台显示节点进度，停止/失败可落到包。
- **SF-003 云端 Artifact API**：`POST /v1/workspaces/:id/artifacts`（幂等键 artifactId+revision）、`GET /v1/artifacts?cursor=`、对象存储、Postgres、认证、配额；页边笔记同步客户端。验收：断网可读本地副本，重连后增量同步不重复。
- **SF-004 打开原始链接/转写稿**：受限 `open_url`（只允许 http/https，禁止 shell 注入）或 Tauri opener。
- **SF-005 注册表完整性**：笔记移出 `ScribeFlow/` 后更新 registry 路径；清理删除笔记的 registry；支持用户改入库目录。
- **SF-006 版本/重生成**：revision/diff/重新生成确认；手动改过的生成笔记不能静默覆盖。
- **SF-007 商业化控制面**：多租户、KMS、SSRF 白名单、媒体合规、队列/限流、计费。
- **SF-008 真实链路验收**：B 站 → 转写 → 提炼 → outbox → 入库，确认 noteId、front matter、注册表、索引、标注、搜索。
- **SF-009 源目录配置**：允许改 `ScribeFlow/` 之外的入库目录。
- **SF-010 ScribeFlow 文档**：写清 v2 marker、outbox、以及 Obsidian legacy 的边界。
- **SF-012（已完成基础版）Yebian 工作区模式**：检测 v2 marker 后隐藏 Obsidian/Nutstore/重打标/回补与无关节点；节点面板/模板/运行详情把发布节点叫「入库：页边笔记」；新建/保存/运行自动补唯一发布节点，无出口链路在运行前被拒绝。
- **SF-013**：把内部 `process.obsidian` 类型改成中性的 `process.publish`，彻底移除 Obsidian 命名和遗留适配器；当前 UI 已改名，但代码/接口/工程 JSON 仍是旧名。

## 5. 验收（已跑）

```bash
cd src-tauri && cargo test --lib          # 29 passed
npm run build
node tools/make-studio-fixture.mjs D:/Develop/yebian-studio-fixture
node --no-warnings tools/verify-studio.mjs  # 9/9
node --no-warnings tools/verify-sidebar.mjs # 20/20（重跑；首轮删除等待曾超时一次）
```

真实 v2 链路（ScribeFlow 真引擎，不调 AI）：

- `source.text → process.obsidian` 在带 v2 marker 的库生成 `.yebian/outbox/sf-…/1/`；
- 页边笔记 `drain_outbox` 返回 `published=1`，写入 `ScribeFlow/SF-002 端到端.md`，`list_artifacts` 为 `derived=false`，`runId=run_sf002`、工程名、模型都在；
- 页边笔记关闭时再跑一次生成 revision 3；重新打开应用后启动 drain，仍只有 1 篇笔记，注册表 revision 3。
- ScribeFlow：`engine.obsidian.test.ts` **46/46**，完整 server 套件 **524/524**；新增 workspace 测试 2 条。
- 截图：`shots/studio-01-overview.png`、`shots/studio-02-note-provenance.png`。

## 6. 诚实边界

- 只测了文本来源与本地文件系统；B 站/转写/长任务没有在当前 v2 路径上跑（SF-008）。
- 没有运行中 UI；制作台只看到 ready/已入库，running 需要 SF-002。
- 没有手动修改冲突检测；新 revision 会覆盖生成笔记，标注锚点可能失效（SF-006）。
- Yebian 模式基础版已隐藏主要 Obsidian 设置/节点，但内部类型仍叫 `process.obsidian`，且部分帮助文案仍可能提到 Obsidian（SF-013）。
- 注册表按 artifactId 命名；移动/删除笔记后的清理未完成（SF-005）。
- sidecar 的 prompt 是 promptBlockId；assets 目前为空；模型来自当前设置，不是 run 快照。
- Yebian 工作区模式通过了 ScribeFlow typecheck/build 和路由/引擎测试，但**没有**在真实 marker 工作区上做一轮原生窗口视觉验收；当前本机 ScribeFlow dev 服务的库路径仍是 `D:\笔记`，模式为 standard。
- 云端不存在；`SF-003` 之前不要把 ScribeFlow 服务暴露到公网。
