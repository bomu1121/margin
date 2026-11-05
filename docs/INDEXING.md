# 索引与启动：现状、调研与完整方案

> 这份文档回答三个问题：
> 1. README §10 的「首次索引移到后台 + 进度条」是不是早期规划？
> 2. 它适应了后续的需求变更吗？
> 3. 只加一个后台线程 + 进度条，是不是长期高效的解法？

---

## 1. 结论

**是早期规划，而且没有跟上后续变更。** 它写在 2026-11-04 的桌面化方案里，当时还没有侧栏文件操作、笔记身份迁移、文件监听的全库重扫、侧栏/图谱的 UI 重做。原文只有一句「首次索引移到后台 + 进度条，解决 10k 篇 5s 启动阻塞」，范围只覆盖**启动那一下**。

**只做「后台 + 进度」不是完整解，直接开发会变成短命补丁。** 当前代码的问题不是「索引没放后台」，而是四件事叠在一起：

1. 索引命令是**同步 Tauri 命令**，跑在主线程上；索引期间 WebView 本身会被挡住。
2. 所有数据库操作共用**一个 `Mutex<Connection>`**；索引持锁时，`list_notes`/搜索/标注都要排队。
3. 一次索引是**整库一个事务**；10k 篇时写锁一拿就是 4 秒以上。
4. 每次文件操作、每次监听事件、每次切库都走**全库扫描**；切走一个 10k 库还要逐行删 10k 条。

实测（release，10k 篇、每篇真实样例正文约 1.5k 字，共 81MB；两轮）：

| 操作 | 第 1 轮 | 第 2 轮 |
| --- | --- | --- |
| `set_vault` 索引 10k | **4.30s** | **4.63s** |
| 同时发起的 `list_notes` | **4.49s** | **4.70s** |
| 从 10k 切回 4 篇（删 10k + 建 4） | **5.74s** | **6.33s** |

`list_notes` 的等待几乎等于整个索引时间——这不是 WebView 慢，是它在等那把锁。

**完整方案不是一句「后台化」，而是一条索引流水线**：索引器 actor + 读写连接分离 + 批量事务 + 增量变更检测 + 重建时 staging/原子切换 + 状态/进度 UI + 崩溃恢复。它应该分两阶段做：

- **阶段 A（日常路径）**：索引器 actor、异步命令、独立读连接、200–500 篇/批的批量事务、文件操作与监听走**定向增量**、进度/状态 UI。解决「启动卡、点一下卡、改一个文件全库扫」。
- **阶段 B（重建/切库）**：索引与用户数据分库、staging 索引 + 原子切换、崩溃恢复、首次索引 UX。解决「切库 6 秒」「重建期间不能写」「中途崩溃留半套索引」。

只做 A 会留下 B 的坑；只做 B 没有 A 的日常路径，仍然改一个文件扫全库。**A + B 才是完整解。**

### 1.1 实现结果（2026-11-05 本轮已落地）

**阶段 A 已实现**：`IndexService` 独立 actor 线程持写连接；读仍走 `Store` 的 WAL 连接；启动、监听、文件操作只处理变化路径；索引命令改为 Tauri async；侧栏显示 `done/total` 进度。

**阶段 B 已实现为文件级分库**：`yebian-index.sqlite` 只放 notes/FTS/index_meta，`yebian.sqlite` 放 annotations/settings。staging 建完整索引文件，完成后关闭 Store 读连接和 actor 写连接，rename 替换索引文件，再重新打开并 attach 用户库。swap 不再复制 10k 行；读连接只停一个文件替换的窗口。另有 15 分钟全量对账、崩溃后清理 staging/恢复 `.old`。

release 实测（10k 篇、真实样例正文 81MB；两轮以上）：

| 操作 | 重做前 | 重做后 |
| --- | --- | --- |
| 4→10k 切库/重建 | 4.30–4.63s，**读命令等 4.49–4.70s** | **3.4–5.7s**（后台、进度条；文件 swap 122–128ms） |
| 10k→4 切回 | 5.74–6.33s | **0.23–1.3s** |
| staging 构建期间 list/search | 被整库锁住 | list p95 5–183ms、search p95 3–118ms（随扫描阶段波动） |
| swap 前后 list / search 最长 | 同 4.49–4.70s | list 0.19–0.32s；search 0.17–0.18s（虚拟滚动后） |
| 单文件外部修改（4 篇） | 全库扫 | 488ms（450ms 防抖 + 38ms） |
| 索引库/用户库 | 同一个 178MB 文件 | index 73KB + user 209KB（4 篇样例） |
| 10k 侧栏实际挂载行数 | 10,001 行 | **27–38 行**（视口 780px + 上下各 480px 预渲染） |
| 验收 | — | 25 个 Rust 测试、20/20 侧栏回归、kill 中断后恢复 |

截图：`shots/index-progress.png`（10k 切库时 9,801/10,000，旧笔记仍可读）。

### 1.2 首次搜索的 1.2s 不是 FTS，是侧栏渲染（2026-11-05）

上一轮把「切库后首次搜索最长 1.28–1.53s」记成「新 FTS 冷缓存」。这一轮先加临时日志（命令内 `Instant`，超过 20ms 才 warn），再跑同一探针：

| 证据 | 数值 |
| --- | --- |
| 探针 search 最长 | 1,156 / 1,204ms |
| 后端 `search_notes` 最长 | 145ms（仅一次超 20ms） |
| 后端 `list_notes` 最长 | 253ms（多条 74–244ms） |
| 10k 树实际挂载 | 10,001 行；切库后主线程要一次性建 1 万个节点 |

命令内耗时解释不了探针的 1.2s，差异在命令之外的 WebView 主线程。改成虚拟滚动后，同一探针的 search 最长 **174–184ms**（各轮 p50 2–11ms、p95 3–118ms），10,001 行的树只挂 **27–38 行**；滚动到底、键盘 `End` 都能落到最后一条。**结论：之前那句「FTS 冷缓存」是错的，冷的是 UI，不是索引。**

**还差的**：list 仍是每次返回全部笔记的 `NoteMeta`（10k 0.19–0.32s、50k 0.92–0.94s，Rust 序列化 + IPC）；100k 未测；没有后台增量 FTS merge；10k 单文件 watcher 未单独测；旧用户库里的 notes/fts 表还留着但不再读取。虚拟滚动测了 10k/50k 扁平库，层级库/筛选/拖拽跨窗口没有逐项压测。

### 1.3 50k 规模：搜索不是瓶颈，FTS 体积已压到 34MB（2026-11-05）

50k 语料仍由 `tools/make-body-bench-vault.mjs` 生成（同一篇 4,540B 正文复制 50,000 份；实际字节 227MB，磁盘占用 402MB，因为 50k 个小文件按簇分配）。用 `tools/probe-scale.mjs` 只轮询小的 `get_index_status`，完成后再发一次 list 和五次 search，避免用 5 万条返回值的读循环干扰索引。

| 阶段/指标 | 实测 |
| --- | --- |
| set_vault 总耗时 | **13.1–15.5s**（contentless；4 轮） |
| parse | 3.6–3.9s |
| staging 写入（含 optimize） | 7.1–7.2s；其中 FTS optimize 0.47–0.49s |
| swap / post scan | 0.19–0.22s / 1.2s |
| list_notes（50k 条） | 918–939ms |
| search（5 次） | 20–40ms（p50 22ms） |
| 侧栏挂载 | 28 / 50,001 行；scrollHeight 2,400,008px；行高 48px |
| 切回 4 篇 | 733–740ms |
| 加载 50k 后私有内存 | 切换完成瞬间 631MB；空闲 8s 后 358.7MB（7 进程；4 篇空闲 261.2MB） |
| 索引文件 | **271,331,328 B（259MiB）**：FTS 34MB + notes 表 230MB；1.2× 源文件实际字节 |

**决策**：
1. **搜索不需要分页**：50k 五次查询 20–40ms，FTS 本身不是瓶颈。
2. **列表暂不分页**：侧栏树需要全部笔记的层级/计数，首屏一次 0.92–0.94s 可接受；如果做到 100k，优先改成按文件夹懒加载或压缩 IPC，而不是给搜索结果加分页。
3. **FTS 改为 contentless + detail=none**：contentful `detail=none` 只能到 647MB（−22%）；contentless 去掉 410MB 的 token 正文存储后，FTS 只剩 **34MB**，整个 50k 索引 259MiB。代价是不能做 phrase 查询（当前搜索只做 token AND/OR，没用到）。
4. **迁移走正常重建**：`index_meta.fts_schema` 不匹配时，启动后台用 staging 重建；旧索引在 swap 前仍可读。4 篇样例的升级重建 <1s；50k 旧索引会付出一次 13–15s。
5. **进度必须覆盖写入阶段**：staging 每 500 条发 `writing` 状态，optimize/swap 发 `committing`，侧栏显示“正在写入索引… n / 50,000”。

50k 单文件更新/还原也实测过：给 `note-0.md` 追加“紫电青霜”，2s 后搜索命中 1 条；覆盖回原文后搜索 0 条。

**边界**：50k 语料是同一篇正文复制，FTS 词典/压缩率与真实多样语料不同；100k 未测；contentless 不支持 phrase 查询，未来若要加短语搜索需要换方案；升级旧索引的一次性重建成本没有在 50k 旧库上单独计时（只验证了 4 篇启动迁移）；50k 的 13–15s 仍是后台等待，不是零成本。

---

## 2. 早期规划与需求变化

### 2.1 原文

README §10：

> 2. 首次索引移到后台 + 进度条，解决 10k 篇 5s 启动阻塞。

DESKTOP.md §8：

> **10k 篇首次索引 4.4–5.4s**：首次建库或点“重建索引”会阻塞启动；日常启动/保存走增量（0.59–0.72s）。如果库更大，需要把首次索引移到后台线程并分页出列表。

这两句的假设是：**只有启动会阻塞**。后续的实现已经把这个假设推翻了。

### 2.2 之后新增的需求

| 变更 | 对索引的含义 |
| --- | --- |
| 侧栏新建/重命名/移动/删除（`feat(note)`） | 每个写操作后都调用一次 `index_vault`；10k 时每次 0.72–0.80s |
| 笔记身份：front matter `id` + 正文指纹回填 | 索引不只是「重建缓存」，还在做 id 迁移、重复 id 处理、标注重挂 |
| 文件监听 | 任何 `.md` 变化 → 450ms 防抖 → **全库** `index_vault(false)` |
| 中文 FTS5 + bigram | 每篇要生成 token、写 FTS；删除要按 rowid 删 FTS 行 |
| 图谱 / 侧栏 / 标注 | 读路径也走同一个 Store；索引持锁时它们全等 |
| 侧栏 UI 重做 | 列表/树/筛选都依赖 `list_notes` 的即时返回 |

结论：**早期计划的「只处理启动」已经不成立**。真正的需求是「全部写路径都不阻塞、日常只处理变化、重建可恢复」。

### 2.3 当前有 9 个全库索引入口

`src-tauri/src/lib.rs` / `ops.rs`：

1. 启动 `setup`：`store.index_vault(path, false)`（`lib.rs:313`）
2. `set_vault`：`index_vault(&path, true)`（`lib.rs:35`）
3. `reindex`：`index_vault(&vault, true)`（`lib.rs:55`）
4. `create_note`：`index_vault(&root, false)`（`lib.rs:126`）
5. `rename_entry`：`index_vault(&root, false)`（`lib.rs:146`）
6. `move_entry`：`index_vault(&root, false)`（`lib.rs:160`）
7. `delete_entry`：`index_vault(&root, false)`（`lib.rs:170`）
8. `restore_entry`：`index_vault(&root, false)`（`lib.rs:184`）
9. watcher 线程：`index_vault(&vault, false)`（`lib.rs:239`）

所以「后台化启动索引」最多解决 8 个入口里的 1 个。

---

## 3. 当前实现

### 3.1 存储层

`src-tauri/src/db.rs`：

- `Store { conn: Mutex<Connection> }`（`db.rs:72`）——一个连接、一个互斥锁，读写都排队。
- `index_vault` 先 `walk_notes` 全库扫描，再 `conn.lock()`，然后 `conn.transaction()`；**整个索引在一个事务里**（`db.rs:165–169`）。
- 事务里做：读旧 `rel_path/mtime`、正文指纹配对、id 迁移、逐篇 `upsert_note`、逐行删除消失的笔记（`db.rs:210–370`）。
- FTS 是普通 FTS5 表（`id UNINDEXED, title, tokens`），`upsert_note` 里 `INSERT INTO notes_fts(rowid, ...)`；删除按 rowid（`db.rs:692+`）。
- 变更检测只看 `mtime`（毫秒），没有 size、没有内容哈希；只有「外部移动」才用正文哈希配对。

### 3.2 命令层

- `index_vault` 被同步 `#[tauri::command]` 调用。Tauri 文档明确：**没有 `async` 的命令跑在主线程**；同步命令里的重活会冻结 UI。
- 读命令（`list_notes`/`search_notes`/`get_note`/`list_annotations`）和写命令共用同一个 `Store`，所以索引一持锁，读也停。

### 3.3 前端

- `loadLibrary()` 并行请求 `getVault`/`listNotes`/`getStats`；`listNotes` 等锁，整个首屏就等。
- 没有 `indexStatus`/`progress` 状态；`reindexLibrary`/`chooseVault` 只是把 `loading` 置 true，然后 await 整个 IPC。
- 侧栏的文件夹树来自 `list_folders`（文件系统，不走 DB），笔记列表来自 `list_notes`（走 DB）；两者在索引期间会不同步。

### 3.4 监听

`notify` 回调只把「有 `.md` 变化」发到一个 channel；线程 450ms 防抖后调用**全库** `index_vault(false)`。它没有把具体路径带下去，也没有区分 rename/create/modify；`notify` 本身不做防抖，平台事件还可能丢（官方文档自己写了）。

---

## 4. 实测

### 4.1 方法

- release 构建，WebView2 156，1456×939。
- `tools/probe-index-lock.mjs`：在页面里同时发起 `set_vault(10k)` 和 `list_notes()`，记录两者相对开始的时间；完成后 `set_vault(4 篇)`，再记录切回耗时。
- 语料：`tools/make-body-bench-vault.mjs` 用真实样例 `shebao.md`（约 1.5k 字正文）复制 10,000 份，共 81MB。**不是** `make-bench-vault` 的短正文语料。
- 两轮，同一台机器；没有重复到统计显著，只作为量级和阻塞关系的证据。

### 4.2 结果

| 轮次 | set_vault 10k | 并发 list_notes | 切回 4 篇 |
| --- | --- | --- | --- |
| 1 | 4,433ms（stats 4,300ms） | 4,493ms | 5,753ms |
| 2 | 4,631ms（stats 4,468ms） | 4,704ms | 6,330ms |

短正文 10k（`make-bench-vault`）的同一探针：set_vault 1,309ms，list_notes 1,350ms，切回 1,125ms。**正文大小会显著影响索引时间**；README 的 4.1–4.2s 对应 1.5k 字正文，是对的口径。

### 4.3 这些数字意味着什么

- 「后台 + 进度」如果只是把 `index_vault` 挪进一个线程，**`list_notes` 仍然会等 4.5s**，因为锁还在；用户看到进度条，但点笔记、搜索、标注仍然卡。
- 如果为了不卡读，改成「每篇一个事务、放锁」，读能动了，但 10k 次提交会把 FTS 段切得很碎，总时间可能更长（SQLite/FTS5 的批量提交经验是「不要逐行提交，也不要一个巨大事务」）。
- 切库 5.7–6.3s 不是「索引慢」，是「删 10k 行 + 重建 4 行」；后台化切库只把卡顿藏起来，操作本身还是 6 秒。
- 这些操作都没有进度、取消、失败恢复；用户只能等。

**测量局限**：并发 IPC 不等于真实点击；`list_notes` 的等待可能包含 IPC 排队；切回时间包含删 10k + 建 4，没拆开；只测了一台机器、两轮；当时没有 50k/100k（50k 后补测见 §1.3）。

---

## 5. 外部调研

### 5.1 SQLite WAL

[SQLite WAL 文档](https://www.sqlite.org/wal.html)：

- WAL 下**读不阻塞写、写不阻塞读**；但**同一时刻只有一个 writer**。
- 读事务看到开始时的快照（end mark），不会看到未提交/之后的提交。
- 长写事务会让 WAL 变大，checkpoint 无法在写事务中间重置 WAL；官方明确「WAL 适合小事务，不适合超大事务」。

**对本项目的含义**：数据库本身可以做到「索引写、界面读」并行；是 `Mutex<Connection>` 把这条路堵死了。把读连接和写连接分开，读就能在索引期间看旧快照——这是比「后台线程」更关键的一步。

### 5.2 FTS5

[FTS5 文档](https://www.sqlite.org/fts5.html)：

- 删除默认是「加 delete-key」，真正的段合并发生在 `merge`/`optimize`/`automerge`/`crisismerge`；大量删除后需要合并才能回收。
- `optimize` 会重写整个 FTS 索引，**可能很慢**；大索引应改用 `merge` 的增量方式（先 `-N` 再 `N`，直到变化量 <2）。
- `automerge` 默认 4，`crisismerge` 默认 16；每次 insert/update/delete 都可能触发合并，**单次写可能突然变慢**。
- `content=` 外部内容表可以避免重复存正文，但必须自己维护一致性；`contentless_delete=1` 支持 DELETE/UPDATE，删除用 tombstone，需要 `deletemerge`。

**对本项目的含义**：当前 FTS 是普通表，按 rowid 删除是对的；但 10k 行逐行删 + 合并策略可能解释切库 5.7s。批量大小、`automerge`/`crisismerge`、定期 `merge` 都要调，不能只改线程。

### 5.3 Tauri 线程与进度

[Tauri calling Rust](https://v2.tauri.app/develop/calling-rust/)：

- 「没有 `async` 关键字的命令在主线程执行」；重活应使用 async 命令/`spawn_blocking`。
- 前端 `await invoke` 不会让同步命令离开主线程。

[Tauri calling Frontend](https://v2.tauri.app/develop/calling-frontend/)：

- 进度用 `tauri::ipc::Channel<T>` 更合适：**有序、高吞吐**；事件系统是 JSON、低吞吐、异步监听可能乱序。
- 长任务应 clone `AppHandle` 后台跑，提供取消。

**含义**：索引器要独立于主线程，进度用 Channel（或节流事件）；取消/状态要进 managed state。

### 5.4 notify

[notify 文档](https://docs.rs/notify/latest/notify/)：

- 核心库**不做防抖**，建议用 `notify-debouncer-mini/full`。
- rename 事件在不同平台/编辑器差异很大；编辑器可能 truncate 或 replace；大目录可能丢事件；网络盘可能完全不发事件。
- 官方建议对不可靠场景用 PollWatcher 兜底。

**含义**：watcher 事件只能当「提示」，不能当真相；必须有启动/定期全量对账。当前 450ms 防抖后全库扫描，正好相反：事件只给了「有变化」，却做了全量。

### 5.5 同类产品

- **Obsidian**：MetadataCache 按文件缓存；用 `stat.mtime` 做增量；rename 用 seed-and-move，不当 delete+create；批量事件合并；分批处理并在批次间让出 UI；长重建会持久化进度。核心原则是「**永远不要每次变化全库重扫**」。
- **Recoll / Xapian**：默认增量，只处理变化的文档；首次/`-z` 才全量；可实时（inotify）或定时；多线程抽取/索引。
- **Joplin**：SQLite FTS4（新版本也支持 FTS5）实时索引；CJK 因为分词问题走非 FTS 回退；搜索层有独立的 query builder，不是把 UI 直接绑在 FTS 上。

**含义**：这些产品都把索引当成「可重建的缓存 + 增量维护」，而不是「每次全量重建」。

### 5.6 增量变更检测

多份实践（rsync/同步工具、桌面索引器、Obsidian 插件）收敛到同一套：

1. 先比 **size + mtime_ns + 文件身份**（Windows file id / dev+ino）；全同就跳过。
2. 只有元数据变了才读内容、算 **content hash**；hash 才是「内容真变了」的裁判。
3. watcher 事件只做低延迟触发；**启动 + 定期全量对账**保证正确性（事件会丢、会漏、关机期间会变）。
4. rename 用文件身份/旧记录 seed-and-move，而不是 delete+create。
5. 原子保存（临时文件 + rename）要判断「DELETE 后文件还在」的情况，避免误删索引。

**含义**：当前只比 `mtime` 毫秒，既可能漏（同 mtime 内容变化），也没有 size/identity/hash；watcher 不带路径。需要升级。

### 5.7 原子重建

三种成熟做法：

| 做法 | 机制 | 代价 |
| --- | --- | --- |
| 影子表 + `ALTER TABLE RENAME` | 新表写满，事务内改名 | 简单；schema 对象要一起处理 |
| 文件级 staging + `os.replace` | 新索引写另一个 DB 文件，完成后原子替换 | 需要关连接；**要求索引和用户数据分库** |
| 版本标记 + 双写 | 新表 + `index_meta.version`，重建期间双写 | 最复杂；但不用锁表 |

**含义**：当前是「原地一个事务重建」，没有 staging/版本；切库/重建期间只能整库不可用。要把索引变成可替换的缓存，最干净的是**索引 DB 与标注/设置 DB 分开**，然后文件级 staging + `os.replace`。

---

## 6. 方案对比

| 方案 | 范围 | 能解决 | 不能解决 |
| --- | --- | --- | --- |
| **A. 后台线程 + 进度条**（原一句话） | 启动索引 | 启动时不再纯白屏 | 读仍等锁；文件操作/监听/切库仍全库扫；无取消/恢复 |
| **B. 索引流水线**（actor + 读连接 + 批量 + 定向增量 + 进度） | 日常路径 | 启动、单文件变化、文件操作、搜索/标注不再长时间等锁 | 重建/切库仍可能阻塞；崩溃恢复不完整 |
| **C. B + 索引分库 + staging/原子切换 + 崩溃恢复** | 全生命周期 | 10k/50k 的启动、增量、重建、切库、崩溃 | 100k 级还需要 UI 虚拟化/搜索分页（可另做） |

**推荐 C**。但如果一次做完风险太大，按 **B → C** 两阶段交付，每阶段都有独立验收；不能把 B 当成最终方案，因为切库 6 秒和「重建期间不能写」仍然在。

---

## 7. 完整设计

### 7.1 组件

```
┌──────────── UI (Vue) ────────────┐
│  list/search/graph/read commands │
└──────────────┬───────────────────┘
               │ invoke (async, no shared write mutex)
┌──────────────▼───────────────┐        ┌──────────────────────┐
│ ReadPool (WAL read conns)    │───────▶│ live index DB         │
│  - list_notes / search / get │        │  notes + FTS + meta   │
└──────────────────────────────┘        └──────────▲───────────┘
                                                   │ batches (200-500)
┌──────────────┐   paths    ┌──────────────┐      │
│ notify       │───────────▶│ Debouncer    │──────┘
│ (hints)      │            │ + queue      │
└──────────────┘            └──────┬───────┘
                                   │ jobs
                          ┌────────▼─────────┐
                          │ IndexActor       │
                          │ single writer    │
                          │ cancel + progress│
                          └────────┬─────────┘
                                   │ Channel<IndexEvent>
                              UI status store
┌──────────────────────┐
│ user DB (annotations │  ← 不参与索引重建
│ + settings)          │
└──────────────────────┘
```

### 7.2 数据模型

**分库**（推荐）：

- `yebian-user.sqlite`：`annotations`、`settings`（用户数据，小，永远不重建）
- `yebian-index.sqlite`：`notes`、`notes_fts`、`index_meta`（可丢弃、可替换的缓存）

迁移：旧的 `yebian.sqlite` 继续作为 annotations/settings 库；新建 `yebian-index.sqlite` 并在后台从笔记库重建索引。旧库里的 notes/FTS 表保留但不读取；首次启动会短暂显示进度而不是旧索引。

**`notes` 增加**：

- `mtime_ns INTEGER`（纳秒；现在只有毫秒）
- `size INTEGER`
- `content_hash TEXT`（BLAKE3/SHA-256；只在元数据变化时算）
- `file_id TEXT`（Windows file id / dev+ino；用于 rename 配对）

**`index_meta`**：

- `root`、`generation`、`state`（idle/scanning/indexing/committing/error/cancelled）、`started_at`、`completed_at`、`last_reconcile`、`schema_version`

**FTS**：

- 保留 rowid 映射；删除按 rowid。
- 评估 `content=` 外部内容表（把 `tokens` 放回 `notes` 列）或 `contentless_delete=1`；避免正文/token 双份存储。
- 设置 `automerge`/`crisismerge`/`deletemerge`；批量后按 `merge` 的 `-N`/`N` 方式增量整理，避免一次 `optimize` 卡住。

### 7.3 任务与状态机

- `IndexActor` 是**唯一写索引的线程**；UI 的写命令不直接碰索引 DB，而是发 job。
- Job 类型：`StartupDelta`、`WatchDelta(paths)`、`FileOp(note|folder)`、`FullRebuild`、`VaultSwitch(path)`。
- 状态：`idle → scanning → indexing → committing → done/error/cancelled`；同一时间只有一个 job。
- 取消：`CancellationToken`/`AtomicBool`；切库/重建时取消旧 job，旧 staging 丢弃。
- 进度：`Channel<IndexEvent>`，字段 `{ state, done, total, current, generation }`；最多 10 次/秒。
- 崩溃：每批一个事务；`index_meta.state` 标记进行中；启动时若发现 `state=indexing` 且 generation 未提交，丢弃 staging/标记 dirty，做 delta 或全量。

### 7.4 启动流程

1. 打开 user DB 和 index DB；读 `index_meta`。
2. 如果有上一次完整索引：**立即** `list_notes` 返回旧快照，UI 先可用。
3. 发 `StartupDelta` job：对账 mtime/size/hash，只处理变化。
4. 如果没有索引（首次）或 schema 变化：发 `FullRebuild`；UI 显示「正在整理 x/y」，文件夹树来自 `list_folders`（文件系统），笔记行可以先显示文件名/骨架，索引完的逐步出现。
5. 不阻塞 `app_ready`；首屏可交互时间与索引时间解耦。

### 7.5 增量与文件操作

- 文件操作（create/rename/move/delete/restore）**不再调用全库 `index_vault`**：
  - create：只解析新文件，插入 notes/FTS。
  - rename/move：更新 `rel_path`/`path`，保持 id，必要时迁移 annotations（id 不变则不用）。
  - delete：删除对应 row + FTS row。
  - 链接改写仍走现有 `links` 逻辑，不依赖全库索引。
- watcher：debouncer 带上**具体路径**；按路径做 delta；rename 事件用 file_id/旧记录 seed-and-move；原子保存用「DELETE 后文件仍存在则跳过」保护。
- 启动 + 每 N 分钟/窗口重新聚焦时做一次**对账**（全量 stat，只有元数据变化才算 hash）；事件丢失/关机期间变化靠它兜底。
- 变更检测：`mtime_ns + size + file_id` 全同 → 跳过；否则读内容算 hash；hash 同 → 只更新元数据；hash 不同 → 重新解析/写 FTS。

### 7.6 批量与锁

- 每批 200–500 篇或 50–100ms（以先到为准）提交一次；读连接在 WAL 下看上一批的快照。
- 不逐篇提交（FTS 段碎、慢）；不整库一个事务（WAL 大、锁久）。
- 写连接只在批次内持锁；读连接独立，不共享 `Mutex<Connection>`。
- 每批后按需 `wal_checkpoint(PASSIVE)`；避免长读事务饿死 checkpoint。
- FTS 的 `automerge`/`crisismerge` 调优；批量后做增量 `merge`，不在用户操作中间做 `optimize`。

### 7.7 重建与切库

- 新建 `staging` 索引 DB（同一目录、不同文件名）；构建；构建期间 watcher 事件进入队列。
- 构建完成后做一次 catch-up（处理构建期间的变化），然后：
  - `os.replace(staging, live)` 原子替换（Windows 同卷可用；索引与用户数据已分库）。
  - 或影子表 + `ALTER TABLE RENAME`（如果不想分库）。
- 旧 live 索引在替换前一直服务读请求；替换后读连接重开/切 generation。
- 切库：取消当前 job；旧索引继续服务（或 UI 显示 loading）；新库 staging 完成后替换；不逐行删 10k。
- 崩溃：staging 不完整 → 启动时删除；live 不受影响。

### 7.8 一致性/失败场景

| 场景 | 行为 |
| --- | --- |
| 索引中用户搜索 | 读旧快照；UI 标「索引中，结果可能不完整」；不阻塞 |
| 索引中用户标注 | 写 user DB，不受索引影响；标注按 note_id 仍有效 |
| 索引中文件被改 | watcher 入队；当前批结束后处理；最后 catch-up |
| 索引中切库 | 取消 job；staging 丢弃；旧索引保留；新库重新开始 |
| 索引中崩溃 | 未提交批次回滚；`index_meta` 标 dirty；下次启动 delta/全量 |
| 索引中磁盘满 | job error；旧索引可用；UI 提示；不破坏 live |
| 外部 rename | file_id/正文哈希配对；唯一才迁移 id；否则当删除+新增 |
| 重复 id | 现有 `used_ids` 逻辑保留；新 id 写回 front matter |

### 7.9 UI 契约

`indexStatus` store：`{ state, done, total, current, generation, error }`。

- 首屏：有旧索引 → 直接显示；无索引 → 文件夹树 + 骨架/文件名 + 进度。
- 搜索：索引中标注「索引中，结果可能不完整」；或只搜已提交批次（明确标注）。
- 图谱：索引完成前不构建全库图，或只用已索引节点并标注。
- 文件操作：乐观更新 + 失败回滚；不等全量索引。
- 进度：Channel 推送，最多 10 次/秒；完成后刷新列表/图谱。

### 7.10 规模目标（目标，不是实测）

| 指标 | 10k（1.5k 字/篇） | 50k | 100k |
| --- | --- | --- | --- |
| 有旧索引时首屏可交互 | < 1s | < 1s | < 1.5s |
| 单文件变更 → 索引可见 | < 100ms | < 100ms | < 150ms |
| 文件操作命令 | < 100ms | < 100ms | < 150ms |
| 索引期间搜索 p95 | < 50ms | < 50ms | < 80ms |
| 首次/重建 | 4–5s，后台，可取消 | < 30s，后台 | < 60s，后台 |
| 切库（同规模） | < 2s（staging 预构建） | < 5s | < 10s |
| 写锁单次持有 | < 100ms | < 100ms | < 150ms |

这些是验收目标，**不是已经实现的数字**。100k 还需要列表懒加载（侧栏虚拟化已在 10k/50k 落地，FTS 体积已在 P3-1 压到 259MiB），属于阶段 C 之后。

---

## 8. 验收与测试计划

1. **夹具**：1k/10k/50k/100k；短正文与 1.5k 字正文两套；嵌套目录与扁平目录。
2. **启动**：有旧索引/无索引；冷缓存/热缓存；测量可交互时间、首次列表、索引完成时间。
3. **增量**：单文件改内容/改 mtime 不改内容/rename/移动目录/原子保存/删除后重建；每条断言 DB 行数、id、annotations 不变。
4. **并发**：索引期间 `list_notes`/`search`/`add_annotation`/`get_note` 的 p50/p95；写锁最大持有时间；WAL 大小。
5. **重建/切库**：切 10k→4、4→10k、10k→10k；旧索引可用性；取消；崩溃（kill -9）后重启。
6. **watcher**：批量移动 1k 文件、编辑器原子保存、事件丢失（停 watcher 改文件再开）、网络盘（可选 PollWatcher）。
7. **FTS**：批量删除/重建后的索引大小、搜索 p95、`integrity-check`。
8. **UI**：10k/50k 树渲染、筛选、滚动；索引中点击/搜索/标注；暗色/明色。
9. **失败**：磁盘满、只读目录、权限、路径含中文/长路径。

---

## 9. 分阶段

### 阶段 0：测量与夹具（本轮已部分完成）

- `tools/make-body-bench-vault.mjs`
- `tools/probe-index-lock.mjs`
- 10k 两轮基线已记录。

### 阶段 A：日常路径（已落地）

- `IndexActor` + 异步命令 + 独立读连接。
- 批量事务（200–500 篇/50–100ms）。
- 文件操作改定向更新；watcher 带路径增量。
- `indexStatus` + `Channel<IndexEvent>` 进度。
- 启动 delta；有旧索引时先可用。
- 验收：10k 下文件操作 <100ms、索引期间搜索 <50ms、启动可交互 <1s、写锁 <100ms。

### 阶段 B：重建/切库（已落地为文件级分库 + staging 文件替换）

- 索引/用户数据分库。
- staging 索引 + `os.replace`/影子表原子切换。
- 取消、崩溃恢复、首次索引 UX。
- 验收：10k 切库 <2s、重建期间读可用、kill 后重启不坏。

### 阶段 C（仅当目标 ≥100k）

- 100k 夹具与验收。
- 列表按文件夹懒加载或压缩 IPC；搜索分页暂不需要（50k 20–40ms）。

---

## 10. 诚实清单

- **10k 与 50k 已实测，100k 仍是目标**。语料是同一篇真实样例正文复制，不是真实分布；50k 索引 834MiB 的旧结论已被 contentless 的 259MiB 取代；语料重复性仍影响压缩率。
- 10k 短正文与 1.5k 字正文的索引时间差 3 倍（1.3s vs 4.5s）；README 的 4.1–4.2s 对应后者。
- 探针是并发 IPC，不是真实点击；`list_notes` 等待可能包含 IPC 排队；切回时间没有拆成「删 10k」和「建 4」。
- 切库 5.7–6.3s 的根因没有 profile：可能是逐行删、FTS 合并、WAL checkpoint 或事务提交的组合；设计里按「批量 + rowid + merge 调优」处理，但没有逐项归因。
- 外部资料以官方文档、项目文档和实践文章为主；没有在相同语料上复现 Obsidian/Recoll/Joplin 的数字，只借鉴架构。
- A/B 已实现并做了 10k/50k 实测、25 个 Rust 测试、20/20 回归和一次 kill 中断恢复；100k、磁盘满、`.old` 杀软冲突仍未测；50k 索引已降到 259MiB（contentless），notes 表 230MB 是剩余大头。
- 分库迁移、staging 原子替换、Windows `os.replace` 的锁/杀软干扰，都需要单独验证。
- 本轮的侧栏 UI 重做没有改变索引性能；不要把手感变化和索引数字混在一起。

---

## 11. 建议

> 2026-11-05 更新：A/B 已按这条路线落地；B 采用用户库/索引库拆分 + 文件级替换。剩余项见 §1.1 与 §10。

1. **不要按「后台 + 进度」这一句直接开发**。它是早期计划的残留，只覆盖启动，且会被当前的单 Mutex/单事务/全库扫描吃掉。
2. **按 A → B 两阶段做完整方案**。A 解决日常路径，B 已按文件级分库落地；每阶段用 10k 夹具验收。
3. **先定规模目标**。如果目标只是「几千篇以内」，A 就够；如果要 10k+ 且长期用，B 的分库/staging 是必要的；如果 100k，再排 C 的验收与索引体积。
4. **验收看四个数，不只看启动**：单文件变更耗时、文件操作命令耗时、索引期间搜索 p95、写锁最大持有时间。只看「启动从 5s 变 1s」会把问题从启动挪到搜索/切库。
