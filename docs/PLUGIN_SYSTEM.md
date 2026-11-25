# 插件系统设计与调研

> 2026-11-25 · 目标：给页边笔记加一层可安装、可禁用、权限可见的插件框架；先做只读命令，后续再扩面板、写入、agent 工具。
>
> 结论先说：**manifest 声明 + 独立 Worker 运行插件 JS + 宿主窄 RPC + 默认拒绝的权限**。不把 Tauri `invoke` 或 Node 交给插件，也不把插件代码塞进主渲染进程。

## 1. 调研：主流产品怎么接插件

### 1.1 横向对比

| 产品 | manifest | 扩展点 | 运行时 / 隔离 | 权限 | 生命周期 | 分发 / 版本 |
| --- | --- | --- | --- | --- | --- | --- |
| VS Code | `package.json`：`main`/`browser`、`activationEvents`、`contributes`、`engines.vscode` | commands / menus / configuration / views / languages / webview | Extension Host 独立进程（Node / Web Worker）；崩溃不拖垮 workbench | 无 per-extension 细粒度权限；Workspace Trust 只能限制一部分 | 懒激活 `activate()` / `deactivate()` | Marketplace `.vsix`；`engines` 控兼容 |
| Obsidian | `manifest.json`：`id`/`name`/`version`/`minAppVersion`/`main` | `addCommand`、ribbon、settings tab、`registerView`、事件 | 与宿主同一 JS 环境，**无沙箱**；官方明确无法限制插件权限 | 无 per-plugin 权限；只有全局 Restricted Mode | `onload` / `onunload`，`register*` 自动清理 | 社区目录 + GitHub release（`main.js`/`manifest.json`/`styles.css`） |
| Logseq | `package.json` + `logseq` 字段：`main`、`mode`（iframe/shadow） | command palette、slash command、UI item、`provideUI` | iframe 或 shadow DOM + 双工 RPC；API 面很宽 | 无权限清单 | `logseq.ready` / `beforeunload` | Marketplace / 开发者模式 |
| Joplin | `manifest.json`：`manifest_version`/`name`/`version`/`app_min_version` | commands、settings、dialogs/panels、白名单 content scripts | 独立 BrowserWindow 进程 + IPC；是稳定性隔离，不是安全沙箱 | 无权限模型；API 可读写数据 | `plugins.register({onStart})`；无 unload | `.jpl` + 官方仓库；`app_min_version` |
| Figma | `manifest.json`：`main`/`ui`/`editorType`/`capabilities`/`permissions`/`networkAccess`/`parameters` | plugin command、menu、relaunch buttons、参数化 Quick Actions | 主线程是精简 JS sandbox（无 DOM/fetch）；UI 是独立 iframe，只通过 `postMessage` 通信 | 能力位 + `permissions` + 域名级 `networkAccess`；`["none"]` 全禁 | 每次启动新建 sandbox，`figma.closePlugin()` 结束 | Community 审核；`api` 锁主版本 |
| Chrome MV3 | `manifest.json`：`background.service_worker`/`content_scripts`/`permissions`/`host_permissions`/`optional_permissions` | action、options、side panel、commands、content scripts、SW | Service Worker 非常驻；content script 在 ISOLATED world | 安装时按项授权；`host_permissions` 独立；可选权限要在用户手势里申请 | 事件唤醒；空闲约 30s 终止；状态需外置 | Web Store 审核；MV3 禁止远程可执行代码 |
| Raycast | `package.json` 超集：`commands`/`preferences`/`tools`/`ai` | view / no-view / menu-bar 命令、AI tools | Node + V8 isolate + 白名单 API；**无 OS 沙箱** | 无 per-extension 权限；靠系统 TCC | 命令加载，退出卸载；有内存上限 | PR 合并到官方仓库；`owner` + `access:"private"` |
| Adobe UXP | `manifest v5`：`entrypoints`/`requiredPermissions` | panel / command | UXP sandbox（无 Node），独立 JS 上下文 | **默认拒绝**：network.domains、clipboard、localFileSystem、launchProcess、webview | entrypoint `create/destroy`；300ms 超时 | Marketplace / 自托管 `.ccx` |
| MCP | JSON-RPC 2.0；`initialize` + `capabilities` | tools / resources / prompts | stdio 子进程或远程 HTTP；宿主负责沙箱 | OAuth / 本地配置 + 宿主 consent；工具注解只是提示 | `initialize → initialized → operation`；传输层关闭 | Registry；协议版本是日期串 |
| Claude Skills | `SKILL.md` + YAML frontmatter | 指令 + 可选脚本/资源 | API 在代码容器、Claude Code 在本机；Skill 本身不是沙箱 | `allowed-tools`（实验）+ 逐次询问 | 按需加载（元数据 → 正文 → 资源） | 文件/仓库/市场；无强制签名 |

### 1.2 提炼出的六条规律

1. **manifest 管“声明什么”，运行时 API 管“能做什么”。** VS Code/Obsidian/Figma/Chrome 都是这个分工；不要把权限散落在代码里。
2. **隔离强度决定安全上限。** VS Code 独立 Extension Host、Joplin 独立进程、Logseq iframe、Figma 双上下文；Obsidian/Raycast/Sketch 是全信任模型，不适合我们这种能读整个笔记库的应用。
3. **权限只有 Tauri/UXP/Chrome 做成一等公民。** 其余靠“只装可信插件”或全有全无开关；我们不能接受把 `app.vault` 直接交给第三方。
4. **UI 和逻辑分上下文。** Figma 的“逻辑 sandbox 无 DOM + UI iframe 只 postMessage”最适合笔记应用：命令需要读笔记，但不该碰主界面。
5. **生命周期要短、状态要外置、失败要可杀。** Chrome SW 随时回收、Figma 每次新建 sandbox、VS Code 可重启 host；插件不能假设自己常驻。
6. **分发从本地开始，签名/审核后置。** Obsidian/Joplin 都是先本地目录，再做目录/商店；我们的 v1 也应该先做“本地插件目录 + 显式安装”，不急着做市场。

### 1.3 明确不学的部分

- **不学 Obsidian 的同环境全权限**：插件能直接拿 `app.vault`、Node、网络；我们的笔记库、标注、任务和同步 token 都在同一进程里。
- **不学 Raycast 的 Node 全信任**：前端工程师写插件很方便，但 `fs`/`net` 无边界；桌面笔记应用不能接受。
- **不学 VS Code 的“扩展 host 与编辑器同权”**：VS Code 的 Workspace Trust 只是降低风险，不是 per-extension 权限。
- **不把 Tauri plugin 当第三方插件机制**：Tauri 2 的 plugin 是编译期 Rust crate + capability，随 app 构建，适合内置能力，不适合运行时安装第三方代码。

## 2. 本项目的约束与取舍

### 2.1 现状

- Tauri 2 + Vue 3 + TypeScript + Rust + SQLite；Windows 安装包约 8.8 MiB；**没有 Node sidecar**。
- 应用是“读笔记 + 标注 + 制作”，**不是 Markdown 编辑器**；v1 不应让插件改写笔记。
- CSP 是 `default-src 'self'; script-src 'self'; connect-src ipc: ...`；没有 `unsafe-eval`。
- 笔记正文可能包含 HTML 视觉块（sandbox iframe），内容本身不完全可信。
- 同步是 op log + 文件 blob；插件配置属于**设备本地**，不应该进笔记库同步。

### 2.2 为什么选 Web Worker

| 方案 | 优点 | 问题 | 结论 |
| --- | --- | --- | --- |
| 主渲染进程直接 `import()` 插件 | 最简单，Vue/DOM 都能用 | 插件能拿 `window.__TAURI_INTERNALS__`，可绕过所有权限；CSP 还要放开 `blob:`/`unsafe-eval` | 不用 |
| 独立 Tauri WebviewWindow | 有独立窗口 | Tauri 会给它注入 IPC；CSP 全局；窗口管理/焦点成本高 | 不用 |
| sandbox iframe + postMessage | UI 富交互；Figma 同款 | CSP 下加载插件代码要处理 blob/自定义协议；iframe 生命周期和尺寸更复杂 | v2 做面板时用 |
| **Web Worker + Blob** | **无 DOM、无 Tauri、无 Node；只能 postMessage；超时可直接 terminate** | 插件不能用 DOM；大计算会占一个线程 | **v1 选它** |
| QuickJS/Boa 嵌入 Rust | 真正的语言级沙箱 | 新增大依赖、构建链和二进制体积；v1 收益不匹配 | 后续评估 |
| 外部进程 / MCP | 语言无关、天然隔离 | 需要 Node/Python 运行时或子进程管理；v1 太重 | 后续做 agent 工具时接 |

Worker 的边界：
- 拿不到 `window`、`document`、`localStorage`、`__TAURI_INTERNALS__`。
- `fetch` 受父页 CSP `connect-src` 限制（当前只允许 IPC/localhost），所以默认**不能联网**。
- 只能通过宿主注入的 `margin` API 读当前笔记、读设置、返回 Markdown。

### 2.3 v1 的扩展点

| 扩展点 | v1 | 说明 |
| --- | --- | --- |
| `commands` | ✅ | 在笔记页的「插件」菜单里运行；返回 Markdown 结果 |
| `settings` | ✅ | boolean / text / select / number；宿主渲染设置表单 |
| `permissions` | ✅ | `notes:read`；默认拒绝，启用时授权 |
| `panels` | ⏳ | 预留 manifest 字段，v2 用 sandbox iframe |
| `notes:write` | ❌ | 应用不是编辑器；写入需要 patch/备份/同步设计，另开 |
| `net.fetch` | ❌ | 默认离线；后续走 Rust 侧域名 allowlist |
| agent tools / MCP | ⏳ | 设计上把 MCP server 当“外部插件运行时”，晚于命令框架 |
| 市场 / 签名 | ❌ | 先本地目录 + 显式安装；签名/审核后置 |

## 3. Manifest

插件目录：`<app_data>/plugins/<id>/`，至少包含 `plugin.json` 和 `main` 指向的 JS。

```json
{
  "id": "example.word-count",
  "name": "字数统计",
  "version": "1.0.0",
  "apiVersion": 1,
  "description": "统计当前笔记的中文字符、英文词数、标题数和预计阅读时间。",
  "author": "页边示例",
  "main": "main.js",
  "permissions": ["notes:read"],
  "settings": [
    { "key": "includeCode", "label": "包含代码/HTML 块", "type": "boolean", "default": true }
  ],
  "commands": [
    {
      "id": "count",
      "title": "统计当前笔记字数",
      "description": "返回字数、标题数和预计阅读时间。",
      "context": "note"
    }
  ]
}
```

约束：
- `id`：`^[a-z0-9][a-z0-9._-]{1,63}$`；目录名必须等于 id。
- `version`：semver；`apiVersion` 目前只接受 `1`。
- `main`：只允许单层文件名（不允许 `../`、绝对路径）。
- `permissions`：白名单；v1 只有 `notes:read`。
- `commands`：最多 20 个；id 唯一；`context` 目前只支持 `note`。
- `settings`：最多 20 个；key 唯一；type 为 `boolean|text|select|number`。
- manifest ≤ 64 KiB，入口 JS ≤ 512 KiB。

## 4. 运行时 API

插件入口是单文件 JS，在 Worker 里执行。它把命令挂到 `margin.commands`：

```js
margin.commands.count = async (ctx) => {
  const note = await margin.notes.read(ctx.noteId)
  const text = note.body
  return { markdown: `## 字数统计\n\n- 字符：${text.length}` }
}
```

宿主注入的 `margin`：

| API | 说明 | 权限 |
| --- | --- | --- |
| `margin.notes.read(noteId)` | 返回 `{ id, title, body }`；只能读当前笔记库 | `notes:read` |
| `margin.settings` | 当前插件设置对象；运行前注入 | — |
| `margin.commands` | 插件注册命令的对象；key 必须匹配 manifest | — |

命令返回：
- `{ markdown: string }`：宿主在结果弹窗里按 Markdown 渲染（支持 ```html 视觉块）。
- `{ message: string }`：纯文本。
- 抛错：宿主显示错误，不崩溃。

Worker 生命周期：
- 每次运行命令创建一个新 Worker；`run` 消息带上 `commandId`、`context`、`settings`。
- 10 秒超时；超时/worker error 立即 `terminate()`。
- 插件入口在加载时只允许注册命令，不应做长耗时工作；长任务放到命令函数里。

## 5. 宿主与权限

- 启用插件时，UI 列出 manifest 请求的权限；用户勾选/确认后才写入 `state.json`。
- `notes:read` 的 RPC 在**前端宿主**检查：插件已启用 + 已授权 + 当前命令存在；Rust 侧只提供 `get_note` 等已有命令。
- 插件拿不到 Tauri IPC。Worker 启动时覆盖 `fetch` / `XMLHttpRequest` / `WebSocket` / `EventSource` / `importScripts` / 子 Worker；实测 `fetch("https://example.com")` 返回「插件 v1 不允许联网」。CSP 的 `connect-src` 仍是第二层防线。
- 禁用/卸载会终止该插件当前 Worker；状态文件原子写入。
- 插件配置存 `<app_data>/plugins/state.json`，**不进同步**；换机器需要重新启用/配置。

## 6. 目录与生命周期

```
<app_data>/plugins/
├── state.json
├── example.word-count/
│   ├── plugin.json
│   └── main.js
├── example.heading-audit/
└── ...
```

- 扫描：应用启动/打开插件页/点“重新扫描”时读取。
- 安装示例：Rust 把内置示例写到 plugins 目录，已存在不覆盖。
- 安装自己的插件：把目录放进 plugins，点“重新扫描”。
- 卸载：删除目录 + state 条目；只允许 app data 下的插件目录。
- 失败：manifest 错误显示在插件页；运行错误显示在结果弹窗；不自动禁用（v1），但可以手动禁用。

## 7. v1 示例插件

1. **字数统计**：`notes:read`；统计 CJK 字符、英文词、H2/H3、预计阅读时间；一个 `includeCode` 设置。
2. **标题体检**：`notes:read`；列出空标题、重复标题、跳级、H2 过多。
3. **链接清单**：`notes:read`；提取 Markdown 链接与裸 URL，按域名统计，列出重复。

这三个都只读，不污染笔记；足以验证 manifest、权限、Worker、设置、结果渲染。

## 8. 安全边界与诚实清单

**已做**
- 插件逻辑在 Worker，无 DOM/Tauri/Node。
- 权限默认拒绝；启用时显式授权。
- 超时 10s + terminate；Worker 崩溃只影响本次运行。
- 入口/清单大小限制；路径只允许单层文件名。
- 插件状态本地保存，不进同步。

**没做 / 后续**
- 没有签名/审核/市场；本地插件等同“用户信任的代码”。
- 插件不能写笔记、不能联网、不能注册面板/agent 工具。
- Worker 仍是同源脚本，能消耗 CPU/内存；超时只管时间，不管内存。
- 没有 per-note scope；`notes:read` 可读整个笔记库。
- 没有持久化运行日志/审计表；v1 只在 UI 显示本次结果/错误。
- 没有 iframe 插件 UI；结果只是 Markdown。

## 9. 参考

- VS Code: [extension manifest](https://code.visualstudio.com/api/references/extension-manifest)、[activation events](https://code.visualstudio.com/api/references/activation-events)、[extension host](https://code.visualstudio.com/api/advanced-topics/extension-host)、[webview](https://code.visualstudio.com/api/extension-guides/webview)、[runtime security](https://code.visualstudio.com/docs/editor/extension-runtime-security)
- Obsidian: [Manifest](https://docs.obsidian.md/Reference/Manifest)、[lifecycle](https://docs.obsidian.md/Plugins/Guides/Manage+plugin+lifecycle)、[plugin security](https://obsidian.md/help/Extending+Obsidian/Plugin+security)
- Logseq: [SDK](https://plugins-doc.logseq.com/)、[ILSPluginUser](https://logseq.github.io/plugins/interfaces/ILSPluginUser.html)
- Joplin: [manifest](https://joplinapp.org/help/api/references/plugin_manifest/)、[plugin spec](https://joplinapp.org/help/dev/spec/plugins/)
- Figma: [manifest](https://developers.figma.com/docs/plugins/manifest/)、[how plugins run](https://developers.figma.com/docs/plugins/how-plugins-run/)、[network access](https://developers.figma.com/docs/plugins/manifest/#networkaccess)
- Chrome: [manifest](https://developer.chrome.com/docs/extensions/reference/manifest)、[service worker lifecycle](https://developer.chrome.com/docs/extensions/develop/concepts/service-workers/lifecycle)、[permissions](https://developer.chrome.com/docs/extensions/develop/concepts/declare-permissions)
- Raycast: [manifest](https://developers.raycast.com/information/manifest)、[security](https://developers.raycast.com/information/security)
- Adobe UXP: [manifest v5](https://developer.adobe.com/photoshop/uxp/2022/guides/uxp_guide/uxp-misc/manifest-v5/)
- MCP: [spec 2025-06-18](https://modelcontextprotocol.io/specification/2025-06-18)、[tools](https://modelcontextprotocol.io/specification/2025-06-18/server/tools)、[security best practices](https://modelcontextprotocol.io/specification/2025-06-18/basic/security_best_practices)
- Claude Skills: [overview](https://platform.claude.com/docs/en/agents-and-tools/agent-skills/overview)、[spec](https://agentskills.io/specification)
- Tauri 2: [plugins](https://v2.tauri.app/develop/plugins/)、[capabilities](https://v2.tauri.app/security/capabilities/)

## 10. 实现与实测（2026-11-25）

### 10.1 落地范围

- Rust `src-tauri/src/plugins.rs`：manifest 校验、扫描、`state.json`、启用/权限、设置校验、示例安装、卸载、打开目录；4 个单测。
- Tauri 命令：`plugins_list` / `plugins_set_enabled` / `plugins_save_settings` / `plugins_source` / `plugins_open_dir` / `plugins_install_examples` / `plugins_uninstall`。
- 前端：
  - `src/lib/pluginWorker.js?raw`：Worker 运行时 + `margin` RPC 桥。
  - `src/lib/pluginRuntime.ts`：每次命令新建 Worker、12s 超时、`notes.read` 宿主代理、结果归一化、Proxy 克隆。
  - `src/stores/plugins.ts`：列表/启用/设置/运行。
  - `src/views/PluginsView.vue` + `/plugins` 路由 + 顶栏「插件」。
  - `PluginCommandButton.vue`：笔记标题下的「插件」菜单；菜单 Teleport 到 body 并用 fixed 定位，避免被 `.doc-scroll` 裁剪；结果 Markdown 弹窗。
  - `src/styles/plugins.css`。
- CSP：`script-src 'self'; worker-src 'self' blob:;`。
- 示例插件：`src-tauri/plugins/examples/{word-count,heading-audit,link-audit}`，用 `include_str!` 安装到 app data。

### 10.2 实测

- `cargo test --lib` **271 passed / 0 failed / 8 ignored**（插件新增 4 个）。
- `npm run build` 通过（main JS 669.86 kB / CSS 321.24 kB）。
- 安装示例：3 个插件写入 `%APPDATA%/com.yebian.notes/plugins/`，默认未启用；启用 3 个后 `plugins_list` 返回 `enabled=true`。
- 在《乌克兰无人艇在班加西击沉水星号》上运行「统计当前笔记字数」：
  - 字符（不含空白）**4,443**，中文字符 **2,396**，英文/数字词 **245**，标题 **5**（H2 5），链接 0，预计阅读 **7 分钟**。
- 「标题体检」：标题 5 个、问题 0 个。
- 「链接清单」：该笔记链接 0 个（预期，正文没有 URL）。
- 设置：关闭 `includeCode` 保存后 `plugins_list` 返回 `{"includeCode":false}`；恢复为 true。
- 权限：临时插件 `test.denied` 没声明 `notes:read`，启用后运行返回 `插件没有获得 notes:read 权限`（11ms），验证宿主侧默认拒绝。
- 失败隔离：临时插件 `test.loop` 死循环，**12.012s** 后超时并 terminate，主界面不受影响。
- 截图：`72-plugins-empty.png`、`73-plugins-enabled.png`、`74-plugin-menu.png`、`75-plugin-result.png`、`76-plugins-dark.png`。

### 10.3 这次没做 / 不要误读

- **不能写笔记**：`notes:write` 没实现；应用不是编辑器，写入要补 patch/版本/同步设计。
- **不能联网**：Worker 继承 `connect-src`，插件没有 host fetch；`net.fetch` 只是设计里的后续权限。
- **没有 UI 面板**：只有命令 + Markdown 结果；`panels` 是 v2 的 iframe 方案。
- **没有 MCP/agent tools**：MCP 作为外部插件运行时的设计在 §1.3 / §2.3，v1 未接。
- **没有签名/市场/审核**：本地插件等同用户信任的代码；启用提示写明“读取整个笔记库”。
- **没有审计表/持久日志**：只在本次结果弹窗和插件页显示错误。
- **Worker 不是内存沙箱**：只按时间 terminate，不能阻止插件吃内存；`notes:read` 也没有 per-note scope。
- **插件状态不同步**：`state.json` 在 app data，换机器要重新启用/配置。
- **未重打安装包**：CSP/Worker 只在 dev 原生窗口验收；安装版要重打后再确认。

