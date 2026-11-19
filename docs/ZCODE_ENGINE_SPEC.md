# ZCode 单次笔记引擎规格（Phase 0，2026-11-19）

> 本文只记录**从本机 ZCode 安装/会话里读到的事实**，以及由事实推出的设计结论。
> 标注：**实测** = 本机文件/请求体；**推断** = 由事实推出但未单独复现；**未知** = 当前权限或数据不足。

## 1. 事实来源

- ZCode 安装：`D:\Develop\ZCode`，Electron；`resources/glm/zcode.cjs` **15,408,074 B**。
- `.node-bundle-meta.json`：`runtime: electron-node`，`entry: zcode.cjs`，源码路径 `apps/zcode-cli/packages/cli/dist/zcode.cjs`。
- 运行方式（实测，来自 `~/.zcode/cli/node-fallback/v1/node.cmd`）：
  `ELECTRON_RUN_AS_NODE=1 "%ZCODE_NODE_FALLBACK_EXEC_PATH%" %*`
- 发行方（`resources/app-update.yml`）：北京智谱华章科技股份有限公司。
- 许可：安装目录只有 `licenses/electron/` 和 Electron/Chromium 许可证；没有应用自身 LICENSE/仓库地址。**是否开源，本地无法确认。**

## 2. ZCode 那次“把一段文稿写成笔记”实际跑的是什么

### 2.1 不是主会话

主会话的请求体（`rollout/model-io-sess_44b00e7d….jsonl` 第一行，实测）包含完整的 ZCode 交互系统提示：身份、Harness、Desktop Context、skills 列表、工具说明，约 110 KB 后才出现 tools。这是**主会话**的编码代理提示，不是笔记任务。

### 2.2 是 general-purpose 子代理（实测）

`agents/<sess>/agent_*/metadata.json` 的 `profileSnapshot`：

```json
{
  "name": "general-purpose",
  "injectAgentsMd": true,
  "source": "built-in",
  "tools": ["*"],
  "systemPrompt": "You are an agent for ZCode CLI. Given the user's message, you should use the tools available to complete the task. Complete the task fully—don't gold-plate, but don't leave it half-done. When you complete the task, respond with a concise report covering what was done and any key findings — the caller will relay this to the user, so it only needs the essentials.\n\nYour strengths:\n- Searching for code, configurations, and patterns across large codebases\n- Analyzing multiple files to understand system architecture\n- Investigating complex questions that require exploring many files\n- Performing multi-step research tasks\n\nGuidelines:\n- For file searches: search broadly when you don't know where something lives. Use Read when you know the specific file path.\n- For analysis: Start broad and narrow down. Use multiple search strategies if the first doesn't yield results.\n- Be thorough: Check multiple locations, consider different naming conventions, look for related files.\n- NEVER create files unless they're absolutely necessary for achieving your goal. ALWAYS prefer editing an existing file to creating a new one.\n- NEVER proactively create documentation files (*.md) or README files. Only create documentation files if explicitly requested."
}
```

- `tools: ["*"]`：全部工具，不是 3 个任务工具。
- `injectAgentsMd: true`：子代理会拿到工作区的 `AGENTS.md`（本机为 `D:\Develop\yebian\AGENTS.md`）。**未知**：是否也注入用户级 `~/.zcode/AGENTS.md`。
- 盲评子代理的实测用量：54,614 输入 / 6,553 输出 / 18,816 缓存读；5 次工具调用；41.8s。

### 2.3 任务与循环

- 任务：一句“把这段文字生成一份美观的笔记” + 素材文件；素材通过 `Read`/`Bash` 读取（REPORT §6），不是预塞进上下文。
- 工具：P132 记录是 `Bash`、`Write`；5×3 对照记录是 `Read`/`Write`（同一 profile，具体每次不同）。
- 循环：原生 tool calls；模型不再调用工具时，回答结束；笔记落在文件里，调用方读文件，**没有 `finish` 工具**。
- 产出：Markdown 文件，不是工具参数里的 JSON 字符串。

### 2.4 模型与采样（实测请求体）

- `model.modelId = "deepseek-flash"`，`model.providerId = "custom:deepseek"`。
- 请求体：`model: "deepseek-flash"`，`max_tokens: 384000`。
- 第一行请求体可见部分**没有** `temperature` / `top_p`（可能省略，用 provider 默认）；**未知**：完整值。
- 制作台当前模型：`deepseek-v4-flash`（settings 表）；两者是否同一快照，**未知**，Phase 0 待核对。

## 3. 对项目的直接结论

1. **不要做 3 工具 / 6 回合 / JSON-in-text 的笔记代理。** ZCode 的进程是“通用文件代理”：短系统提示 + 全部文件/Shell 工具 + 原生 tool calling + 多回合 + 文件产出。
2. **一次任务 ≠ 简化引擎。** 单次任务的语义只去掉“用户对话层”；工具、回合、上下文按 ZCode 的来。
3. **引擎放 Rust。** 同一个 app 进程，无 Node sidecar、无 webview worker、无 pending 表；工具是进程内函数，模型调用走 provider。
4. **工作区模型。** 每次任务建一个临时工作区：
   - `material.md`：素材；
   - `note.md`：模型要写的笔记；
   - 工具 `read_file / write_file / edit_file / list_dir / bash(受限)` 只在这个目录内。
5. **停止条件。** 模型返回不带 tool_calls 的 assistant 消息 = 结束；引擎读 `note.md`，跑确定性门禁；不引入 `finish` 工具。
6. **工具集按任务完整给。** 文件读写 + 列目录 + 受限 bash（工作区内、无网络、无危险命令）。不做“3 工具上限”；是否加 `bash` 用对照实验决定。
7. **提示词。** 先用上面的 general-purpose 原文 + 一句任务；不复制主会话 110KB 交互提示。需要“决策文档”体裁时，把体裁 brief 放进 user 消息，而不是改引擎。
8. **模型对齐。** `deepseek-flash` / `deepseek-v4-flash` 先核对；`max_tokens` 用模型目录的 max_output；`temperature` 先按 provider 默认（不传），不沿用制作台的 0.25，除非对照证明 0.25 更好。

## 4. 仍未确认（Phase 0 剩余）

- `zcode.cjs --help` 是否有非交互入口；本会话执行被权限监控拦。
- 5×3 对照里每次的确切工具直方图（Bash/Read/Write 的比例）。
- `temperature/top_p` 的完整值（请求体可见段没有）。
- `injectAgentsMd` 是否注入用户级 AGENTS.md。
- `deepseek-flash` 与 `deepseek-v4-flash` 是否同一快照。

## 5. 复现命令（只读）

主会话请求体第一行（可按字节切片，不依赖 jq/node）：

```bash
head -c 4000 "C:/Users/<user>/.zcode/cli/rollout/model-io-<session>.jsonl"
tail -c +110001 "…jsonl" | head -c 1500
```

子代理 profile：

```bash
read "C:/Users/<user>/.zcode/cli/agents/<sess>/agent_<id>/metadata.json"
```
