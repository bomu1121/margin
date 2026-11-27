# 可整合的开源项目调研（2026-11-27）

> 结论先行：**最值得先做的是 MCP 客户端 + MCP 服务端**；其次是**本地混合检索**；离线 ASR、OCR、文档解析属于第二梯队。白板、CRDT 同步、图数据库不建议现在换。
>
> 这份文档里的 GitHub 数据是 2026-11-27 通过 GitHub API 读取的当时值；星数只作活跃度参考，不代表质量。工程量是**按现有代码结构估的**，不是实测工时；所有未跑过的验证都在最后一节列出。

## 1. 结论

### 推荐顺序

| 优先级 | 方向 | 一句话理由 | 工程量（估算） |
| --- | --- | --- | --- |
| **P0** | **MCP 客户端 + 服务端** | 一个协议把浏览器、文件、文档、检索、外部 SaaS 都变成“可插工具”；页边本身也能被 Claude Desktop / Cursor 当数据源 | 客户端 MVP 5–8 天；服务端 3–5 天 |
| **P1** | **本地混合检索** | 现有 FTS5 只懂字面；笔记/出处/史料库需要语义召回，而且要离线 | 1–2 周（含 20 查询基准） |
| **P2** | **离线 ASR** | 视频笔记是主链路，转写是持续成本；本地模型可离线、可批量 | 1–3 周（含 CER/RTF 基准） |
| **P2** | **本地 OCR** | 截图、扫描 PDF、白板图片目前只能靠外部模型；RapidOCR 可以本地补 | 3–7 天 |
| **P3** | **文档解析（PDF/Office）** | 现在 `pdf-extract` 只出纯文本；Docling/GROBID 能保留结构 | 走 MCP 3–5 天；原生打包 2–4 周 |
| **P3** | **Typst 导出 PDF/EPUB** | 目前只有 Markdown 导出；Typst 是 Rust 原生、许可干净 | 3–5 天 |

### 不建议现在做

- **用 Excalidraw/tldraw 重做白板**：Excalidraw 是 React，现有白板是 Vue 自绘；tldraw 是自定义商业许可。收益不够覆盖迁移成本。
- **用 CRDT（Loro/Automerge/Yjs）替换现有同步**：现有 HLC op log 已经工作，替换是数据迁移级风险。
- **引入 Kuzu**：仓库已 archived（最后 push 2025-10-10）。
- **MinerU 作为默认 PDF 解析**：Apache-2.0 之外还有附加商业条款，需要先读条款再决定。
- **把 AGPL 服务直接塞进安装包**：RSSHub/Karakeep/Linkwarden 更适合当外部服务，用 API 接。

## 2. 调研口径

- 数据源：GitHub API `/repos/{owner}/{repo}`，读取 `stargazers_count`、`license.spdx_id`、`pushed_at`、`archived`；许可证有歧义的再读 `LICENSE` 原文。
- 代码背景：Tauri 2 + Rust 后端 + Vue 前端；已有 SQLite/FTS5、Markdown vault、同步 op log、`note_engine` 工具循环、插件 Worker/iframe。
- 评估维度：**对页边的直接价值**、**与 Rust/Tauri/离线的契合度**、**接入成本**、**许可证/运维风险**。
- 没有做性能基准；所有“几天/几周”都是工程量估算，不是实测。

## 3. 页边现在能接哪些面

| 面 | 现状 | 适合接什么 |
| --- | --- | --- |
| `production/note_engine.rs` | 固定 6 个工具：read/write/edit/list/bash/fetch_url，可选 search_web | MCP 动态工具、文档解析、OCR |
| SQLite `notes` + FTS5 | 字面检索，bigram | 向量检索、混合排序、rerank |
| `notes` / `note_citations` / `facts` | 结构化笔记、出处、事实 | MCP 资源、语义索引、导出 |
| 本地史料库 | 36 万段、FTS + 接续阅读 | 语义检索、引用候选 |
| 插件系统 | Worker 命令 + iframe 面板 | MCP 配置页/工具授权页 |
| 同步 | 自研 op log + HLC | 不建议换 CRDT；可作为 MCP 服务端数据源 |

## 4. 方向矩阵

星数和最后 push 为 2026-11-27 GitHub API 读数；“接入方式”是建议，不是现状。

| 方向 | 代表项目 | 许可 | ★ | 最后 push | 建议接入方式 | 工程量 |
| --- | --- | --- | ---: | --- | --- | --- |
| MCP 客户端/服务端 | `modelcontextprotocol/rust-sdk` (rmcp) | Apache-2.0 / MIT 过渡 | 4,151 | 2026-11-26 | Rust crate | M |
| MCP 参考工具 | `modelcontextprotocol/servers` | 混合（逐 server 看） | 92,292 | 2026-11-25 | 外部 stdio/HTTP server | S |
| 浏览器工具 | `microsoft/playwright-mcp` | Apache-2.0 | 40,194 | 2026-11-18 | MCP server | S |
| 本地向量检索 | `asg017/sqlite-vec` | Apache-2.0 | 8,289 | 2026-05-18 | SQLite 扩展 | M |
| 本地 embedding | `Anush008/fastembed-rs` | Apache-2.0 | 1,060 | 2026-11-22 | Rust crate + ONNX | M |
| 嵌入式向量库 | `lancedb/lancedb` | Apache-2.0 | 11,995 | 2026-11-27 | Rust crate（另建索引） | M |
| 离线 ASR | `k2-fsa/sherpa-onnx` | Apache-2.0 | 16,270 | 2026-11-27 | `sherpa-rs`/C API | M–L |
| 离线 ASR | `ggml-org/whisper.cpp` | MIT | 55,573 | 2026-11-26 | `whisper-rs` | M–L |
| 快速 ASR | `SYSTRAN/faster-whisper` | MIT | 27,052 | 2026-11-25 | Python sidecar | L |
| 中文 ASR | `modelscope/FunASR` | MIT | 21,144 | 2026-11-26 | Python sidecar | L |
| 文档解析 | `docling-project/docling` | MIT | 71,128 | 2026-11-26 | MCP/CLI（Python） | S（MCP）/ L（原生） |
| 文档解析 | `datalab-to/marker` | Apache-2.0 | 41,087 | 2026-11-26 | CLI/服务 | L |
| 文档解析 | `opendatalab/MinerU` | Apache-2.0 + 附加条款 | 84,400 | 2026-11-26 | 外部服务 | L |
| 学术 PDF | `grobidOrg/grobid` | Apache-2.0 | 5,248 | 2026-11-26 | Java 服务 | M |
| OCR | `RapidAI/RapidOCR` | Apache-2.0 | 8,638 | 2026-11-27 | ONNX Runtime | M |
| OCR | `PaddlePaddle/PaddleOCR` | Apache-2.0 | 93,351 | 2026-09-16 | Python/ONNX | M–L |
| 网页正文 | `mozilla/readability` | Apache-2.0 | 11,563 | 2026-08-04 | JS/Node 或移植 | S |
| 导出 PDF/EPUB | `typst/typst` | Apache-2.0 | 57,454 | 2026-11-26 | Rust crate | S–M |
| 导出 | `jgm/pandoc` | GPL-2.0 | 47,100 | 2026-11-26 | 外部二进制 | S |
| RSS/稍后读 | `miniflux/v2` | Apache-2.0 | 9,981 | 2026-11-27 | 外部服务 API | M |
| RSS 路由 | `DIYgod/RSSHub` | AGPL-3.0 | 47,413 | 2026-11-27 | 外部服务 | M |
| 书签/稍后读 | `karakeep-app/karakeep` | AGPL-3.0 | 30,876 | 2026-11-25 | 外部服务 API | M |
| 书签/稍后读 | `linkwarden/linkwarden` | AGPL-3.0 | 20,543 | 2026-11-26 | 外部服务 API | M |
| 白板 | `excalidraw/excalidraw` | MIT | 136,384 | 2026-11-27 | React 组件/iframe | L |
| 白板 | `tldraw/tldraw` | 自定义许可 | 51,619 | 2026-11-27 | React SDK | L（许可风险） |
| CRDT | `loro-dev/loro` | MIT | 6,329 | 2026-11-26 | Rust crate | L（迁移风险） |
| CRDT | `automerge/automerge` | MIT | 6,759 | 2026-11-26 | Rust crate | L |
| 图数据库 | `kuzudb/kuzu` | MIT | 4,019 | **2025-10-10 archived** | 不采用 | — |
| 图数据库 | `oxigraph/oxigraph` | Apache-2.0 | 2,067 | 2026-11-26 | Rust crate | M |
| 本地 LLM | `ollama/ollama` | MIT | 188,272 | 2026-11-26 | HTTP provider | M |
| 本地 LLM | `ggml-org/llama.cpp` | MIT | 136,444 | 2026-11-27 | Rust/C++ | L |
| Agent 评测 | `promptfoo/promptfoo` | MIT | 27,734 | 2026-11-27 | 外部 CLI | S |

## 5. 重点方向拆解

### 5.1 MCP 客户端 + 服务端（首选）

**项目**：`modelcontextprotocol/rust-sdk`（rmcp），官方 Rust SDK，4,151★，2026-11-26 仍在更新；许可证是 MIT → Apache-2.0 过渡，README 说明新贡献 Apache-2.0、未重新授权的旧贡献仍是 MIT。它实现 MCP `2026-07-28` 规范，兼容 `2025-11-25`，支持 stdio / HTTP transports、client/server、tools/resources/prompts。

**为什么适合页边**

- 页边已经有 `note_engine` 工具循环，MCP 客户端只是把“工具来源”从固定数组换成 server 发现；不需要重写 agent。
- 页边本身就是数据源：笔记、出处、事实、史料库都可以通过 MCP 服务端暴露给外部 agent。
- 接 MCP 后，文档解析、OCR、浏览器、Git、数据库这些能力都可以先用现成 server，不必逐个原生打包。比如 Playwright MCP（40,194★，Apache-2.0）直接给 agent 浏览器操作；官方 reference servers（92,292★）覆盖 filesystem/git/fetch/memory 等。

**客户端 MVP**

1. app data 下增加 `mcp.json`：server id、transport、command/args/env 或 URL/token、启用状态、允许的工具。
2. 启动时/运行前连接 server，`list_tools` 拿到 JSON Schema，转成 `ToolSpec`；`execute_tool` 按 `server/tool` 路由，超时、结果截断（比如 20k 字符）、错误隔离。
3. UI 第一版可以只做设置页：添加一个 stdio server（如 `npx @modelcontextprotocol/server-filesystem <dir>`），启用/禁用，查看工具列表。
4. 安全：server 是任意进程，必须显式启用；env 不写日志；工具名/描述展示给用户；第一次调用确认；只允许配置的目录/网络范围。

**服务端 MVP**

- 工具：`search_notes(query, limit)`、`read_note(id)`、`list_citations(note_id)`、`search_history_corpus(query)`；先只读。
- 传输：stdio（Claude Desktop/Cursor 配置）或仅 `127.0.0.1` 的 HTTP + bearer token；不暴露 vault 绝对路径。
- 价值：外部 agent 可以直接引用你的笔记和史料库，而不是复制粘贴。

**工程量**：客户端 MVP 5–8 天，服务端 3–5 天，完整权限/审计/设置 2–3 周（估算）。**风险**：MCP server 任意代码执行、协议版本演进、Windows 子进程管理、长任务/流式结果。

详细协议流、配置 schema、工具映射、服务端工具和代码触点见 [`MCP_DESIGN.md`](MCP_DESIGN.md)。

**验证计划**：接一个 Playwright MCP + 一个 filesystem MCP，跑一篇需要查网页和读本地文件的笔记；记录工具调用次数、失败率、单次延迟；不先追求多 server。

### 5.2 本地混合检索（第二优先）

**项目**：`sqlite-vec`（Apache-2.0，8,289★，最后 push 2026-05-18）+ `fastembed-rs`（Apache-2.0，1,060★，2026-11-22）。模型可选 `bge-small-zh-v1.5`（体积小、中文）或 `Qwen3-Embedding-0.6B`（质量更好、约 1.2GB，fastembed-rs 需要 `qwen3` feature）。

**为什么适合**

- 现有 FTS5 只能字面匹配；语义检索能解决“我知道意思但想不起词”的笔记/出处召回。
- 复用现有 SQLite，不引入独立数据库进程；适合离线、单机、可备份。
- 可以只索引 `notes` 的标题/正文/出处，不把整个 vault 塞进向量库。

**MVP**

1. 按标题/小节/段落切 chunk，存 `note_id / block_id / citation_id / text / embedding`。
2. `sqlite-vec` 建 `vec0` 表；`fastembed-rs` 本地生成向量；FTS5 BM25 与向量 cosine 用 RRF 合并。
3. 先做 20 条中文查询的离线基准：FTS vs 向量 vs 混合的 recall@5、p95 延迟、索引大小；再决定默认排序。
4. 不急着做 rerank；先看混合召回是否值得。

**工程量**：1–2 周（估算），主要花在 chunk 策略、索引重建和基准。**风险**：`sqlite-vec` Rust crate 还是 `0.1.10-alpha`；向量表是暴力搜索，chunk 超过十万级要重新评估；模型下载和 CPU 占用；Qwen3 模型更大。

**不要先做**：Qdrant/Meilisearch 这类独立服务；单机笔记量不值得多一个进程。

### 5.3 离线 ASR（视频链路）

**项目**：`sherpa-onnx`（Apache-2.0，16,270★，支持 SenseVoice/Paraformer、VAD、说话人分离；Rust `sherpa-rs` 0.6.8）；`whisper.cpp`（MIT，55,573★，`whisper-rs` 0.16）；`faster-whisper`（MIT，27,052★，Python/CTranslate2）。

**为什么适合**：视频笔记是页边主链路，MiMo API 是持续成本且依赖网络；本地模型可以离线、批量、避免音频上传。

**MVP**：对 3 条已有视频（中文口播、多人、噪声各一）比较：CER、标点/人名、CPU RTF、内存、模型体积。先做“可选本地 provider”，保留 API 回退；模型不塞安装包，按需下载并校验 SHA-256。

**工程量**：1–3 周（估算）。**风险**：SenseVoice 的中文标点/长音频切分要调；whisper large-v3 模型 1.5GB、CPU 慢；sherpa-rs 需要编译 ONNX Runtime 相关依赖。

### 5.4 文档解析 + OCR

**文档**：`docling`（MIT，71,128★）适合通用 PDF/Office→Markdown，保留标题/表格/阅读顺序；`marker`（Apache-2.0，41,087★）偏 PDF→Markdown；`MinerU`（84,400★）中文 PDF 强，但 Apache-2.0 之外有附加商业条款；`GROBID`（Apache-2.0，5,248★）专攻学术 PDF 的元数据/参考文献。

**OCR**：`RapidOCR`（Apache-2.0，8,638★）是 ONNX 轻量方案，适合截图/扫描页；`PaddleOCR`（Apache-2.0，93,351★）更重但生态全。

**建议**：先通过 MCP 接 Docling/GROBID 的 server/CLI，不要原生打包 Python；OCR 先用 RapidOCR 做“图片→文字”小工具，服务出处补全/白板图片说明。

**工程量**：MCP 方式 3–5 天；原生 Docling 2–4 周（估算）。**风险**：Python/Java 运行时、GPU、扫描件、中文竖排/表格；MinerU 条款。

### 5.5 导出：Typst

`typst`（Apache-2.0，57,454★）是 Rust 原生排版系统，可做 Markdown→PDF/EPUB 的高质量导出；比 Pandoc（GPL-2.0）更适合直接依赖。当前只有“导出 Markdown”，加 PDF/EPUB 对分享/归档是直接价值。

**工程量**：3–5 天（估算），主要花在中文衬线字体嵌入、标题/表格/引用模板。**风险**：中文字体授权与体积；复杂表格/脚注。

## 6. 按你的实际需求选

| 你想要的 | 首选 | 次选 |
| --- | --- | --- |
| Agent 能接更多工具/数据 | **MCP 客户端** | MCP 服务端 |
| 让 Claude/Cursor 读页边笔记 | **MCP 服务端** | — |
| 搜笔记/出处更准 | **本地混合检索** | 本地 rerank |
| 转写不花钱/能离线 | **sherpa-onnx / whisper.cpp** | faster-whisper |
| 吃 PDF、扫描件 | **Docling/GROBID（MCP）** | MinerU（先读条款） |
| 图片/截图取字 | **RapidOCR** | PaddleOCR |
| 导出好看 PDF/EPUB | **Typst** | Pandoc（外部） |
| 收集博客/RSS | Miniflux（Apache-2.0） | RSSHub（AGPL，外部服务） |
| 白板协作 | 暂缓 | Excalidraw（MIT，但迁移大） |
| 多人实时同步 | 暂缓 | Loro/Automerge（迁移风险） |

## 7. 不建议/暂缓

- **Kuzu**：GitHub 已 archived（最后 push 2025-10-10），不要作为新依赖。
- **tldraw**：许可证是自定义商业条款，不是 OSI 开源；生产使用需另谈。
- **Recogito**：`recogito-js` 已 archived/deprecated，不适合作为新标注库。
- **MinerU**：Apache-2.0 附加商业条款，商用/分发前必须读原文。
- **CRDT 替换同步**：现有 HLC op log 已在多设备跑通；换 Loro/Automerge 需要数据迁移、冲突语义、服务端改造，收益主要是协作，不是当前刚需。
- **Excalidraw 重做白板**：现有白板已经和 agent 卡片/研究流程耦合，React 组件迁移成本高。
- **AGPL 服务内置**：RSSHub/Karakeep/Linkwarden 用外部服务 + API 更稳；不要直接打包。

## 8. 验证计划（还没跑）

| 方向 | 最小实验 | 要记录的数字 |
| --- | --- | --- |
| MCP 客户端 | 接 filesystem + Playwright MCP，跑一篇需要网页+本地文件的笔记 | 工具调用数、失败率、单次 p50/p95、token 增量 |
| MCP 服务端 | Claude Desktop/Cursor 连本地只读 server，搜 3 篇笔记 | 首次连接时间、工具成功率、返回大小 |
| 混合检索 | 39 篇笔记 + 20 条中文查询 | FTS/向量/混合 recall@5、p95、索引 MB |
| 离线 ASR | 3 条视频 | CER、RTF、内存、模型 MB |
| OCR | 20 张中文截图/扫描页 | 字符准确率、延迟、模型 MB |
| Docling | 5 份 PDF（中英/双栏/扫描） | 标题/表格保真、耗时 |
| Typst | 3 篇不同结构笔记 | 页数、字体体积、构建时间 |

## 9. 诚实清单

- **没有跑任何上述集成原型**；本文是选型调研，不是实测报告。
- GitHub ★/push 是 2026-11-27 API 读数，会变；不能当性能/安全证明。
- 工程量是估算，按现有代码结构粗估，未做任务拆解和排期。
- 许可证只读了仓库 `LICENSE`/API；没有做法律审查，尤其 MinerU 附加条款、AGPL 服务、模型权重许可（BGE/Qwen/SenseVoice）需要单独确认。
- 没有评估 Windows 打包后的体积/签名/杀软影响；本地 ASR/OCR/Docling 都会显著增加安装包或首次下载。
- 没有评估多用户/多设备场景；MCP 服务端默认按单机只读设计。
