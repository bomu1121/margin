# 页边笔记（Yebian）

页边笔记是一个**本地桌面笔记工作区**：中间读正文，右侧看一跳关系与标注；内置制作台可以从 B站链接、本地音视频或文稿直接生成可标注的笔记，不再跳转外部平台。标注锚点丢了会**明确显示「锚点失效」**，不会静默消失。

**制作在页边笔记进程内完成**：Rust 任务引擎负责来源、转写、AI 加工和入库；ScribeFlow 只保留为旧 Artifact/outbox 协议的参考实现，不再是产品入口。

技术栈：**Tauri 2 + Vue 3 + TypeScript + Rust + SQLite(FTS5)**。Windows 安装包 8.32 MiB，冷启动到首篇笔记渲染中位 0.68s。

> 这是 `D:\Develop\scribe-notes-lab` 里 09 笔记页原型的桌面化版本。标签台（08）、局部关系图和全库力场（07）都进来了，并带 BFS 找路径；06 那种按主题串讲的链式叙事还没移植，见「没做完的」。

---

## 1. 现在能做什么

| 能力 | 状态 | 说明 |
| --- | --- | --- |
| 读 ScribeFlow 的 Markdown | 已实现 | 解析中文 front matter；索引不改写文件，只有侧栏里的显式操作才会写 |
| 笔记页 | 已实现 | 标题 / 导语 / 速览 / 分节正文 / 速查表；本地衬线字体 |
| 标签检索 | 已实现 | 32 个标签/属性，AND / OR，点结果直接开笔记 |
| 侧栏整理 | 已实现 | 新建笔记/文件夹、重命名、拖拽移动、删除进 `.trash`（8 秒内可撤销）；移动前先固化 `id`，并全库改写 `[[链接]]` 与相对路径链接 |
| 后台索引 | 已实现 | notes/FTS 独立索引库，启动/切库/重建由独立线程构建，侧栏显示 done/total 进度；切库是文件替换；文件操作和监听只更新变化的笔记 |
| 侧栏目录树 | 已实现 | 按笔记库的物理文件夹生成，可折叠、递归计数；另带收藏置顶、就地搜索、排序切换；2026-11-05 重做密度与状态（30/48px 行、16px lead 槽、roving tabindex、拖拽反馈、移动 picker 与撤销），见 [`docs/SIDEBAR.md`](docs/SIDEBAR.md) §11 |
| 制作台（原生） | 已实现 | 顶栏「制作」：B站链接/收藏/本地音视频/粘贴文稿 → 10 个内置模板 → 转写/校对/加工/导图/练一练/攻略/溯源 → Artifact v2 outbox → 笔记 + 索引 + 注册表；模型接入支持 13 个 provider、自动拉模型和 models.dev 参考价（CNY），不用手填单价；UP 订阅能在更新后自动建任务，结果先到 `待整理`，托盘/开机自启可选；溯源事实已独立入库并可单独重新核验；无 Node sidecar、无端口。详见 [`docs/PRODUCTION.md`](docs/PRODUCTION.md) |
| 全文搜索 | 已实现 | SQLite FTS5 + 自写 CJK bigram；`养老金` 能命中正文 |
| 局部关系图 | 已实现 | Canvas 力导图：悬停高亮 + 粒子流、拖拽、全屏展开；点邻居换篇、点标签去检索 |
| 全库力场（图谱） | 已实现 | 顶栏「图谱」：DOM/SVG 力场；语义缩放（远景点阵 / 近景卡片）、点击聚焦面板、筛选、搜索、BFS 找路径；默认最近 40 篇，可切全部（上限 200） |
| 段落标注 | 已实现 | 标记 / 存疑 / 待办；存 SQLite；刷新/重启后仍在 |
| 锚点失效提示 | 已实现 | 正文改动后块 ID 变了，右栏显示「锚点失效」，保留原文引用可删 |
| 文件监听 | 已实现 | `notify` + 450ms 防抖，外部改文件后自动增量索引 |
| 单实例 / 窗口状态 / 日志 | 已实现 | 第二次启动聚焦已有窗口；窗口几何持久化；日志在 app data |
| 安装包 | 已实现 | NSIS `currentUser` 简体中文；静默安装/卸载已验收 |

**它不是什么**：不是 Markdown 编辑器，不双向链接，不做云同步，不是 Obsidian 替代品。它只负责「把 ScribeFlow 导出的东西读好、标好、连起来」。

---

## 2. 怎么跑

前置：

- Node 22+
- Rust 1.96+，MSVC Build Tools
- Windows 10/11 + WebView2 Runtime（Win11 自带）

```bash
cd D:\Develop\yebian
npm install
npm run tauri dev        # 开发模式，Vite 1420 + 原生窗口
npm run tauri build      # 产物：src-tauri/target/release/bundle/nsis/*.exe
```

首次启动自带 29 篇按话题整理的样例（历史 / 时事 / 科技 / 社会 / 游戏 / 开发测试）。点右上角「选择库」，选 ScribeFlow 的导出目录（例如 `D:\Develop\scribe-flow\apps\server\data\outputs` 下的某个 `run_*`，或它的上级目录）。

数据位置：

- 笔记：索引阶段只读；侧栏的新建/重命名/移动/删除会写文件（删除进 `.trash/`，可撤销）
- 标注 + 设置：`%APPDATA%\com.yebian.notes\yebian.sqlite`
- 索引（可删，重启重建）：`%APPDATA%\com.yebian.notes\yebian-index.sqlite`
- 日志：`%LOCALAPPDATA%\com.yebian.notes\logs\yebian.log`

> 改名前（Scribe Notes）的测试数据在 `%APPDATA%\com.scribeflow.notes`，页边笔记不再读取；那里面只有 4 篇样例、0 条标注，没有迁移。

想恢复出厂：退出应用，删掉上面的 `yebian.sqlite*` 和 `yebian-index.sqlite*`，再启动。只删索引文件会保留标注并按笔记库重建索引。

---

## 3. 输入格式

一篇笔记就是一个 `.md`，开头是 ScribeFlow 的中文 front matter：

```markdown
---
标题: 中国社保的累退、体制内外差距与“法币时代”养老生产力论
类型: 视频笔记
来源: B站
作者: 内部看美国
原始链接: https://www.bilibili.com/video/BV...
创建日期: 2026-10-14
人物:
- 马斯克
事件:
- 养老金改革
时期:
- 当代
tags:
- 来源/B站
- UP主/内部看美国
- 领域/财经
- 类型/视频笔记
---

# 标题

## 速览

- 一条判断

## 正文

### 小节

正文段落……
```

解析规则：

- `标题` 为空时取第一个 `#`；再没有就用文件名
- `作者` 为空时从 `UP主/xxx` 或 `UP主名称/xxx` 标签提取
- `领域/xxx` 是笔记的领域分类（笔记页抬头、标签台、图谱用）；没有就是「未分类」
- 左侧栏是**笔记库的目录树**，按文件实际所在的文件夹生成，与 front matter 无关——在资源管理器里移动文件，侧栏就跟着变
- `人物` / `事件` / `时期` / `tags` 支持 YAML 列表
- 正文里的 `**术语**（解释）：说明` 会进「速查」区
- `来源/` 标签参与全局图，但**不算实质邻居**——否则所有 B 站笔记会连成一团

---

## 4. 性能（本机实测）

机器：Intel Core Ultra 9 275HX / 63.4GB / Windows 11 26200 / WebView2 156 / Rust 1.96.1。  
所有数字都是**实测**，原始输出见 [`docs/experiment-log.txt`](docs/experiment-log.txt)，结构化数据见 [`docs/measured.json`](docs/measured.json)。

| 指标 | 实测 |
| --- | --- |
| 安装包 | **8,722,874 B（8.32 MiB）** |
| 主程序 | 21,914,112 B（20.90 MiB） |
| 冷启动 → 窗口可见 | 49–321ms（中位 62ms，5 次，1456×939；321ms 为进程启动抖动） |
| 冷启动 → 首篇笔记 DOM | **657–697ms**（中位 676ms，1456×939） |
| 空闲私有内存（整棵树） | **258.3–278.1MB**（5 次，1456×939，启动后 4s，中位 261.2） |
| 空闲工作集（整棵树） | 467.5–490.9MB（重复计共享 DLL，仅参考） |
| 空闲 CPU | 0–0.39%（其中一次 0.39 为启动采样离群；其余 0–0.03%） |
| 10k 切库/重建（staging + 文件级 swap） | 3.4–5.7s（后台，带进度；swap 前旧内容可读） |
| 10k 切回 4 篇 | 0.23–1.3s（后台） |
| 单文件外部修改 → 索引可见（4 篇） | 488ms（450ms 防抖 + 38ms） |
| 切库 swap 期间读延迟（10k） | list p95 5–183ms、最长 0.19–0.32s；search p95 3–118ms、最长 0.17–0.18s |
| 10k 侧栏实际挂载行数 | **27–38 行**（总 10,001 行；虚拟滚动，视口 780px + 上下各 480px 预渲染） |
| 50k 切库/重建 | **13.1–15.5s**（contentless 索引；后台，进度含“writing”阶段） |
| 50k 切回 4 篇 | 0.73–0.74s |
| 50k 侧栏实际挂载行数 | **28 行**（总 50,001；scrollHeight 2,400,008px） |
| 50k list / search | **0.92–0.94s** / **20–40ms** |
| 50k 索引文件 | **271,331,328 B（259MiB）**；其中 FTS 34MB、notes 表 230MB；源文件 227MB 实际字节 |
| 50k 加载后私有内存 | 切换完成瞬间 631MB，空闲 8s 后 **358.7MB**（7 个进程；4 篇空闲 261.2MB） |
| Store 层 1k / 10k 首次写入（旧微基准，不含调度） | 336–368ms / 4.1–4.2s |
| 搜索 1k 篇（FTS p50/p95） | 1.27–1.32ms / 1.31–1.45ms |
| 搜索 10k 篇（FTS p50/p95） | 5.74–5.97ms / 6.94–7.09ms |
| 搜索无命中 → LIKE 兜底 10k | 68–82ms |
| 打开笔记（Rust 取正文） | 69–167µs |
| 标注写入（SQLite p50/p95） | 21–38µs / 27–59µs |
| 局部图数据构建 1k / 10k（p50） | 0.85ms / 10.28ms |
| 局部图布局（10 节点，p50/p95） | 0.27–0.54ms / 0.68–2.07ms |
| 关系图静止时 rAF | 0（仅入场 620ms + 悬停/拖动重绘） |
| 图谱构建 4 篇（36 节点 / 37 边） | **9ms**（8–12ms，5 次 release） |
| 图谱构建最近 40 篇（122 / 320） | **58ms**（55–62ms，5 次 release） |
| 图谱构建全部 80 篇（168 / 640） | **97ms**（96–97ms，3 次 release） |
| 图谱构建全部 200 篇（288 / 1600） | **263ms**（262–267ms，3 次 release） |
| 图谱静止时 rAF | 0（构建后无循环；缩放/拖动才改 transform） |

Store 微基准每篇正文 1521 个中文字符（真实样例正文）。切库/监听数字来自 `tools/probe-swap-reads.mjs`、`tools/probe-index-lock.mjs` 的 release 实测：10k 语料是真实样例正文复制（81MB），多轮；swap 期间读不是零阻塞，见 §8。**本轮只更新了安装包/主程序大小；冷启动、内存和图性能数字仍来自 2026-11-05~06 构建，没有在 8.18 MiB 的拆分构建上重测。**2026-11-05 的虚拟滚动复测（`docs/measured.json` → `sidebar_virtualization`）记录了 10k 侧栏只挂载 27–38 行、搜索最长 184ms；此前 1.28–1.53s 的「首次搜索」是 10k 行 DOM 渲染，不是 FTS 冷缓存。

**一个必须说清的结论**：Tauri 的内存优势被夸大了。这个应用启动 4 秒时私有内存 258–278MB（2026-11-05 构建，5 次中位 261.2MB；上一构建为 234–245MB）；Tauri 本体约 7MB，其余是 WebView2 的浏览器/GPU/渲染进程。Tauri 真正赢的是安装包和启动，不是内存。详细口径和限制见 [`DESKTOP.md` §7](DESKTOP.md#7-性能预算-vs-实测)。

---

## 5. 标注是怎么锚定的

1. 前端把 Markdown 解析成块（段落 / 引用 / 列表 / 表格 / 代码）。
2. 每个块用**归一化文本**算 FNV-1a 哈希，作为 `block_id`；同一篇里重复文本加 `-2`。
3. 标注存 `(note_id, block_id, quote, body, kind, created_at)`。
4. 重新渲染同一份 Markdown，块 ID 不变 → 标注自动回到原位。
5. 段落文字改了 → 块 ID 变了 → 右栏出现「锚点失效」，保留 `quote`，只给删除，不假装还在。
6. 还原文字 → 块 ID 回来 → 标注重新锚定（已实测）。

代价：**改一个字就会失效**，这是内容哈希的必然结果，不是 bug。想要「改文字仍跟随」需要位置锚点 + 文本相似度回退，v1 没做。

身份模型：笔记的 `id` 优先取 front matter 里的 `id:`；没有的话回退到**相对路径的 SHA-256**。应用内的移动/重命名会先把当前 id 写进 front matter，再改路径，所以标注不会丢；在资源管理器里移动的旧笔记，会用正文指纹做一次配对回填（仅在不歧义时）。

---

## 5.5 关系地图

右侧不是静态 SVG，而是一块 Canvas「关系地图」：

- **颜色即类型**：金色=笔记、蓝色=标签/领域/UP 主、绿色=人物、灰色=时期；相邻笔记连到共同属性。
- **悬停**：节点放大、相关边高亮、粒子沿边流动，并显示完整标签；其余边与节点自动变暗。
- **交互**：节点可拖动；点相邻笔记直接换篇，点标签/人物/时期进标签台检索。
- **全屏**：右上角按钮打开 overlay，可滚轮缩放、拖背景平移、重置视图；Esc/× 关闭。
- **性能**：布局只算一次（10 个节点，p50 0.27–0.54ms），入场动画 620ms；**静止后没有 rAF 循环**，只有悬停/拖动时重绘。没有用 DOM 节点堆图。

局限：这个卡片只是**当前笔记的一跳关系**；全局/最近范围的知识图在顶栏「图谱」（默认最近 40 篇，可切全部）。拖动位置只保留到换篇/刷新。

---

## 5.6 全库力场（图谱页）

顶栏「图谱」打开 `/graph`。这不是旧的 Canvas 小球图，而是 07-force 原型的 DOM/SVG 重写：笔记是卡片，标签是胶囊，边是 SVG 线，光晕完全去掉。

**布局**

- 自写力导（斥力 + 弹簧 + 向心），不是 d3；笔记卡片按 184×56 的**矩形**做碰撞，标签/人物按圆做碰撞。
- 初始位置用黄金角螺旋；标签按连接度给一个外扩轨道，避免所有公共标签叠在中心；收尾做一次压缩 + 24 轮碰撞松弛。
- 结果是标签和卡片不重叠；40 篇笔记的构图是横向有机形状，不再是一列列小点。

**语义缩放**

- 远景（<78%）：笔记是 14px 点（反向缩放，屏幕上始终 14px），只显示连接度最高的 7 个标签名；这是一张“知识地图”。
- 近景（≥102%）：点展开成 184×56 卡片，标题/作者/日期可读；点击/搜索会自动飞到 108%。
- 中间档只显示点，避免半大不小的卡片挤在一起。

**交互**

- 悬停：邻居高亮、其余变暗、tooltip 显示完整标题。
- 点击笔记：**不跳转**；聚焦它、飞到 108%、打开右侧面板。面板里有元信息、标签（可继续聚焦）、速览前 3 条、经标签/人物连到的笔记，以及显式的「打开笔记 →」。
- 点击标签/人物/时期：面板列出用它连到的笔记；「在标签台查看 →」跳到标签检索。
- 找路径：点两篇笔记，BFS 最短关系；路径边流动虚线，底部显示链条。BFS 跳过 `来源/` 公共 hub 和当前隐藏的类型。
- 筛选：笔记/标签/人物/时期四类开关；人物/时期默认隐藏。
- 搜索：只搜当前图谱范围（最近 40 或全部）；回车聚焦第一条。
- 缩放/平移/适配：滚轮以鼠标为中心缩放，拖背景平移，右下角缩放条。

**规模**

- 默认只画**最近 40 篇**（按日期），HUD 明示「仅画最近 40 篇 / 共 N 篇」；「全部」按钮切到全部，硬上限 200 篇。
- 这不是性能上限，是可读性上限：40 篇的远景点阵已经需要 300+ 条边，80 篇开始变成毛球；200 篇只用来看分布/搜索定位。
- 全部 200 篇 288 节点 / 1600 边，构建 263ms（release，3 次）；这 263ms 是同步的，点击「全部」时会卡这一下，之后没有 rAF。

实测（release WebView2，`GraphView.build()` 内 `performance.now()`，5/5/3/3 次）：

| 范围 | 节点 / 边 | 构建 p50 | 范围 |
| --- | --- | --- | --- |
| 4 篇样例 | 36 / 37 | **9ms** | 8–12ms |
| 最近 40 / 共 80 | 122 / 320 | **58ms** | 55–62ms |
| 全部 80 | 168 / 640 | **97ms** | 96–97ms |
| 全部 200 | 288 / 1600 | **263ms** | 262–267ms |

计时包含力导 320 拍 + 碰撞松弛 + DOM 构建 + 首次适配；**不含 CSS 首帧绘制**。所以它不是“点击到看见”的端到端时间，页面可能再多 1–2 帧。数据来自 [`tools/measure-force-many.mjs`](tools/measure-force-many.mjs)，语料由 [`tools/make-bench-vault.mjs`](tools/make-bench-vault.mjs) 生成。

---

## 6. 桌面端问题怎么处理的

只列结论，依据和版本号在 [`DESKTOP.md`](DESKTOP.md)：

| 问题 | 处理 |
| --- | --- |
| 多开 | `tauri-plugin-single-instance` 第一个注册；第二次启动 1.08s 退出并聚焦已有窗口 |
| 窗口尺寸爆炸（混合 DPI） | window-state ≥2.2.2 + 自定义 clamp：<800×560 或 >最大屏×2 就重置 1440×900 居中 |
| WebView2 resize 后中文输入法候选框跑左上角 | resize 时 blur 焦点，160ms 后恢复 |
| 隐藏到托盘仍合成、空闲 CPU 高 | 不做托盘，关闭即退出 |
| 路径含中文/括号 | 全程 `PathBuf`/UTF-8；`\\?\` 前缀统一剥掉 |
| 库路径被移动/删除 | 启动校验，回退到内置样例并重写设置 |
| 中文搜索 | FTS5 `unicode61` + 自写 bigram；ASCII 整词小写；无命中再 LIKE |
| 权限过大 | 所有写操作都走 Rust 命令并限制在笔记库内；不开 shell/fs 插件；capabilities 只有 `core:default` + `dialog:default` |
| CSP / 离线 | `script-src 'self'`，无远程资源；字体本地 WOFF2 |
| 日志 | stdout + LogDir；启动、索引、监听、回退都有记录 |

---

## 7. 验收记录

原生窗口（release 构建）里实际点过：

- 笔记页 / 标签台 / 从标签结果打开笔记：正常
- 搜索「养老金」：返回社保笔记
- 图谱搜索「社保」→ 聚焦面板 → 点「打开笔记」才跳到笔记页：正常
- 图谱找路径：沙利文 → 类型/学习笔记 → AI Infra，3 步；路径边虚线流动：正常
- 选择库：原生文件夹对话框打开，Esc 取消正常
- 标注：新增 → SQLite 1 行 → 刷新仍在 → 改正文显示「锚点失效」→ 还原后重新锚定 → 删除后 SQLite 0 行
- 第二次启动：进程数保持 1
- 保存的库路径不存在：回退到 sample-vault，路径显示正常
- 静默安装 → 启动安装版 → 静默卸载：退出码 0，目录删除
- 侧栏整理：新建笔记/文件夹 → 重命名 → 拖拽移动（含链接源）→ 删除进 `.trash` → 8 秒内撤销：重做后的 release 构建上仍 20/20；wikilink 与相对链接按预期改写，标注在改名+移动后仍锚定（`shots/accept-01-start.png` … `accept-05-final.png`，脚本 `tools/verify-sidebar.mjs`）
- 侧栏交互重做：方向键/Home/End/Enter/F2/M/Delete/Esc、拖拽 650ms 自动展开、源行变暗与「移入」、移动 toast 撤销：走查通过；笔记/图谱 × 明/暗四组可读性审计 0 违规（`docs/SIDEBAR.md` §11，`shots/ui-after-*.png`）
- 侧栏虚拟滚动：10k 扁平库实际挂载 27–38 行（总 10,001），搜索最长 174–184ms，滚到底和键盘 End 正常，同一套侧栏回归 20/20（`docs/SIDEBAR.md` §11.6，`docs/measured.json` → `sidebarVirtualization`）
- ScribeFlow 制作台：夹具 3 个 artifact + 1 篇普通笔记；侧栏、详情、preview、打开笔记、来源条、右侧来源卡、待入库状态 **9/9** 通过（`tools/verify-studio.mjs`，截图 `shots/studio-01-overview.png`、`shots/studio-02-note-provenance.png`）
- 从页边笔记新建制作：打包版点「新建制作」后，Chrome 地址栏完整保留 `?new=1&yebianVault=D:/Develop/yebian-e2e-fixture`；连接后弹「新建制作」，空白工程隐藏、页边笔记链路排第一；建出的画布是 `文本 → AI 校对 → AI 加工 → 入库：页边笔记`（4 节点 3 连线，`shots/studio-03-canvas.png`），不再落到空的「未命名工程」
- 索引流水线：10k 切库/重建时进度条正常、旧内容可读；`tools/verify-sidebar.mjs` 20/20 回归通过；kill 中断后重启回到旧索引并清理 staging；单文件外部修改 488ms 可见（`docs/INDEXING.md`）

> 标注流程的截图和数字来自改名前的同一份代码；改名只动了品牌、bundle id、数据目录和可执行文件名，没有改标注逻辑。
>
> 侧栏这组是通过 CDP 在 release 原生窗口里派发 DOM 事件完成的，不是物理鼠标；限制见 §8。

截图在 [`shots/`](shots/)：`release-note-final.png`（最终笔记页）、`release-force-final.png`（4 篇卡片图）、`force-panel-final.png`（点击聚焦面板）、`force-40-stretch.png`（80 篇合成库的最近 40 篇点阵）、`force-40-focus.png`（同一合成库的近景卡片）、`force-search-final.png`、`force-people-final.png`、`force-path-final.png`、`anno-08.png`、`anno-11-orphan.png`、`anno-12-reanchored.png` 等。

---

## 8. 没做完的 / 已知限制

- **06 的链式叙事页没单独移植**。07 的全库力场已移植到顶栏「图谱」，并带 BFS 找路径；06 那种按主题串讲的视图仍只在 lab。
- **图谱默认最近 40 篇，全部上限 200 篇**：不做聚类/分层/按需加载；超过 200 只画最近 200。200 篇构建 263ms 是同步阻塞；节点本身不能拖，只能拖背景。
- **图谱搜索只搜当前范围**：要搜全库先点「全部」切到全部；最近 40 模式下更早的笔记不在结果里。
- **远景是点阵不是卡片**：40+ 篇默认落在远景档，必须滚轮或点击放大到 102% 才显示标题；这是语义缩放，不是渲染失败。
- **图谱构建计时不含首帧绘制**：表里的 9/58/97/263ms 是 JS 建图 + DOM 定位 + 首次适配，不是点击到像素上屏的端到端时间。
- **内存 258–278MB**（2026-11-05 虚拟滚动构建，5 次中位 261.2），比上一构建的 233.6–245.3MB（中位 244.2）高约 17MB；4 篇启动只挂 5 行，没有证据把差值归因于虚拟滚动，但同机复测先按回归记录。超过最初 150MB 的目标；原因见上，WebView2 固定成本。
- **10k 切库/重建 3.4–5.7s**：staging 构建 + 文件级 swap，后台执行、有进度。swap 事务不再复制 10k 行；list 最长 0.19–0.32s（1 万条元数据序列化），search 最长 0.17–0.18s。**不是零阻塞**。
- **50k 切库 13.1–15.5s**：contentless 索引下 parse 3.6–3.9s、staging 7.1–7.2s（含 FTS optimize 0.48s）、swap 0.19–0.22s、post 1.2s；写入阶段有逐批进度。list 一次返回 5 万条要 0.92–0.94s；search 20–40ms；侧栏仍只挂 28 行。**50k 索引 259MiB**（FTS 34MB + notes 230MB），是 227MB 源文件实际字节的 1.2 倍；100k 未测。
- **FTS 已改为 contentless + detail=none**：FTS 本体从 410MB 的内容表 + 33MB postings 降到 **34MB**；索引 schema 升级时在启动后台重建，旧索引在 swap 前仍可读。50k 单文件更新/还原已实测：新词命中 1 条，还原后 0 条。剩余大头是 notes 表的 230MB 正文；contentless 的代价是不能做 phrase 查询（当前搜索没用到）。
- **10k 切回 4 篇 0.23–1.3s**：同样后台；旧索引在 swap 前可用。
- **单文件外部修改（4 篇）488ms**：450ms 防抖 + 38ms 解析/写入；10k 未单独测。watcher 事件会丢，15 分钟一次全量 stat 对账兜底。
- **索引库拆分**：notes/FTS/meta 在 `yebian-index.sqlite`，annotations/settings 在 `yebian.sqlite`；重建只替换索引文件，用户数据不参与。staging 内做 FTS `optimize`；没有后台增量 merge。旧库里的 notes/FTS 表保留但不读取。
- **切库期间写**：swap 会关闭并重开索引读连接（122–128ms），标注写入最多等这一小段（busy_timeout 30s）；不会丢数据。
- **中文输入法 / GDI 泄漏 / 长路径 / 125%·150% 双屏**没有真机逐项验收。
- **没有代码签名和自动更新**；未签名安装包会被 SmartScreen 拦。
- **没有在无 WebView2 的干净 Windows 上测安装**。
- **搜索质量只测了延迟**，没有在真实笔记上算召回/精确率；bigram 是离线可复现的折中，不是最优解。
- **前端长文渲染没单独计时**；50k 字笔记的 `marked` + DOMPurify 可能更慢。
- **在资源管理器里移动的旧笔记**（没有 front matter `id`）只有在正文指纹唯一匹配时才能回填 id；同内容多文件或正文被同时改动时仍会变成孤儿，没有全局孤儿视图。
- **没有独立视觉验收**：visual-judge 只支持 pptx/docx/xlsx/pdf/poster/chart，不支持原生窗口/HTML；截图是人工检查。
- **可读性门槛现在通过**：`tools/audit-text.mjs` 在笔记页/图谱页 × 明/暗四组都退出 0（12px / 4.5:1）。制作台 2026-11-06 也统一到 12px 下限：home 从 144/178 个 <12px 降到 0，settings/tasks/task detail/wizard/Bili 登录在明暗两组均 0 违规。
- **索引基准语料是同一篇正文的变体**，token 分布比真实库重复，首次索引数字只作量级参考；图谱基准是 80/200 篇合成笔记（42 个主题、28 个人物、12 个 UP 主），不是用户真实库。
- **侧栏文件操作只在应用内被跟踪**：新建/重命名/拖拽移动/删除已跑通（`tools/verify-sidebar.mjs`，20/20）；在资源管理器里移动文件时，只有正文指纹唯一匹配才能回填 id，同内容多文件或正文同时改动仍会变成孤儿。
- **侧栏虚拟滚动只测了 10k 扁平库**：10,001 行时实际挂载 27–38 行，滚到底和键盘 End 能落到最后一条；层级深、筛选后、拖拽跨窗口没有逐项压测。行高固定为 30/48/26px，依赖 CSS 与 JS 常量一致；没有 `aria-setsize`/`aria-posinset`；滚动把焦点行卸载后，Tab 停靠点会移到视口首行。
- **收藏夹首屏约 100 条 + 加载更多**：默认收藏夹 1051 条不会再一次铺满；每次「加载更多」约 20 条。观看历史仍固定最近 100 条，没有继续翻页。
- **断点续跑首版**：检查点绑定创建时笔记库和非密钥配置快照；旧任务没有快照只能重跑全部。检查点在本机 SQLite，不随 vault 同步；失败任务会保留，成功任务会清理。
- **断点续跑未做杀进程重启的完整 UI 走查**：只复用了现有 interrupted 状态和 store 单测；发布崩溃用单测覆盖，不是进程级注入。
- **提示词版本**：自定义提示词每次保存生成版本，可看当前草稿 vs 历史版本的逐行 diff 并恢复；恢复本身生成新版本，不删历史。
- **模型设置收尾**：636 个模型支持推荐/上下文/输入价/输出价排序；模型 ID 一键复制（走 Tauri clipboard 插件，不弹 WebView 权限框）；目录没收录或自定义 provider 可选手动 CNY 价目，`手动 > 目录 > 不估算`。
- **标注重定位**：重生成/覆盖后块 ID 变化时，可在「锚点失效」区点一次「尝试重新定位」；高置信唯一匹配才迁移，相近候选/短句/大幅改写继续保留失效，不静默错挂。
- **可重复验收**：`npm run verify:resume` 18/18、`npm run verify:cleanup` 6/6；脚本使用临时 vault、本地 mock，测试后恢复原 vault/settings，并检查任务列表没有被误删。
- **清理与续跑对齐**：默认「清理已完成任务」只清成功任务；失败/中断/取消任务要单独确认清理，避免误删检查点；清理临时音频会跳过运行中和可续跑任务，并报告保留数量。
- **相关笔记字段**：人物/事件/时期/公司/概念等所有已配置字段都参与匹配；真实文本任务已验证生成笔记 front matter 的公司/概念能命中旧笔记并追加 `[[链接]]`。
- **断点续跑**：失败/中断/取消后，每个 AI 调用、ASR 分段、下载和提示词分段都有检查点；继续只补缺失部分，发布按 `noteId + runId + body hash` 认领，避免重复 revision。文本失败实测从 4 次请求/¥120 降到 3 次请求、2 次成功/¥80；ASR 两段失败后第一段不再重发。
- **成本统计修复**：校对、合并、属性抽取的 token 之前没有计入 usage；现在都计入，同一 mock 任务从 1M/1M ¥40 修正为 2M/2M ¥80（带抽取为 3M/3M ¥120）。旧任务数字不回填，工程里旧记录的金额是下限。
- **手改冲突首版**：重跑检测到笔记被手改时会询问「保留两份 / 覆盖并备份 / 取消」；任务详情「版本」可浏览生成稿和手改备份并恢复。恢复是整篇替换，不做逐段合并。
- **系列综述仍合并**：视频笔记等模板多选视频会拆成「每个视频一个任务、一篇笔记」；「系列综述」保留原来的合并语义，仍是一篇综述。
- **制作台是原生 Rust 实现**：不再跳 ScribeFlow Web 平台；B站公开链接、收藏夹/稍后再看/历史、扫码登录、AI/ASR 都在本机。真实账号已验证列表、多选建任务和登录过期路径，但没有逐条打开所有视频；联网溯源已用真实 DeepSeek Search 跑完整任务（927 秒视频，10 条主张/4 条可印证），但只有一条视频、未抓网页正文；练一练只用 mock 模型验证流程；多输入默认拆成每个视频一篇，「系列综述」仍合并一篇；手改冲突有「保留两份 / 覆盖并备份」；模型接入的真实对话只验了 DeepSeek，Anthropic/Gemini/OpenRouter、断网缓存、OpenRouter 实报未验；MiMo ASR 已用真实 18 分钟视频跑通（10 分钟分段）；速览版已恢复 SF 的两步配方（速览 / 正文 / 速查 / 弦外之音），但旧 SF 的 Obsidian callout 排版在原生阅读器里尚未兼容。提示词版本只覆盖自定义提示词，恢复是整段替换、不做合并。ASR 单价仍为 0，估算金额暂时只含 AI token。完整边界见 [`docs/PRODUCTION.md`](docs/PRODUCTION.md) 第 6 节。
- **侧栏验收用的是合成事件**：脚本通过 CDP 派发 `DragEvent`/`KeyboardEvent`，不是物理鼠标；真实鼠标的 4px 阈值、落点抖动、中文输入法 type-ahead 和 8 秒撤销边界没有逐项验证。
- **侧栏拖拽只支持「移入文件夹」**：不支持拖拽排序、多选拖动、拖到两个条目之间；移动的键盘替代路径是右键「移动到…」。
- **历史史料库（独立史料检索系统，live 自动引用未命中）**：史料单独入库（sources/passages/citations），事实引用原文 passage；**事实按 `domain` 分成「历史事实 / 其他事实」，只有历史事实显示「史料」区**，事实详情可人工纠正归类。独立搜索链路是 `查询规划（work/section/queries）→ ctext 官方 API / Wikisource / Internet Archive / NLC / 一史馆 / HathiTrust → authority 分层 → 抓全文 → 切 passage → 确定性/模型选逐字原文 → 子串校验`；只有 primary/transcription/ocr 全文才自动写 citation，link_only/二手/无 Key 只进候选池。**ctext 网页抓取被明确禁止，只走 `api.ctext.org`；当前本机 Wikisource/IA 直连超时、ctext 无 Key 只能拿书目，因此没有 live 自动 citation，史料库仍只有早前人工验证的 1 条《明史》**。候选会显示 provider/authority/status/reason，不冒充史料；设置页可填 ctext API Key。**没有 Key/网络失败时，候选行可「补录原文」：打开候选页复制正文，程序切 passage、逐字对齐并写 citation**；这条补录路径已真机通过并清理。详见 [`docs/HISTORY_SEARCH_DESIGN.md`](docs/HISTORY_SEARCH_DESIGN.md)。
- **历史链路质量基线（2026-11-09）**：用 UP `王山水读通鉴` 7 条真实视频 + 已有嘉靖笔记跑完，8 篇历史笔记 / 43 条历史事实；7 个新任务 1,730s、455,810 tokens、¥1.0705。42 条新事实 42/42 标为 history；前 34 条 work/section 人工核对 34/34 未见明显错书，长笔记 8 条实际检索的 top1 书目与时代相符；库内 43/43 有候选（含 12 条评测补搜 + 1 条长笔记手动补搜），41/43 top1 是 ctext 书目（苏轼文集、江表传两个缺口只能外链）；网页证据 39/43；自动 citation **0**，全库仍只有 1 条人工《明史》。自动 trace 覆盖 30/43（limit 8）；长笔记（13 分钟 / 9 主张）前两次卡 66% >15 分钟，修复后 **383s 成功、100%**，但自动史料入库仍只跑 8/9，第 9 条手动补搜 4 条候选。详见 [`docs/HISTORY_EVAL.md`](docs/HISTORY_EVAL.md)。
- **待补录候选队列**：史料库 →「待补录」→ `/sources/pending`；按事实聚合 `candidate` 候选，显示 provider/authority/reason，支持「打开来源」和「补录原文」。补录与事实详情共用同一个 `CandidatePasteDialog`；成功写 citation、候选标 cited，失败标 rejected。当前队列 360 条候选 / 43 条事实 / 0 条已引用；仍需用户打开页面复制原文，不自动抓 ctext/Wikisource/IA。没有候选的事实不会进队列，需要先在事实详情手动「自动找史料」。详见 [`docs/SOURCE_QUEUE_DESIGN.md`](docs/SOURCE_QUEUE_DESIGN.md)。
- **事实整理工作台（M1 + 拆分 M2）**：事实库新增「整理事实」→ `/facts/curate`；按已有关系展示重复候选（same_as 成对合并）、冲突（contradicts + 处理结论/备注）、复合事实（启发式检测 + 相关原子事实 + 拆分/标记）、无来源事实。M2 支持把复合事实人工拆成 2–8 条原子事实：复用 normalized 相同的已有事实、复制笔记 mention，**证据/引用留在父事实不复制**；父事实标记「已拆分」，可撤销。**阅读右栏保留父事实原句，子事实只在事实详情/事实库展开**——原子事实是核查单位，不是阅读单位。新增 `fact_splits` / `fact_conflict_resolutions` / `fact_reviews`；不做自动拆句、语义聚类、归档。当前 59 条事实：0 重复 / 0 冲突 / 2 复合 / 4 无来源 / 59 未整理；真机用真实「小布什家族」事实验证 4 条拆分（2 条复用 + 2 条新建）并撤销，库回到 59 条；测试数据已清理。详见 [`docs/FACT_SPLIT_DESIGN.md`](docs/FACT_SPLIT_DESIGN.md)。
- **新增页面 UI 整改（2026-11-09）**：按 1440/1040 两档走查事实库、事实详情、事实整理、史料库、史料详情、待补录队列和拆分弹窗。已删「补录离线史料」；队列按事实组折叠（默认只展开第一组）、候选行压缩、补录改主按钮、加 provider/authority 筛选；冲突面板区分数字/文字；拆分入口只在复合事实出现；事实库范围/状态分组、行内 meta 去噪；整理台统计卡 4 个可行动项、空 section 收成一行；事实详情空 section 收起、候选独立 section、sticky 锚点。完整问题和前后对照见 [`docs/UI_AUDIT.md`](docs/UI_AUDIT.md)；最终构建 `cargo test --lib` 166 passed / 7 ignored，包 8,722,874 B（8.32 MiB）。
- **左右栏与浮层滚动条（2026-11-09）**：左栏 180–440px 可拖、可收起，双击/键盘复位，宽度和收起状态持久化；右栏 300–560px 可拖、可收起为 28px 边缘条；顶栏开关和右栏边缘展开条都能找回。滚动条改成浮层 thumb：靠近右缘 90ms 后才出现，900ms 无操作 180ms 淡出，快速划过不闪；thumb 可拖。`yebian.sidebar.collapsed` 与 NoteList 文件夹折叠状态分开存。详见 [`docs/PANEL_INTERACTION.md`](docs/PANEL_INTERACTION.md)；截图在 [`shots/panels/`](shots/panels/README.md)。
- **事实争议状态审计（2026-11-10）**：全量 59 条事实、55 条核验中，22 条 disputed 逐条对照 evidence quote；**11 条是证据支持却被误标“存在争议”**，1 条是复合/重复，10 条是真实/可争议。根因是代码把 verdict“存在争议”直接映射成 evidence contradicts，prompt 也没定义争议口径；已改为 `relation` 字段 + 按 evidence relation 决定 status，并修复 11 条数据。状态从 `29 verified / 22 disputed` 变为 **40 verified / 11 disputed**。详见 [`docs/FACT_AUDIT.md`](docs/FACT_AUDIT.md)；截图 [`53-facts-after-audit.png`](shots/audit/53-facts-after-audit.png)。
- **标注改为选区驱动（2026-11-10）**：删掉每段右侧 `+` 入口；拖选正文（可跨段）→ 右栏「标注」出现临时卡片，3 色 + 标记/存疑/待办 + 单行输入，`Enter` 保存、`Esc` 取消；保存后正文用精确 `<mark>` 高亮，点高亮回看，卡片按正文顺序编号。SQLite 增 `segments_json`/`color`，旧标注回退整块；`relocate_annotations` 同步 segment blockId。**M1 的页边卡片仍在右栏，没有排到段落同高**；键盘 H/N、hover 预览、移动端未做。见 [`docs/ANNOTATION_REDESIGN.md`](docs/ANNOTATION_REDESIGN.md)；截图 `shots/audit/58,60`。`cargo test --lib` 166 passed / 7 ignored，包 8,722,874 B。
- **笔记阅读页排版整改（2026-11-09）**：标题按 `：:，,、；;` 分块，不再把「机/会」拆行；字号 clamp(30,2.7vw,39)，正文容器 ≤760px 时 28px。笔记不再署 UP 名：侧栏改 Obsidian 式 30px 单行，每篇只有标题（有标注时行尾一个数字 badge）；图标 lead 19px 与标题行框等高，图标 16px 并上移 1px，星标也上移 1px，标签台/事实笔记组也不再显示 UP；标题下只留 `2,461 字 · 原视频`，日期、UP、发布/运行/生成信息收进右栏「信息」，原视频直接打开 B 站。原来两条元信息（作者/日期/字数 + ScribeFlow badge/run id/prompt/时间/按钮）合并为一行署名，删掉重复 run id 和 badge。署名 20px、三档单行、溢出 0；速览恒为单栏；正文列宽上限 860px，三档 DOM 实测正文 860/622/430，标题均 2 个视觉行。栏宽保留用户偏好，≤1180 视口有效值压到左 280 / 右 330，回宽屏恢复。详见 [`docs/NOTE_READING_AUDIT.md`](docs/NOTE_READING_AUDIT.md)；前后对照见 [`shots/audit/`](shots/audit/README.md)。未验 900 宽/缩放/暗色/触屏，只测一篇中文笔记；原视频是外链不是内嵌播放；侧栏不再显示日期。
- **侧栏搜索（2026-11-09）**：删除顶栏搜索，侧栏「筛选笔记…」改成唯一入口「搜索笔记…」；140ms 防抖调用现有 FTS，结果显示 `N 篇笔记` + 标题 + 路径，标题/路径不含 query 时标「正文命中」。搜索框修掉黑色实心圆点（SVG 缺 `fill:none`）和 border+outline 双层粗环，focus 改为 1px 蓝边 + 2px 淡蓝光晕；`Ctrl+K` 聚焦侧栏，`Esc` 清空恢复树。1040 下长标题块允许内部换行，不再被右栏裁切。详见 [`docs/SEARCH_DESIGN.md`](docs/SEARCH_DESIGN.md)；截图 [`shots/audit/44..48`](shots/audit/README.md)。未做正文片段/高亮、正则/过滤语法、搜索历史；只测 24 篇库。
- **UP 订阅自动制作首版**：真实 UID 2 添加后基线 43 条、0 任务；显式补跑 1 条创建 auto 任务并成功，笔记落 `待整理`；重复检查不重复建任务。自动检查仅在应用运行时进行，托盘/开机自启可选；失败/中断不自动重试，手动检查没有单独 60 秒/UP 冷却；只读空间第一页 50 条，不反查手写同 URL 笔记。**2026-11-09 空间请求稳定性**：space/WBI/buvid/followings/login 最多 3 次重试，错误分类显示；瞬时传输失败首次退避 2 分钟而不是 60 分钟。`寒清关下说书人` 的瞬时 `error sending request` 复检已恢复正常。
- **E-01 首轮验证发生过一次任务记录误删**：当时的清理命令未按 vault 过滤，`verify:cleanup` 在 sample-vault 上删除了 10 条任务记录；笔记文件未受影响，但任务历史无法恢复。现在清理已按 vault 过滤，脚本会先确认切换临时 vault 后任务列表为空，再执行破坏性断言。
- **安装/卸载已在较早的同数据层包验收；8.32 MiB 最终包只做了覆盖安装并启动**：覆盖后 13 篇笔记、16 条事实、6 条合并记录、B站账号和订阅仍在；静默卸载/重装验收仍来自较早的 7.56 MiB 包，不是这个最终包。**没有重测没有 WebView2 的干净机器，也没有勾选卸载时删除应用数据。**
- **云同步（P0 首版）**：记录面同步标注、设置、事实/史料/任务/订阅等表（设备本地日志、断点、页面缓存和 API Key/Cookie/本机路径除外），文件面同步 vault 里的 md/json；采用「服务器追加日志 + 本地 SQLite 物化」，行级 LWW + HLC + 墓碑，文件用 SHA-256 内容寻址 + 路径清单。冲突自动处理，被覆盖一方进 `sync_conflicts`，不弹手动选择。服务端 `sync-server/` 只存不透明 op 与 blob，token 只存哈希。设置页「云同步」填一次服务器/库 ID/token 后**自动同步**：本地同步表写入会触发脏标记，后台 2 秒内推送；远端默认每分钟拉取；「手动同步一次」只是兜底。**实测**：14 个同步测试通过（含真实 HTTP 双设备收敛和本地写入触发脏标记），`cargo test --lib` 159 passed / 7 ignored，服务端 4 个测试通过，前端 build 通过。**未做**：E2E 加密、冲突查看 UI、同 `normalized` 事实的跨设备自动合并、正文块级合并、服务端 blob GC；`production_logs/steps/checkpoints/history_page_cache` 不同步。方案与边界见 [`docs/SYNC_DESIGN.md`](docs/SYNC_DESIGN.md)，服务端在 [`sync-server/`](sync-server/)。

---

## 9. 目录

```
yebian/
├── src/                         Vue 前端
│   ├── views/NoteView.vue       笔记页：正文 / 局部图 / 标注
│   ├── views/TagsView.vue       标签台：AND/OR、结果
│   ├── views/GraphView.vue      图谱页：DOM/SVG 力场 / 语义缩放 / 找路径
│   ├── components/              NoteList（目录树/收藏/筛选）/ NoteBlock / RelationGraph
│   ├── lib/                     api / markdown 解析 / 图模型 / note-tree
│   └── styles/                  本地字体 + 笔记页 + force.css + sidebar.css
├── src-tauri/
│   ├── src/lib.rs               commands、watcher、启动、单实例
│   ├── src/db.rs                SQLite、FTS5、bigram、增量索引、身份迁移
│   ├── src/ops.rs               新建/重命名/移动/删除、路径校验、回收站
│   ├── src/links.rs             wikilink / Markdown 链接解析与改写
│   ├── src/vault.rs             扫描、front matter、id、原子写
│   ├── src/window.rs            clamp / DPI
│   ├── tauri.conf.json          窗口、CSP、bundle
│   └── capabilities/default.json
├── sample-vault/                4 篇样例
├── tools/                       measured / shot / click / db 脚本
├── docs/                        measured.json + experiment-log.txt + INDEXING.md + INDEXING-SPLIT.md + SIDEBAR.md + PRODUCTION.md + PRODUCTION_BACKLOG.md + MODEL_PROVIDER_DESIGN.md + SCRIBEFLOW.md + HISTORY_SOURCES_DESIGN.md
├── shots/                       原生窗口截图
├── DESKTOP.md                   选型、桌面坑、性能口径、诚实清单
└── README.md
```

---

## 10. 下一步

按优先级：

1. 06 的链式叙事 / 主题聚焦视图（全库力场和找路径已经落地）。
2. 索引流水线：A（actor/读写分离/批量/定向增量/进度）+ B（索引库拆分 + staging 文件替换/崩溃恢复）+ 侧栏虚拟滚动 + contentless FTS 已落地；50k 已测（索引 259MiB、切库 13.1–15.5s、list 0.92s、search 20–40ms），剩余是 100k 压测、列表懒加载/压缩 IPC、FTS 增量 merge、10k 单文件 watcher 隔离。设计与实测见 [`docs/INDEXING.md`](docs/INDEXING.md)。
3. 原生制作台：B站/本地文件/文稿来源、10 个内置模板、扫码登录/收藏夹、AI/ASR/检索、Artifact v2 Outbox/ACK 都已落进页边笔记；Rust 101/101、前端 build 通过，文本与真实 B站音轨的 mock 端到端已跑通；模型接入/自动计价首版已落地（13 provider、models.dev 636 模型、CNY 汇率、预算提醒）；UP 订阅自动制作首版已通过真实 B站空间 + 本地 mock 端到端。收藏夹/稍后/历史、多选建任务、取消/中断/重试已真机验收；剩余边界（系列综述分篇语义、三方合并、真实检索、云端、手动价目账单核验、UP 订阅长期风控）见 [`docs/PRODUCTION.md`](docs/PRODUCTION.md) §6。旧 ScribeFlow 协议历史见 [`docs/SCRIBEFLOW.md`](docs/SCRIBEFLOW.md)。
3. 全局孤儿标注视图（只剩「外部移动且指纹不唯一」这一种情况会产生孤儿）。
4. 中文输入法、GDI、长路径、双屏 DPI 的真机验收。
5. 代码签名 + 自动更新（要证书/密钥，最后做）。
