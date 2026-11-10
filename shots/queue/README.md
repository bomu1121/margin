# 待补录候选队列截图（2026-11-09）

入口：史料库 →「待补录」→ `/sources/pending`

## 截图

| 截图 | 内容 |
| --- | --- |
| [01-pending-queue.png](01-pending-queue.png) | 287 条待补录 / 34 条涉及事实 / 0 条已引用；旧基线 |
| [02-paste-from-queue.png](02-paste-from-queue.png) | 队列里打开「补录原文」；共享 CandidatePasteDialog |
| [04-pending-queue-360.png](04-pending-queue-360.png) | 长笔记后 360 条待补录 / 43 条涉及事实 / 0 条已引用 |
| [05-paste-from-queue-360.png](05-paste-from-queue-360.png) | 360 条基线下的补录弹窗 |
| [06-source-library-pending-360.png](06-source-library-pending-360.png) | 史料库顶部「待补录 360」 |
| [07-ninth-fact-candidates.png](07-ninth-fact-candidates.png) | 长笔记第 9 条萧道成：原本 0 候选，手动自动找史料后 4 条候选、0 条引用 |

## 实测

- 队列按事实分组，首条事实 12 条候选；
- 「补录原文」从队列打开，预填 ctext · 金史詳校；
- 未提交测试文本，库仍 sources=1 / citations=1；
- 长笔记后队列 360 / 43 / 0；第 9 条事实先手动自动找史料，才从 42 条有候选事实变成 43 条；
- `cargo test` 166 passed / 7 ignored；
- 包 8,722,874 B（8.32 MiB）/ exe 21,914,112 B（20.90 MiB）。

## 边界

- 仍需用户打开候选页复制原文；不自动抓 ctext 网页/Wikisource/IA。
- 候选可能不是同一版本；补录前要确认页面。
- 没有批量补录。
