# 页边笔记的桌面端：为什么是 Tauri 2，以及桌面坑怎么解

> 目标不是“把网页套个壳”，而是做一个**本地笔记应用**：冷启动快、内存低、离线可用、读写本地文件不卡、中文输入和中文搜索不打折、装到 Windows 上不弹奇怪的警告。
>
> 这份文档记录：选型依据、每个桌面端常见问题的处理、当前实现状态、性能预算与实测、以及没验证的部分。

---

## 1. 结论

**Tauri 2 + Vue 3 + Vite + TypeScript + Rust + SQLite（rusqlite, bundled FTS5）。**

| 层 | 选择 | 理由 |
| --- | --- | --- |
| 壳 | Tauri 2 | 复用系统 WebView2；安装包 6.42MiB、首屏 0.68s 已实测；**内存优势在本机不成立**（见 §7） |
| 前端 | Vue 3 + Vite + TS | 和现有前端栈一致；09 原型的组件可以平移 |
| 后端 | Rust commands | 扫描/解析/索引/标注/文件监听都在本地；不额外起 Node 进程 |
| 存储 | SQLite + FTS5 | 标注与索引一个文件；FTS5 中文检索；WAL + busy_timeout |
| 中文检索 | 自写 bigram 分词 + FTS5 | 不依赖词典/模型；离线、可解释；比 unicode61 默认分词可用 |
| 字体 | 本地 WOFF2 子集 | 不联网；Noto Serif SC 400/600 各 ~1.5MB，总计 ~3.1MB |

**不用 Electron 的原因**：这个应用的前端就是本地 Web 页面，不需要 Node 运行时；二手对比里 Electron 常被报 85MB+ 基线、150–300MB 空闲内存、1–2s 冷启动（未在本机复测），对这个场景是纯成本。Tauri 的代价是要维护一条 Rust 缝，以及 WebView2 的 Windows 差异——后者在下面逐条处理。

> 二手基准（不是我的实测）：2026 年多篇对比文章给出的 Tauri 2 约 3–15MB 安装包、30–80MB 空闲内存、<500ms 冷启动；Electron 约 85–244MB、150–300MB、1–3s。**本机实测 Tauri 空闲私有内存 258–278MB，落在二手 Electron 区间内**，所以本文不引用“Tauri 内存是 Electron 的 1/10”这类说法；第 7 节给本机实测。

---

## 2. 进程与数据流

```
┌──────────────────────────── Tauri 窗口 ────────────────────────────┐
│  Vue 3 (WebView2)                                                  │
│  ├─ 笔记页：正文 / 局部关系图 / 标注                                │
│  ├─ 图谱页：DOM/SVG 力场 / 语义缩放 / 找路径                       │
│  ├─ 标签台：多选标签 → 交集 → 结果                                  │
│  └─ 搜索：只发 query，结果由 Rust 返回                              │
│        │ invoke（JSON，小载荷）                                     │
│  ┌─────▼─────────────────────────────────────────────────────────┐  │
│  │ Rust 后端                                                     │  │
│  │  ├─ vault：walkdir 扫描 .md，解析 front matter                 │  │
│  │  ├─ index：bigram 分词 → SQLite FTS5                          │  │
│  │  ├─ annotations：SQLite（块 ID + quote + body + kind）         │  │
│  │  └─ watcher：notify，450ms 防抖 → 重建索引 → emit 事件         │  │
│  └───────────────────────────────────────────────────────────────┘  │
│  本地：<app_data>/yebian.sqlite（标注/设置）                   │
│        <app_data>/yebian-index.sqlite（索引，可重建）          │
└────────────────────────────────────────────────────────────────────┘
```

- **笔记库只读**：应用扫描 Markdown，不写回。标注/索引只在 SQLite。
- **小载荷 IPC**：列表只传元数据（不含正文）；打开某篇才取 `body`；搜索只返回前 40 条元数据。
- **索引重建**：启动/监听触发时**增量**（`rel_path` + 毫秒 mtime）；只对变过的文件重新解析、重写 FTS。库根变化、FTS 与 notes 行数不一致、点“重建索引”按钮时强制全量。FTS 的删除从“按未索引的 `id` 删”改成“按 `notes.rowid` 删”——前者是 O(N²)，10k 条二次索引实测 119s；改完 0.72s。

---

## 3. 桌面端常见问题 × 本项目的处理

| 问题 | 处理 | 状态 |
| --- | --- | --- |
| 开两个实例、第二个把第一个顶掉 | `tauri-plugin-single-instance`，**注册在所有插件之前**；第二次启动时 show/unminimize/focus 已有窗口 | 已实现；实测第二次启动 1.08s 退出、进程数保持 1 |
| 窗口位置/大小不恢复 | `tauri-plugin-window-state` 持久化 | 已实现；本机恢复成功 |
| 保存的笔记库被移动/删除，启动后空白或路径错 | 启动时校验 `vault_path` 仍是目录；不是则回退到内置 `sample-vault`、重写设置；`index_root` 变化时强制刷新所有绝对路径 | 已实现 + 实测（含安装版） |
| 路径带 `\\?\` 前缀 | `display_path` 统一剥掉；笔记路径、库路径都走这一层 | 已实现 + 实测 |
| 多显示器不同缩放，窗口被撑到 65535×53251，WebView2 白屏，插件还把坏尺寸存下来 | 升级到 window-state ≥2.2.2，并在 `setup` / `ScaleFactorChanged` 后**自定义 clamp**：宽度/高度超过最大显示器 2 倍、或小于 800×560，就重置 1440×900 并居中；中心点不在任何显示器内也居中 | 已实现 |
| WebView2 在窗口 resize 后把中文/韩文输入法的候选框画到屏幕左上角（#5675） | resize 开始时如果焦点在 input/textarea，先 blur，160ms 后恢复焦点，重置 IME composition 窗口 | 已实现（需真机中文输入法验收） |
| `window.hide()` 到托盘后 WebView2 仍在合成，空闲 CPU 高 | 本应用**不做托盘/隐藏窗口**；关闭即退出。不引入“隐藏但仍在跑”的路径 | 设计规避 |
| WebView2 鼠标移动导致 GDI 对象泄漏（#5536） | 运行时无法从应用侧修；记录为已知风险，长会话监控 GDI 数 | 待观察 |
| 字体依赖 Google Fonts，离线变丑/FOUT | 所有字体本地 WOFF2：Noto Serif SC 400/600（OFL）、Sora、Newsreader、Bricolage | 已实现 |
| 图谱规模与卡顿 | 自写力导 + 矩形碰撞；默认最近 40 篇，可切全部（上限 200）；80 篇构建 97ms，200 篇 263ms；构建后无 rAF | 已实现；见第 7 节 |
| 关系图卡顿/动画一直跑 | Canvas 2D（DPR 自适应）；布局只算一次；静止无 rAF；只在入场/悬停/拖动重绘 | 已实现；10 节点布局 p50 0.27–0.54ms |
| 中文搜索默认分词不可用（整句一个 token） | FTS5 `unicode61` + **自写 CJK bigram**：`养老金` → `养老 老金`；ASCII 保留整词小写；查询同分词；无结果再 `LIKE` 兜底 | 已实现 |
| SQLite 并发写冲突 | WAL、`busy_timeout=5000`、单连接 `Mutex`；标注写入与索引重建串行 | 已实现 |
| 文件监听风暴 | `notify` + 450ms 防抖 + 只对 `.md` 事件触发；重建后 `emit("vault-changed")` 让前端刷新 | 已实现 |
| 中文/括号路径（`C:\Users\Admin（无密码）`） | 全程 `PathBuf` / UTF-8；SQLite 文件在 app data 目录；不做 shell 字符串拼接 | 已实现（待实测） |
| 超长路径 | Rust `std::fs` + walkdir；Windows 长路径策略未逐项验证 | 待验证 |
| 未签名安装包的 SmartScreen | 开发构建不签名；正式分发需代码签名证书（见第 6 节） | 未做 |
| 自动更新 | 需要签名密钥 + HTTPS endpoint；第一版不做，避免半套更新把用户卡死 | 未做 |
| 安装/卸载 | NSIS `currentUser` + 简体中文；`sample-vault` 作为资源打进安装包 | 已实现；静默安装/卸载 exit 0，安装版跑通 |
| CSP / 远程内容 | `script-src 'self'`、`connect-src ipc: http://ipc.localhost`、无远程资源；capabilities 只开 `core:default` + `dialog:default` | 已实现 |
| 权限过大 | 不启用 shell/fs 插件；文件夹选择走 dialog；文件读取在 Rust 内 | 已实现 |
| 崩溃无日志 | `tauri-plugin-log`：stdout + LogDir；启动/索引/监听都有 info/warn/error | 已实现 |
| 高 DPI 模糊/错位 | 窗口用逻辑尺寸；CSS 像素；window-state 物理像素由 clamp 兜底 | 待实测 125%/150% |
| 字体/图标网络请求 | 无 CDN；字体本地；图标是 SVG 文本/系统字符 | 已实现 |

---

## 4. 为什么中文搜索不用 jieba

调研到的两条路线：

1. **FTS5 trigram**：零依赖，任意 CJK 子串可搜；但一个 10k 条目的基准给出约 78% 召回 / 45% 精确率，索引也更大。
2. **jieba 预分词**：同一基准约 97% 召回 / 94% 精确率、3ms 延迟；但要带词典，且 Rust 的 `jieba-rs` 依赖 `include-flate/libflate`，在离线构建里多一层依赖。

本次选择**第三种：自写 bigram**。

- `养老金账户` → `养老 老金 金账 账户`；查询 `养老金` → `养老 老金`，AND 命中。
- 不需要词典、模型或额外 crate；索引/查询完全对称，行为可解释。
- 代价：索引比整词方案大；单字查询基本靠 `LIKE` 兜底；语义近义词（“养老” vs “退休”）不解决。
- 这是**质量与离线可复现的折中**；如果以后接受带词典，jieba 的 97/94 明显更好。

> 上述 78/45 与 97/94 来自检索摘要里的一份 10k 条目基准，我没有复跑那份基准；本项目的搜索质量用第 7 节的样例查询单独测。

---

## 5. 数据模型

```sql
notes(id PK, path, rel_path, title, up, date, domain,
      tags JSON, people JSON, events JSON, periods JSON,
      body, run_id, mtime, chars)
notes_fts(id UNINDEXED, title, tokens, tokenize='unicode61')
annotations(id PK, note_id, block_id, quote, body, kind, created_at)
settings(key PK, value)
```

- `id` = 相对路径的 SHA-256 前 16 位；文件移动会得到新 id（旧标注成为孤儿，前端显示“锚点失效”）。
- `block_id` = 渲染时按块文本归一化后的 FNV-1a 哈希；同一篇内重复文本加 `-2` 后缀。
- 标注存 `quote` 原文，块找不到时仍可读、可删，不静默丢。
- `run_id` 从 `run_*` 父目录提取，用于显示“这一次渲染”。

---

## 6. 分发（现在没做，但必须知道代价）

| 平台 | 要求 | 代价 |
| --- | --- | --- |
| Windows | 代码签名证书（OV/EV 或 Azure Trusted Signing）；NSIS 安装包 | 未签名会触发 SmartScreen；证书有年费/审核 |
| macOS | Developer ID 签名 + notarytool 公证 | 需要 Apple Developer 账号 |
| Linux | AppImage/deb；WebKitGTK 运行时 | 本机未验证 |

第一版只做 Windows NSIS（`installMode: currentUser`，简体中文），已产出安装包并做过静默安装/卸载验收；**没有证书就不对外分发**，只在本机/信任环境用。自动更新未做。

实测（2026-11-04 构建）：安装包 5,551,228 B（5.29 MiB）；安装后是 `yebian.exe` + `uninstall.exe` + `sample-vault/`；静默安装、启动、静默卸载的退出码均为 0。安装版从安装目录的 `sample-vault` 读取样例，路径显示正常。

---

## 7. 性能：预算 vs 实测

本机：Intel Core Ultra 9 275HX（24C/24T）、63.4GB RAM、Windows 11 26200、WebView2 156.0.4314.58、Rust 1.96.1。**下面全部是实测**；原始输出在 `docs/experiment-log.txt`，结构化数据在 `docs/measured.json`。

| 指标 | 预算 | 实测 | 结论 |
| --- | --- | --- | --- |
| 安装包 | < 25MB | **6,861,829 B（6.54 MiB）** | 通过；安装验收在 11-04 构建上 |
| 主程序 exe | — | 15,164,928 B（14.46 MiB） | — |
| 冷启动：窗口可见 | < 1.5s | 49–321ms（中位 62ms；321ms 为进程启动抖动） | 通过（窗口先出现，内容后到） |
| 冷启动：首篇笔记 DOM 完成 | < 1.5s | **657–697ms**（中位 676ms，1456×939） | 通过 |
| 空闲私有内存（整棵进程树） | < 150MB | **258.3–278.1MB**（5 次，1456×939，4s，中位 261.2） | **未达标** |
| 空闲工作集（整棵进程树） | — | 467.5–490.9MB | 工作集重复计共享 DLL，仅作参考 |
| 其中 Tauri 本体 | — | 私有 **约 6.9MB**；WebView2 宿主另计 | WebView2 是主要占用 |
| 空闲 CPU（整棵树） | ~0 | 0–0.39%（一次启动采样离群；其余 0–0.03%） | 通过；关系图静止无 rAF |
| 10k 切库/重建（staging + 文件替换） | — | **3.4–5.7s**（后台，带进度） | swap 122–128ms；读尖峰见第 8 节 |
| 10k 切回 4 篇 | — | 0.23–1.3s | 后台 |
| 单文件外部修改（4 篇） | — | 488ms（450ms 防抖 + 38ms） | — |
| Store 层 1k / 10k 首次写入（微基准） | — | 336–368ms / 4.1–4.2s | 不含调度/换库 |
| 搜索 1k 篇（FTS，p50/p95） | < 50ms | 1.27–1.32ms / 1.31–1.45ms | 通过 |
| 搜索 10k 篇（FTS，p50/p95） | < 50ms | 5.74–5.97ms / 6.94–7.09ms | 通过 |
| 搜索无 FTS 命中，LIKE 兜底 1k / 10k | — | 5.5–5.6ms / **68–82ms** | 10k 时接近可感知 |
| 打开一篇（Rust 取元数据+正文） | < 100ms | 69–167µs | 通过；不含前端 parse/paint |
| 笔记列表 1k / 10k | — | 5ms / 52–63ms | 通过 |
| 标注写入 + 删除（SQLite，p50/p95） | < 20ms | **21–38µs / 27–59µs** | 通过 |
| 局部图数据构建 1k（p50/p95） | — | 0.85ms / 2.61ms | 通过 |
| 局部图数据构建 10k（p50/p95） | — | 10.28ms / 12.60ms | 通过；打开一篇笔记的取数成本 |
| 局部图布局（10 节点，p50/p95） | < 16ms | 0.27–0.54ms / 0.68–2.07ms | 通过；只算一次 |
| 关系图静止时 rAF | 0 | 0（入场 620ms + 悬停/拖动重绘） | 无持续动画 |
| 图谱构建 4 篇（36 节点 / 37 边） | < 50ms | **9ms**（8–12ms，5 次） | 通过 |
| 图谱构建最近 40 篇（122 / 320） | < 100ms | **58ms**（55–62ms，5 次） | 通过 |
| 图谱构建全部 80 篇（168 / 640） | < 200ms | **97ms**（96–97ms，3 次） | 通过 |
| 图谱构建全部 200 篇（288 / 1600） | < 500ms | **263ms**（262–267ms，3 次） | 通过，但同步阻塞 |

**测量口径（重要）**

- 冷启动“窗口可见”用 `EnumWindows` 每 10ms 轮询一个 ≥800×500 的可见窗口；“首篇笔记 DOM 完成”由前端在 `nextTick` 后 `invoke('app_ready')`，Rust 用进程内 `Instant` 打点。两者都不是“像素已上屏”，但后者比窗口可见更接近用户感知。
- 内存是 **7 个进程的算术和**（`Get-Process` 的 `PrivateMemorySize64` / `WorkingSet64`）。工作集会重复计算每个进程映射的同一份 DLL，所以 467.5–490.9MB 不能当作“多占同样多的物理内存”；私有内存更可靠。
- 最终 DOM/SVG 图谱版（1456×939）：4s 私有内存 5 次 258.3–278.1MB（中位 261.2）；上一构建为 233.6–245.3MB（中位 244.2），差值与虚拟滚动无因果证据。旧 Canvas 版本数字已作废，不再引用。65 秒采样（263.5 → 262.4 → 259.3 → 256.2MB）来自 2026-11-04 构建，当前构建没有重跑。
- 索引/搜索/标注写入基准是 release 构建、临时库、1000/10000 篇，每篇正文 1521 个中文字符（直接取真实样例 `shebao.md` 的正文，不是短句拼接）。两次运行的差异 10–20%，LIKE 兜底与标注写入的波动更大；表里给两次的区间，索引首次值取较慢的一次。字号/栏宽调整只改 CSS，不影响这组数据。
- 局部图数据/布局是在 Node 22 里直接跑 `src/lib/graph.ts` 与 `src/lib/graphLayout.ts`，只计数据与力导计算，不含 Canvas 绘制；Canvas 只在一个 620ms 入场动画和悬停/拖动时重绘，静止无 rAF。
- 图谱构建计时在 release WebView2 内用 `GraphView.build()` 的 `performance.now()`：含 320 拍力导 + 碰撞松弛 + DOM 构建 + 首次适配，**不含 CSS 首帧绘制**。用 `tools/measure-force-many.mjs` 重复 3–5 次；语料由 `tools/make-bench-vault.mjs` 生成。
- 没有把“打开笔记”的前端 `marked` 解析 + DOMPurify + Vue 渲染单独计时；这一项只有 Rust 侧数字。

### 7.1 端到端验收（原生窗口，release 构建）

| 场景 | 结果 | 证据 |
| --- | --- | --- |
| 笔记页 / 标签台 / 从标签打开笔记 | 正常 | `shots/release-note-final.png`、`shots/tags-04.png`、`shots/note-from-tags.png` |
| 关系图 hover / 全屏 overlay / 点邻居换篇 | 正常 | `shots/graph-hover5-crop.png`、`shots/graph-overlay-final.png`、`shots/graph-navigate2.png` |
| 图谱：4 篇卡片 / 点击聚焦面板 / 40 篇点阵 | 正常 | `shots/release-force-final.png`、`shots/force-panel-final.png`、`shots/force-40-stretch.png` |
| 搜索“养老金” | 返回社保笔记 | `shots/release-search.png` |
| 图谱搜索「社保」→ 聚焦面板 → 打开笔记 | 正常；点击不跳转，显式按钮才跳 | `shots/force-search-final.png`、`shots/force-panel-final.png` |
| 图谱找路径：沙利文 → 类型/学习笔记 → AI Infra | 3 步路径，虚线高亮 | `shots/force-path-final.png` |
| 标注新增 → 保存 | UI 高亮 + 编号 + 右栏卡片；SQLite 1 行 | `shots/anno-07.png`、`shots/anno-08.png` |
| 刷新/重启后标注仍在 | 从 SQLite 恢复 | `shots/anno-09-reload.png` |
| 改正文 → 锚点失效 | 橙色“锚点失效”卡片，保留 quote，可删除 | `shots/anno-11-orphan.png` |
| 还原正文 → 重新锚定 | 高亮和编号恢复 | `shots/anno-12-reanchored.png` |
| 删除标注 | SQLite 1 → 0 | `shots/anno-15-deleted.png` |
| 第二次启动 | 1.08s 退出，进程数保持 1 | `docs/experiment-log.txt` |
| 保存的库路径失效 | 回退到 sample-vault 并重写设置 | `docs/experiment-log.txt` |
| 静默安装 → 启动 → 静默卸载 | 退出码 0，安装目录删除（2026-11-04 构建） | `docs/experiment-log.txt` |
| 侧栏整理：新建 → 重命名 → 拖拽移动 → 删除 → 撤销 | 重做后的 release 构建上 20/20；链接改写与标注跟随正常；键盘/拖拽/暗色见 `docs/SIDEBAR.md` §11 | `tools/verify-sidebar.mjs`、`shots/accept-01-start.png` … `accept-05-final.png` |
| 索引流水线：10k 切库/重建 + 进度 + 崩溃恢复 | 文件级索引替换；旧内容可读；kill 后重启回到旧索引并清理 staging | `docs/INDEXING.md`、`tools/probe-swap-reads.mjs` |

> 标注流程截图（`anno-07.png` … `anno-15-deleted.png`）拍摄于改名前；改名只动了品牌、bundle id、数据目录和可执行文件名，没有改标注代码。

---

## 8. 没验证的 / 已知风险

- **内存超预算**：目标是 150MB，实测私有 258.3–278.1MB（5 次中位 261.2）。Tauri 本体只有约 7MB，其余是 WebView2 的 6 个进程（浏览器/GPU/渲染/utility）。这不是“Tauri 写得重”，是 Windows WebView2 的固定成本；如果必须压到 150MB，只能换方案或关 GPU，不在本次范围。
- **10k 切库/重建 3.4–5.7s**：staging 构建 + 文件替换（122–128ms），后台、有进度，swap 前旧内容可读。list 最长 0.19–0.32s；search 最长 0.17–0.18s（此前 1.28–1.53s 是侧栏渲染，不是 FTS 冷缓存）。不是零阻塞。50k 切库 13.1–15.5s（parse 3.6–3.9s、staging 7.1–7.2s、swap 0.19–0.22s），list 0.92–0.94s、search 20–40ms、侧栏仍只挂 28 行；50k 索引 259MiB（FTS 34MB + notes 230MB）。10k 切回 4 篇 0.23–1.3s；100k 未测。
- **监听与对账**：watcher 只做 450ms 防抖；事件带路径，正常修改只增量处理（4 篇实测 488ms，450ms 防抖 + 38ms）。目录/丢失事件会退回全量 stat；另有 15 分钟一次全量对账兜底。10k 单文件增量未单独测。
- **中文输入法**：resize 后 blur/refocus 是对 WebView2 #5675 的缓解，不是官方修复；没有用真实中文输入法在窗口 resize 后逐项验收。
- **GDI 泄漏**（#5536）：没有做长时间鼠标移动的 GDI 计数；如果 WebView2 156 仍存在，只能监控/重启。
- **长路径**：没有专门验证 >260 字符的路径；安装路径含中文/括号已跑通。
- **高 DPI**：没有在 125%/150% 双屏下拖窗口实测；clamp 覆盖“尺寸爆炸/中心出屏”，没覆盖所有 DPI 组合。
- **代码签名/自动更新**：未做。未签名安装包会被 SmartScreen 拦；安装包只在信任环境用。
- **干净机安装**：安装/卸载在本机（已装 WebView2）通过；没有在无 WebView2 的干净 Windows 上测 bootstrapper 下载。
- **图谱**：顶栏「图谱」是 DOM/SVG 力场，默认最近 40 篇，可切全部（上限 200）；带筛选、搜索、语义缩放和 BFS 找路径。200 篇构建 263ms 是同步阻塞；节点不能拖动，位置不持久化；远景是点阵，放大到 102% 才显示卡片。4 篇样例和 40/80/200 篇构建已在 release 原生窗口验收。
- **06 链式叙事**：07 的全库力场和找路径已落地；06 那种“按主题串讲”的链式视图仍只在 lab。
- **相关性**：局部图只按 front matter 的标签/人物/时期连；`来源/B站` 这类公共标签被排除在“实质邻居”之外，所以没有实质标签的笔记会显示“没有邻居”。这是数据现实，不是渲染 bug。
- **搜索质量**：bigram + FTS5 只测了延迟，没有在真实笔记上标注 query/相关文档算召回/精确率；第 4 节的 78/45 与 97/94 是二手基准，不是本项目实测。
- **视觉验收**：桌面原生窗口的截图是人工检查（文字溢出、重叠、路径、暗色对比）；没有独立 visual-judge——它只支持 pptx/docx/xlsx/pdf/poster/chart，不支持原生窗口/HTML。
- **依赖版本**：`tauri 2.11.5`（离线缓存可用的稳定版），不是 2.12.x；npm api 固定 2.11.1，插件 2.4/2.9/2.7。升级需重新跑构建与验收。
- **基准语料**：10k 篇是同一篇正文的变体，token 分布比真实库重复；首次索引数字可作量级参考，不代表任意真实语料。
- **前端长文渲染**：没有单独测 50k 字笔记的 `marked` + DOMPurify + Vue 渲染耗时；现有样例 1.4k 字。
- **侧栏验收方式**：`tools/verify-sidebar.mjs` 在 release 原生窗口里通过 CDP 派发合成 DOM 事件，不是物理鼠标；落点提示和 8 秒撤销边界未逐项验证。
- **安装验收的构建**：静默安装/卸载是在 2026-11-04 的 5.29 MiB 构建上做的；2026-11-06 的 6.54 MiB 构建没有重新安装。

---

## 9. 参考

- Tauri 2 文档：<https://v2.tauri.app/>
- Tauri 配置（CSP / windows / bundle）：<https://v2.tauri.app/reference/config/>
- single-instance：<https://v2.tauri.app/plugin/single-instance/>
- window-state：<https://v2.tauri.app/plugin/window-state/>
- logging：<https://v2.tauri.app/plugin/logging/>
- updater / 签名：<https://v2.tauri.app/plugin/updater/> · <https://v2.tauri.app/distribute/sign/windows/>
- WebView2 性能最佳实践：<https://learn.microsoft.com/microsoft-edge/webview2/concepts/performance>
- WebView2 IME resize 问题：<https://github.com/MicrosoftEdge/WebView2Feedback/issues/5675>
- WebView2 GDI 泄漏：<https://github.com/MicrosoftEdge/WebView2Feedback/issues/5536>
- window-state 多屏 DPI 修复案例：<https://github.com/farion1231/cc-switch/pull/6056>
- 隐藏窗口仍合成：<https://newreleases.io/project/github/nextai-translator/nextai-translator/release/v0.6.20>
- SQLite FTS5：<https://sqlite.org/fts5.html>
- rusqlite（bundled 自带 FTS5）：<https://docs.rs/rusqlite/latest/rusqlite/>
- Noto Serif SC（OFL）：<https://github.com/notofonts/noto-cjk>
