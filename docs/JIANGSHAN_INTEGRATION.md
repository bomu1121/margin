# 江山时序整合设计（v1 面板）

> 2026-11-25 · 目标：把开源项目「江山时序」接进页边笔记，按当前历史笔记的年代/地点打开地图；同时把插件系统从“只读命令”扩到“隔离面板”，但不把第三方地图代码塞进主渲染进程。

## 0. 结论

**v1 采用“插件 Worker 解析上下文 + 宿主面板 + 跨域 sandbox iframe 加载官方地图”的方案。**

- 插件代码仍然运行在 sandboxed Web Worker：只拿 `notes:read`，只返回 `year/place`，不接触 DOM、Tauri IPC 或地图 iframe。
- 地图是跨域 HTTPS 页面，放在 `sandbox="allow-scripts allow-same-origin allow-popups allow-forms"` 的 iframe 里；父窗口和地图互不同源，不能读对方 DOM。
- 宿主只把 `year/place/lang/lock` 拼进地图 URL，**不把笔记正文传给第三方**。
- 面板只在用户点命令时创建，关闭即销毁；不预加载、不常驻，避免地图数据/Canvas 占用后台内存。
- v1 不把地图数据打进安装包；P236 已改为把裁剪后的约 49 MB 站点作为插件资源随安装包分发（NSIS 17.57 MiB），安装时复制到 app data，由本机 loopback 服务加载。

## 1. 上游事实

- 仓库：`DeanChensj/jiangshan-map`（GitHub）
- 许可：**GPL-3.0**（站点 `LICENSE` 与 GitHub 仓库一致）；允许本地部署与二次开发，分发修改版需按 GPL 提供源码。`data/atlas` 的 CHGIS/Hartwell 边界数据另有学术使用限制，需署名且不可商用，详见 `site/data/atlas/README.md`。
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
- 年份：从 `periods`/标题/人物/事件匹配朝代表（明嘉靖→1540、东汉末→200、北宋→1100 等）；匹配不到就留空，不再用 1100 预览。
- 地点：先从标题/事件里匹配常见古城关隘词表（昆阳、长安、洛阳、赤壁、蓝关等），再从正文前 4,000 字匹配；没有就留空，由面板字段手填。
- 面板有年份/地点/人物/事件四个字段，年份和地点都支持逗号多站；点「定位」重载地图，不要求重新跑插件。**没有年份时不加载 iframe**，避免地图默认显示公元前 403 年造成“定位成功”的错觉。

### 5.1.1 AI 定位（P226，已实现）

- 面板字段旁新增「AI 定位」；点击后宿主调用 `map_context(note_id)`，用当前配置的模型把笔记压成 `{year, place, person, event, confidence, reason}`。
- 只根据笔记内容；不是历史笔记、无 API Key、模型输出不合法时返回错误，面板保留规则值，用户可手改。
- 结果按 `标题 + 正文 + tags + periods + people + events` 的 SHA-256 缓存到 app data `map-context-cache.json`；同一篇内容不变时第二次 0 模型调用；设备本地，不进同步，也不发给地图。
- 地图 iframe 仍只接收 `year/place/person/event/lang/lock`，不接收笔记正文；`event` 只接受上游事件键，AI 的事件中文名会先映射，未知名称不会发过去。
- 实测《刘秀的军事能力》：P226 首次 **6s**，`year=23, place=昆阳, confidence=0.86`；P228 重跑同一篇，模型返回 `year=null, place=昆阳, confidence=0.72`（prompt 2,098 / completion 1,050，6.9s），说明模型不会稳定补外部年份。第二次同内容命中缓存，0 模型调用；面板再次打开会自动复用（`AI · 缓存`）。
- 面板显示本次 AI 的 confidence、prompt+completion tokens、耗时、可用的 provider cost 和依据；结果只是候选，点「存入笔记」后才持久化。

### 5.1.2 写回笔记与数据层（P228，界面已被 P237 取代）

> P237 起地图面板不再显示年份/地点/人物/事件输入框和定位/AI/保存字段条；改成左侧「本篇地图」清单 + 单点地图 + 正文地名开关。下面这些 front matter 与读取优先级仍然有效，但 UI 以 9.11 为准。

- front matter 使用扁平键，避免 `split_front_matter` 处理嵌套 YAML：
  `地图年份 / 地图地点 / 地图人物 / 地图事件 / 地图来源 / 地图置信度 / 地图依据 / 地图内容哈希`。
- 读取优先级：**笔记 front matter > 规则 > 缓存补全 > 现场模型调用**。笔记里已有 `地图年份/地点` 时，打开面板 4ms 返回，不再调用模型；规则已有年份/地点时，缓存只补空缺，不覆盖规则，也不把缓存的 person/event 塞进 URL（否则会从地点卡切成人物卡）。
- 面板字段条显示「笔记 · AI/手动 · 86% · 1.2k tok · 6.9s · 依据」；「存入笔记」把当前年份/地点/人物/事件/来源/置信度/依据写进 front matter，并立即按新值重载地图。
- **多站路线**：年份/地点字段是文本，可写 `1079,1080,1094` / `湖州,黄州,惠州`，宿主原样编码给地图；front matter 保存为 `"1079,1080,1094"`（带逗号时加引号），读取时 `地图年份` 保留原始路线、`year` 取第一站。人物可逗号多人；事件字段接受 `anshi/jingkang/qintongyi/chuhan/changzheng`，未知中文事件名不会当作键发送。
- **年份是硬门槛**：没有年份时显示空态，不加载 iframe；地点可空，但面板会提示“地图只按年份显示”。这修掉了之前 `year=1100` 兜底和空年份默认跳到公元前 403 年的误导。
- 写入走 `vault::write_map_fields`：保留 BOM、CRLF、标题/tags/正文与其他 front matter；旧 `地图*` 键先删再写，清空地点不会留旧值；写入是原子替换。
- 回填工具：`tools/map-backfill.mjs`。默认只扫描历史笔记且不调用模型；`--preview --limit=N` 调用模型但不写；`--apply --limit=N --min-confidence=0.8` 只写同时有年份、地点且达到置信度阈值的结果；`--only=<noteId>` 可只审/只写一篇。避免“一键把所有历史观评论都写错地图”。

### 5.2 后续阶段

- **AI 上下文**：P226 已实现（见 5.1.1）；P228 补了单篇写回和成本展示，P229/P234 已并入月度账本并显示最近 8 条单次调用。剩余是**AI 成稿自动生成 `地图*`**（当前只有回填工具）、人工复核清单和历史调用回填。
- **写回型上下文**：P228 已完成单篇「存入笔记」；P235 起带内容哈希并提示 stale；AI 成稿流程还没有自动写入，避免每篇成稿都多一次定位成本和未审核的猜测。
- **其他入口**：P231 已接**出处卡片**（按当前笔记的保存/缓存上下文打开）和**白板里带 noteId 的 agent 卡**；P232 已接**侧栏搜索结果的「地图」图标**；P233 已给可推出年份的 sticky/link/todo 卡片加只读地图。图片/agent 卡走各自逻辑；没有裸年份识别。
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
- GPL 归属：插件页显示作者 `DeanChensj` 与许可证 `GPL-3.0`；本地离线包随包附 `LICENSE`。CHGIS/Hartwell 数据限制需在文档中保留。

## 8. 性能

- v1 不在安装包里放地图；安装包保持约 9 MB。
- iframe 只在命令触发时创建；关闭即销毁，不常驻。
- 首次打开依赖网络；地图自身的 GeoJSON/terrain 由上游按需加载，不受页边笔记索引影响。
- 面板字段改动只重建 iframe URL，不重新跑插件 Worker。
- P236 起有本地站点；首屏、GeoJSON 解析、Canvas 内存和冷启动的独立基准仍未做。

## 9. 实测

### 9.1 P226（2026-11-25，安装版）

- 插件：`jiangshan.atlas` v1.0.4，`apiVersion 2`；安装内置插件后启用并授权 `notes:read`，`plugins_list` 返回 `enabled=true`、`panels` 1 个；内置版本升级到 1.0.4 时覆盖 `plugin.json/main.js`，保留启用/权限状态（单测 `bundled_plugin_upgrade_replaces_files_and_keeps_state`）。
- 入口：历史笔记《刘秀的军事能力：昆阳和隗嚣两次，最后都是他自己上手》→ 标题下「插件」菜单 →「在江山时序中查看」。
- 插件推断：`periods=东汉初` + 标题「昆阳」→ `year=30, place=昆阳`；URL 为 `https://deanchen.me/jiangshan-map/?embed=1&year=30&place=昆阳&lang=zh&lock=0`。地图正常渲染，点地溯源卡片直接展开「昆阳 · 河南省 · 今平顶山叶县」。
- 面板：1180×800（安装版窗口 1491×980 下）；1024×760 下自动全屏；标题、域名、跨域沙箱、年份/地点字段、加载/错误/重试、浏览器打开、关闭均可用。
- 字段重定位：把年份/地点改成 `208 / 赤壁` 点「定位」，iframe src 变为 `...year=208&place=%E8%B5%A4%E5%A3%81&lang=zh&lock=0`；关闭后 `.plugin-panel` 从 DOM 消失，iframe 销毁。
- 非历史笔记（`领域/半导体`）命令返回「这篇笔记不是历史内容，江山时序没有可定位的年代。」，不打开面板。
- 构建：`cargo test --lib` **275 passed / 8 ignored**（插件 6 + map_context 2）；`npm run build` 通过；安装包 `9,495,473 B` / 主程序 `24,840,192 B`（installer SHA256 `2E1DE9D6…`，安装后 exe `1FEB280C…`）。
- 截图：`shots/audit/p225-map-panel.png`（1180×800 地图面板）、`p225-map-panel-1024-full.png`（1024 全屏）、`p214-plugins-map-card.png`（插件页）。

### 9.2 P228 写回与可信化（2026-11-26，dev 原生窗口）

- 插件升级到 `jiangshan.atlas` **v1.0.6**；宿主新增 `map_context_save`，面板新增「存入笔记」。
- 已保存笔记优先：`map_context(note_id, cachedOnly=true)` 读 front matter **4ms** 返回 `source=note, origin=manual, year=23, place=昆阳, confidence=1`，`promptTokens=0`、`completionTokens=0`；不再调用模型。地图 URL 为 `?embed=1&year=23&place=昆阳&lang=zh&lock=0`，重开面板 hint 为「笔记 · 手动 · 100% · …」。
- AI 实测（《刘秀的军事能力》，2026-11-26）：prompt **2,098** / completion **1,050** tok，**6,917ms**，`confidence=0.72`，`year=null`、`place=昆阳`、`person=刘秀`、`event=昆阳之战`；模型没有补外部年份。将年份手填为 23 后保存为 manual，reason 写明「年份为人工补全」。这比 P226 那次 `year=23/0.86` 更保守，说明同篇重跑会有模型差异。
- front matter 落盘（CRLF 笔记）：`地图年份: 23`、`地图地点: "昆阳"`、`地图人物: "刘秀"`、`地图事件: "昆阳之战"`、`地图来源: "manual"`、`地图置信度: 1`、`地图依据: "…"`；标题/tags/正文未动。服务端同步到 `seq 11647`，blob hash `85c49ab6f089…`，抽查 blob 含上述 7 个键。
- 无年份时不再加载 iframe：年份/地点都空的历史观评论显示「还没有可定位的年份」空态，hint「先填年份，或点 AI 定位。」；`hasIframe=false`。只缺地点时仍按年份加载，hint 变金色「地点为空；地图只按年份显示，可补地点后定位」。
- 1180×800 面板 `clientWidth=scrollWidth=1178`、`clientHeight=scrollHeight=798`；1024×760 全屏下字段条不横向溢出，年份输入 160px，三个按钮「定位 / AI 定位 / 存入笔记」可见。
- 回填工具 `tools/map-backfill.mjs` 默认扫描：**13 篇**历史笔记没有保存的地图上下文；`--preview --limit=2` 实际调用模型，两篇都因无年份/地点被跳过（3,789 + 584 = **4,373 tok**），**0 写入**。全量 `--preview --limit=13` 用 **34,843 tok**，12 篇 no-year/no-place，只有《贞观老百姓…》得到 `629 / 关中 / 0.83 / 李世民 / 贞观之治`；`--apply --only=yn-11d02dc932ef676ccf76778f` 命中缓存、**0 tok** 写入该篇，重开面板显示「笔记 · AI · 83%」。
- 构建：`cargo test --lib` **285 passed / 0 failed / 8 ignored**（P228 新增 map_context 2 + vault 3）；`npm run build` 通过（main 681.23 kB / gzip 228.94 kB）。
- 截图：`shots/audit/p228-map-note-context.png`（23 年昆阳 + 笔记 hint）、`p228-map-ai-note.png`（629 年关中 + AI 回填 hint）、`p228-map-empty.png`（无年份空态）、`p228-map-empty-1024.png`（1024 空态）、`p228-map-ai-note-1024.png`（1024 地图 + 三按钮）。

### 9.3 P229 地图定位并进本月账本（2026-11-26）

- `map_context` 每次**现场模型调用**后，把 usage 写进新表 `production_usage_events`（`kind=map_context`、笔记标题、provider/model、prompt/completion tokens、估算 CNY、priced、created_at）；笔记/缓存命中 0 token，不写事件。
- 价格走现有 `ProductionService::price`：DeepSeek 目录价 -> 本次 prompt **2,079**（其中 cached 1,920）+ completion **357**，估算 **¥0.001755**，`priced=true`；目录价拿不到时只记 tokens，金额为 0。
- 账本实测：调用前 `spend=15.6376 / tokens=15,915,761 / usageEvents=0`；调用后 `spend=15.6394 / tokens=15,918,197 / usageEvents=1`，`jobs` 仍是 201（没有伪造任务）。设置页「本月用量」显示 **1 次单次调用**，`byModel` 的 jobs 从 192→193（含一次单次调用）。
- 事件已同步到服务器 `seq 11649`（`table:production_usage_events`，id `usage_eceb…`，estimated_cost `0.001755432`）；`production_usage_events` 加进 `SYNCED_TABLES`，两台设备共享同一份本月支出。截图 `shots/audit/p229-map-usage-ledger.png`。
- 自动预算闸门读同一个 `usage_summary`，所以地图调用会算进 `auto_settings.monthly_spend_cny`；当前预算 ¥100、已用 15.6%，没有触发暂停。
- 测试/构建：`cargo test --lib` **286 passed / 0 failed / 8 ignored**（新增 `one_off_usage_events_join_the_monthly_ledger`）；`npm run build` 通过（main 681.57 kB / gzip 229.04 kB）。
- **边界**：P229 上线前的 map 调用（刘秀 3,148 tok、两次回填预览 4,373 + 34,843 tok）没有回填账本；账本只有汇总，没有逐条调用列表；旧版本应用不认识新表，要更新后才同步/显示；`byModel.jobs` 现在混入单次调用计数，但顶部 `jobs` 仍是真实任务数。

### 9.4 P230 多站路线、人物/事件与规则召回（2026-11-26）

- 插件 **v1.0.8** 的 panel 从 2 个字段扩到 4 个：年份、地点（都可逗号多站）、人物（可多人）、事件（上游事件键）；URL 模板加 `person={person}&event={event}`。宿主把 `event` 中文名映射为 `anshi/jingkang/…`，未知名称省略，避免把“昆阳之战”当事件键发给上游。
- **规则召回**：新增 25 条可审计事件地名表（昆阳之战 23/昆阳、赤壁 208/赤壁、壬寅宫变 1542/北京、青苗法 1069/开封、安史之乱 755/长安等）。对当前 14 篇历史笔记跑规则：2 篇日期型 `periods`（2025-08）被拒绝，12 篇中 **6 篇年份+地点都有、2 篇只有年份、4 篇都没有**；刘秀从 P228 的 `year=30/昆阳` 变成 **23/昆阳**，壬寅宫变从 period 约 1540 变成 **1542/北京**，青苗法从无地点变成 **1069/开封**。复测截图 `p230-map-rule-recall.png`。
- **假阳性修复**：单字朝代（明/元/清）只允许从 `periods/tags` 匹配，不再从标题匹配；`写明`、`美元` 中的“明/元”不再生成 1500/1300。`periods` 里的 `2025-08` 这类发布日期不再算历史，`领域/法律` 笔记不再显示「打开地图」。
- **缓存策略**：打开面板时规则值优先；AI 缓存只补缺失的年份/地点，不覆盖规则已有值，也不自动注入 person/event。实测壬寅宫变保持规则 `1542/北京`，不再被旧缓存（`1542/无地点`）覆盖成空地点。
- **多站实测**：输入 `1079,1080,1094` / `湖州,黄州,惠州`，iframe src 为 `year=1079%2C1080%2C1094&place=%E6%B9%96%E5%B7%9E%2C%E9%BB%84%E5%B7%9E%2C%E6%83%A0%E5%B7%9E`；地图渲染 3 站路线、全程 1318 km（`p230-map-route.png`）。front matter 路线 round-trip 由单测覆盖（`地图年份: "1079,1080,1094"`）。
- **人物参数**：P230 曾默认把保存的 `person` 发进 URL，结果地图从地点卡切成人物卡；P231 改为字段旁默认关闭的「人物轨迹」勾选。勾选后 URL 带 `person=刘秀`，地图显示人物小结/生平大事（`p230-map-person.png` / `p231-map-person-toggle.png`）；默认仍按年份/地点显示。
- **1024**：四个字段 + 三个按钮在 1024 全屏下保持一行，年份/地点输入 150px；截图 `p230-map-rule-1024.png`。
- 测试/构建：`cargo test --lib` **288 passed / 0 failed / 8 ignored**（新增 route round-trip/save 路径）；`npm run build` 通过（main 682.55 kB / gzip 229.41 kB）。
- **边界**：规则仍只认 25 个事件和约 100 个地名，12 篇里仍有 4 篇无年份；AI 仍可能只给地点不给年份；多站路线目前要手填，AI 不保证返回路线；P231 已把 person 改为默认关闭的「人物轨迹」勾选；P232 全局搜索、P236 离线包已做，双向桥仍未做。

### 9.5 P231 出处卡片与白板入口（2026-11-26）

- 新增可复用 `MapPanelLauncher`：接受 `noteId`，先读该笔记已保存/缓存的 `{year,place,person,event}`，再挂同一个 `PluginPanelHost`；插件未启用/未安装时按钮自动隐藏。
- **出处卡片**：历史笔记每条出处的动作行新增「地图」；在《贞观老百姓…》的 20 条出处上实测，点击后直接打开 `629/关中/李世民/贞观之治`，hint「笔记 · AI · 83%」，默认显示地点卡而不是人物卡。
- **人物轨迹**：字段条新增默认关闭的「人物轨迹」勾选；不勾选时 URL `person=` 为空，勾选后带 `person=李世民` 并切到人物小结/生平大事。这样保存 person 不会默认劫持地点视图。
- **白板**：`BoardCard` 的 agent 卡在 `noteId` 存在时显示「地图」；导入 `plugins.css`，复用同一个面板。非历史笔记（当前白板上的 Agent-Autoloop 研究稿）会被 `historicalOnly` 隐藏，不显示空入口；历史 agent 卡走同一条已测面板路径。
- **插件加载并发 bug**：一页 20 条出处会同时挂 20 个 launcher，旧 `loadPlugins()` 在 `plugins.loading` 时直接 return，导致只有第一个组件拿到插件、其余 19 个永久隐藏。改成共享同一个 `loadPromise`，所有 await 者都拿到最终列表；复测 20/20 launcher 出现（修复前 0）。
- **1024**：四个字段 + 人物轨迹勾选 + 三个按钮在 1024 全屏下一行，截图 `p231-map-citation-1024.png`；白板窗口导入插件 CSS 后视觉未改。
- 测试/构建：`cargo test --lib` **288 passed / 0 failed / 8 ignored**（本轮只改前端）；`npm run build` 通过（main 665.99 kB / gzip 223.24 kB）。
- 截图：`p231-map-citation.png`（出处打开 629/关中）、`p231-map-person-toggle.png`（人物轨迹）、`p231-map-citation-1024.png`（1024）、`p231-whiteboard.png`（白板无回归）。
- **边界**：P231 时全局搜索入口未做（P232 已接）；白板只有带 noteId 的 agent 卡有入口，便签/链接卡没有；出处卡片只是复用笔记上下文，不按单条引文推断地点；没有保存上下文的出处笔记会打开空面板；人物轨迹默认关闭，需手动勾选。

### 9.6 P232 全局搜索入口（2026-11-26）

- 侧栏搜索结果行改为 `div role=button`（原来是 button，不能嵌套按钮），右侧加一个默认半透明的「地图」图标；只有 `领域/历史` 或非日期 `periods` 的结果显示，点击不触发打开笔记。
- 点击后走同一个 `MapPanelLauncher`：读笔记保存/缓存上下文，再挂 `PluginPanelHost`。实测搜索「贞观」唯一结果出现地图图标，点击打开 `629/关中/李世民/贞观之治`，hint「笔记 · AI · 83%」；没有上下文的搜索笔记会打开空面板。
- `MapPanelLauncher` 是多根组件，之前给它的 `class`（搜索定位/白板按钮样式）没有落到根 button；改为 `inheritAttrs: false` + `v-bind="$attrs"`，搜索图标和白板按钮样式都生效。
- `cargo test --lib` **288 passed / 0 failed / 8 ignored**（只改前端）；`npm run build` 通过（main 666.42 kB / gzip 223.39 kB）。
- **边界**：只接了侧栏搜索，不是全局命令面板；结果行只按 note 的 tags/periods 判断历史，不做正文级历史识别；没有保存地图上下文的搜索结果会打开空面板；白板自由便签/链接卡的文本定位器仍未做。

### 9.7 P233 白板自由卡片的文本定位（2026-11-26）

- 新增 `src/lib/mapText.ts`：从卡片文字里识别「公元/前 X 年」、25 条事件地名表、39 个多字朝代/时期和古城关隘词表；单字朝代（明/元/清）不参与，`写明`/`美元` 不会生成年份。
- `BoardCard` 的 sticky/link/todo 卡片头部，在能推出年份时显示小地图图标；`MapPanelLauncher` 新增 `text`/`requireYear`，没有年份的卡片直接隐藏，不用空入口。卡片打开的是只读地图面板（没有 `noteId`，所以没有 AI 定位/存入笔记）。
- 实测：临时便签「昆阳之战 公元23年」出现地图按钮，打开 `23/昆阳`（`p233-map-sticky.png`）；测试后删除该便签。1040 白板窗口里四字段 + 人物轨迹 + 定位一行，无横向溢出；没有 noteId 时 AI/存入按钮按设计不出现。
- `cargo test --lib` **288 passed / 0 failed / 8 ignored**（只改前端）；`npm run build` 通过（main 666.42 kB / gzip 223.39 kB；whiteboard 66.23 kB）。
- **边界**：文本规则是插件事件表的镜像，需手动保持同步；不识别裸年份（`1079` 无“年”字）；只有 sticky/link/todo 有入口，图片/agent 卡走各自逻辑；不能把白板文字保存成笔记地图上下文。

### 9.8 P234 本月账本的逐条单次调用（2026-11-26）

- `usage_summary` 增加 `recentEvents`：按 `created_at` 倒序返回最近 8 条 `production_usage_events`，含 label/model/prompt/completion/估算金额/priced/时间；设置页「本月用量」下方新增「最近单次调用」列表。
- 单次金额小于 0.01 元时用 4 位小数（`formatCny`），避免像之前一样显示 `¥0.00`。实测《自污…》一行显示 `deepseek-v4-flash · 2,436 tok · ¥0.0018`，日期 `2026年11月26日`（`p234-usage-events.png`）。
- 测试：`usage_summary_groups_current_month_and_marks_unpriced_jobs` 增加 `recentEvents` 校验（1 条、label、promptTokens）；`cargo test --lib` **288 passed / 8 ignored**；`npm run build` 通过（main 667.05 kB / gzip 223.57 kB）。
- **边界**：只显示最近 8 条，没有分页/删除/导出；列表是只读；P229 之前的调用仍未回填；`byModel.jobs` 仍混入单次调用计数。

### 9.9 P235 地图上下文的失效检测（2026-11-26）

- front matter 新增 `地图内容哈希`：`map_context_save` 写入当前 `标题 + 正文 + tags + periods + people + events` 的 SHA-256；`from_note` 读取时重新计算，哈希不一致则返回 `stale=true`。
- 面板 hint 追加「笔记已变更，建议重跑 AI」，并转金色提示；不会自动覆盖/重跑，用户点 AI 定位后重新保存即可。自 P235 之后保存的上下文开始带哈希；旧笔记没有哈希时不误报，重新保存一次后启用。
- 单测：保存后 `from_note.stale=false`；改正文后 `stale=true`；front matter round-trip 覆盖 `地图内容哈希`。
- `cargo test --lib` **288 passed / 8 ignored**；`npm run build` 通过（main 667.05 kB / gzip 223.57 kB）。
- **边界**：哈希不含 `地图*` 字段本身（改地图字段不会触发失效）；只比较内容，不判断 AI 结果是否仍正确；旧上下文需重新保存一次才有哈希；UI 没有“一键重跑并覆盖”按钮。

### 9.10 P236 离线地图包（2026-11-26）

- 插件升级到 **v1.0.9**。把上游裁剪后的站点（`index.html`、css、js、data、terrain、relief、LICENSE）放进 `src-tauri/plugins/examples/jiangshan-atlas/site/`，约 **49 MB**；`tauri.conf.json` 作为 bundle resource 映射到 `plugins/jiangshan-atlas/site`，安装内置插件时从资源目录复制到 app data `plugins/jiangshan.atlas/site/`（首次约 275ms，本机 SSD）。
- **服务方式**：Tauri asset protocol 会把 Windows 整条路径编码成一个 segment，`css/style.css` 会解析到 origin 根而不是站点目录，实测页面无样式、地图黑块；改为 Rust 里一个只绑 **127.0.0.1** 的临时端口 HTTP 服务，根目录是 app data `plugins/`，按相对路径读文件。iframe src 实测 `http://127.0.0.1:59857/jiangshan.atlas/site/index.html?embed=1&year=629&place=关中…`，CSS/JS/数据/地形全部加载，地图正常渲染（`p236-map-offline.png`），面板域名显示「本地地图」。
- 安全：服务只监听 loopback；路径校验拒绝 `..`/反斜杠/绝对路径，越界请求 404；单测 `plugin_site::tests::serves_site_files_and_rejects_traversal` 覆盖静态文件与 traversal；`PluginManager::site_entry` 另有路径范围测试。远端 https URL 仍保留，作为本地站点缺失/加载失败的回退。
- **安装包**：NSIS `18,423,357 B`（**17.57 MiB**），主程序 `24,976,384 B`（23.82 MiB）；站点资源 49 MB 在 release 资源目录，压缩后安装包只增加约 8–9 MiB。`cargo test --lib` **290 passed / 0 failed / 8 ignored**（新增 plugin_site + site_entry 2 项）；`npm run build` 通过（main 667.05 kB / gzip 223.56 kB）。
- **边界**：地图的 Google Fonts 仍是外链，真正断网时字体回退，但地图主体/数据不依赖网络；站点资源约 49 MB 未压缩进 exe，而是安装包资源；站点文件目前在工作区未提交（49 MB，需要随插件一起提交/分发）；旧安装包不认识 `local`/站点，需要新构建；离线首屏、GeoJSON 解析、Canvas 内存和冷启动没有单独基准。

### 9.11 P237 地图面板改为地点清单 + 单点 + 正文开关（2026-11-26）

- 新增 `AtlasPanel.vue`，替换 atlas 的 `PluginPanelHost`：去掉整条年份/地点/人物/事件/定位/AI/存入笔记字段条（DOM 里 `.plugin-panel-fields=false`），改为地图上方一条横向「本篇地图」chip 清单 + 全宽单个地点地图。选中哪个 chip，iframe 就只加载那一个 `year/place`，不再用逗号路线。
- 清单来源：笔记已保存/缓存的地图上下文 + `extractMapPoints()`（事件表、古城关隘表、时期、显式年份）。《刘秀的军事能力》实测 3 个 chip：`隗嚣 32年·陇右`、`昆阳之战 23年·昆阳`、`西城 约23年`；默认选中保存的昆阳，点隗嚣后地图切到 32/陇右。近似年份标「约」，没有年份的 chip 不可点。
- 增加 `mapLinks` store：地图横条右端「正文地名可点」开关；开启后 `NoteView` 用 TreeWalker 把正文里的地名/事件包成 `.note-map-token`（虚线下划线），点击直接打开地图并选中该点。实测《刘秀》正文生成 12 个 token，点「隗嚣」打开 32/陇右；关闭开关后 token 数回到 0。开关按笔记存在 localStorage，不参与同步。
- 为什么不放竖栏：218px 竖栏在 1136 面板里占 19%，地图只剩 918px。改横条后横条 48px，1180 下地图 **918×694 → 1134×646**（宽 +23.5%、面积 +15.0%）；1024 下 **806×708 → 1024×660**（+27.1% / +18.4%）；520 白板宽下横条 46px，无横向溢出。截图 `p238b-map-rail-1180.png`、`p238b-map-rail-1024.png`、`p238b-map-rail-520.png`。
- `cargo test --lib` **290 passed / 0 failed / 8 ignored**（本轮只改前端）；`npm run build` 通过（main 669.34 kB / gzip 224.35 kB；whiteboard 66.24 kB）。
- **边界**：清单仍靠规则表，不是 AI 多地点抽取；地点年份可能标「约」或缺失；没有手动加地点/AI 补一个/保存按钮；旧的多站路线字段仍能被旧数据读取，但新 UI 不再展示；正文 token 只包正文 `.prose`，不处理标题、链接、代码和标注卡；开关设备本地，不同步。

### 9.12 P238 地图打开即可操作（2026-11-26）

- 用户问「点击以操作地图也是你加的吗」：不是。这是上游 embed 模式自带的 `#embed-shield`——透明层拦截 `pointerdown/pointermove/pointerup/mousedown/touchstart/touchmove/wheel/dblclick`，点一次才 `is-hidden`，并且地图根节点 `mouseleave` 时会重新出现，所以在面板里来回切 chip 会反复要求激活。
- 处理：本地站点新增 `site/local.css`（在 `style.css` 之后加载）设 `#embed-shield { display: none !important; }`，不改 475KB 的 `app.js`。`_embedActive` 在整个 bundle 里只有 2 处引用，都在 shield 内部，隐藏后没有其它逻辑依赖。
- 版本：`jiangshan.atlas` **1.0.10**。`install_examples` 只在版本更高或站点缺失时重拷，必须靠版本号让已安装机器更新；它是插件页的手动命令（「安装内置插件」），单独改 `site/` 或重启应用都不会自动生效。
- 实测：app data `index.html` 含 `local.css`，HTTP **200 / 570 B**；iframe 内 `#embed-shield` computed `display:none`；未点地图直接滚轮，`#atlas-svg` viewBox `384.6 287.9 384.6 250.0` → `553.0 332.4 247.7 161.0`，地图立即缩放。
- 安装包：P238 后 NSIS **18,428,120 B / 17.57 MiB**，主程序 24,980,480 B；未安装复验。
- **边界**：补丁是 vendored 站点的一部分，重新下载上游站点后要补回 `local.css` 和 `index.html` 的 link；代价是不再有“避免宿主文章滚动被地图抢走”的保护，但面板是弹层主内容，这个保护本来就不适用。

### 9.13 P240 地图内元素默认精简 + 「显示」开关（2026-11-26）

- 默认（P241 起）：隐藏右侧资料卡 `#genealogy-card`、底部时间轴 `#scrubber-dock`；显示左下图层图例 `#map-key`、左上品牌/年份徽标 `#embed-badge`；`#embed-open` 与面板头部外部打开重复，永久隐藏。
- 顶栏「显示」菜单四项（朝代时间轴 / 地点资料卡 / 图层图例 / 年份徽标），默认值 `{timeline:false, card:false, legend:true, badge:true}`，存 localStorage `yebian.atlas.layers.v1`。
- 机制：`index.html` 的 `<html>` 预挂 `yb-hide-card yb-hide-timeline`；`local.css` 按 `html.yb-hide-*` 加 `display:none !important`；`local.js` 接受宿主 `postMessage({type:'yebian:atlas-ui', state})` 增删类；`AtlasPanel.vue` 在 iframe load 和开关变化时发消息。开关只增删类，移除后回到上游自己的 display（卡片 flex、时间轴 grid）。
- 实测：默认资料卡/时间轴 `none`、图例/徽标 `flex`；勾选资料卡后 display `flex`；截图 `p241-atlas-default-1180.png`、`p240-atlas-layers-menu.png`。
- 插件版本 **1.0.12**，需要在插件页手动「安装内置插件」重拷站点；远端回退站点没有桥，开关禁用。
- **边界**：依赖上游 id；时间轴默认关后只能靠 chip 切年份；资料卡仍需点地图地点才出现；设置不随笔记同步；已有 localStorage 不强制覆盖。P241 后 NSIS **18,431,021 B / 17.58 MiB**，主程序 24,980,480 B；main 668.80 kB / gzip 224.19 kB、whiteboard 66.26 kB。

### 9.14 P242 面板头部去重（2026-11-26）

- 去掉标题上方重复的「江山时序」kicker、域名「本地地图」、技术标签「跨域沙箱」；这三个只对调试有意义，面板里纯噪声。
- 头部从 54px 压到 **44px**，标题 16→15px；保留标题和刷新/外部打开/关闭。1180 下两行头部合计 92px，少 10px。
- 截图 `p242-atlas-compact-1180.png`。
- **边界**：通用插件面板仍保留域名/隔离标签；地图面板不再区分本地站点与远端回退。P242 后 NSIS **18,428,757 B / 17.58 MiB**，主程序 24,980,480 B。

### 9.15 P243 仅年份匹配不再伪造地点（2026-11-26）

- 《陈寿说诸葛亮“应变将略非其所长”…》只有 `periods:["三国"]`，没有 events，正文也没有命中地名表；worker 把三国映射到代表年 **250**，place 为空。旧 `addInitialPoint()` 用 `place || '初始地点'` 兜底，于是出现「初始地点 250年」。
- 现在：无 place/event 时生成 `全图` chip（place 空），清单头显示「仅年份」；正文已有地点时不插全图。`inferYear` 返回 `{year, source}`，`open` 增加 `yearSource`；`source=period` 的年份在 chip 上标「约」。
- 实测：该文 `本篇地图 / 仅年份`、`全图 / 约250年`、iframe `year=250&place=`；《刘秀》仍 `3 处`。插件 **1.0.13**；截图 `p243-atlas-year-only.png`。P243 后 NSIS **18,429,724 B / 17.58 MiB**，主程序 24,980,480 B。
- **边界**：250 是三国代表年，不是确切纪年；正文没有地名/事件命中时不会硬造地点。

### 9.16 P244 插件页展示地图信息（2026-11-26）

- 插件页重做为左列表 + 详情；地图插件默认选中。详情显示一句话介绍、官网、开源仓库、许可证、截图和 6 条功能，不再只有技术描述。
- manifest 新增 `homepage`、`repository`、`license`、`features`、`icon`、`screenshots`；地图插件 1.0.14 用 `assets/favicon.svg` 做图标、`assets/atlas-preview.jpg`（1000×614，113 KB）做截图。CSP `img-src` 增加 `http://127.0.0.1:*` 才能读到 loopback 资源。
- 许可纠正：作者 `DeanChensj`，`license` **GPL-3.0**，与站点 `LICENSE`、GitHub 仓库一致；此前文档里的 MIT 全部改正。CHGIS/Hartwell 边界数据的非商业/署名限制保留在 §1 与 `site/data/atlas/README.md`。
- **边界**：图标/截图只支持插件 `site/` 下的相对路径；本地截图是面板地图区域的静态裁剪，不随主题变化；远程仓库链接由系统浏览器打开。P244 后 NSIS **18,551,103 B / 17.69 MiB**，主程序 25,004,544 B。

**还没测/限制**

- 离线站点已打包并实测渲染，但没有独立的离线首屏/GeoJSON/Canvas 内存/冷启动基准；官方站不可达时现在会走本地站点，本地站点缺失才回退到错误/浏览器打开。
- 地图字体仍从 Google Fonts 加载，完全断网时字体会回退；地图主体、数据、地形不依赖网络。
- 1024×760 下已测：面板切换为全屏（≤1100px 媒体查询），iframe 宽度 1024、无横向溢出；地图时间轴完整显示（`p225-map-panel-1024-full.png`）。
- AI 定位是单次模型推断，可能把地点/年份判错；面板显示本次 tokens/耗时/置信度/依据，P229 已并入本月账本，P234 显示最近 8 条单次调用，但没有分页/删除/导出、人工复核清单或 P229 之前的历史回填。规则词表仍约 100 个，AI 失败时回退到它。
- 年份是硬门槛：没有年份不加载地图，避免默认跳到公元前 403 年；地点可空。只保存地点、不保存年份的旧上下文可以读出来，但地图不会加载，需要补年份。`map_context_save` 会拒绝没有年份的保存。
- 写回已经是单篇可用；**AI 成稿流程不会自动生成 `地图*`**，批量回填工具默认只扫描，`--apply` 需要显式指定 `limit`、`min-confidence`，不是自动后台任务。
- P235 起保存的上下文带 `地图内容哈希`，正文改动会标 `stale` 并提示重跑；P235 之前的旧存档没有哈希，不会误报，需重新保存一次后才启用失效检测。地图字段本身不在哈希里，改地图参数不会触发失效。
- 没有双向桥，地图里的点选不会写回笔记；AI 结果不会自动写进 front matter，必须点「存入笔记」或跑回填工具。

## 10. 下一阶段决策点

1. **离线**：已按“插件资源”方案做掉（P236）：站点 49 MB 随安装包，NSIS 17.57 MiB；后续只需决定是否再压资源/做增量更新。
2. **回填策略**：AI 成稿时自动写 `地图*`，还是保持现在的“单篇存入 + 显式 CLI 回填”？后者更可控、无额外成稿成本；前者需要先解决置信度门槛和人工复核。
3. **成本账本**：`map_context` 目前把 tokens/耗时显示在面板，未进制作台月度账本；是否把地图定位也纳入预算口径。
4. **双向**：是否需要从地图回写地点到笔记/出处；这需要地图侧 postMessage 或 wrapper，不能靠跨域 DOM 读取。

## 11. 参考

- 仓库：<https://github.com/DeanChensj/jiangshan-map>
- 在线：<https://deanchen.me/jiangshan-map/>
- 嵌入说明：仓库 `README_ZH.md` 的「嵌入到你的网页」
- 机器接口：`llms.txt`、`js/atlas_api.js`、`scripts/mcp_server.js`
- 许可：仓库 `LICENSE`（GPL-3.0）
