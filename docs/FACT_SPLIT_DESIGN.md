# 复合事实拆分 M1（2026-11-09）

> 入口：事实详情 →「拆分…」；事实整理工作台 → 复合事实卡 →「拆分」。
> 目标：把一条包含多个断言的事实拆成可独立核验的原子事实，同时不伪造证据归属，并且可以撤销。

---

## 1. 为什么不是“复制所有来源”

一条复合事实的证据/引用可能只支持其中一个断言。例如：

> 小布什的父亲是前美国总统，祖父是摩根兄弟合伙人，曾祖父是钢铁和铁路大亨，外祖父是默克公司创始人。

网页证据只引用了“曾祖父是钢铁石油大王”一句。如果拆分时把这条证据复制给四个子事实，就会让另外三个子事实看起来也有来源——这是**伪造支持**。

所以 M1 的硬规则：

| 数据 | 拆分时怎么处理 | 理由 |
| --- | --- | --- |
| 子事实文本 | 用户确认/编辑后创建；同 normalized 的已有事实直接复用 | 避免重复事实 |
| 笔记提及（mention） | 复制到每个子事实 | 笔记原文确实包含这些断言；子事实在事实详情里要能指回笔记 |
| 网页证据（fact_evidence） | **留在原事实**，不复制 | 一条证据通常只支持一个断言 |
| 史料引用（citations） | **留在原事实**，不复制 | 同上；拆分不做逐句证据映射 |
| 核验记录（fact_checks） | 留在原事实 | 核验结论针对整条复合事实 |
| 检索候选（history_search_candidates） | 留在原事实 | 候选不是引用，也不应自动归属 |
| 事实关系（fact_relations） | 不复制 | 关系是整条事实之间算出来的；硬拷会污染 |
| 整理标记（fact_reviews） | 原事实标记为 `reviewed`；拆分前状态写入快照 | 拆分完成视为已整理；撤销时恢复 |
| 原事实状态 | 改为 `split`，事实库保留（可筛选「已拆分」） | 审计记录；不物理删除 |
| 笔记右栏（阅读层） | **保留父事实**；子事实不在右栏拆成碎片，只在事实详情/事实库展开 | 阅读需要原句的因果、转折和语气；原子事实是核查单位，不是阅读单位 |

**子事实默认是无来源的候选事实**。它们需要用户逐条核验（网页证据 / 自动找史料 / 粘贴补录），而不是继承父事实的结论。

---

## 2. 数据模型

新增表（`facts.rs` 的 `CREATE TABLE IF NOT EXISTS`，启动时自动建）：

```sql
CREATE TABLE fact_splits (
  id TEXT PRIMARY KEY,
  parent_fact_id TEXT NOT NULL,
  child_ids_json TEXT NOT NULL DEFAULT '[]',
  parent_status TEXT NOT NULL DEFAULT '',
  snapshot_json TEXT NOT NULL DEFAULT '{}',
  split_at INTEGER NOT NULL,
  undone_at INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_fact_splits_parent ON fact_splits(parent_fact_id, split_at DESC);
```

`snapshot_json` 保存：

- 拆分前的父事实状态、`updated_at`、`last_checked_at`；
- 拆分前的 `fact_reviews` 行（可能为空）；
- 每个子事实是否在拆分前已存在（`existed`）；
- 本次拆分**实际插入**的 mention id（撤销时只删这些，不动用户后来加的）。

`parent_status` 存的是**拆分前**状态；父事实当前状态统一是 `split`。

---

## 3. 拆分算法

`FactStore::split(parent_id, children)`：

1. 校验 2–8 条；每条 2–300 字；不能重复；不能和父事实完全相同。
2. 在事务里读父事实（text/status/domain/event/historical_status/claim_kind/updated_at/last_checked_at）。
3. 逐条子事实：
   - 按 `normalized` 查已有事实；存在且不是父事实、也不是已拆分事实 → **复用**；
   - 不存在 → 新建 `candidate` 事实，继承 domain/event/historical_status/claim_kind，`queries=[]`。
4. 把父事实的 mention 复制到每个子事实；`INSERT OR IGNORE`，记录真正插入的 id。
5. 父事实 `status='split'`，`updated_at=now`。
6. `fact_reviews` 父事实写 `reviewed`（保留原 note），快照记录拆分前 review。
7. 写 `fact_splits` 审计行。
8. 提交；返回 `FactSplit`（含子事实状态、证据数、引用数、`canUndo`）。

**没有做**：模型自动拆句、自动给子事实分配证据、自动建立 related 关系。M1 是人工确认的拆分。

---

## 4. 撤销算法

`FactStore::undo_split(split_id)`：

1. 读未撤销的 split 与快照。
2. **预检**：对每个“新建的子事实”检查是否仍然干净：
   - mention 数不能超过本次插入数；
   - 没有 evidence / citation / fact_check / review / relation / history candidate。
   任一不满足 → 拒绝撤销，提示“子事实已经有新增的证据、核验、关系或检索候选”。
   拆分前已存在的原子事实**不删除**，所以它们有自己的证据不影响撤销。
3. 事务里：
   - 删除本次插入的 mention；
   - 删除“新建且仍干净”的子事实；
   - 恢复父事实 status/updated_at/last_checked_at；
   - 恢复父事实 review（快照为空则删除 review）；
   - 标记 `fact_splits.undone_at`。
4. 返回 `FactSplit`（`undone_at` 非 0，`canUndo=false`）。

撤销不会删除已有原子事实，也不会删除父事实的证据/引用。

---

## 5. UI

### 5.1 事实详情

- 顶部动作「拆分…」只在复合事实出现：≥2 个逗号/分号 + 文本长度≥30 + 有 related 事实（与整理工作台同一启发式）。普通事实不会显示，避免误拆。
- 已拆分事实：状态 pill「已拆分」；正文下方显示：
  - 子事实列表（状态、网页证据数、史料数）；
  - 「撤销拆分」按钮；
  - `canUndo=false` 时禁用并说明原因。
- 右侧 rail 增加「拆分 N 条子事实」。

### 5.2 拆分弹窗

`FactSplitDialog.vue`：

- 展示原事实；
- 默认候选 = 已有 related 原子事实 + 按 `，；,;` 切出的从句；
- 用 bigram 相似度去掉 related 与从句的近似重复；
- 用户可增删/改写（2–8 条）；
- 提示“其中 N 条已有事实会直接复用”；
- 明确提示证据/引用留在原事实；
- 后端错误原样展示（重复、太长、已拆分等）。

### 5.3 事实整理工作台

- 复合事实卡增加「拆分」按钮（同一弹窗）。
- 已拆分父事实不再出现在复合/重复/冲突/无来源/未整理列表里。
- 拆分后子事实会进入「无来源」清单，等待逐条核验。

### 5.4 事实库

- 状态筛选新增「已拆分」；父事实行显示「已拆分」。
- 事实总数会把子事实算进去；这是预期，因为子事实是独立事实。

### 5.5 笔记右栏（阅读层）

- 后端 `facts_for_note` / `fact_count_for_note` **保留父事实**，排除 active split 的子事实。
- 阅读右栏显示原来的复合事实（状态「已拆分」），不把一条原句拆成四条碎片。
- 子事实只在事实详情/事实库/整理台展开；它们仍然是核查单位。
- 这是本轮根据“阅读是否正确”做的修正：拆分适合核查，不适合替代阅读原文。

---

## 6. 实测（2026-11-09，真实库）

### 6.1 场景

真实复合事实：

> 小布什的父亲是前美国总统，祖父是摩根兄弟合伙人，曾祖父是钢铁和铁路大亨，外祖父是默克公司创始人。

- 事实 id：`fact_45555c8f1c2c0dfa82e4c062`；
- 拆分前：`disputed`、1 篇笔记引用、1 条网页证据、2 条 related 原子事实、无 review；
- 拆分目标 4 条：2 条已有原子事实（曾祖父、外祖父）+ 2 条新建（父亲、祖父）。

### 6.2 拆分结果

- 父事实 `status='split'`；状态 pill 显示「已拆分」；
- `fact_splits` 1 条 active；
- 子事实：
  - 曾祖父（复用，`verified`，1 条网页证据）；
  - 外祖父（复用，`unsupported`，0 条网页证据）；
  - 父亲（新建，`candidate`，0 条网页证据）；
  - 祖父（新建，`candidate`，0 条网页证据）；
- 父事实仍有 1 条网页证据、1 条 mention；没有证据复制到新子事实；
- 每个子事实都有 1 条复制 mention（已有事实从 1 条变 2 条，新建事实 1 条）；
- 父事实 review 变为 `reviewed`；
- 事实库状态筛选「已拆分」显示 1 条；总数从 59 变 61（2 条新事实，2 条复用）。

### 6.3 撤销结果

- 父事实恢复 `disputed`；`updated_at` / `last_checked_at` 与拆分前一致（1794068583104）；
- review 行删除（拆分前没有）；
- 2 条新建子事实删除；
- 2 条已有原子事实保留；它们复制得到的 mention 删除，mention 数回到 1；
- `fact_splits.undone_at` 非 0；
- 库回到 **59 事实 / 63 mentions / 0 reviews / 0 active splits**；
- 测试产生的 2 条 undone split 记录随后清理，库里没有测试残留。

### 6.4 截图

- `shots/split/01-dialog.png`：拆分弹窗，4 条建议，其中 2 条已有事实复用；
- `shots/split/02-result.png`：拆分结果，父事实「已拆分」+ 4 条子事实；
- `shots/split/03-undone.png`：撤销后父事实恢复「存在争议」；
- `shots/split/04-facts-filter.png`：事实库「已拆分」筛选；
- `shots/split/05-note-rail-before.png`：拆分前，笔记右栏第 5 条是复合父事实；
- `shots/split/06-note-rail-after.png`：拆分后，右栏仍只有同一条父事实（标「已拆分」），没有变成 4 条阅读碎片；
- `shots/curation/09-composite-split-action.png`：整理工作台的「拆分」入口。

---

## 7. 测试

- `cargo test`：**165 passed / 7 ignored**（新增 4 个 split 测试）；
- `npm run build`：`vue-tsc -b && vite build` 通过；
- 新增单测：
  - `split_reuses_existing_child_and_keeps_evidence_on_parent`
  - `undo_split_removes_new_children_and_restores_parent`
  - `split_rejects_invalid_children_and_double_split`
  - `undo_split_refuses_after_a_new_child_gets_evidence`
- 安装包：**8,714,113 B（8.31 MiB）**；exe：**21,882,880 B（20.87 MiB）**。

---

## 8. 诚实清单

- **没有逐条证据映射**：M1 把 evidence/citation 留在父事实；子事实默认无来源。不是“拆完就有证据”。
- **没有自动拆句**：候选从句是启发式；主语可能缺失（例如“祖父是摩根兄弟合伙人”），用户要自己补全。
- **没有自动建立 related 关系**：新建子事实不会跑 `link_related`；避免撤销时被自动关系卡住，但也意味着子事实暂时没有关系图。
- **撤销有安全限制**：新建子事实一旦有新证据/核验/候选/关系，就不能撤销；这是为了避免删掉用户后来补的数据。
- **撤销会覆盖拆分区间的 review 修改**：快照只保存拆分前的 review；拆分后如果又手动改了父事实的整理标记，撤销会恢复拆分前状态，不检测这层冲突。
- **复用按 normalized 精确匹配**：标点/繁简/措辞不同不会自动复用，可能生成近似重复；需要用户合并。
- **没有批量拆分**：一次一条。
- **父事实保留在事实库**：不是物理删除；默认列表会显示「已拆分」，可用状态筛选。
- **UI 真机只测了真实小布什事实的一次拆分/撤销**；其它复合事实（含铅水管）和“撤销被拒绝”的 UI 状态没有逐项真机点击，后端单测覆盖拒绝逻辑。
- **笔记右栏父事实优先已截图验收（05/06）**；为了让父事实出现在这条笔记的右栏，截图时临时把 note artifact 的 runId 指到父事实 mention 所在 run，截图后已恢复。当前笔记右栏仍按最新 run 过滤，旧 run 的父事实可能不出现；这是已有行为，不是拆分造成的。
- **没有测历史事实的 citation 迁移**：按设计不迁移，父事实引用留在原地；本轮测试事实是通用事实，0 citation。
