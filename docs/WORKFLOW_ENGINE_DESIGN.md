# 工作流引擎：把「提示词模板」换成可观察的执行链路

> 2026-11-17 · 课题：能否把会话 `sess_320b28bf-…` 那套「写 → 渲染 → 看 → 打补丁」的流程，做成项目里的新链路，并且能看到整套工作流的执行过程
> **结论：可行，但这不是「再加一个模板」。要做的是四层基础设施：工作流是数据、运行有存储、执行器能分支和循环、界面能看每个节点的输入输出。** 不含视觉自愈约 3–4 周；含视觉再加 1 周。最大风险不是模型，是运行图 UI 的成本。

---

## 1. 现在的链路差在哪（代码级）

| 事实 | 位置 | 影响 |
| --- | --- | --- |
| 一个 1,330 行的 `run()` 顺序函数，模板靠 `if template_id == ...` 分支（6 处） | `pipeline.rs:125-` | 加一步就是改 Rust；无法声明式扩展 |
| 步骤是扁平列表：`index/key/label/status/detail` | `mod.rs:273` | 没有输入、输出、尝试次数、耗时、成本 |
| 中间产物只写日志 | 实跑 P111：转写/校对/成稿全在 `production_logs` | 无法对产物做 diff、重跑、验收 |
| 成功任务清 checkpoint | P111 实测 `production_checkpoints` 为空 | 运行结束即失忆 |
| UI 是「步骤列表 + 日志」，1.5s 轮询 | `ProductionTaskDialog.vue`（862 行，4 个 tab） | 看不到图、看不到门禁、看不到产物 |
| 没有渲染/测量节点 | 全仓 | 只能生成文本，无法验证排版 |

一句话：现在是「一条流水线跑到底，结果写进日志」；要的是「一张可暂停、可回放、每步都有产物的图」。

## 2. 业界已经有的形态（研究结论）

- **LangGraph Studio**：把图渲染成实时图，逐节点步进，检查每个 checkpoint 的状态，并能从任意节点重放。这是「可观察工作流」最接近的参照。
- **Temporal**：持久执行，工作流历史可回放，但只到步骤级，没有 LLM/工具级细节。
- **产物仓库**：大输出按 hash/URI 存文件，不塞进运行状态；这是长文/截图/PDF 的唯一可行做法。
- **OpenTelemetry GenAI 约定**：每个节点、每次工具调用一个 span，token/成本挂在 span 上。

可借的三条：**图 + 节点检查器**（Studio）、**持久与重放**（Temporal）、**产物按内容寻址**（通用做法）。不需要引入它们的运行时，概念照搬即可。

## 3. 目标架构

### 3.1 工作流是数据

```json
{
  "id": "booklet",
  "nodes": [
    { "key": "source",  "kind": "source" },
    { "key": "asr",     "kind": "asr", "split": 60 },
    { "key": "refine",  "kind": "llm", "prompt": "校对" },
    { "key": "outline", "kind": "llm", "prompt": "大纲", "out": "outline.json" },
    { "key": "chapter", "kind": "llm", "each": "outline.chapters", "out": "chapter-{n}.md" },
    { "key": "figures", "kind": "llm", "out": "figures.json" },
    { "key": "assemble","kind": "assemble" },
    { "key": "build",   "kind": "render", "template": "book" },
    { "key": "measure", "kind": "measure", "gate": "strict" },
    { "key": "print",   "kind": "print" },
    { "key": "publish", "kind": "publish" }
  ],
  "edges": [
    ["source","asr"],["asr","refine"],["refine","outline"],["outline","chapter"],
    ["chapter","figures"],["figures","assemble"],["assemble","build"],
    ["build","measure"],["measure","print"],["measure","patch"],["patch","build"]
  ],
  "loops": { "measure->patch->build": { "max": 3 } }
}
```

节点有**类型化的端口**：`llm` 产 Markdown/JSON，`render` 产 HTML，`measure` 产指标 JSON，`print` 产 PDF。旧模板（video-note 等）编译成线性图，行为不变。

### 3.2 运行存储

新增三张表，`production_steps` 保留为兼容视图：

| 表 | 关键字段 |
| --- | --- |
| `run_nodes` | run_id, node_key, status, attempt, started_at, ended_at, tokens, cost, error, **inputs_json**, **outputs_json** |
| `run_artifacts` | id, run_id, node_key, kind (md/html/png/pdf/json), path, bytes, sha256, meta_json |
| `run_gates` | run_id, node_key, gate, passed, metrics_json |

产物落在 `.yebian/runs/<run_id>/<node>/` 下；HTML/PNG/PDF 不写库，库里只存路径与 hash。成功运行也保留（与现在的 checkpoint 清理策略分开）。

### 3.3 执行器

- 拓扑推进，节点级重试；
- `gate` 返回值决定走哪条边（`measure` 失败 → `patch`）；
- `loop` 有次数上限，超限落 `stopped` 并保留全部尝试；
- 每个节点开始/结束都写 `run_nodes` 并发事件，前端无需轮询也能更新（现有 14 处 emit 可复用）。

### 3.4 渲染节点（最大工程点）

Rust 里起本机 Edge headless：

1. 按 ffmpeg 的模式发现 `msedge.exe`（PATH → Program Files → 注册表），解析一次并缓存；
2. 起 `--headless=new --remote-debugging-port=0`，从 stderr 解析端口；
3. 自己写一个最小 CDP 客户端（`tokio-tungstenite`，约 300–400 行）：`Page.navigate` / `Runtime.evaluate` / `Page.captureScreenshot` / `Page.printToPDF`；
4. 测量脚本复用 `tools/booklet/measure.mjs` 里的表达式（指标口径已经定好）。

备选：隐藏 WebView2 窗口（不启外部进程），但 WebView2 的 `PrintToPdf` 在 wry 里没有现成封装，风险更高。

### 3.5 界面：能看到整套流程

```
┌ 运行 · booklet · run_xxx ────────────────────────────── 12:41 · ¥0.42 ┐
│  source → asr → refine → outline → chapter×7 → figures → assemble      │
│         → build → measure ─┐                                           │
│                    ↑       │                                           │
│                    └ patch ┘  (第 2/3 轮)                              │
├───────────────────────────────┬────────────────────────────────────────┤
│ 节点详情 · measure            │ 产物                                   │
│ 状态 失败 → 重跑 2 次          │ book.html 306 KB                       │
│ 输入 assemble.md              │ book.pdf  501 KB / 17 页               │
│ 输出 metrics.json             │ fig-03.png                             │
│ 门禁 overflow 0 / clip 0      │                                        │
│ [查看指标] [从这步重跑]        │ [打开] [下载] [对比上一轮]              │
└───────────────────────────────┴────────────────────────────────────────┘
```

必须有的四件事：
1. **图**：节点状态着色、边上的条件、循环次数；
2. **节点抽屉**：输入/输出 artifacts、日志、指标、tokens、耗时、尝试次数；
3. **重放**：从任一节点重跑，下游按 hash 失效；
4. **对比**：同一节点两次尝试的输出 diff（文本 diff；图给并排）。

## 4. 工作量与可行性

| 模块 | 规模 | 风险 |
| --- | --- | --- |
| 工作流 spec + 执行器（Rust） | ~800–1,100 行 | 中：要把现有 `if template` 分支迁进去 |
| 运行存储 + 产物（Rust/SQLite） | ~500–700 行 | 低 |
| 渲染节点（Edge + CDP，Rust） | ~400–500 行 | **中高**：CDP 客户端要自己写，Windows 进程管理要稳 |
| 运行图 + 节点检查器（Vue） | ~800–1,200 行 | **高**：这是用户每天看的界面 |
| 旧模板迁移 + 兼容视图 | ~300 行 | 中：订阅、续跑、成本不能回归 |
| 视觉检查节点 | ~300 行 + 模型成本 | 取决于是否接视觉模型 |

合计约 3,000–3,800 行。按本项目现有节奏（P1xx 一轮 1–3 天），**4 期、3–4 周**是合理估计；其中 UI 占一半。

**可行性判断**：技术上没有硬阻塞（不引入新运行时、不依赖 Node sidecar、不打网络服务），真正的成本在「图 + 检查器 + 重放」这套 UI，以及 Rust 侧 CDP 的稳定性。

## 5. 分期与验收

### W0 产物与运行存储（**已实现，2026-11-17**）

- 新增 `production_run_nodes` / `production_run_artifacts` 两张表；`mark()` 同步写节点，`node_finish` 按用量基线算 **token 与成本增量**。
- 产物落 `.yebian/runs/<run>/<node>/` 并登记 sha256；成功运行也保留。已接四条路径：source.json、transcript-NN.md、refined-NN.md、publish/note.md。
- 新命令 `production_run_graph`、`production_artifact_text`；任务详情新增「流程」tab：横向节点带 + 节点列表 + 文本产物预览。
- 实测：230 字文稿 4 节点 33s（refine 1,147 tok、prompt 6,243 tok），3 个产物 10 KB；`cargo check` 0 error、`npm run build` 通过。

### W1 声明式工作流 + 执行器（**booklet 模板已落地，执行器仍是命令式**）

- spec 存 DB；旧模板编译成线性图；执行器支持顺序、门禁、循环。
- **验收**：`video-note` 与 `long-note` 跑出的结果与现在逐字节一致；任务详情显示图而不是步骤列表；门禁失败能走回边。

### W2 渲染与测量节点（**渲染器 + 后处理节点已实现，未接入执行器**）

- 已做：`production/render.rs` 发现本机 Edge，注入测量脚本后用 `--dump-dom` 取指标、`--print-to-pdf` 出 A4 PDF、`--screenshot` 截全页；前端「信息 → 小册子」一键调用，并把 build/measure/print 三个节点追加进任务的运行图。测量当门禁：不过则 print 记 skipped 并清掉旧 PDF。实测 230 字笔记 5 页、7 节点 7 产物、四项指标全 0；注入超宽元素可让门禁失败。
- 未做：把它包成工作流节点（`build`/`measure`/`print`），以及失败时的回边。
- **验收**：同一篇小册子，测量指标与 `tools/booklet/run.mjs` 一致（0 溢出、17 页）；失败时能看到指标 JSON。

### W3.5 视觉自检智能体（**几何版已实现**）

- 已做：测量脚本检测空框、标签重叠、文字超出容器、SVG 越界；`bookAgent.ts` 跑「渲染 → 出问题 → 模型给一个 patchFigure/replaceSection → 再渲染」，最多 4 轮，问题未清空拒绝结束；保存走 `note_write_body`（备份 + 保留 front matter）。
- 实测：注入 5 长名称 layers 图 → 8 个问题 → 一次改图 → 0；注入两个 35 字节点 → 1 个越界 → 0，导出 18 页 0 违规。
- 未做：视觉模型（不是看图，是几何启发式）；CSS 级修改；独立 judge。

### W3 图 + 检查器 + 重放（**部分实现**）

- 已做：节点带、每节点耗时/用量/成本、产物列表与文本预览、运行中每 1.5s 刷新。
- 未做：真正的有向图（边/条件/循环）、产物对比、从节点重跑、失败回边。
- **验收**：改坏一张图 → measure 失败 → 从 `patch` 重跑 → 下游重建 → 图上能看到两条尝试。

### W4 视觉检查（可选，3–5 天）

- 切片交视觉模型 → 结构化问题 → patch 节点重写 spec。
- **验收**：复现会话里 9 类看图缺陷中的至少 5 类。

## 6. 风险与不做

- **UI 被低估**：图 + 抽屉 + diff 是 800 行以上的新界面，必须先做低保真再写。
- **CDP 稳定性**：Edge 版本变化、端口、超时；要有「无法渲染」的明确状态，不能假通过。
- **幂等**：从节点重跑要按产物 hash 失效下游，否则会重复扣费。
- **兼容**：订阅、续跑、成本统计、清理策略都不能回归，`production_steps` 保留兼容视图。
- **不做**：可视化搭建工作流（拖拽画布）、跨设备运行、多租户。

## 7. 诚实清单

- 工作量是**估算**：按现有文件规模（`pipeline.rs` 1,330 行、`ProductionTaskDialog.vue` 862 行）外推，不是实测。
- LangGraph Studio / Temporal 的能力来自公开资料，**没有实际使用过**它们。
- Rust 侧 CDP 与 Edge 发现**没有写任何代码验证**；「约 300–400 行」是经验估计。
- 视觉节点依赖尚未决定的视觉模型；`deepseek-v4-flash` 是否支持图片输入仍未验证。
- 本文只做设计，没有改动任何执行代码。
