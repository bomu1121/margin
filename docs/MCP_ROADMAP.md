# MCP 后续路线图（2026-11-27，P257）

> 结论：**MCP 客户端能力已经够用，暂时不继续接通用 server。** 保留 `mcp.json` 作为扩展口；只有当某个 server 通过下面的成本/质量门槛，才进入实现。
>
> 背景：Playwright MCP 读一个真实 B 站页花了 **41.6 s / 228k prompt tokens / ¥0.06378**，不能作为默认能力；其余“通用 MCP”对页边没有明确刚需。

## 1. 当前基线

- 客户端：`src-tauri/src/mcp.rs`，stdio + Streamable HTTP，手动 `mcp.json`，默认关闭
- 已验证：Playwright MCP、`server-everything`、动态 WebSocket 页、白板研究、真实 B 站页
- 已修：`fetch_url` 异步、MCP 取消、重复工具日志
- 未实现：页边自己的只读 MCP 服务端
- 详见 [`MCP_VALIDATION.md`](MCP_VALIDATION.md)

## 2. 候选 MCP 调研

| 候选 | 与现有流程的关系 | 实测/资料 | 结论 |
| --- | --- | --- | --- |
| **Bilibili 字幕/评论类 MCP** | 直接命中视频笔记主链路：原生字幕可替代/减少 ASR，评论可补充上下文 | 官方 Registry 有托管版 `com.52choujiang/bilibili-insights`（24 工具、约 6.3k token 工具描述、需 API key）；社区有本地 `bilibili-mcp` 类项目，质量/许可需审计 | **最高优先级候选，但先做原生能力评估** |
| **Reader / URL→Markdown MCP**（Jina、Firecrawl、file2markdown） | 替代大部分 Playwright 页面读取，一次返回 Markdown，减少 snapshot 重试和 token | 官方 Jina MCP 文档给出 `read_url` 25k token 截断、Streamable HTTP；本机实测 `mcp.jina.ai` 初始化超时，未能验证；Firecrawl 需 API key | **第二候选，先解决网络/Key，再测 10 页成本** |
| **Docling / 文档解析 MCP** | PDF/Office → 带表格/标题的 Markdown，补出处核对 | Registry 有托管 Docling；本地 Python 安装重 | 只有在 PDF/Office 真正进入制作链路时才做 |
| **OCR MCP**（OCR Router / RapidOCR） | 白板图片、扫描页取字 | Registry 有托管 OCR；本机无独立 OCR MCP 实测 | 图片输入变频繁再做 |
| **Calibre / Zotero MCP** | 本地电子书库 / 文献管理 | Registry 有；当前项目没有证据表明用户依赖 Calibre/Zotero | 条件性，不主动接 |
| **GitHub/Slack/Notion/数据库/Obsidian/Memory/Time** | 通用开发或 SaaS 工具 | 生态很多 | **不接**：与“中文视频/史料/笔记”主流程关系弱 |

### 为什么 Bilibili 候选最值得看

现有 `source → transcribe → refine → agent_note → publish → cites` 中，转写是持续成本和错误来源。如果原生字幕覆盖足够高，价值比“再接一个浏览器”大得多。

但要注意：**这可能不需要 MCP。** 页边已经有 B 站登录、视频元数据和音频链路；如果字幕 API 能直接接，原生实现比多一个 Node server 更稳。MCP 只在“想复用第三方现成实现/需要评论/搜索”时才值得。

### 为什么 Reader 候选值得看，但暂不承诺

Playwright 的成本不是“浏览器慢”，而是**无障碍树 snapshot 太大、模型会多次重试**。一次 `read_url → Markdown` 如果能把单页控制在 25k tokens 内，成本会低一个数量级。

但本机对 `mcp.jina.ai` 的初始化直接超时，说明网络可达性本身是前置条件；Firecrawl 需要 API key。**在连通性和单页成本没测出来之前，不写进产品默认能力。**

## 3. 只在这些条件满足时才开发

### A. Bilibili 字幕/评论

前置实验：

1. 选 20 条已有视频（中文口播、不同 UP、有/无 CC 各半）。
2. 对比原生字幕覆盖率、字数、时间戳、错字率、获取耗时。
3. 对比现有 ASR 的音频秒数/费用/耗时。
4. 记录无字幕时的回退路径。

通过门槛（建议）：

- 原生字幕覆盖 ≥ **70%**；
- 有字幕的视频，转写成本下降 ≥ **50%**；
- 总耗时不超过现有链路的 **1.2×**；
- 失败可回退到现有 ASR，不阻断任务。

不通过就不做；不要为了“接 MCP”而接。

### B. Reader / Markdown MCP

前置实验：

1. 10 个页面：静态、JS 渲染、长文、付费墙/反爬各 2–3 个。
2. 每个页面只允许 1–2 次工具调用。
3. 记录成功率、返回字符/token、耗时、是否丢失正文/表格。

通过门槛：

- 单页 ≤ **25k tokens**；
- 成功率 ≥ **80%**；
- 与 Playwright 相比，单页费用下降 ≥ **50%**；
- 不把页面内容发给不可信第三方（隐私边界要明确）。

### C. Docling / OCR

只有当“PDF/Office/截图成为制作来源”出现真实需求时才做。门槛：

- 5 份 PDF 的标题/表格/阅读顺序人工抽检通过；
- 20 张中文截图的字符准确率可量化；
- 单份文档的成本和时间可接受。

## 4. 分阶段计划

### P0（已完成，保持）

- MCP 客户端、设置页、allowlist、超时、取消、日志
- 不默认启用；不接通用 server
- 保留 `mcp.json` 扩展口

### P1（按需触发）：Bilibili 字幕能力

- 优先评估**原生字幕 API**，不是先写 MCP server
- 如果第三方 Bilibili MCP 明显更省事，再包一层；必须审计来源、cookie 传递和许可
- 验收：20 条视频对照表 + 成本/覆盖率数字

### P2（按需触发）：Reader MCP

- 先解决 Jina/Firecrawl 的连通性和 Key
- 用 10 页对照 Playwright
- 如果通过，只作为“网页正文读取”的备选，不替代 Playwright 的交互能力

### P3（按需触发）：Docling/OCR

- 等 PDF/图片输入成为真实工作流
- 先托管/外部 server，确认质量再考虑原生打包

### P4（最后）：Registry 浏览与一键配置

- 官方 Registry 已是数万级（2026-10 快照约 49k，11 月初约 55k，口径不一）
- 但列表质量/安全差异大；只做**精选预设**，不做“全量商店”
- 一键写 `mcp.json` 前必须展示 transport、工具、权限、风险

## 5. 暂不做的

- 不把 Playwright 作为默认成稿能力
- 不把 MCP 挂到 UP 订阅自动任务
- 不做写笔记的 MCP 服务端
- 不做通用 server 市场/评分/安装器
- 不为了“像 ZCode”补 OAuth/SSE/资源系统

## 6. 保留的口子

现有扩展口已经足够：

- `%APPDATA%\com.yebian.notes\mcp.json`
- 任意 stdio / Streamable HTTP server
- `tools.allow` 白名单
- 任务开始时按配置注册；没有配置时零行为变化
- 后续只需要在设置页加“预设”，不需要重写 Agent

## 7. 诚实边界

- 本轮只做了 MCP 调研和已有客户端验证；**没有实现 Bilibili 字幕 MCP、Reader MCP、Docling/OCR**
- Jina MCP 本机初始化超时，未完成真实读取对比
- 社区 Bilibili MCP 的仓库/许可证/维护状态需要逐个审计，不能仅凭 Registry 描述采用
- Registry 数量是第三方全量快照，不是官方公布总数
- 真实 B 站 Playwright 成本是单次样本，不代表所有页面

## 8. 参考

- [MCP 官方 Registry](https://registry.modelcontextprotocol.io/)
- [MCP 官方参考 servers](https://github.com/modelcontextprotocol/servers)
- [Jina MCP](https://github.com/jina-ai/mcp)
- [Playwright MCP](https://github.com/microsoft/playwright-mcp)
- [Bilibili hosted MCP（Registry）](https://registry.modelcontextprotocol.io/)
- [ProtocolProbe registry snapshot](https://zenodo.org/records/23648419)
