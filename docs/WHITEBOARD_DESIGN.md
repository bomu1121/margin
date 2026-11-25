# 白板 v1

> 一个独立浮窗，用来先收集、再局部研究。不是自由画笔，也不是第二套笔记列表。

## 1. 产品边界

- **收集**：便签、待办、链接、图片；粘贴、拖入、快速输入三种入口。
- **整理**：自由拖动、缩放、选中；位置和内容都进 SQLite，跨设备同步。
- **研究**：选中 1–N 张卡 → 写一句研究要求 → 后台跑 `whiteboard-research` job → 生成一篇笔记；agent 卡片回链到笔记。
- **窗口**：独立 `whiteboard` 窗口，无边框、可缩放、可置顶；不加载主 App 外壳。
- **多工作区**：一个窗口内可新建/切换/重命名/删除多块白板；切换只加载当前板，旧板保留。
- v1 不做：自由画笔/连线、协作光标、卡片分组、图片 OCR 以外的高级视觉编辑。

## 2. 入口与窗口

- 页面：`whiteboard.html` + `src/whiteboard.ts` + `src/WhiteboardApp.vue`；Vite 多页构建。
- 窗口：Rust `WebviewWindowBuilder`，label `whiteboard`，初始 1040×760（P213 从 520×760 放宽），最小 360×480，`decorations(false)`。
- 打开方式：主窗口顶栏「白板」、托盘「打开白板」、全局快捷键 `Ctrl+Alt+B`。
- 开发环境用 `http://localhost:1420/whiteboard.html`，release 用打包后的 `whiteboard.html`；窗口创建必须从 async command 交给主线程（同步 command 在主线程再 `run_on_main_thread` 会死锁）。
- 自绘标题栏：拖动区域、置顶、最小化、关闭。置顶状态存 `localStorage`，窗口几何由 `tauri-plugin-window-state` 管理。

## 3. 数据模型

```sql
whiteboard_boards(
  id TEXT PRIMARY KEY, name TEXT, created_at INTEGER, updated_at INTEGER
)
whiteboard_items(
  id TEXT PRIMARY KEY, board_id TEXT NOT NULL, kind TEXT NOT NULL,
  x REAL, y REAL, w REAL, h REAL, z INTEGER,
  text TEXT, title TEXT, url TEXT, asset_path TEXT,
  color TEXT, checked INTEGER,
  note_id TEXT, job_id TEXT, data_json TEXT,
  created_at INTEGER, updated_at INTEGER
)
```

- `board_default` 只在整库还没有任何白板时创建一次；删掉后不会被下次启动复活。
- 板列表按 `updated_at DESC, created_at DESC, id DESC`；切换器只查元数据，不查卡片。
- 每块板的卡片只在打开时加载；切换会清空上一块的卡片、图片 data URL、agent 状态，内存跟随当前板而不是总板数。
- `kind`: `sticky | link | image | todo | agent`。
- `data_json` 只放稳定数据（图片来源尺寸、agent 的 `sourceItemIds`）；运行进度不落库，由前端轮询 `production_get(job_id)`。旧版本写入的 `captionHeight` / `captionUsage` 会留在原卡片里，但新 UI 不再读写它们。
- 索引：`whiteboard_items(board_id, z, created_at)`。
- 两张表都在 `sync/registry.rs` 的 `SYNCED_TABLES` 里，按 HLC LWW 同步。

## 4. 命令

`src-tauri/src/whiteboard.rs` + `lib.rs`：

- `open_whiteboard`
- `whiteboard_board`（兼容旧调用，返回默认板或第一块板）
- `whiteboard_boards`：轻量板列表（id/name/createdAt/updatedAt）
- `whiteboard_board_create(name)` / `whiteboard_board_rename(id, name)` / `whiteboard_board_delete(id)`；删除在事务里级联卡片，且拒绝删除最后一块板
- `whiteboard_items(board_id)`：只查当前板，按 `z, created_at`
- `whiteboard_item_save(input)` / `whiteboard_item_delete(id)`
- `whiteboard_import_image(path)`：读本地图片，SHA-256 命名，写到 vault `whiteboard/assets/<hash>.<ext>`，返回相对路径 + data URL。
- `whiteboard_save_image(base64, ext)`：剪贴板图片。
- `whiteboard_asset_data(path)`：只允许 `whiteboard/assets/` 下的 png/jpg/webp/gif，返回 data URL。
- `whiteboard_link_preview(url)`：复用 `production/web_fetch.rs`，SSRF 安全，15 秒超时；失败回退域名。
- `whiteboard_image_investigate(id, mode, request_id)`：用户点「查来源」或「搜相似」时启动一条完整的溯源流程：
  - 视觉模型读图，逐字摘录文字、识别平台/主体/类型，并给 4–6 条检索词；
  - 360 识图上传图片，拿同图/相似页面（`/r?src=st&srcsp=home` → `data-imgurl` → `/stu?a=mrecomm`）；trace.moe 对动画场景做匹配（≥85%）；
  - 用视觉检索词调联网检索（DeepSeek Search / Tavily / 智谱，按现有配置）；
  - 文本模型汇总来源候选、相似图、证据和下一步检索词；
  - 通过 `image-investigation-progress` 事件实时推 `vision / reverse / search / synthesis` 四段状态；返回 `sources`、`similar`、`providers`、`usage`，**不写回卡片**。
- `whiteboard_research(board_id, item_ids, prompt)`：拼素材、创建 agent 卡片、启动 job。
- `whiteboard_open_note(note_id)`：显示主窗口并 emit `open-note`，主窗 router 跳转。

## 5. 收集交互

- 顶部快速条：回车生成便签；`http(s)` 文本生成链接卡并异步取标题/描述；粘贴图片生成图片卡。
- 画布：空白拖动平移、滚轮以指针为中心缩放（0.4–2.0）、双击空白新建便签；卡片拖动/右下角缩放；图片卡缩放锁定原图比例（可视图片框 = 卡片宽 − 2px 边框、卡片高 − 23px 标题栏 − 2px 边框，按指针位移在宽/高方向的投影换算，最小 160×40）；点击选中，Shift 多选；Delete 删除；Esc 取消。
- 新卡片落点：以当前可视区域中心为起点做环形搜索，并夹在可视范围内，底部预留 64px 给选择栏/缩放胶囊；避免新卡直接落在屏幕外。
- 链接卡：域名、摘要、底部「打开链接」和卡片工具里的箭头都可直接跳转；标题保留为可编辑输入。卡片空白/拖动手柄只做选中和拖动，避免为了研究选卡时误开浏览器。预览失败不阻塞。
- 便签里的链接：快速输入/粘贴整段文字时，如果含 `http(s)` 且总长 ≤600 字，直接生成链接卡，URL 前面的标题作为初始标题；超过 600 字仍生成便签，避免丢正文。已有便签含 URL 时，正文里的 URL 直接渲染成蓝色下划线链接，点击打开；便签平时是只读链接视图，双击进入 textarea 编辑，提交后回到链接视图。链接**不做 hover 变色**：折行 URL 由多个行内盒组成，行间有空隙，变色会随鼠标进出反复切换；反馈只靠蓝色下划线和 pointer。Windows 打开链接用 `ShellExecuteW`，不用 `explorer.exe`（后者遇到带 `?` 的 URL 会打开文件资源管理器）。
- 图片卡：图片优先，按 `object-fit: contain` 完整显示，不裁切；**没有常驻说明区、没有自动识图、没有「AI 识图」按钮**。选中单张图片时，底部选择栏才出现「查来源 / 搜相似」，卡片本身不再挂浮动按钮；点开是一条约 20–60 秒的自动调查，关闭即丢弃，不写回卡片、不占卡片空间。
- 新建图片卡按原图比例初始化；旧版本自由缩放留下的非比例卡片，在打开工作区时收拢一次（阈值 2px，只缩小、不移动），避免 `contain` 在卡片里留白边。
- 图片调查面板：
  - 加载时缩略图跑扫描线，并实时显示四步：读图（OCR/主体/类型）→ 反查（360 识图 / trace.moe）→ 交叉检索（视觉词）→ 汇总（来源候选/证据/相似图）；
  - 「查来源」优先展示来源候选（标题、站点、置信度、证据），再展示相似图；
  - 「搜相似」优先展示 360/trace.moe 的相似图网格，来源判断折叠在下方；
  - 底部折叠「画面识别与文字」「检索词（可手动复核）」；检索词只是兜底，不是主流程；
  - 面板明确写出「反查会把图片上传到 360 识图；trace.moe 只用于动画场景；结果是线索，不是逐像素确认的出处」。
- 结果可信度：模型判 `not_found` 时不把 360 相似页塞进「来源候选」，只在「搜相似」或折叠区展示相似图；360 结果单独不能升级为 `found`，没有动画精确命中时最多 `likely`。模型生成的 URL 必须出现在反查/检索证据里，否则丢弃，避免编造链接。
- 待办：圆形 checkbox + 可自动长高的 textarea。
- 便签默认中性纸面，不再默认黄色；需要区分时可在「普通 / 蓝 / 绿」之间切换。
- 便签、待办支持文字选择；文字变长时卡片向下长高，内部 `overflow:hidden`，滚轮交给画布平移/缩放。
- 外部拖入：Tauri `onDragDropEvent`；图片走导入，非图片生成带文件名/路径的便签（v1 不解析文件内容）。
- 删除：6 秒撤销 toast；撤销用原 id 重建。

### 5.1 多工作区（P213）

- 顶栏 `工作区 | <当前板名> ⌄` 打开工作区菜单；菜单只显示板名，不显示卡片数量，避免每次打开菜单做全表 `COUNT`。
- 新建后立即进入内联重命名；重命名只改板名，不动卡片。
- 删除是两步确认：第一次把按钮变成「确认删除」并提示将删除多少张卡片，4 秒内再点才执行；后端在事务里删板 + 级联卡片，并拒绝删除最后一块板。
- 切换时先保存旧板的 pan/zoom，再加载新板并恢复它自己的视图；视图存 `localStorage`，不参与同步。
- 只加载当前板：板列表查询只取元数据，卡片查询固定 `WHERE board_id = ?`；切换后把上一板的 `assetUrls` / `linkLoading` / `agentStates` 裁到当前板，避免图片 data URL 和 agent 状态随板数累积。
- 删除板时取消该板还没触发的 debounce 保存；保存路径也会检查板是否还在列表里，避免删除后又被 `ensure_board` 复活。
- 其他设备新建/改名的板不主动推送 UI；打开工作区菜单时刷新一次板列表（轻查询），已删除的当前板会回落到第一块板。

## 6. 研究链路

- 隐藏模板 `whiteboard-research`：`source → agent_note → publish`，source 只有 `text`；模板名「白板研究」，agent 步骤名「研究 Agent」。它不是制作台的视频笔记 / AI 成稿模板。
- `whiteboard_research` 把选中卡片拼成 Markdown：标题、正文、URL、图片路径/说明；图片的 `text` 只作为历史上下文出现一次（旧版识图或用户手填），新的图片查询结果不会自动进入这里。
- `JobOptions.note_task` 写入研究要求，`options.prompt` 固定为「白板研究」；`note_engine` 对白板模板使用 `WHITEBOARD_RESEARCH_PROMPT`，并在配置了检索 Key 时额外暴露 `search_web` 工具。后端进度标签按模板区分：白板研究显示「白板研究 Agent」，不再写「AI 成稿（Rust 引擎）」；旧 job 的前端展示也会把遗留的 AI 成稿标签归一成「研究 Agent」。
- **研究运行面板**：开始后 `AgentSheet` 不关闭，直接进入运行态；轮询 `production_run_snapshot`，展示 3 个真实阶段（读取白板素材 / 研究 Agent / 保存笔记）、进度条、Agent 回合、耗时、token、最近工具调用（读取链接 / 检索 / 读素材 / 写笔记等）和停止按钮；完成后显示「打开笔记」；可从 agent 卡片的「研究过程」重新打开。
- 结果走现有 `publish`，默认落 `待整理/`；agent 卡片写 `job_id`，job 成功后才显示「打开笔记」。
- agent 卡片状态（running/done/failed、progress、current_step、error）来自轮询，不写 `whiteboard_items`，避免每秒同步。
- 白板窗口关闭不影响 job；重开白板会按 `job_id` 恢复轮询。
- 研究 Agent 的 `bash` 工具在 Windows 上用 `CREATE_NO_WINDOW` 启动 `cmd /C`，避免研究过程中弹出命令行黑窗；`media.rs` 的 ffmpeg 也已有同样标志。

## 7. 同步

- 白板表：`whiteboard_boards`、`whiteboard_items` 进入 op log。
- 图片：只把 vault `whiteboard/assets/` 下的 png/jpg/jpeg/webp/gif 加入文件面；其他目录图片不同步，避免把整个 vault 的附件上传。
- 另一台设备拉到图片后，`whiteboard_asset_data` 才能显示；未到时卡片显示「图片同步中…」。
- 旧版本 App 不认识白板表和 `whiteboard/assets` 图片；两台都要更新到含白板的构建。

## 8. 视觉与动效

- 用 App 现有主题变量（paper/surface/rail/ink/line/blue/gold/green/red），卡片无明显大色块，便签只做低饱和纸张色。
- 画布点阵随 pan/zoom 移动；卡片 180ms 缩放淡入；选择是 1px 蓝环 + 轻阴影；拖动降阴影。
- 研究面板 220ms `cubic-bezier(.22,1,.36,1)` 从底部滑出；研究运行态用呼吸圆环 + 进度条 + 工具调用逐条淡入，agent 运行点不持续闪烁；`prefers-reduced-motion` 关闭圆环、进度和条目动画。
- 图片操作在选中单张图片时进入底部选择栏，卡片内不再有浮层；调查面板加载时缩略图上有 1.5s 扫描线，四段步骤按真实事件切换 ✓/ spinner/×；`prefers-reduced-motion` 下这些动画全部关闭。
- 已验证：520×760 下调查面板 `clientHeight 648 / scrollHeight 1779`，纵向滚动、无横向溢出；加载态四步实时可见，结果态来源卡片/3 列相似图网格（12 张全部缩略图，`contain` 完整显示不裁切，标题在下方）/底部说明都在 520 宽内；图片卡 700×700 选中栏五按钮、拖动 322×345→472×495→412×435 全程 aspect error 0 / letterbox 0px；520 宽选中栏两行、`scrollWidth 258 == clientWidth 258`；截图：`07-image-actions-light`、`11-hover-actions`、`14-investigate-loading`、`19-investigate-final-top`、`21-investigate-grid-3col`、`22-investigate-grid-fixed-preview`、`23-sticky-link`、`24-sticky-link-card`、`25-sticky-inline-link`、`26-sticky-inline-link-100`、`28-research-progress`、`29-research-final`、`38-image-selection-bar`、`39-image-resize-horizontal`、`40-image-resize-vertical`、`41-image-selection-bar-520`、`42-image-ratio-normalized`、`43-sticky-link-final`。旧的手动检索面板截图 `08/09/10` 和中间稿 `15/16` 保留为历史，不是当前流程。

## 9. 实测与限制

- Rust：`cargo test --lib` **265 passed / 0 failed / 8 ignored**（含白板 CRUD、多板隔离/级联/默认板不复活、图片 hash/路径、研究素材去重、360 JSON/vision/synthesis 解析、同步注册、文件白名单）。
- 前端：`npm run build` 通过；白板入口 JS 约 58.6 kB、CSS 约 25.7 kB（压缩前）。
- 窗口实测：主窗口顶栏打开/关闭/重开、置顶切换、无边框拖动区均可用；520×760 下调查面板纵向滚动（648/2221），无横向溢出。
- 收集实测：便签落库；链接 `https://example.com` 取到标题 Example Domain 和摘要；图片 49,979 B → `whiteboard/assets/<sha256>.png`。
- **多工作区实测（P213）**：
  - 后端：`研究` / `写作` 两块板各 1 张卡，`board_default` 0 张；`whiteboard_items` 按板隔离；删除板级联卡片、拒绝删除最后一块板、默认板删除后重启不复活，均有单测。
  - SQL 层基准（`tools/bench-whiteboard-boards.mjs`，SQLite 同引擎查询）：100 板 × 200 卡（20,000 行）时，工作区元数据查询 **p50 0.05ms / p95 0.05ms**，单板 200 卡 **p50 0.28ms / p95 0.33ms**，全表 20,000 卡 **p50 39.25ms / p95 49.60ms**；200 板 × 200 卡（40,000 行）时，元数据 **p95 0.15ms**、单板仍是 **p95 0.35ms**、全表 **p95 171.43ms**。切换器从不走全表查询。
  - UI 实测：300 张卡的板从点击切换到 300 个 `.wb-card` 完成渲染约 **169ms**（含 20ms 轮询间隔）；`whiteboard_items` IPC 连续 5 次 **7–8ms / 300 行**。
  - 内存实测（`HeapProfiler.collectGarbage` 后 JSHeapUsedSize）：1 张卡的板 **2.3MB**，切到 300 张卡的板 **8.0MB**，切回 1 张卡的板 **2.4MB**，只残留约 0.1MB；证明缓存随当前板回收，不随板数累积。
  - 视觉：`shots/audit/p213-whiteboard-1040.png`（新初始宽度）、`p213-whiteboard-switcher.png`（切换器）、`p213-whiteboard-delete-armed.png`（两步删除确认）、`p213-whiteboard-360.png`（窄窗）。
- **反查实测（2026-11-24，真实安装版 + DeepSeek）**：
  - 动漫图：360 识图 20 条、联网检索 19 条、trace.moe 0；25.7s；读图+汇总 4767/4084 tokens；判定 `likely`，模型归到「DeepSeek娘 / 蓝色大肥鱼」二创系列，明确说未找到原作者/首发页；来源候选 4 条（memecub 等）+ 360 反查 2 条低置信候选。
  - 新闻截图（武契奇死亡威胁）：360 识图无关；联网检索命中网易/卫星社/腾讯/新浪同文报道；24.5s；5072/3928 tokens；判定 `likely`，4 条来源候选（3 条 high、1 条 medium），summary 给出卫星社→参考消息→门户转载链。
  - 评论截图：360 识图返回无关图；trace.moe 对截图里的动漫头像给出 0.964，但视觉类型是 `social`，已明确跳过并写 `trace.moe ... 不采用`；20.6s；5009/3243 tokens；判定 `not_found`，`sources=[]`，不硬凑来源。
  - 探针阶段每次 4 次检索、¥0.0200；当前实现最多 6 组，最终安装版成功 5 次、¥0.0250；结果只在面板，卡片 `w/h/text/dataJson` 未变。
  - 最终安装版复跑动漫图：21.2s、5601/2938 tokens、4 个来源候选、12 张相似图全部有缩略图；520×760 面板 clientHeight 648 / scrollHeight 1779、无横向溢出。
- 研究实测：便签 → 白板研究 job → success → 笔记生成 → 点「打开笔记」主窗跳转；agent 卡片状态轮询修正后显示「研究完成 100%」。
- **研究面板实测（P210）**：打开已有 job（`run_20772d50…`）的「研究过程」：3 阶段全部 done，标签为「读取白板素材 / 研究 Agent / 保存笔记」，16 回合、1m54s、651.6k tok；最近工具调用显示为「模型思考 / 执行命令 / 修改笔记 / 读取素材」，没有「AI 成稿」；面板 526/526、无嵌套滚动条；截图 `28-research-progress`、`29-research-final`。`bash` 路径已加 `CREATE_NO_WINDOW`，单测 `bash_runs_inside_the_workspace` 通过；没有再用真实任务观察黑窗。
- 同步实测（P200）：服务端看到 `whiteboard_boards` 1 条、`whiteboard_items` 30 条、白板图片 file upsert/delete 各 1 条；多板复用同一套板表/卡表，P213 没有新增同步表。
- 未验证/限制：
  - 多板：删除板只删卡片行，不删 `whiteboard/assets/` 里的内容寻址图片（图片可能被别的板复用；当前没有孤儿回收）；板列表不分页，200 块板基准通过，上千块未测；其他设备删板后，本机要打开一次工作区菜单才会回落；同一块板两台同时编辑仍是单卡 HLC LWW。
  - 360 识图是**相似/同图页面索引**，不是逐像素核验；没有动画精确命中时 verdict 最多 `likely`，不会写 `found`。
  - trace.moe 只对动画场景有效；社交/新闻/商品截图里的动漫头像会被类型判断丢弃，避免误报。
  - 外部服务依赖网络和反爬：Google Lens/Yandex/Bing 的网页反查在当前网络不可用或要 Key/会话，SauceNAO 无 Key 返回 403；当前主链路是 360 + trace.moe。
  - 反查会把图片上传到 360；trace.moe 只在动画/无字截图时上传。没有额外的每次确认弹窗，但卡片操作 tooltip 和面板脚注都写明。
  - 查询结果不持久化，关闭面板再开要重新跑；后端请求不会因为关闭面板而取消，仍可能消耗 token。
  - 识图/检索成本只在面板和 run log 显示，未并入 production 月度预算；模型描述仍有随机性，summary 里的“最可能”是推断，不是事实。
  - 360/模型可能把“同系列”误当成“同图”；来源卡片上的置信度和证据必须一起看。
- 安装包 `9,404,269 B` / 主程序 `24,569,856 B`（installer SHA256 `16218BA0…`，安装后 exe `A5B22C8E…`）；P213 已安装验收。
- 卡片位置/文本是 HLC LWW；两台同时拖同一张卡会有一方覆盖。

## 10. 后续

- 模板化研究（不同研究框架）；板级模板/复制。
- 增加更多反查提供方（SauceNAO/SerpApi/Bing Visual Search 需要 Key；Google Lens/Yandex 需要可用网络/公网图片 URL），并做结果投票。
- 反查取消（关闭面板后停止后端请求）、结果缓存、图片 OCR/视觉引用更细。
- 卡片分组、框选、连线；只读分享。
- 把白板研究/图片调查成本纳入预算面板。
- 研究任务取消后的半成品恢复、运行中关闭白板后的独立进度入口（现在可从 agent 卡重开）。
