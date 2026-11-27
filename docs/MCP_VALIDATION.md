# MCP 客户端真实验证（2026-11-27，P256）

> 目的：把 P255 只做过 stub 的 MCP 客户端，放进真实 AI 成稿/白板研究链路；同时验证 HTTP transport、超时、取消和真实站点。
> 所有数字都是本机实测；单次样本，不是 p50/p95。

## 1. 环境

- 安装版：`D:\Program Files (x86)\页边笔记`，包含 MCP 客户端与 `fetch_url` 异步修复
- Playwright MCP：`@playwright/mcp 0.0.86`，stdio，headless Chrome
- HTTP MCP：`@modelcontextprotocol/server-everything 2.4.0`，Streamable HTTP，`127.0.0.1:3001/mcp`
  - 该包声明 Node >=24；本机 Node v22.23.2 仍能启动并完成 `echo`/长任务，只记录这一兼容性风险
- 模型：页面设置中的默认 AI 模型；费用按应用内参考价估算
- 测试素材均为本机临时页面，测试笔记/任务/白板卡已删除

## 2. 动态页面读取：MCP vs 无 MCP

测试页静态 HTML 只有「正在加载…」；暗号由 WebSocket 在约 300 ms 后推送，静态 HTML 和脚本里都没有暗号。

| | 挂 Playwright MCP | 只用 fetch_url |
| --- | --- | --- |
| 工具 | `browser_navigate` → `browser_wait_for` → `browser_snapshot` | `fetch_url` |
| 结果 | 读到 `蓝鲸-7419` | 明确写“未取到”，不编造 |
| 耗时 | **10.1 s** | **14.2 s** |
| tokens | 15,965 prompt / 1,143 completion | 15,945 / 2,386 |
| 估算费用 | **¥0.00728** | **¥0.01330** |

第一版测试页把暗号放在可直接请求的 `/fact` 接口，结果无 MCP 的 Agent 绕过页面拿到了答案；这次对照作废，最终改用 WebSocket 页面重做。

## 3. 白板研究链路

流程：链接卡 → 研究 Agent → MCP 浏览器 → 研究笔记。

- 结果：成功；研究笔记包含 `蓝鲸-7419` 和 WebSocket 动态结论
- 耗时：**18.2 s**
- 用量：22,111 prompt / 3,123 completion；**¥0.01881**
- 研究 Agent 共 **7 回合**；MCP 调用：`browser_navigate`、`browser_wait_for`、`browser_snapshot`
- 同时调用了一次 `fetch_url` 做静态比对

## 4. HTTP transport

- `mcp_config_test`：`everything` 连接成功，发现 **16** 个工具，**40 ms**
- `echo` 任务：输出 `Echo: HTTP-OK`，**6.1 s**，11,272 / 712 tok，**¥0.00509**

## 5. 超时与取消

### 超时

- HTTP 工具 `trigger-long-running-operation` 持续 8 s，`timeoutMs=1000`
- 结果：任务继续，笔记原文记录 `MCP 工具超时（1s）：trigger-long-running-operation`
- 耗时：8.1 s；6,097 / 998 tok；**¥0.00561**

### 取消（修复后）

- 工具执行 8 s，第 2 s 调用 `production_cancel`
- 任务状态：**9 ms** 变为 cancelled
- 之后等待 10 s：日志没有出现该工具完成记录，只有「已取消」
- 修复前：同一实验在取消后 **6.97 s** 仍写入了工具完成日志
- 结论：`CancellationToken` 已传入 MCP 调用；页边侧会立即停止等待并退出任务。**底层 server 是否继续执行工具本身不在页边控制范围内**。

## 6. 真实 B 站页面

- URL：`https://www.bilibili.com/video/BV1YbyUzXEsZ`
- Playwright MCP 成功读到标题、UP 主、播放/弹幕/点赞/投币/收藏等即时数字，写进笔记
- 耗时：**41.6 s**；228,222 prompt / 6,506 completion；**¥0.06378**
- 代价：15 回合，多次 snapshot ref 失效后重试；页面数字无文字标签，笔记按位置标注，并明确写出“评论数看不到”“弹幕内容看不到”
- 结论：真实站点可用，但**快照上下文成本很高**；长页面/动态页面需要限制 snapshot 次数和目标区域

## 7. 本次修复

1. `tool_fetch_url` 从 `reqwest::blocking` 改为异步 `reqwest::Client`
   - 修复 `Cannot drop a runtime in a context where blocking is not allowed` 导致的 tokio worker panic
   - 无 MCP 对照此前卡在 90% 180 秒，修复后 14.2 秒完成
2. MCP 调用传入 `CancellationToken`
   - `McpSession::call` 用 `tokio::select!` 在取消时立即返回
   - 执行工具后再次检查 token，取消后不再继续下一回合
3. 删除 note engine 中重复的工具日志
   - `hooks.turn` 和 `hooks.log` 原本各写一遍 `#n tool · observation`
   - 现在每次工具调用只保留一条
4. MCP stub 增加 `slow` 工具和取消单测
5. `.gitignore` 增加 `.playwright-mcp/`

## 8. 测试

- `cargo test --lib`：**303 passed / 0 failed / 8 ignored**
- MCP 单测：**8/8**，其中 `cancellation_aborts_pending_tool_call` 验证 80 ms 取消后 2 秒内返回
- 前端构建：`npm run build` 通过（main 639.10 kB / gzip 215.84 kB，CSS 268.53 kB）
- 安装包：**19,067,384 B / 18.18 MiB**，主程序 **27,491,840 B**

## 9. 仍未验证 / 边界

- Docling、OCR、PDF 解析 MCP 没有实跑
- Claude Desktop / Cursor 查询页边没有验证：页边只读 MCP **服务端尚未实现**
- 只有单次样本，没有 p50/p95、失败率、并发
- Playwright MCP 图片结果仍是占位，截图证据没有落盘入上下文
- 真实站点页面结构会变；本次的 8179 播放/9 弹幕只是当时快照
- `server-everything` 要求 Node >=24，本机 v22.23.2 只验证了 `echo` 和长任务
- 取消只保证页边停止等待；底层 HTTP server 可能继续执行工具
