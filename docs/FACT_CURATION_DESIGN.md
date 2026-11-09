# 事实整理工作台（M1 + 拆分 M2）

> 状态：**M1 已实现并真机验收**（2026-11-09）；**M2 复合事实拆分已实现并真机验收**（2026-11-09）
> 入口：事实库 →「整理事实」→ `/facts/curate`；事实详情 →「拆分…」
> 目标：处理已有关系能识别出的重复、冲突、复合、无来源事实；M2 增加人工确认的复合事实拆分，不做自动拆分，不做语义聚类。
> M2 的迁移规则、撤销和实测见 [`FACT_SPLIT_DESIGN.md`](FACT_SPLIT_DESIGN.md)。

---

## 0. 为什么先做这个

史料链路已经能找候选、补录原文、逐字引用。当前更大的问题是事实本身：

- 16 条通用事实里存在复合事实（一条塞了 3–4 个断言）；
- 有事实没有网页证据也没有史料；
- 冲突/重复关系没有“已处理”的结论；
- 一条复合事实无法用单段史料对齐。

所以先做整理工作台，把事实变干净，再谈覆盖率和搜索质量。

---

## 1. M1 范围

### 做

1. **重复候选**：读取已有 `same_as` 关系，成对展示；选一条保留，另一条合并。复用现有 merge/undo。
2. **冲突**：读取已有 `contradicts` 关系，成对展示；记录处理结论：
   - `needs_research` 待查
   - `different_scope` 口径不同
   - `keep_both` 保留双方
   - `one_side_wrong` 一方有误
   - 可写备注；不改事实状态，不自动合并。
3. **复合事实**：启发式检测：
   - 文本有 ≥2 个 `，/；` 分隔；
   - 文本长度 ≥30；
   - 有 `related` 关系事实；
   - 展示相关原子事实，支持「标记待拆分 / 标记已整理 / 清除」。
4. **无来源**：没有 `fact_evidence` 且没有 `citations` 的事实；支持「已看过 / 清除」。
5. **整理标记**：`fact_reviews` 表，状态 `reviewed` / `needs_split`；可清除。
6. **统计卡**：重复 / 冲突 / 复合 / 无来源 / 未整理。

### 不做

- **不自动拆分复合事实**。M2 的拆分由用户确认子事实；证据/引用默认留在父事实，不复制，避免把“一条证据支持整条复合事实”伪造成“支持每个子事实”。
- **不做语义聚类**。M1/M2 只用已有 `same_as/contradicts/related` 关系；模型语义聚类留到下一阶段。
- **不自动删除/归档事实**。
- **不改事实真值**；冲突处理只是记录编辑判断。

---

## 2. 数据

新增两张表（`FactStore::open` 自动建）：

```sql
fact_conflict_resolutions (
  id, fact_a, fact_b, resolution, note, created_at, updated_at,
  UNIQUE(fact_a, fact_b)
)

fact_reviews (
  fact_id PRIMARY KEY, status, note, updated_at
)
```

- `fact_a/fact_b` 按 id 排序后存储，避免一对冲突存两次；
- resolution 只允许 `needs_research/different_scope/keep_both/one_side_wrong`；
- review 只允许 `reviewed/needs_split`；空状态删除记录。

### API

- `production_fact_conflict_resolutions`
- `production_fact_conflict_resolve(input)`
- `production_fact_reviews`
- `production_fact_review_set(factId, status, note)`

### M2 拆分数据与接口

新增 `fact_splits` 审计表（parent、children、拆分前状态、快照、split_at、undone_at）。接口：

- `production_fact_split(id, children)`：校验 2–8 条，复用 normalized 相同的已有事实；复制 mention，父事实标记 `split`；
- `production_fact_split_info(id)`：读取 active 拆分、子事实状态/证据数/引用数、`canUndo`；
- `production_fact_split_undo(splitId)`：仅当“新建子事实仍无新证据/核验/候选/关系”时允许撤销；恢复父事实状态和 review。

完整算法与迁移矩阵见 [`FACT_SPLIT_DESIGN.md`](FACT_SPLIT_DESIGN.md)。

---

## 3. UI

`/facts/curate`：

- 顶部统计卡；
- 重复候选：单选保留项 + 合并按钮；
- 冲突：左右事实对照 + 处理方式 + 备注；
- 复合：事实文本 + 相关原子事实 +「拆分 / 标记待拆分 / 标记已整理」；
- 无来源：事实行 + 已看过；
- 所有事实标题可点进详情；
- 事实详情：多断言事实显示「拆分…」；已拆分事实显示子事实、撤销按钮和 `canUndo` 原因；
- 事实库筛选新增「已拆分」。

事实库顶部新增「整理事实」入口。

---

## 4. 实测（2026-11-09）

17 条基线时（后续历史语料把库扩到 59 条）：

- 17 条事实 = 1 历史 + 16 通用；
- 重复候选 **0**（之前的合并已经把 same_as 关系处理掉）；
- 冲突 **0**（当前没有 contradicts 关系）；
- 复合事实 **2**：
  - 小布什家族复合事实（4 个断言）+ 2 条相关原子事实；
  - 含铅水管复合事实（2 个断言）+ 2 条相关原子事实；
- 无来源 **1**；
- 未整理 **17**。

> 2026-11-09 质量基线第二轮后，库增至 59 条事实（43 历史 + 16 通用）；当前工作台统计为 0 重复 / 0 冲突 / 2 复合 / 4 无来源 / 59 未整理。新增历史事实尚未逐条整理；截图见 `shots/curation/07-overview-50-facts.png`（旧基线）和 `shots/curation/08-overview-59-facts.png`（当前）。

真机操作：

- 复合卡「标记待拆分」→ `fact_reviews` 写入 `needs_split`，未整理 17→16，卡片显示「待拆分」；测试后清除。
- 无来源「已看过」→ `fact_reviews` 写入 `reviewed`；测试后清除。
- 临时插入 1 条 `contradicts` 关系 → 冲突区显示 1 对；选择「口径不同」并写备注 → `fact_conflict_resolutions` 写入 1 条；测试后删除临时关系与处理记录。
- 最终库：sources=1 / source_passages=1 / citations=1；`fact_reviews=0`；`fact_conflict_resolutions=0`。

测试：

- `cargo test`：**147 passed / 7 ignored**；
- 新增 M1 单测：冲突处理规范化/校验、整理标记写入/清除；
- 新增 M2 单测：拆分复用/证据留在父事实、撤销恢复、非法子事实、撤销拒绝；
- 包：**8,587,010 B（8.19 MiB）** / exe **21,518,336 B（20.51 MiB）**。

截图：

- `shots/curation/01-overview.png`：统计与空重复/冲突；
- `shots/curation/02-overview-composite.png`：2 条复合事实；
- `shots/curation/03-needs-split.png`：标记待拆分；
- `shots/curation/04-unsourced.png`：复合 + 无来源；
- `shots/curation/05-conflict.png`：临时冲突对（测试数据）；
- `shots/curation/06-conflict-resolved.png`：冲突处理已记录（测试数据）；
- `shots/curation/08-overview-59-facts.png`：59 条事实概览；
- `shots/curation/09-composite-split-action.png`：复合卡「拆分」入口；
- `shots/split/01..04`：拆分弹窗、结果、撤销、事实库筛选。

---

## 5. 诚实边界

1. **复合检测是启发式**，不是语义判断；会把“有相关关系且多逗号”的事实标出来，可能误报/漏报。
2. **拆分默认不迁移证据/引用**。M2 只拆文本和 mention；证据/引用留在父事实，子事实默认无来源。逐证据映射（把一条证据指定给某个子事实）还没做。
3. **没有语义聚类**。重复候选只来自已有 `same_as`（词面+数字规则），同一事实的不同表述可能漏掉。
4. **冲突处理不改变事实状态**；`disputed` 仍按原核验结果，处理结论只是编辑记录。
5. **冲突选项没有左右方向**：`one_side_wrong` 只记录“有一方有误”，不指定哪一方；需要具体判断时写在备注。
6. **没有批量操作**：一次合并一对、标记一条。
7. **整理标记不参与事实库过滤**；标记只是工作台状态，不隐藏事实。
8. **最终库的 2 条复合事实仍未永久拆分**；M2 真机用「小布什家族」做过一次拆分并撤销，库回到 59 条事实。含铅水管那条没有真机拆分。
9. **拆分子事实的候选/证据不会自动迁移**；撤销后也不会恢复父事实的 review（拆分前没有 review 时）；这些行为由快照和单测覆盖。
10. **新建子事实不跑 `link_related`**；它们暂时没有关系图，避免自动关系让撤销不可用。

---

## 6. 下一阶段

1. **逐证据映射**：拆分时把父事实的 evidence/citation 指定给某个子事实，而不是全部留在父事实；这需要每条证据的 quote 与子事实逐字对齐。
2. **语义聚类**：用模型对全库事实做候选分组，人工确认；仍然不自动合并。
3. **批量整理**：多选标记、批量合并同一组。
4. **冲突工作台增强**：指定一方有误、差异字段（年份/数字/口径）结构化。
