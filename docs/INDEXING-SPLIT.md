# 文件级索引切换：施工方案

> 这是 `docs/INDEXING.md` 阶段 B 的施工版。目标不是再加一层缓存，而是把
> **notes + FTS** 从用户数据库里搬出去，让重建/切库变成一次文件替换，而不是
> 一次持续数秒的写事务。

## 1. 目标与验收

| 项 | 目标 |
| --- | --- |
| 10k 切库/重建 | staging 构建后文件切换 **< 200ms**；期间 list/search 不被 swap 阻塞 |
| 10k 切回 4 篇 | 同上 |
| 旧实现残留 | swap 瞬间 list 最长 1.12s、search 最长 1.54s（2026-11-05 实测） |
| 回归 | 24 个 Rust 测试、20/20 侧栏验收、kill 恢复 |
| 规模 | 先验 10k；50k 作为下一轮，不在本文件承诺 |

### 实现结果（2026-11-05）

- `Store` 拆成 `yebian.sqlite`（annotations/settings）与 `yebian-index.sqlite`（notes/FTS/meta）；读连接在 index 库上 `ATTACH user`。
- `IndexService` 建 staging 索引文件；更换时关闭 writer 和 Store 读连接，`rename` 替换，再重开并 attach；`.old` 备份用于回滚，启动时恢复。
- 10k 实测：切库 **3.4–5.7s**，文件 swap **122–128ms**；切回 4 篇 **0.23–1.3s**。
- 读：staging 期间 list p95 5–183ms、search p95 3–118ms；swap 前后 list 最长 0.19–0.32s、search 最长 0.17–0.18s。此前记的 search 1.28–1.53s 是 10k 侧栏 DOM 渲染，虚拟滚动后消失（见 INDEXING.md §1.2）。
- 索引库 106KB、用户库 209KB（4 篇样例）；旧单库的 notes/fts 表保留但不读取。
- 24 个 Rust 测试、20/20 侧栏回归、kill 在 staging 期间后重启回到旧索引并清理 staging。

**没达到的**：list 仍返回全部 1 万条 `NoteMeta`（0.19–0.32s）；50k/100k 未测；`.old` 的杀软/占用冲突没有真机压测。

## 2. 数据布局

```
%APPDATA%/com.yebian.notes/
├── yebian.sqlite          # 用户数据：annotations、settings
├── yebian-index.sqlite    # 派生索引：notes、notes_fts、index_meta
├── yebian-index.old       # 切换备份，成功后删除
└── yebian.staging.sqlite  # 构建中的新索引，崩溃后丢弃
```

**为什么拆**：用户数据（标注/设置）永远不能被重建影响；索引是可丢弃缓存。
拆分后 `os.replace`/rename 交换索引文件时，标注数据库完全不动。

## 3. Store 的读路径

- `Store` 持两个连接：
  - `user: Mutex<Connection>`：annotations/settings 读写。
  - `index: Mutex<Option<Connection>>`：索引库只读；主库是 `yebian-index.sqlite`，
    `ATTACH` 用户库为 `user`，这样 `NOTE_SELECT` 仍能在一条 SQL 里取
    `annotation_count`。
- 读方法（list/search/get/notes_containing/stats 的 notes 部分）走 `index`。
- `index` 是 `Option`：swap 时 `take()` 关闭连接，替换文件后重新 open + attach；
  读命令在锁上短暂等待（毫秒级），不会等一个写事务。
- 旧 `Store::index_vault` 删除；索引写只由 `IndexService` actor 做。

## 4. 文件切换协议

1. `IndexService` 在 `yebian.staging.sqlite` 构建 notes+FTS+meta（journal OFF）。
2. actor 关闭自己的 writer 连接。
3. `Store::swap_index(staging)`：
   - 锁 `index`，`DETACH user`，`take()` 并关闭旧连接；
   - 若 `yebian-index.old` 存在，先清理；
   - `rename(live, old)`；`rename(staging, live)`；
   - 失败则把 `old` 还原成 `live`；
   - 重新打开 live、`ATTACH user`，替换 `index`；
   - 删除 `old`。
4. actor 重新打开 writer；跑一次 post-swap stat 对账。
5. 任一步失败：旧 `live` 还原，staging 丢弃，状态 error；读者仍用旧连接或重开。

## 5. 崩溃状态

| 崩溃点 | 启动处理 |
| --- | --- |
| staging 构建中 | 删除 staging；live 不变 |
| live 已改名为 old，staging 未替换 | `live` 不存在、`old` 存在 → `old` 还原 |
| staging 已替换 live，old 未删 | 使用 live；删除 old |
| swap 后 writer 未重开 | 打开 live；启动 delta |
| 读连接未重开 | `Store::open` 重新打开 index + attach user |

启动时 `Store::open` 先做上述恢复，再 `IndexService` 清理 staging。

## 6. 迁移

- 现有单库 `yebian.sqlite` 的 annotations/settings 原样保留。
- 新 `yebian-index.sqlite` 首次创建为空；`IndexService` 的启动 delta 从 vault
  重建（10k 后台，有进度）。旧库里的 notes/fts 表成为遗留，不参与读取；
  迁移完成并确认后可在后续版本删除/压缩。
- `settings.vault_path` 不变；旧索引不可读时用户会短暂看到空列表 + 进度条，
  而不是一个半旧半新的索引。

## 7. 测试

1. Store：迁移后 annotations/settings 仍可读；旧 notes 表不参与。
2. Swap：staging 替换后 notes/FTS 来自新文件；旧文件备份删除；失败回滚。
3. 崩溃：在 rename 前后各模拟一次（保留 old/staging），启动恢复。
4. 读：swap 期间 list/search 不报错；最大等待目标 <200ms。
5. 业务：20/20 验收；24 个 Rust 测试；重建保 id/标注；外部 rename 保 id。
6. 规模：10k body-sized 库切进/切回；记录 list/search 最长等待；50k 留待下一轮。

## 8. 诚实边界

- 文件替换在 Windows 上要求没有打开句柄；Store 读连接和 actor writer 必须先关闭。
  杀软/备份软件仍可能短暂占用文件，`rename` 失败时按回滚路径处理。
- 迁移不复制旧 notes/fts，首次启动会重新索引；这是有意的，避免复制一套
  可能已过期的索引。
- 本方案不改变 50k/100k 的搜索/列表分页；那是独立工作。
