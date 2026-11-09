# 复合事实拆分截图（2026-11-09）

入口：事实详情 →「拆分…」；事实整理 → 复合事实 →「拆分」。

| 截图 | 内容 |
| --- | --- |
| [01-dialog.png](01-dialog.png) | 真实「小布什家族」复合事实；4 条建议，2 条已有原子事实直接复用 |
| [02-result.png](02-result.png) | 拆分后：父事实「已拆分」，4 条子事实；证据仍在原事实 |
| [03-undone.png](03-undone.png) | 撤销后：父事实恢复「存在争议」，拆分按钮回来 |
| [04-facts-filter.png](04-facts-filter.png) | 事实库「已拆分」筛选只显示父事实 |
| [05-note-rail-before.png](05-note-rail-before.png) | 拆分前：笔记右栏第 5 条是复合父事实 |
| [06-note-rail-after.png](06-note-rail-after.png) | 拆分后：右栏仍是同一条父事实，标「已拆分」；4 条子事实没有变成 4 条阅读碎片 |

相关整理工作台截图：[`../curation/09-composite-split-action.png`](../curation/09-composite-split-action.png)。

> 05/06 为了让父事实出现在这条笔记的右栏，临时把 note artifact 的 runId 指到 `run_07f5...`（父事实 mention 所在 run），截图后已恢复 `run_6f01...`。当前笔记右栏仍按最新 run 过滤。

## 实测

- 父事实：1 篇笔记、1 条网页证据、2 条 related 原子事实、无 review；
- 4 条拆分目标：2 条已有事实复用 + 2 条新建；
- 父事实状态 `split`；review 变 `reviewed`；证据/引用留在父事实；
- 笔记 mention 复制到 4 条子事实；
- 撤销后：父事实恢复 `disputed`，新建子事实删除，已有子事实保留，复制 mention 删除，review 删除；
- 库回到 59 事实 / 63 mentions / 0 reviews / 0 active splits；
- `cargo test`：147 passed / 7 ignored；
- 包 8,587,010 B（8.19 MiB）/ exe 21,518,336 B（20.51 MiB）。

## 边界

- 子事实默认无证据/引用，需要逐条核验；
- 候选从句可能缺主语，需要用户改写；
- 新建子事实一旦有新证据/核验/候选/关系，不能撤销；
- 没有逐证据映射，没有批量拆分。
