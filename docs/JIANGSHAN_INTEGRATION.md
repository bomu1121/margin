# 江山时序整合设计（v1 面板）

> 2026-11-25 · 目标：把开源项目「江山时序」接进页边笔记，按当前历史笔记的年代/地点打开地图；同时把插件系统从“只读命令”扩到“隔离面板”，但不把第三方地图代码塞进主渲染进程。

## 0. 结论

**v1 采用“插件 Worker 解析上下文 + 宿主面板 + 跨域 sandbox iframe 加载官方地图”的方案。**

- 插件代码仍然运行在 sandboxed Web Worker：只拿 `notes:read`，只返回 `year/place`，不接触 DOM、Tauri IPC 或地图 iframe。
- 地图是跨域 HTTPS 页面，放在 `sandbox="allow-scripts allow-same-origin allow-popups allow-forms"` 的 iframe 里；父窗口和地图互不同源，不能读对方 DOM。
- 宿主只把 `year/place/lang/lock` 拼进地图 URL，**不把笔记正文传给第三方**。
- 面板只在用户点命令时创建，关闭即销毁；不预加载、不常驻，避免地图数据/Canvas 占用后台内存。
- v1 不把地图数据打进安装包；安装包仍约 9 MB。离线本地包作为下一阶段，单独评估 48 MB 级资源。

## 1. 上游事实

- 仓库：`DeanChensj/jiangshan-map`（GitHub）
- 许可：**MIT**，允许本地部署与二次开发；需要保留版权/许可声明。
- 形态：纯前端静态站点，原生 JS + Canvas/SVG，无 npm 构建；`index.html` + `js/` + `data/atlas/` + `assets/terrain/`。
- 规模（本机 clone 实测）：仓库 128 MB；运行所需裁剪后约 **48 MB**（`data` 34 MB、`js` 7.9 MB、`terrain` 6.1 MB、css/index 约 0.25 MB）。音频/预览图/hero 约 30 MB，可删。
- 覆盖：公元前 403 年—2026 年，18 个时期，200+ 幅疆域/政区快照，757 处城池地标，125+ 人物行迹。
- 嵌入：`?embed=1` 隐藏导航/搜索，只保留地图、精简时间轴和标题牌；支持 `year/place/person/event/hover/from/to/lock/zoom/lang`。
- 深度链接：`?year=819&place=蓝关`；跨年路线用 `year=1079,1080,1094,1097&place=湖州,黄州,惠州,儋州`。
- 机器接口：`window.atlasAPI.locate/inspect/resolvePlace/openPerson/search`；另有零依赖 stdio MCP（`scripts/mcp_server.js`）和 CLI 截图脚本。
- **embed 模式会隐藏 `#map-search-box`**。所以宿主必须提供年份/地点输入，不能把搜索完全交给 iframe。

来源：仓库 `README_ZH.md`、`llms.txt`、`js/atlas_api.js`、`css/style.css` 的 `.is-embed` 规则。

## 2. 为什么 v1 不直接打包 48 MB 地图

| 方案 | 安装包 | 离线 | 更新 | 隔离 | 结论 |
| --- | --- | --- | --- | --- | --- |
| 跨域 iframe 加载官方站 | 不变（约 9 MB） | 否 | 跟随上游 | 跨域 + sandbox，最强 | **v1** |
| 全量打包静态站 | +约 48 MB 资源（压缩后仍显著增大） | 是 | 跟 app 版本 | 本地 iframe/自定义协议，需要路径校验 | 二期 |
| 首次启动下载压缩包 | 不变 | 下载后可离线 | 需版本清单/哈希 | 同本地包 | 二期候选 |
| 只接 MCP/CLI，不嵌地图 | 不变 | 否 | 跟随上游 | 无 UI 隔离问题 | 不满足“地图在笔记里看” |

v1 先验证**触发时机、隔离、面板交互、设置与错误态**；这些是长期资产。地图数据是否离线化，不应阻塞面板协议。

## 3. 架构

```
笔记页
  └─ 「插件」菜单
       └─ jiangshan.atlas 命令
            └─ sandboxed Worker（notes:read）
                 └─ margin.notes.read(noteId)
                      → { title, periods, people, events, tags }
                 └─ inferYear / inferPlace
                      → { panel: { id: "atlas", params: { year, place } } }

宿主 PluginPanelHost
  ├─ 校验插件声明的 panel（只允许 https）
  ├─ 字段条：year / place（可编辑，点「定位」重载）
  ├─ URL 模板：https://deanchen.me/jiangshan-map/?embed=1&year={year}&place={place}&lang={lang}&lock={lock}
  └─ 跨域 sandbox iframe（allow-scripts allow-same-origin allow-popups allow-forms）
       └─ 地图自己的 Canvas / 时间轴 / 点地溯源
```

数据边界：

- 插件 Worker 能读笔记，但只能返回 `year/place` 等原始值；宿主不会把 `body` 拼进 URL。
- iframe 不知道笔记 id、标题、正文、标注、事实或 token。
- 关闭面板即销毁 iframe；地图的 DOM/Canvas/网络缓存不常驻主界面。

## 4. Manifest 扩展

`apiVersion: 2` 新增可选 `panels`；v1 命令插件继续兼容。

```json
{
  "id": "jiangshan.atlas",
  "name": "江山时序",
  "version": "1.0.0",
  "apiVersion": 2,
  "main": "main.js",
  "permissions": ["notes:read"],
  "settings": [
    { "key": "lang", "label": "地图界面语言", "type": "select", "options": ["zh", "zh-tw", "en"], "default": "zh" },
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
        { "key": "year", "label": "年份", "type": "number", "placeholder": "如 741；公元前用负数" },
        { "key": "place", "label": "地点", "type": "text", "placeholder": "如 长安 / 昆阳 / 蓝关" }
      ]
    }
  ],
  "commands": [
    { "id": "open", "title": "在江山时序中查看", "context": "note" }
  ]
}
```

约束：

- `panels` 最多 5 个；`id` 与命令结果里的 `panel.id` 对应。
- `url` 必须是 `https://`；`{key}` 占位符由宿主用设置、命令参数和面板字段填充。
- 面板尺寸限制在 480–1800 × 320–1200。
- 面板字段类型只支持 `text|number`，最多 6 个。
- 面板 URL 不能包含 `javascript:`、`data:`；不开放本地文件 URL。

## 5. 触发时机

### 5.1 v1（已实现）

- 入口：笔记标题右侧直接「打开地图」（命令 `primary:true`）；不再放在「字数 · 作者」元信息行，也不需要先打开插件菜单再选。次要命令才进「更多」。
- 只对**历史笔记**开放：`tags` 里有 `领域/历史`，或 `periods` 非空；技术/时事笔记返回“不是历史内容”，避免把“美元”里的“元”误判成元朝。
- 年份：从 `periods`/标题/人物/事件匹配朝代表（明嘉靖→1540、东汉末→200、北宋→1100 等），匹配不到用 1100 预览。
- 地点：先从标题/事件里匹配常见古城关隘词表（昆阳、长安、洛阳、赤壁、蓝关等），再从正文前 4,000 字匹配；没有就留空，由面板字段手填。
- 面板字段可改年份/地点，点「定位」重载地图；不要求重新跑插件。

### 5.1.1 AI 定位（P226，已实现）

- 面板字段旁新增「AI 定位」；点击后宿主调用 `map_context(note_id)`，用当前配置的模型把笔记压成 `{year, place, person, event, confidence, reason}`。
- 只根据笔记内容；不是历史笔记、无 API Key、模型输出不合法时返回错误，面板保留规则值，用户可手改。
- 结果按 `标题 + 正文 + tags + periods + people + events` 的 SHA-256 缓存到 app data `map-context-cache.json`；同一篇内容不变时第二次 0 模型调用；设备本地，不进同步，也不发给地图。
- 地图 iframe 仍只接收 `year/place/lang/lock`，不接收笔记正文。
- 实测《刘秀的军事能力》：首次 **6s**，`year=23, place=昆阳, confidence=0.86`；第二次 `cached=true`；地图跳到公元 23 年昆阳并展开点地溯源。面板再次打开会自动复用 AI 缓存（`AI · 缓存`），手改过的年份/地点存本机并在下次打开显示 `上次定位`。

### 5.2 二期

- **AI 上下文**：点地图时用当前模型从标题/正文/claims 抽出 `{year, places, person, event}`，结果缓存（note id + 内容 hash）；失败或低置信时回退 v1 规则。这样不再依赖有限词表，但会增加一次模型成本和延迟。
- **写回型上下文**：AI 成稿流程为历史笔记生成 `地图:` front matter（`year/place/person/event`），稳定、可审阅；需要改成稿模板与回填工具。
- **其他入口**：出处卡片（史料地名）、白板卡片（粘贴的古城/古战场）、全局搜索；都走同一个面板协议。
- **双向**：在地图 iframe 内选中地点/人物后，通过 `postMessage` 回传宿主（需要地图侧支持或 wrapper 注入 `atlasAPI`），用于把地名写进笔记/出处。v1 不做，避免把第三方 DOM 事件当数据源。

## 6. 面板交互与视觉

- 桌面：居中大浮层，默认 1180×800；标题行显示插件名、面板名、域名和「跨域沙箱」徽标；右上角重新加载/浏览器打开/关闭。
- 字段条：年份、地点两个短输入 + 主按钮「定位」；改字段不会在每个按键上重载 iframe。
- 加载态：地图首次打开可能几秒，显示纸张色底 + spinner + “首次打开可能需要几秒”；关闭/重新加载都在宿主控制。
- 窄屏（≤900）：全屏面板，字段条换行；地图不缩小到不可用。
- 地图本身使用自己的宣纸设色；宿主只提供中性 chrome，不覆盖第三方 CSS。
- 插件页：显示“面板”“域名”“跨域沙箱”，并保留设置表单；插件状态只存本机。

## 7. 安全与隐私

- 命令插件：Worker 无 DOM/Tauri/Node；`notes:read` 默认拒绝；网络原语被覆盖。
- 面板插件：iframe 跨域；父页不能读地图 DOM，地图不能读父页；CSP `frame-src 'self' https://deanchen.me`。
- 只向地图发送 `year/place/lang/lock`；不发送笔记正文、标题、人物列表、标注、事实、token。
- `sandbox` 不含 `allow-top-navigation`；地图不能把整个应用导航走。
- 第三方地图可能设置自己的 cookie/localStorage（属于 `deanchen.me`），不与页边笔记共享登录态。
- 面板 URL 由 manifest 声明，插件启用即信任；本地插件仍等同用户信任的代码。
- MIT 归属：插件描述/面板页显示“DeanChensj / 江山时序（MIT）”；本地离线包需随包附 LICENSE。

## 8. 性能

- v1 不在安装包里放地图；安装包保持约 9 MB。
- iframe 只在命令触发时创建；关闭即销毁，不常驻。
- 首次打开依赖网络；地图自身的 GeoJSON/terrain 由上游按需加载，不受页边笔记索引影响。
- 面板字段改动只重建 iframe URL，不重新跑插件 Worker。
- v1 没有预加载/离线缓存；如果后面做本地包，需要单独测首屏、GeoJSON 解析、Canvas 内存和冷启动。

## 9. 实测（2026-11-25，安装版）

- 插件：`jiangshan.atlas` v1.0.4，`apiVersion 2`；安装内置插件后启用并授权 `notes:read`，`plugins_list` 返回 `enabled=true`、`panels` 1 个；内置版本升级到 1.0.4 时覆盖 `plugin.json/main.js`，保留启用/权限状态（单测 `bundled_plugin_upgrade_replaces_files_and_keeps_state`）。
- 入口：历史笔记《刘秀的军事能力：昆阳和隗嚣两次，最后都是他自己上手》→ 标题下「插件」菜单 →「在江山时序中查看」。
- 插件推断：`periods=东汉初` + 标题「昆阳」→ `year=30, place=昆阳`；URL 为 `https://deanchen.me/jiangshan-map/?embed=1&year=30&place=昆阳&lang=zh&lock=0`。地图正常渲染，点地溯源卡片直接展开「昆阳 · 河南省 · 今平顶山叶县」。
- 面板：1180×800（安装版窗口 1491×980 下）；1024×760 下自动全屏；标题、域名、跨域沙箱、年份/地点字段、加载/错误/重试、浏览器打开、关闭均可用。
- 字段重定位：把年份/地点改成 `208 / 赤壁` 点「定位」，iframe src 变为 `...year=208&place=%E8%B5%A4%E5%A3%81&lang=zh&lock=0`；关闭后 `.plugin-panel` 从 DOM 消失，iframe 销毁。
- 非历史笔记（`领域/半导体`）命令返回「这篇笔记不是历史内容，江山时序没有可定位的年代。」，不打开面板。
- 构建：`cargo test --lib` **275 passed / 8 ignored**（插件 6 + map_context 2）；`npm run build` 通过；安装包 `9,495,473 B` / 主程序 `24,840,192 B`（installer SHA256 `2E1DE9D6…`，安装后 exe `1FEB280C…`）。
- 截图：`shots/audit/p225-map-panel.png`（1180×800 地图面板）、`p225-map-panel-1024-full.png`（1024 全屏）、`p214-plugins-map-card.png`（插件页）。

**还没测/限制**

- 没有测离线首屏；官方站不可达时只显示超时/错误 + 浏览器打开。
- 首次加载依赖网络；没有把 48 MB 本地包打进安装包。
- 1024×760 下已测：面板切换为全屏（≤1100px 媒体查询），iframe 宽度 1024、无横向溢出；地图时间轴完整显示（`p225-map-panel-1024-full.png`）。
- AI 定位是单次模型推断，可能把地点/年份判错；当前没有 token/成本账本，也没有人工复核表。规则词表仍约 100 个，AI 失败时回退到它。
- 没有双向桥，地图里的点选不会写回笔记；AI 结果不会自动写进 front matter。

## 10. 下一阶段决策点

1. **离线**：是否接受 +约 48 MB 安装包（或首次下载 30–50 MB 缓存）？
2. **上下文**：是否接受每次首次打开调用一次模型（成本/延迟）来换更准的年份/地点？
3. **双向**：是否需要从地图回写地点到笔记/出处；这需要地图侧 postMessage 或 wrapper，不能靠跨域 DOM 读取。

## 11. 参考

- 仓库：<https://github.com/DeanChensj/jiangshan-map>
- 在线：<https://deanchen.me/jiangshan-map/>
- 嵌入说明：仓库 `README_ZH.md` 的「嵌入到你的网页」
- 机器接口：`llms.txt`、`js/atlas_api.js`、`scripts/mcp_server.js`
- 许可：仓库 `LICENSE`（MIT）
