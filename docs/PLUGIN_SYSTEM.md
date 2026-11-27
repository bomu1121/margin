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
| `commands` | ✅ | 在笔记页的「插件」菜单里运行；返回 Markdown 或面板 |
| `settings` | ✅ | boolean / text / select / number；select 支持 `optionLabels`；宿主渲染设置表单 |
| `permissions` | ✅ | `notes:read`；默认拒绝，启用时授权 |
| `panels` | ✅ | v2：跨域 sandbox iframe + URL 模板 + 字段；当前“江山时序”使用 |
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
  "homepage": "https://example.com/plugin",
  "repository": "https://github.com/example/plugin",
  "features": ["统计中文字符和英文词", "估算阅读时间"],
  "icon": "assets/icon.svg",
  "screenshots": ["assets/shot.jpg"],
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
- `version`：semver；`apiVersion` 接受 `1`（命令插件）和 `2`（命令 + 面板）。
- `main`：只允许单层文件名（不允许 `../`、绝对路径）。
- `permissions`：白名单；目前只有 `notes:read`。
- `homepage` / `repository`：可选，只接受 `https://`，≤500 字符；插件页渲染成外链按钮。
- `license`：可选，≤40 字符，例如 `GPL-3.0`；只展示文本，不自动推断。
- `features`：可选，最多 8 条，每条 1–120 字符，用于插件详情「能做什么」。
- `icon` / `screenshots`：可选，必须是插件 `site/` 下的相对路径（禁止 `..`、反斜杠、绝对路径）；screenshots 最多 3 个。宿主通过 loopback 站点服务器读取，离线可用；没有 `site/` 的插件回退到内置图标。
- `commands`：最多 20 个；id 唯一；`context` 目前只支持 `note`。
- `settings`：最多 20 个；key 唯一；type 为 `boolean|text|select|number`；`select` 可用 `optionLabels` 显示中文。
- `panels`：最多 5 个；`url` 必须是 `https://` 模板；`{key}` 占位符由宿主填充；尺寸 480–1800 × 320–1200；字段最多 6 个，类型 `text|number`。
- manifest ≤ 64 KiB，入口 JS ≤ 512 KiB。

面板示例（`jiangshan.atlas`）：

```json
{
  "id": "jiangshan.atlas",
  "name": "江山时序",
  "version": "1.0.1",
  "apiVersion": 2,
  "main": "main.js",
  "permissions": ["notes:read"],
  "settings": [
    {
      "key": "lang",
      "label": "地图界面语言",
      "type": "select",
      "options": ["zh", "zh-tw", "en"],
      "optionLabels": { "zh": "简体中文", "zh-tw": "繁體中文", "en": "English" },
      "default": "zh"
    },
    { "key": "lock", "label": "锁定年份（隐藏时间轴）", "type": "boolean", "default": false }
  ],
  "panels": [
    {
      "id": "atlas",
      "title": "江山时序",
      "url": "https://deanchen.me/jiangshan-map/?embed=1&year={year}&place={place}&lang={lang}&lock={lock}",
      "width": 1180,
      "height": 800,
      "fields": [
        { "key": "year", "label": "年份", "type": "number" },
        { "key": "place", "label": "地点", "type": "text" }
      ]
    }
  ],
  "commands": [{ "id": "open", "title": "打开地图", "context": "note", "primary": true, "icon": "map", "when": "history" }]
}
```

命令可以返回 `{ panel: { id, params } }`；宿主按 `panel.id` 找声明、填 URL、打开面板。v1 的 `{markdown}` / `{message}` 仍然有效。

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
├── jiangshan.atlas/
│   ├── plugin.json
│   ├── main.js
│   └── site/
└── ...
```

- 扫描：应用启动/打开插件页/点“重新扫描”时读取。
- 安装内置插件：Rust 把内置插件写到 plugins 目录，已存在不覆盖；目前只剩江山时序。
- 安装自己的插件：把目录放进 plugins，点“重新扫描”。
- 卸载：删除目录 + state 条目；只允许 app data 下的插件目录。
- 失败：manifest 错误显示在插件页；运行错误显示在结果弹窗；不自动禁用（v1），但可以手动禁用。

## 7. 内置示例（已于 P254 移除）

`example.word-count`、`example.heading-audit`、`example.link-audit` 三个测试插件已从代码和内置安装列表移除，只保留 `jiangshan.atlas`。下面 10.2 里关于字数统计/标题体检/链接清单的实测是历史记录，不再能在新安装里复现。

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
- 内置插件：`src-tauri/plugins/examples/jiangshan-atlas`，用 `include_str!` 安装到 app data；三个测试示例已于 P254 移除。

### 10.2 实测

- `cargo test --lib` **292 passed / 0 failed / 8 ignored**（P244 后实跑；P237 时 290；P226 时 275）。
- `npm run build` 通过（P244 main 675.64 kB / gzip 226.42 kB；whiteboard 66.26 kB）。
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
- **面板 v2 已有一个真实插件**：江山时序；面板是跨域 iframe，不是本地 DOM/canvas；命令仍是 Markdown/面板二选一。面板没有双向 postMessage，不能把地图里点选的地点写回笔记；P228 只把面板已经解析出的 `year/place` 存入 front matter。
- **没有 MCP/agent tools**：MCP 作为外部插件运行时的设计在 §1.3 / §2.3，v1 未接。
- **没有签名/市场/审核**：本地插件等同用户信任的代码；启用提示写明“读取整个笔记库”。
- **没有审计表/持久日志**：只在本次结果弹窗和插件页显示错误。
- **Worker 不是内存沙箱**：只按时间 terminate，不能阻止插件吃内存；`notes:read` 也没有 per-note scope。
- **插件状态不同步**：`state.json` 在 app data，换机器要重新启用/配置。
- **未重打安装包**：CSP/Worker 只在 dev 原生窗口验收；安装版要重打后再确认。


## 11. 面板插件 v2 与“江山时序”（2026-11-25，P225）

### 11.1 运行时

- 命令在 Worker 里读笔记，返回 `{ panel: { id, params } }`；宿主用 `PluginPanelHost.vue` 打开面板。
- 面板 URL 只能来自 manifest 的 `https://` 模板；宿主只允许 `https:` frame，并显示域名 +「跨域沙箱」。
- iframe `sandbox="allow-scripts allow-same-origin allow-popups allow-forms"`；地图是独立 origin，不能读页边笔记 DOM，也不能拿 `__TAURI_INTERNALS__`。
- 面板只在命令触发时创建，关闭即销毁；不常驻、不预加载。
- 命令可以声明 `primary:true` + `icon` + `when`（`always`/`history`）；主命令直接出现在笔记标题右侧的操作区，次要命令才进「更多」菜单。
- 面板字段由 manifest 声明，宿主渲染年份/地点等输入；改字段点「定位」才重建 URL，不在每个按键重载。**P237 起江山时序不再走这套字段条**，改用 `AtlasPanel.vue` 的地点清单（见 11.5）；通用字段条仍供其他面板插件使用。
- 25 秒加载超时；超时/错误显示重试和「在浏览器打开」。
- 面板参数只来自笔记的 `year/place/person/event/lang/lock`；**不把笔记正文、标题、标注、token 传给第三方**。

### 11.2 江山时序触发

- 插件：`jiangshan.atlas` v1.0.10（P238 本地补丁：隐藏 embed 激活盾），`apiVersion: 2`，`permissions: ["notes:read"]`。
- 只对历史笔记开放：`领域/历史`，或 `periods` 里存在非日期项；`2025-08` 这类发布日期不算历史（P230 修），技术/财经笔记返回“不是历史内容”。
- 年份：正文前 4,000 字的“公元 23 年/前 202 年”优先；其次 25 条事件地名表（昆阳之战 23/昆阳、壬寅宫变 1542/北京、青苗法 1069/开封等），再其次 `periods`/标题/人物/事件匹配朝代表；匹配不到留空，不再兜底 1100；没有年份时宿主不加载 iframe。单字朝代只允许从 `periods/tags` 匹配，避免“写明/美元”生成 1500/1300。
- 地点：标题/事件先匹配事件地名表和常见古城关隘词表（昆阳、长安、洛阳、赤壁、蓝关等），再扫正文前 4,000 字；没有就留空。
- ~~面板有 4 个字段：年份、地点（都可逗号多站）、人物（可多人）、事件（上游事件键）~~ **P237 已移除这条字段条**：入口改为地图上方一条「本篇地图」chip 横条，点一个 chip 只加载一个地点；多站逗号路线不再出现在 UI。
- 面板「AI 定位」：宿主 `map_context(note_id)` 用当前模型抽 `{year,place,person,event,confidence,reason}`，按内容 SHA-256 缓存到 app data；失败/无 Key 回退规则。地图收 `year/place/person/event/lang/lock`；AI 缓存只补规则缺的年份/地点，不覆盖规则，也不自动注入人物/事件。手改字段存本机并标「上次定位」。
- 命令 `primary:true` + `icon` + `when:history`：历史笔记标题右侧直出「打开地图」，非历史不显示；次要命令才进「更多」。
- 设计、上游调研、离线/双向路线见 [JIANGSHAN_INTEGRATION.md](JIANGSHAN_INTEGRATION.md)。

### 11.3 内置插件升级

- `install_examples` 现在会在**内置 manifest 版本更高**时覆盖 `plugin.json`/`main.js`，保留 `enabled/permissions/settings` 状态；同版本不覆盖用户文件。这保证地图插件迭代能更新到已安装机器。

### 11.4 面板 UI 改版：地点清单 + 单点（2026-11-26，P237）

- 用户反馈字段条（年份/地点/人物/事件/定位/AI 定位/存入笔记/hint）「繁琐且多余」，多站路线也「杂乱不堪」。P237 把 atlas 从通用 `PluginPanelHost` 拆出，改用 `AtlasPanel.vue`：`.plugin-panel-fields` 不再渲染。
- 顶部「本篇地图 · N 处」横条：`extractMapPoints()` 从标题 + `events/periods` + 正文抽地点/事件，事件 chip 给出精确年份（如 隗嚣 32年·陇右、昆阳之战 23年·昆阳），地名 chip 复用规则年份并标「约」；最多 12 个，chip 内只显示 `label` + `年份 · 地点`，超出横向滚动。
- 地图一次只加载一个点：URL 只带该点的 `year/place/event`，`person` 固定为空；点 chip 即切换，不再拼逗号多站路线。
- 选中优先级：**显式 initial（正文 token / 搜索行）> 保存的上下文 > 第一个 chip**。这样从正文点「隗嚣」不会被默认的昆阳覆盖。
- 开关「正文地名可点」在横条右端：`mapLinks` store 持久化到 localStorage（按笔记），`NoteView` 用 TreeWalker 把正文文本节点里的地点/事件包成 `.note-map-token`，点击直接开对应单点；关闭立即 unwrap。实测开启后 12 个 token，点隗嚣开 32/陇右，关闭归零。
- 布局（用户追问“会不会太占地图区域”后改）：原 218px 竖栏在 1136 面板里占 19%，改为 **48px 横向 chip 条**后地图宽度 **918→1134px（+23.5%）**、面积 **+15.0%**；1024 宽下 **806→1024px（+27.1%）**、面积 **+18.4%**；高度只少 48px（−6.9%）。520/1024/1180 三档实测横条 46–48px，无横向溢出；≤900px 隐藏「本篇地图」文字，只留 chip + 开关。
- 可点性：token 下划线原用 `--blue-line`，对纸色对比 **1.44:1**，100% 缩放下几乎看不出可点；改为 `--blue` 65%（**3.17:1**，暗色 1.67→**3.59**），仍用虚线。数值按 sRGB 相对亮度 + WCAG 对比公式计算。
- 安装包：P237 后重打 NSIS **18,429,517 B / 17.58 MiB**，主程序 24,980,480 B；未在安装版上重新验收面板。
- 截图：`p238b-map-rail-1180.png`、`p238b-map-rail-1024.png`、`p238b-map-rail-520.png`（竖栏旧版：`p238-map-list-inline.png`）。

### 11.5 地图打开即可操作（2026-11-26，P238）

- 「点击以操作地图」不是宿主加的，是上游 embed 模式自带的 `#embed-shield`：透明层拦截 `pointerdown/move/up`、`mousedown`、`touch*`、`wheel`、`dblclick`，点一次才加 `is-hidden`；`root` 的 `mouseleave` 又会把它复原。
- 面板本身是弹层主内容，没有正文滚动需要保护，所以本地站点新增 `site/local.css`（紧跟 `style.css` 之后）设 `#embed-shield{display:none!important}`。没有改上游 `app.js`；grep `_embedActive` 全文件只有 2 处，均在该 shield 内部，隐藏后没有其它逻辑依赖。
- 插件版本升到 **1.0.10**：`install_examples` 只在版本更高或站点缺失时重拷，靠版本号让已安装机器拿到补丁；它是手动命令，要进插件页点「安装内置插件」，应用启动不会自动更新。
- 实测：app data 的 `index.html` 已含 `local.css`，HTTP **200 / 570 B**；iframe 内 `#embed-shield` computed `display:none`；**未点地图**在 `#svg-stage-wrap` 上滚轮一次，`#atlas-svg` viewBox `384.6 287.9 384.6 250.0` → `553.0 332.4 247.7 161.0`（放大），证明手势直达地图。
- 安装包：P238 后 NSIS **18,428,120 B / 17.57 MiB**，主程序 24,980,480 B；未在安装版上重新验收。
- **边界**：补丁属于 vendored 站点，重新下载上游站点后要补回 `local.css` 和 `index.html` 的 `<link>`；盾隐藏后滚轮/触摸不再传给宿主（面板里符合预期）；副作用是没了“避免文章滚动被地图抢走”的保护，但只影响本地面板副本。

### 11.6 弹层层级（2026-11-26，P239）

- 原因：插件面板、图谱、制作台三个弹层都是 **z-index 120**，而侧栏右键菜单 200、toast 210、确认框 300、笔记插件菜单 220 都比它高，所以菜单/toast 会穿到弹窗上面。用户反馈“这个之前在其他弹窗里也出现过”。
- 约定：导航 40 / 浮层 60 / 菜单 200–230 / 模态 400 / 模态内确认 420 / toast 500。改动：`.plugin-panel-overlay`、`.graph-overlay`、`.prod-overlay`、`.welcome` → **400**；`.plugin-result-overlay` → **410**；`.confirm-backdrop`、`.prod-conflict-overlay` → **420**；`.toast`、`.prod-toast`、`.plugin-action-toast` → **500**。菜单 `200–230` 不动。
- 实测（dev + CDP）：overlay computed `z-index: 400`；页面上注入 `z-index:220`、`300` 两个测试块后，面板中心 `document.elementFromPoint` 返回 `IFRAME.plugin-panel-iframe`（不是测试块）；再注入 `500` 的块才命中测试块，说明顺序符合约定。
- **边界**：只调层级，没有改菜单打开/关闭逻辑；没有逐弹窗做视觉回归（图谱/制作台只验证了 CSS 值）。

### 11.7 地图内元素默认精简（2026-11-26，P240）

- 默认（P241 起，用户指定）：`朝代时间轴 ✗ / 地点资料卡 ✗ / 图层图例 ✓ / 年份徽标 ✓`。时间轴和资料卡默认隐藏；图例、徽标默认显示；右上「在江山时序里打开」（`#embed-open`）和面板头部的外部打开重复，永久隐藏。
- 顶栏「显示」菜单提供四个复选框（时间轴/资料卡/图例/徽标），状态存 localStorage `yebian.atlas.layers.v1`；打开开关后即时生效，不重载 iframe。默认值在 `AtlasPanel.vue::readLayers()` 里，站点 `<html>` 预挂 `yb-hide-card yb-hide-timeline`。
- 通信：`index.html` 的 `<html>` 预挂隐藏类；`local.css` 只负责 `html.yb-hide-* #target { display:none !important }`；`local.js` 监听 `message`，只接受 `{type:'yebian:atlas-ui', state}`，按布尔值增删 `yb-hide-*` 类。宿主 `AtlasPanel.vue` 在 iframe `load` 和开关变化时 `postMessage` 到 iframe origin。
- 为什么用类而不是内联样式：打开开关只需移除类，元素回到上游原本的 `display`（`#genealogy-card` 是 flex、`#scrubber-dock` 是 grid），不用在补丁里复刻布局。
- 实测（P241 后）：默认 `class="yb-hide-card yb-hide-timeline is-embed"`，资料卡/时间轴 computed `none`，图例/徽标 `flex`；勾选资料卡后 `display:flex`；时间轴打开后回 `grid`。截图 `p241-atlas-default-1180.png`；菜单/旧默认见 `p240-atlas-layers-menu.png`、`p240-atlas-clean-1180.png`。
- **边界**：依赖上游元素 id，重 vendor 时需核对；远端回退站点没有 `local.js`，复选框禁用并提示“不支持”；时间轴默认关后只能用 chip 切换年份；资料卡仍需在地图上点地点才会出现；设置按设备存，不同步；已有 localStorage 选择的设备不会被强制切到新默认。P241 后 NSIS **18,431,021 B / 17.58 MiB**，主程序 24,980,480 B；`npm run build` main 668.80 kB / gzip 224.19 kB、whiteboard 66.26 kB。

### 11.8 地图面板头部去重（2026-11-26，P242）

- AtlasPanel 头部删掉三样重复/技术信息：标题上方的 kicker「江山时序」（与 `<h2>` 重复）、`.plugin-panel-domain`（「本地地图」或 host）、`.plugin-panel-isolation`（「跨域沙箱」）。
- 头部压缩：`.map-panel .plugin-panel-head` `min-height: 44px`（原 54）、padding `6px 8px 6px 14px`、标题 15px（原 16）；保留标题和刷新/外部打开/关闭三个操作。
- 1180 下头部 + chip 条共 **92px**，比之前少 10px，全部给地图；截图 `p242-atlas-compact-1180.png`。P242 后 NSIS **18,428,757 B / 17.58 MiB**，主程序 24,980,480 B；`npm run build` main 668.80 kB / gzip 224.19 kB、whiteboard 66.26 kB。
- **边界**：通用 `PluginPanelHost` 的面板仍显示域名和「跨域沙箱」；地图面板不再显示本地/远端回退状态；标题仍占一行，没有和 chip 条合并（窄到 520 时会挤）。

### 11.9 仅年份匹配（2026-11-26，P243）

- 现象：某笔记清单出现「初始地点 250年」，但正文没有任何地点。`addInitialPoint()` 之前用 `place || '初始地点'` 兜底，把“只有年份”伪装成了地点。
- 数据：该文 `events` 为空、正文不命中 25 条事件表和地名表，唯一信号是 `periods:["三国"]`；worker 的 `PERIODS` 把「三国」映射到代表年 **250**，所以 year 有、place 空。
- 改法：无 place/event 时只生成 `全图` chip（`id=initial-year`，place 为空），清单头显示「仅年份」；若正文已经解析出地点，就不再插全图。worker 的 `inferYear` 改为返回 `{year, source}`，`open` 参数增加 `yearSource`（`explicit`/`event`/`period`），宿主对 `period` 年份标「约」。
- 实测：该文 `本篇地图 / 仅年份`、chip `全图 / 约250年`、iframe `year=250&place=`；《刘秀》仍 `3 处` + 三个地点 chip。插件 **1.0.13**；截图 `p243-atlas-year-only.png`。P243 后 NSIS **18,429,724 B / 17.58 MiB**，主程序 24,980,480 B。
- **边界**：250 是「三国」代表年，不是文章确切纪年；没有地名/事件命中时不会硬造地点，具体位置仍需 AI 定位或手补；只有年份且正文无地点时才出现「全图」入口。

### 11.10 边界

- P236 起地图优先从本地插件站点加载：49 MB 裁剪站点进安装包资源，安装到 app data 后由 127.0.0.1 loopback 服务提供；远端站保留为回退。地图字体仍走 Google Fonts，断网时回退；离线首屏/GeoJSON/Canvas 基准未做。
- 没有双向桥：地图里的点选/人物轨迹不能回写笔记。
- AI 地点抽取已做（P226），P228 又把结果写回笔记 front matter：`地图年份/地点/人物/事件/来源/置信度/依据/内容哈希`；面板优先读笔记，`map_context_save` 保存，跨设备随笔记同步。年份是硬门槛，没有年份不加载地图。P229 把每次现场 `map_context` 调用写入同步表 `production_usage_events`，并进制作台「本月用量」；笔记/缓存命中 0 token 不写；P234 显示最近 8 条单次调用；P235 用内容哈希标 stale。结果仍可能错；账本没有分页/删除/人工复核，P229 之前的调用未回填；AI 成稿不会自动生成地图字段，批量回填用 `tools/map-backfill.mjs` 显式执行。P230 支持多站路线和 person/event 参数，规则召回 12 篇历史笔记里 6 篇年份+地点都有、2 篇只有年份、4 篇都没有；事件表只有 25 条，仍需 AI/手填补。P231 把面板入口扩到出处卡片和白板 agent 卡：`MapPanelLauncher` 读笔记上下文后挂同一个 `PluginPanelHost`；人物轨迹默认关闭（保存 person 不再默认切人物卡）；白板非历史卡隐藏。还修了 `loadPlugins` 并发：20 个入口同时 mount 时共享 `loadPromise`，否则只有第一个组件拿到插件。P232 把入口扩到侧栏搜索结果：历史笔记行右侧显示地图图标，点击走同一 launcher；`MapPanelLauncher` 多根组件补了 `inheritAttrs:false` + `v-bind="$attrs"`，让搜索定位/白板按钮 class 生效。P233 让白板 sticky/link/todo 卡片按文本识别年份/事件后打开只读地图（无 noteId 时没有 AI/存入）；P234 给本月账本加最近 8 条单次调用列表；P235 在 front matter 写 `地图内容哈希`，正文改动后返回 `stale` 并在面板提示重跑。P236 把 49 MB 地图站点作为插件资源随安装包分发，安装后由 loopback HTTP 服务提供，离线可用；NSIS 17.57 MiB。P237 把 atlas 面板从“字段条 + 多站路线”改成“左侧本篇地点清单 + 右侧单点”，并加正文地名可点开关；清单是规则抽取（事件表 25 条 + 地名表），不是 AI 多点识别，年份可能缺失或为约值；新面板暂时没有手动加地点、单点 AI 定位、存入笔记，旧的多站 front matter 仍可读但 UI 不展示。
- 面板插件没有签名/市场；启用本地插件仍等同信任其代码。地图数据/许可归属见 JIANGSHAN 文档（GPL-3.0；CHGIS/Hartwell 边界数据另有限制）。

## 12. 插件页重做（2026-11-26，P244）

- 问题：旧页面是单列大卡片，把介绍、权限、命令、面板、设置、安装路径全部摊开；4 个插件就占满整页，地图插件只有一句技术描述，没有功能、截图和来源链接。
- 调研：VS Code Marketplace / Open VSX 用列表 + 详情页承载 README、仓库和版本；Obsidian 社区插件用列表 + 详情弹层，头部放图标/作者/许可证；ZCode Marketplace 规范把 hero、信息区和高级信息分开。共同点是**列表紧凑、详情丰富、渐进披露、外链真实**。
- 新结构：左侧 300px 已安装列表（搜索、全部/已启用/有面板/示例过滤、状态点、示例标签），右侧详情（hero、官网/仓库、许可证、截图、功能、命令/面板用法、设置、权限、折叠技术信息）。默认选中 `jiangshan.atlas`；窄窗 <900px 左栏变横向列表并自动把选中项滚进可视区。
- manifest 展示字段：`homepage`、`repository`（https，≤500）、`license`（≤40）、`features`（≤8 条，每条 ≤120）、`icon`、`screenshots`（≤3，site 下安全相对路径）。`PluginInfo` 全部透传给前端；旧 manifest 不受影响。
- 状态修复：`enabled && needsPermission` 显示琥珀「缺少权限」而不是绿色「已启用」；统计只算 `enabled && !error && !needsPermission`；设置增加 dirty 提示并在保存后用返回值重置；卸载改行内二次确认。
- 许可纠正：地图站点 `LICENSE` 与 GitHub 均为 **GPL-3.0**，旧文档和 `author` 里的 MIT 是错的；插件 1.0.14 改作者 `DeanChensj`、`license: GPL-3.0`，并在 JIANGSHAN 文档保留 CHGIS/Hartwell 数据限制。
- 实测：默认详情 6 条功能、官网/仓库两个按钮、`GPL-3.0`、截图 `naturalWidth=1000`；搜索「地图」1 行，示例过滤 3 个；禁用字数统计后统计 `3 个已启用 · 3 个命令`，恢复后回 4；设置改动出现「未保存」、保存后消失。`cargo test --lib` **292 passed / 8 ignored**；截图 `p244-plugins-final.png`、`p244-plugins-1024.png`、`p244-plugins-900.png`、`p244-plugins-520f.png`。
- **边界**：没有远程市场、评分、下载量、更新检查和签名；图标/截图只支持插件 `site/` 下的相对路径；详情不渲染 README/Changelog；地图面板仍需在历史笔记里打开；已安装机器需手动点「安装内置插件」才会重拷 1.0.14 站点。P244 后 NSIS **18,551,103 B / 17.69 MiB**，主程序 25,004,544 B；main 675.64 kB / gzip 226.42 kB、whiteboard 66.26 kB。
