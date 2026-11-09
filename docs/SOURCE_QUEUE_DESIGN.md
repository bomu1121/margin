# 待补录候选队列（2026-11-09）

> 入口：史料库 →「待补录」→ `/sources/pending`
> 目的：把自动检索得到的 `candidate` 候选集中起来，用“打开页面 → 粘贴原文 → 自动对齐”把候选变成 citation，而不是让用户逐条点进事实详情。

## 为什么需要

质量基线第二轮后库里有 360 条候选、43 条历史事实，但只有 1 条人工 citation。候选分散在事实详情里，逐条找太慢；需要一条按事实聚合的队列。

## 数据与行为

- 读取 `history_search_candidates` 中 `status='candidate'` 的记录；
- 按 `fact_id` 分组，显示事实、引用笔记、候选 provider/title/authority/reason；
- 每行「打开来源」走系统浏览器；
- 每行「补录原文」打开共享 `CandidatePasteDialog`：
  - 用户从候选页复制正文/段落；
  - 后端切 passage、确定性/模型挑句、逐字子串校验；
  - 成功写 source/passage/citation，候选标 `cited`；
  - 失败标 `rejected`，0 citation；
- 补录成功后队列刷新，候选从待补录列表消失；
- 事实详情里的候选补录与队列共用同一个组件。

## 接口

- `production_history_candidates_all(status, limit)`：按状态列出候选；
- `production_fact_history_candidate_import(candidateId, text)`：已有补录接口。

## 实测

- 2026-11-09 长笔记后，`/sources/pending` 显示 **360 条待补录候选 / 43 条涉及事实 / 0 条已引用**；
- 队列按事实分组，首条“蒙古灭金所用时间长于金灭辽”显示 12 条候选；
- 「补录原文」弹窗从队列正常打开，预填 `ctext · 金史詳校`；
- 未提交测试文本，未新增 citation；库仍 sources=1 / citations=1；
- 长笔记第 9 条事实（萧道成）原本 0 候选、不会出现在队列；在事实详情手动「自动找史料」后新增 4 条候选，队列涉及事实从 42 变为 43；
- 截图：`shots/queue/01-pending-queue.png`、`02-paste-from-queue.png`；更新后的计数在 `04-pending-queue-360.png`、`05-paste-from-queue-360.png`、`06-source-library-pending-360.png`。

## 诚实边界

- 队列只是把候选集中，**没有自动获取全文**；仍需用户打开页面复制。
- ctext 无 Key 时候选是 link-only；Wikisource/IA 网络失败时没有候选或只有外链。
- 候选可能包含相关但非同一版本的书；补录时必须确认页面。
- 没有批量选择/批量补录；一次一条。
- 已引用候选不在待补录队列；fact 详情仍能看到全部候选。
- 队列只列已有候选的事实；0 候选事实（如长笔记第 9 条）不会出现，需要先在事实详情手动「自动找史料」。
- 360 条候选来自 43/43 历史事实，但其中 1 条是长笔记手动补搜，不是自动 trace 覆盖。
