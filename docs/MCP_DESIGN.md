# MCP 客户端 + 服务端：实现原理与对页边的具体优化

> 结论：MCP 不是“再写一个插件系统”，而是把**工具/数据提供方**从页边代码里解耦。客户端让 `note_engine` 能调用外部 server 的工具；服务端让 Claude Desktop、Cursor 等外部 agent 能直接检索页边的笔记、出处和史料库。两件事共用一套协议，但信任边界完全不同。
>
> **实现状态（2026-11-27，P255/P256）**：客户端已落地并有 **8** 个单测；设置页新增「MCP 连接」编辑/测试入口。P256 已补真实 AI 成稿、白板研究、HTTP transport、超时/取消和真实 B 站页验证，详见 [`MCP_VALIDATION.md`](MCP_VALIDATION.md)。页边自己的只读服务端（本文 §4）**尚未实现**，所以目前只能“页边调用外部 server”，不能“外部 agent 查询页边”。

## 1. 一页纸

```
                 页边（MCP Host）
┌──────────────────────────────────────────────┐
│  note_engine 工具循环                         │
│    ├─ 本地工具：read/write/edit/bash/fetch    │
│    └─ MCP 工具：mcp__<server>__<tool>         │
│              │                               │
│        McpManager（连接/发现/调用/超时）       │
│         │ stdio                    │ HTTP     │
│         ▼                          ▼          │
│   Playwright MCP              Docling MCP     │
│   filesystem MCP              OCR MCP         │
└──────────────────────────────────────────────┘

外部 agent（Claude Desktop / Cursor）
        │ MCP HTTP/stdio
        ▼
┌──────────────────────────────────────────────┐
│ 页边 MCP Server（只读）                       │
│  search_notes / read_note / list_citations   │
│  search_history_corpus                       │
│        │                                     │
│  现有 SQLite / FTS5 / vault                   │
└──────────────────────────────────────────────┘
```

对页边的直接优化：

1. **agent 工具从 6 个固定工具变成可扩展工具集**：加一个 server 就能加浏览器、PDF、数据库、OCR，不用改 Rust、不用发版。
2. **不把所有开源项目打进安装包**：Docling、Playwright、OCR 这些用外部 server 跑，页边只做协议适配。
3. **页边从“笔记应用”变成“可被其他 agent 查询的本地知识源”**：外部 agent 能读笔记、出处、史料库，而不是复制粘贴。
4. **复用现有 agent 循环、任务日志、取消机制**：MCP 是工具层，不替换 `note_engine`。

## 2. MCP 协议原理

MCP（Model Context Protocol）是 JSON-RPC 2.0 之上的协议，定义三种角色：

- **Host**：用户正在用的应用，负责权限、UI、把多个 client 组织起来。页边就是 host。
- **Client**：host 内部与某个 server 的一对一连接。页边的 `McpManager` 为每个 server 建一个 client。
- **Server**：提供工具/资源/提示词的外部程序，可以是本地子进程，也可以是 HTTP 服务。

### 2.1 生命周期

1. client 发 `initialize`：协议版本、client 能力、clientInfo。
2. server 返回协议版本、server 能力、serverInfo。
3. client 发 `notifications/initialized`。
4. 正常请求：`tools/list`、`tools/call`、`resources/list`、`resources/read`、`prompts/list`、`prompts/get`。
5. 关闭：stdio 关 stdin / 杀进程；HTTP 结束 session。

能力是双向声明的：server 声明自己支持 `tools/resources/prompts/logging/completions` 等；client 声明自己支持 `sampling/roots/elicitation` 等。页边 v1 只需要 client 的 tools 能力，server 端只需要 tools + resources。

### 2.2 传输

- **stdio**：client 启动子进程，JSON-RPC 消息按行写 stdin/stdout；stderr 是日志。适合本地 server（`npx`、`uvx`、独立二进制）。
- **Streamable HTTP**：POST 发 JSON-RPC，响应可以是 JSON 或 SSE 流；适合常驻服务、多客户端、远程 server。
- **旧 HTTP+SSE**：兼容老 server；新实现优先 Streamable HTTP。

页边 v1 建议：

- 客户端先做 **stdio + Streamable HTTP**；stdio 是本地工具的主力。
- 服务端先做 **localhost HTTP + bearer token**；stdio 模式后续再加，给 Claude Desktop 这种只支持 command 的客户端用。

### 2.3 工具调用

`tools/list` 返回：

```json
{
  "name": "browser_navigate",
  "description": "Open a URL in the browser",
  "inputSchema": {
    "type": "object",
    "properties": { "url": { "type": "string" } },
    "required": ["url"]
  }
}
```

`tools/call` 返回 content blocks：

```json
{
  "content": [
    { "type": "text", "text": "..." },
    { "type": "image", "data": "...", "mimeType": "image/png" }
  ],
  "isError": false
}
```

工具执行失败应该用 `isError: true` + 错误文本，而不是 JSON-RPC 协议错误；协议错误只用于请求本身不合法。

### 2.4 资源与提示词

- **Resources**：按 URI 读取的数据，例如 `note://yn-xxx`、`history://shiji/卷1`。适合把笔记/史料作为“可读数据”暴露，而不是包装成工具。
- **Prompts**：预置提示词模板，适合“总结这篇笔记”“找出处”这类固定动作。页边 v1 可以先不做 prompts。
- **Sampling**：server 反向请求 client 调模型；页边 v1 不开放，避免外部 server 偷用模型额度。
- **Elicitation**：server 请求用户输入；v1 不开放，避免 UI 被外部 server 控制。

## 3. 页边作为 MCP 客户端

### 3.1 目标

让 `note_engine` 在一次笔记任务中，除了现有 `read_file / write_file / edit_file / list_dir / bash / fetch_url / search_web`，还能调用外部 MCP server 的工具。

典型场景：

- **浏览器**：Playwright MCP 打开 JS 渲染页面、点击、截取可读文本；`fetch_url` 只能拿静态 HTML。
- **文档**：Docling MCP 把 PDF/Office 转成 Markdown，保留标题/表格；页边不再自己解 PDF。
- **数据库**：SQLite/Postgres MCP 让 agent 查结构化数据。
- **OCR**：RapidOCR MCP 把截图/扫描页转文字。
- **外部 SaaS**：GitHub、Slack、Notion 等 MCP server，按需接。

### 3.2 组件

```
src-tauri/src/mcp/
├── mod.rs          // McpManager、配置、生命周期
├── client.rs       // rmcp client 封装：connect/list/call/close
├── config.rs       // mcp.json 校验、server 配置
└── tools.rs        // MCP Tool -> 本地 ToolSpec 映射
```

配置放在 app data，例如 `%APPDATA%/com.yebian.notes/mcp.json`：

```json
{
  "servers": [
    {
      "id": "playwright",
      "transport": "stdio",
      "command": "npx",
      "args": ["-y", "@playwright/mcp@latest"],
      "env": {},
      "enabled": true,
      "timeoutMs": 30000,
      "tools": { "allow": ["browser_navigate", "browser_snapshot"] }
    },
    {
      "id": "docling",
      "transport": "http",
      "url": "http://127.0.0.1:8765/mcp",
      "authToken": "<token>",
      "enabled": false,
      "tools": { "allow": ["convert_document"] }
    }
  ]
}
```

约束（已按此实现）：

- `id` 只允许 ASCII 字母/数字/`.`/`_`/`-`，最长 64；重复 id 会被拒绝，server 配置不写日志。
- `env` 里的密钥不进入 job 日志、不进入模型上下文；`authToken` 只用于 HTTP 的 Authorization，不打印。
- `tools.allow` **为空表示不暴露任何工具**；放行全部必须显式写 `"*"`。默认不自动放行所有工具。
- 外部 server 默认关闭（`enabled: false`），用户显式启用。

实际配置支持字段：`id`、`transport`（`stdio` / `http`）、`command`、`args`、`env`、`url`、`authToken`、`enabled`、`timeoutMs`、`tools.allow`。HTTP 的 `headers` 尚未支持。

### 3.3 工具发现与合并

`McpManager` 在任务开始时：

1. 读取启用的 server。
2. 连接并 `initialize`。
3. 调 `tools/list`，把每个工具变成内部结构：

```rust
struct McpTool {
    server: String,
    name: String,          // 原始工具名
    description: String,
    input_schema: Value,
}
```

4. 生成模型可见的 `ToolSpec`：

```text
name:        mcp__playwright__browser_navigate
description: [MCP: playwright] Open a URL in the browser
parameters:  原始 inputSchema（必要时做 JSON Schema 清洗）
```

命名空间 `mcp__<server>__<tool>` 解决重名；工具名总长度要限制（例如 64 字符），超长截断/哈希。

5. 与本地工具合并：本地工具保持原名，MCP 工具加前缀；模型看到的是一张统一工具表。

**上下文预算**：每个工具的描述 + schema 都占 token。v1 建议：

- 每个 server 最多暴露 20 个工具；
- 描述截断到 500 字符；
- 只暴露 allow 列表；
- 多个 server 时按任务类型启用，不要一次挂 100 个工具。

### 3.4 调用路由

`execute_tool_async()` 目前只特判 `search_web`；改造后：

```rust
if name.starts_with("mcp__") {
    return mcp_manager.call(name, args, token).await;
}
```

`call()`：

1. 解析 server/tool；
2. 检查 allow 列表；
3. `tools/call`，带超时（默认 30s，可配置）；
4. content blocks 转成本地 observation（已实现）：
   - `text` 直接拼接；
   - `image` / `audio` 只写类型和字节数占位，**没有落盘**，避免把 base64 塞进模型上下文；
   - `resource` 只写 URI 占位，`resourceLink` 也只写 URI；
5. 总输出截断到 20,000 字符（与现有工具一致）；
6. `isError` 转成 `MCP 工具返回错误：...`；
7. 相关日志只写 server 连接/失败和工具名冲突，不写配置里的密钥。

超时：每次 `tools/call` 默认 30s，可配 1–300s；超时返回 `MCP 工具超时（30s）`。**取消**：P256 起 `CancellationToken` 会传入 MCP 调用，`tokio::select!` 在取消时立即返回并退出任务；页边只能停止等待，外部 server 是否继续执行工具本身不在页边控制范围内。stdio server 在任务结束、连接对象 Drop 时由 rmcp 关闭子进程。

### 3.5 失败与降级

- server 连接失败：日志警告，**不阻断任务**；该 server 的工具不出现，本地工具继续。
- 工具调用失败：把错误作为 observation 返回给模型，让模型决定重试/换工具；不要直接让整个 job 失败。
- server 崩溃：标记不可用，当前任务不回连（避免循环）；下一任务再试。
- 超时：返回 `MCP 工具超时（30s）`；不要无限等。
- 输出过大：截断并标注 `…（已截断，原 N 字符）`。

### 3.6 UI（已实现第一版）

- 设置页新增「MCP 连接」：显示应用数据目录下的 `mcp.json` 路径、JSON 编辑区、保存、测试连接、逐 server 的发现工具/错误/耗时。
- 配置缺失时展示一份 `playwright` 禁用示例；保存后文件才出现，避免后台偷偷创建。
- 页面明确标注：`allow` 为空不暴露工具、`"*"` 才放行全部、stdio server 以当前用户权限运行任意程序。
- **未做**：任务日志里的 MCP 调用耗时、首次启用确认弹窗、工具勾选器。当前只能编辑 JSON。

任务运行页日志里显示 `mcp__playwright__browser_navigate · 1.2s` 仍是后续项。

## 4. 页边作为 MCP 服务端

### 4.1 目标

让外部 agent（Claude Desktop、Cursor、其他 MCP host）直接查询页边的本地知识，而不是导出 Markdown。

### 4.2 工具设计（只读 v1）

| 工具 | 参数 | 返回 |
| --- | --- | --- |
| `search_notes` | `query`, `limit` | `id / title / relPath / snippet / updatedAt` |
| `read_note` | `id` | `title / tags / body Markdown / citations`（截断 50k 字符） |
| `list_citations` | `noteId` | `quote / work / section / original / url / status` |
| `search_history_corpus` | `query`, `limit` | `work / section / text / url / passageId` |
| `get_note_outline` | `id` | 标题层级 + block id，便于外部 agent 定位 |

v1 **不提供写工具**。写入涉及版本、冲突、同步、用户确认；先把读打通。

### 4.3 传输与鉴权

- **localhost HTTP**：绑定 `127.0.0.1`，随机端口，bearer token（32 字节随机，存 app data，设置页可复制/轮换）。
- 只允许本机；不做 CORS；不监听 `0.0.0.0`。
- 速率限制：例如 60 请求/分钟；单次返回上限 1 MB。
- 审计：记录时间、工具、参数摘要、返回字节数；不记录笔记全文。
- **stdio 模式**：后续加 `yebian --mcp-stdio`，只读打开 vault DB，stderr 日志；给 Claude Desktop 用。

### 4.4 数据映射

- `search_notes` 复用现有 `Store::search_notes` / FTS5。
- `read_note` 复用 vault 解析 + `get_note`，返回 Markdown，不返回绝对路径。
- `list_citations` 读 `note_citations` 表；没有出处时返回空数组。
- `search_history_corpus` 复用现有本地史料检索；返回 work/section/passage，不返回数据库路径。
- 所有查询走只读连接；服务端不触发索引重建/同步写。

### 4.5 外部使用示例

Claude Desktop 配置（HTTP）：

```json
{
  "mcpServers": {
    "yebian": {
      "url": "http://127.0.0.1:61234/mcp",
      "headers": { "Authorization": "Bearer <token>" }
    }
  }
}
```

对话：

> 你：帮我找页边里关于“刘秀 昆阳”的笔记，并列出它的出处。
>
> Claude：调用 `search_notes` → `read_note` → `list_citations`，返回笔记摘要和出处列表。

这就把页边从“只能人看”变成“agent 可查的本地知识源”。

## 5. 对本项目的具体优化

### 5.1 现在 vs 接 MCP 后

| 场景 | 现在 | 接 MCP 后 |
| --- | --- | --- |
| 加一个新工具 | 改 `tool_specs` + `execute_tool` + Rust 发版 | 加一个 server 配置，重启任务即可 |
| 浏览器/JS 页面 | `fetch_url` 只拿静态文本 | Playwright MCP 打开、点击、取快照 |
| PDF/Office | `pdf-extract` 纯文本，4MB/20s 上限 | Docling/GROBID MCP 返回结构化 Markdown |
| 图片/扫描件 | 没有本地 OCR | RapidOCR MCP 转文字 |
| 外部数据库 | 不能查 | SQLite/Postgres MCP 工具 |
| 外部 agent 用页边 | 导出/复制 | MCP 服务端直接 `search_notes/read_note` |
| 新能力交付 | 每个都进安装包 | 外部 server 独立升级，页边只维护协议 |

### 5.2 和现有插件系统的区别

| | 插件（现有） | MCP（新增） |
| --- | --- | --- |
| 触发 | 用户点命令/面板 | 模型在 agent 循环里调用 |
| 运行位置 | sandboxed Web Worker | 外部进程/HTTP server |
| 权限 | `notes:read`，默认拒绝 | server 自己的权限；页边只做 allowlist |
| 能做什么 | 读当前笔记、返回 Markdown/面板 | 浏览器、文件、数据库、文档……取决于 server |
| 信任边界 | 插件代码在 Worker，拿不到 IPC | server 是任意代码，信任级别更高 |
| 关系 | 用户主动操作 | agent 自动调用 |

两者不冲突：插件适合“用户在笔记上点一下”的小工具；MCP 适合“agent 在任务中自己决定用”的外部能力。

### 5.3 具体收益（可验收）

- **减少发版**：新增一个 MCP server 不需要改 Rust；对比现在每加一个工具就要改 `tool_specs` + 重打安装包。
- **任务能力**：同一篇需要查网页的笔记，`fetch_url` 可能拿不到 JS 内容；Playwright MCP 可以拿到渲染后的文本/截图。验收看任务是否完成、引用 URL 是否可用。
- **文档链路**：PDF 不再只靠 `pdf-extract` 的纯文本；Docling MCP 能保留标题/表格，出处核对更容易。
- **外部互操作**：Claude Desktop/Cursor 可以直接查 39 篇笔记 + 本地史料库；验收看 3 个查询是否能返回正确 id/出处。
- **上下文成本**：工具描述/schema 也占 token；所以 v1 要 allowlist + 描述截断，不能“挂满所有 server”。

## 6. 安全模型

MCP server 不是沙箱。一个 stdio server 就是“以当前用户权限运行的任意程序”；HTTP server 也可能访问网络/文件。页边必须把它当高信任扩展。

控制项：

1. **显式启用**：默认没有 server；启用时展示 command/URL 和风险。
2. **工具 allowlist**：只暴露用户勾选的工具；默认空。
3. **进程隔离**：stdio 子进程独立；任务结束/超时杀掉；不允许 shell 拼接，`command/args` 分开传。
4. **输出限制**：20k 字符截断；图片落盘不进上下文；单次返回上限。
5. **网络策略**：v1 不做网络沙箱，但记录 server 配置；远程 HTTP 只允许用户显式添加。
6. **提示注入**：工具输出是数据，不是指令；system prompt 里明确；高风险工具（写文件/支付/发布）需要人工确认。
7. **服务端只读 + token + localhost**：外部 agent 不能写笔记、不能拿绝对路径；token 可轮换。
8. **审计**：job 日志记录 `server/tool/耗时/字节`；服务端记录请求摘要，不记录全文。

## 7. 实施阶段（估算）

| 阶段 | 内容 | 估算 |
| --- | --- | --- |
| P0 spike | `rmcp` client + filesystem MCP，跑一篇需要本地文件的笔记；记录连接/调用/失败 | 1–2 天 |
| P1 client MVP | `mcp.json`、连接管理、tools/list、schema 映射、调用路由、超时/截断、日志 | 5–8 天 |
| P2 server MVP | localhost HTTP + token，4 个只读工具，复用现有搜索/笔记/出处 | 3–5 天 |
| P3 硬化 | allowlist UI、首次确认、审计、取消/杀进程、fake server 测试 | 1–2 周 |
| P4 stdio server | `--mcp-stdio` 只读模式，Claude Desktop 配置 | 3–5 天 |

**总工程量**：3–5 周做到可日常使用（估算，未拆任务）。

### 验收指标

- 连接：20 次启动，server 连接成功 ≥95%；server 挂掉不影响本地工具。
- 工具：fake server 的 3 个工具调用成功；超时/取消/超大输出/路径穿越都被正确处理。
- 任务：一篇需要网页 + 本地文件的笔记，agent 能完成并保留 URL；对比只用 `fetch_url` 的完成率。
- 外部：Claude Desktop/Cursor 能发现 4 个只读工具，3 个查询返回正确笔记/出处。
- 成本：记录 MCP 工具 schema 占用的 token 增量；确保 allowlist 后不超预算。

## 8. 不解决 / 风险

- **MCP 不提供语义检索**：它只是工具协议；混合检索仍需 sqlite-vec/embedding。
- **MCP 不是沙箱**：server 能做什么取决于它自己；页边只能做 allowlist/进程隔离/输出限制。
- **协议仍在演进**：`rmcp` 支持 `2026-07-28`，但 server 可能只支持旧版本；要做版本协商/兼容。
- **前置运行时**：很多 reference server 要 `npx`/`uvx`；Windows 用户可能没有 Node/Python。页边要么提示安装，要么内置一个受控运行时。
- **上下文膨胀**：工具越多，schema 越占 token；必须 allowlist/按需启用。
- **延迟**：Playwright/Docling 可能比本地工具慢；需要超时和降级。
- **外部服务端暴露笔记**：必须默认关闭、localhost、token、只读；不能一键开到公网。
- **许可证**：`rmcp` 是 MIT→Apache-2.0 过渡；接入的 server 各有自己的许可证，需逐个确认。

## 9. 代码触点

| 文件 | 实际改动 |
| --- | --- |
| `src-tauri/Cargo.toml` | 增加 `rmcp 3.12`（client + child-process + streamable-http-client + io）；server features 未启用 |
| `src-tauri/src/mcp.rs`（新，单文件） | `McpSession`、配置校验/读写、stdio + HTTP 连接、工具命名/合并、allowlist、超时/截断、测试 |
| `src-tauri/src/production/note_engine.rs` | `NoteEngineOptions.mcp`；`tool_specs()` 追加 MCP 工具；`execute_tool_async()` 路由 `mcp__` |
| `src-tauri/src/production/pipeline.rs` | 任务开始前 `McpSession::connect_from_app(app)`；没有 `mcp.json` 时为 `None` |
| `src-tauri/src/lib.rs` | 命令：`mcp_config_read` / `mcp_config_save` / `mcp_config_test` |
| `src/views/SettingsView.vue` | 「MCP 连接」面板：编辑、保存、测试、结果列表 |
| `src/lib/settingsSections.ts` | 新设置分区 `mcp` |
| `src/types.ts` / `src/lib/api.ts` | `McpConfigView`、`McpTestResult` 与 3 个调用 |
| `src/styles/settings.css` | MCP 面板样式 |
| `src-tauri/tests/fixtures/mcp_stub.cjs` | 测试用 stdio server（initialize/list/call/ping） |
| 服务端（§4） | **未实现**，没有 `src/bin/mcp_server.rs` |

## 10. 诚实清单

**已实测**

- `cargo test --lib`：**303 passed / 0 failed / 8 ignored**；其中 MCP 单测 **8** 个，包含真实 `node tests/fixtures/mcp_stub.cjs` 的 initialize → tools/list → tools/call，以及 80 ms 取消后 2 秒内返回。
- 设置页保存/测试走真实 Tauri 命令：dev 里 stub 返回 `stdio · 42 ms · 通过 · echo`，静默安装后的 release 冷启动 **425 ms** 通过；保存后 `mcp.json` 落盘；删掉文件后回到禁用示例；非法 JSON 保存被拒且不写文件。
- P256 真实验证（详见 [`MCP_VALIDATION.md`](MCP_VALIDATION.md)）：Playwright MCP 读 WebSocket 动态页 10.1 s / ¥0.00728；无 MCP 对照读不到；白板研究 18.2 s / ¥0.01881；HTTP transport 40 ms 连接、`echo` 成功；1 s 超时按预期返回；取消 9 ms 变为 cancelled 且不再写工具完成日志；真实 B 站页 41.6 s / 228k prompt tokens / ¥0.06378。
- 没有 `mcp.json` 时，dev 实例的笔记页、插件页、江山时序面板都能正常打开；地图 loopback 返回 200 / 115,533 B。

**未验证 / 限制**

- **服务端未实现**：外部 agent 还不能查询页边。
- 页边取消只会停止等待；外部 HTTP server 可能继续执行工具本身。
- 图片/音频/资源只返回占位，不落盘；模型看不到截图内容。
- HTTP transport 已实测 Streamable HTTP，但没有做异常断线/重连；stdio 在 Windows 上的子进程清理只验证了测试进程能退出，没有验证异常崩溃后的残留。
- 没有用 Claude Desktop/Cursor/Docling/OCR 实际联调；Playwright MCP 与 `server-everything` 已实跑。
- 没有做恶意 server、提示注入、超大 schema 的测试；`MAX_TOOLS_PER_SERVER` 和描述截断是硬编码。
- 没有做 MCP 调用的 token/耗时统计，也没有在任务日志里展示。
- 真实 B 站页快照成本很高；长页面需要限制 snapshot 次数和目标区域，否则 token 会快速膨胀。
- 安装包体积已测：NSIS **19,067,384 B / 18.18 MiB**，主程序 **27,491,840 B**。
- `rmcp` 许可证是 MIT/Apache-2.0 过渡；接入的每个 server 许可证需要单独确认。
