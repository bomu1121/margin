# ScribeFlow 制作台与 Artifact 协议

> 状态：**本地 Artifact 协议、制作台与 ScribeFlow 原生 sidecar 已实现（2026-11-05）**；实时运行状态与云端仍未做。
> 这是后续所有 ScribeFlow 集成工作的唯一入口。找“还有什么能继续开发”时，先读本文件的状态矩阵和 `SF-xxx` 列表，不要从 README 的旧结论猜。

## 1. 目标边界

- **ScribeFlow 是可替换的生产者**：画布、节点、提示词、转写、运行记录留在它自己的 Web 应用里。
- **页边笔记是系统 of record**：Markdown、索引、标注、关系图留在页边笔记。
- 两边不共享 SQLite，不把 Vue Flow 嵌进 Tauri。
- 唯一耦合是 `ScribeFlow/` 目录下的 Markdown + 可选 sidecar。即使 ScribeFlow 被舍弃，笔记仍然可读、可搜索、可标注。

## 2. 当前实现（本地 v1）

### 2.1 目录与时序

```text
<vault>/
└── ScribeFlow/
    ├── 中国社保的累退….md
    └── 中国社保的累退….md.artifact.json   # 可选，富元数据
```

1. ScribeFlow 的 Obsidian 输出节点把 `vaultPath` 指向页边笔记库，`folder` 设为 `ScribeFlow`（或任意目录）；目标库有 `.yebian-vault` 时，ScribeFlow 自动写稳定 `id` 和 `<note>.md.artifact.json`。
2. 页边笔记的文件监听看到 `.md`，正常建立索引；`id` 优先使用 front matter，没有时使用路径哈希。
3. 制作台扫描 `ScribeFlow/` 下的 Markdown 和 `*.artifact.json`，按 `note_id`/相对路径关联。即使 ScribeFlow 不写 sidecar，现有 Obsidian 输出的 `来源/作者/原始链接/创建日期` 也能显示；运行/模型/成本会为空。
4. 笔记页读取同一份 Artifact，显示来源、运行、生成时间和入库状态。sidecar 与 Markdown 同目录时，笔记即使不在 `ScribeFlow/` 也能被制作台找到。

### 2.2 必需/可选字段

Markdown front matter（ScribeFlow 现有输出已经包含大部分）：

```yaml
id: 01J...                 # 强烈建议，保证改文件名后不丢标注
标题: ...
类型: 视频笔记
来源: B站
作者: ...
原始链接: https://...
发布日期: 2026-10-14
创建日期: 2026-11-05
tags:
  - 类型/视频笔记
人物: [...]
事件: [...]
时期: [...]
```

sidecar（`<note>.md.artifact.json`，ScribeFlow 对带 `.yebian-vault` 的库自动写；手工夹具也可提供）：

```json
{
  "schema": "yebian.artifact.v1",
  "artifactId": "<稳定 id>",
  "noteId": "<与 front matter id 相同>",
  "revision": 1,
  "status": "published",
  "note": "ScribeFlow/中国社保的累退….md",
  "title": "中国社保的累退…",
  "source": { "kind": "bilibili", "label": "B站", "url": "https://…", "author": "…", "publishedAt": "2026-10-14" },
  "run": { "projectId": "…", "projectName": "社保专题", "runId": "run_…", "nodeId": "…", "prompt": "观点提炼 · 成文版", "model": "…" },
  "generatedAt": "2026-11-05T08:42:00+08:00",
  "usage": { "promptTokens": 0, "completionTokens": 0, "estimatedCost": 0, "currency": "CNY" },
  "assets": [{ "name": "transcript.md", "path": "…", "kind": "transcript", "bytes": 42000 }]
}
```

sidecar 是优先元数据；缺失时制作台从 front matter 降级读取。sidecar 损坏或超过 2MB 会被忽略，笔记本身不受影响。

### 2.3 更新/重复/移动规则

- 同一路径重新生成：`.md` 覆盖，`id` 若在 front matter 中保持不变；标注块按内容哈希锚定，正文变化会进入「锚点失效」而不是静默删除。
- 生成到新文件名：视为新笔记；要保留标注必须让 ScribeFlow 写稳定 `id`。
- 页边笔记内移动：现有逻辑先写 `id` 再移动，Artifact 仍可按 sidecar 的 `noteId` 关联；若没有 sidecar，制作台对移出 `ScribeFlow/` 的笔记会失去展示入口（见 SF-005）。
- 同一 `noteId` 多个 sidecar：按 `revision` 最大的胜出；没有 revision 时按生成时间/文件时间。

## 3. 状态矩阵

| 能力 | 状态 | 证据/说明 |
| --- | --- | --- |
| 制作台路由与侧栏 | 已实现 | `src/views/StudioView.vue`、`src/components/StudioSidebar.vue` |
| 扫描 `ScribeFlow/` Markdown | 已实现 | `src-tauri/src/artifact.rs`；无 sidecar 时按 front matter 降级 |
| 可选 sidecar 富元数据 | 已实现 | `.md.artifact.json`；损坏/超 2MB 会忽略 |
| 笔记页来源条 | 已实现 | `NoteView.vue` 的 `.prov-strip` 与右侧「来源与制作」 |
| 本机连接状态 / 打开画布 | 已实现 | 仅探测 `127.0.0.1:8787`；固定 URL，不接受任意输入 |
| ScribeFlow 自动写 `id`/sidecar | **已实现** | SF-001；目标库有 `.yebian-vault` 才写 sidecar，普通 Obsidian 不写 |
| 运行中任务列表/SSE | **未做** | SF-002 |
| 云端 Artifact API/多租户 | **未做** | SF-003 |
| 笔记页打开原始链接 | **未做** | SF-004 |
| 移出 `ScribeFlow/` 后的 Artifact 关联 | **未做** | SF-005 |
| 重新生成/版本对比 UI | **未做** | SF-006 |
| 真实 ScribeFlow 端到端验收 | **未做** | SF-008 |

## 4. 未完成 backlog（按编号找）

- **SF-001（已完成）ScribeFlow 原生发布适配器**：目标库有 `.yebian-vault` 时写稳定 `id` + `<note>.md.artifact.json`；连续运行保留 noteId、revision 递增；普通 Obsidian 不写 sidecar。测试在 ScribeFlow `engine.obsidian.test.ts`。
- **SF-002 运行中状态**：ScribeFlow 运行开始时写 `status: running` 的占位 artifact，结束后更新为 `published`；或者页边笔记通过本机 API/SSE 读取 `/api/runs?status=running`。验收：画布运行时制作台显示节点进度，停止/失败能落到 sidecar。
- **SF-003 云端 Artifact API**：`POST /v1/workspaces/:id/artifacts`（幂等键 artifactId+revision）、`GET /v1/artifacts?cursor=`、对象存储、Postgres、认证、配额；页边笔记增加同步客户端。验收：断网可读本地副本，重连后增量同步不重复。
- **SF-004 打开原始链接/转写稿**：用受限的 `open_url`（只允许 http/https，禁止 shell 注入）或 Tauri opener 打开 B 站链接；转写稿作为 asset 打开。
- **SF-005 中心 Artifact 索引**：sidecar 放在 `<vault>/.yebian-artifacts/` 或把 artifact 元数据写进 notes 表，解决笔记移出 `ScribeFlow/` 后丢失制作台入口；同时避免每个目录散落 sidecar。
- **SF-006 版本/重生成**：保留 revision、diff、重新生成确认；正文变化导致锚点失效时给出明确提示；默认不静默覆盖。
- **SF-007 商业化控制面**：多租户、密钥 KMS、SSRF 白名单、媒体下载合规、队列/限流、计费；当前 ScribeFlow 无认证的单用户服务不能直接公网。
- **SF-008 真实链路验收**：用 ScribeFlow 真跑一条 B 站 → 转写 → 观点提炼 → 发布，确认 noteId、front matter、sidecar、索引、标注、搜索全链路。
- **SF-009 源目录配置**：允许用户改 `ScribeFlow/` 之外的入库目录；当前固定目录。
- **SF-010 ScribeFlow 输出文件夹文档**：在 ScribeFlow 设置页/文档写清“把 Obsidian vaultPath 指向页边笔记库、folder=ScribeFlow”。

## 5. 验收命令（已跑）

```bash
# Rust
cd src-tauri && cargo test --lib

# 前端
npm run build

# 夹具 + CDP（实现后补脚本）
node tools/make-studio-fixture.mjs D:/Develop/yebian-studio-fixture
node --no-warnings tools/verify-studio.mjs
```

## 6. 诚实边界

- 当前 ScribeFlow 的 Obsidian 输出不带 `id`/sidecar；制作台会降级到路径 id 和 front matter，运行/成本字段为空。
- 制作台只扫描 `ScribeFlow/`；用户在别处生成的笔记不会出现。
- 本机连接状态只探测端口，不代表认证/权限可用。
- 没有真正的云同步；`SF-003` 之前不要把 ScribeFlow 服务暴露到公网。
- 本轮验收结果：**9/9**（制作台侧栏、3 个 artifact、详情、预览、打开笔记、来源条、右侧来源卡、待入库状态）；Rust 测试 **27 passed**；夹具为 `tools/make-studio-fixture.mjs`，截图 `shots/studio-01-overview.png`、`shots/studio-02-note-provenance.png`。
- SF-001 已在 ScribeFlow 发布路径实现：目标库有 `.yebian-vault` 时，写稳定 `id` + `<note>.md.artifact.json`；连续两次运行保持 `noteId`、revision 从 1 到 2；普通 Obsidian 库不写 sidecar。ScribeFlow `engine.obsidian.test.ts` **45/45** 通过，完整 server 套件 **521/521**。
- 旧的 ScribeFlow 输出（本功能之前生成的）仍然没有 `id`/sidecar，制作台会按 front matter 降级；重新跑一次该工程后才会产生。
