# 历史链路质量基线截图（2026-11-09）

语料：B站 UP `王山水读通鉴`（UID 431055518）6 条视频 + 已有嘉靖笔记。

## 截图

| 截图 | 内容 |
| --- | --- |
| [00-wizard.png](00-wizard.png) | 新建制作向导 |
| [01-template.png](01-template.png) | 选择「速览与溯源」模板 |
| [02-anshi-note-rail.png](02-anshi-note-rail.png) | 安史之乱笔记右栏：7 条事实，全部「历史」，7 条有网页证据 |
| [03-zizhi-candidates.png](03-zizhi-candidates.png) | 「安史之乱持续八年」事实：候选 9 条，top1 ctext《資治通鑑》 |
| [04-stuck-task.png](04-stuck-task.png) | 13 分钟 9 主张长视频验证 limit=8：卡在 66%，最终取消；已加 90s verify 超时 |

## 数据

- 7 篇历史笔记 / 34 条历史事实；总库 50 事实（16 general + 34 history）。
- 6 个新任务全部 success：1,347s、361,323 tokens、¥0.8410。
- 33/33 新事实标 history；规划 34/34 人工核对无明显错书。
- 候选 34/34；ctext top1 32/34；2 个缺口（苏轼文集、江表传）只有外链。
- 网页证据 30/34；自动 citation 0；全库 1 条人工《明史》。
- 自动 trace 覆盖 22/34（batch limit 3/6；最终代码 limit 8 未重跑）。
- `cargo test`：166 passed / 7 ignored。
- 包 8,722,874 B（8.32 MiB）/ exe 21,914,112 B（20.90 MiB）。

完整报告：[`docs/HISTORY_EVAL.md`](../../docs/HISTORY_EVAL.md)。
