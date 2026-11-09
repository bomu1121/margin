# 事实整理工作台 M1（2026-11-09）

入口：事实库 →「整理事实」→ `/facts/curate`

17 条基线时：

- 重复候选 0（已有 same_as 关系已被之前的合并处理掉）
- 冲突 0（没有 contradicts 关系）
- 复合事实 2（小布什家族、含铅水管；都带相关原子事实）
- 无来源 1
- 未整理 17

2026-11-09 长笔记后，当前库 59 条事实（43 历史 + 16 通用）：

- 重复候选 0
- 冲突 0
- 复合事实 2
- 无来源 4
- 未整理 59

## 截图

| 截图 | 内容 |
| --- | --- |
| [01-overview.png](01-overview.png) | 统计卡 + 重复/冲突空态；旧阈值下复合为 0 |
| [02-overview-composite.png](02-overview-composite.png) | 阈值修正后复合事实 2 条，显示相关原子事实 |
| [03-needs-split.png](03-needs-split.png) | 复合事实标记「待拆分」，未整理 17→16 |
| [04-unsourced.png](04-unsourced.png) | 复合事实 + 无来源清单「已看过」 |
| [05-conflict.png](05-conflict.png) | 临时插入 1 条 contradicts 关系后的冲突 UI（测试数据） |
| [06-conflict-resolved.png](06-conflict-resolved.png) | 选择「口径不同」+备注并记录（测试数据已清理） |
| [07-overview-50-facts.png](07-overview-50-facts.png) | 质量基线后 50 事实：0 重复 / 0 冲突 / 2 复合 / 4 无来源 / 50 未整理 |
| [08-overview-59-facts.png](08-overview-59-facts.png) | 长笔记后 59 事实：0 重复 / 0 冲突 / 2 复合 / 4 无来源 / 59 未整理 |
| [09-composite-split-action.png](09-composite-split-action.png) | 复合事实卡新增「拆分」入口 |

## 真机验收

- 复合卡「标记待拆分」→ `fact_reviews` 写入 `needs_split`，未整理 17→16；
- 无来源「已看过」→ `fact_reviews` 写入 `reviewed`；
- 临时 contradicts 关系 → 冲突区 1 对；选择 `different_scope` + 备注 → `fact_conflict_resolutions` 写入；
- 测试后清除 reviews、临时关系、resolution；最终 `sources=1 / source_passages=1 / citations=1`，reviews=0，conflict_resolutions=0；
- M2 拆分：真实「小布什家族」事实拆 4 条（2 复用 + 2 新建），父事实证据留存、review 置 reviewed；撤销后回到 59 事实 / 63 mentions / 0 reviews / 0 active splits；详见 [`../split/README.md`](../split/README.md)；
- `cargo test`：147 passed / 7 ignored；
- 包 8,587,010 B（8.19 MiB）/ exe 21,518,336 B（20.51 MiB）。

## 诚实边界

- 复合检测是启发式（≥2 个逗号/分号 + 长度≥30 + 有 related 事实），会误报/漏报；
- M2 有拆分，但没有逐证据映射；子事实默认无来源，证据/引用留父事实；没有归档；
- 没有语义聚类；重复候选只来自已有 `same_as` 词面+数字关系；
- 冲突处理不改变事实状态；
- `one_side_wrong` 只记录“有一方有误”，不指定哪一方；具体判断写备注；
- 没有批量操作。
