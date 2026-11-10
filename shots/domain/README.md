# 事实领域分区 + 独立史料检索（2026-11-08）

这组截图对应产品修正后的状态：

- 事实按 `domain` 分成「历史事实 / 其他事实」，只有历史事实显示「史料」区；
- 历史事实可「自动找史料」：查询规划 → ctext 官方 API / Wikisource / IA / 外链源 → 候选排序 → 全文抓取 → 逐字选句；
- 只有全文源 + 逐字 quote 才写 citation；目录/外链/二手只进候选池；
- 事实详情显示候选 provider/authority/status/reason；可人工纠正归类、移除引用；
- 史料库/详情只读；保留轻量「补录离线史料」。

## 数据

- 事实 17 条：1 条历史事实 + 16 条通用事实；
- 历史事实：`fact_cc798cf03ac6efb10e3e7ef2`「严嵩无他才略，唯一意媚上，窃权罔利。」；
- 史料 1 份《明史·严嵩传》/ 原文 1 段 / 引用 1 条；
- 事件 0；
- 自动找史料真机：规划 `明史·卷三百八·奸臣传·严嵩`，7 条候选；ctext 无 Key、Wikisource 直连失败，**自动引用 0 条，未新增 source/citation**；
- ctext live：`searchtexts?title=明史` 返回书目；`gettext` 无 Key 返回 `ERR_REQUIRES_AUTHENTICATION`；
- 归类真机：general ↔ history 切换，史料区隐藏/恢复；
- 离线补录真机：提交 1 份测试 source/passage/citation（1/1 → 2/2），移除 citation 后删除测试 source/passage，回到 1/1/1；
- **候选补录真机**：ctext《明史》候选粘贴原文 → 写入 1 条 citation（1→2），候选标 cited；随后移除 citation、删除测试 source/passage、候选恢复 candidate，库回到 1/1/1；
- `cargo test`：165 passed / 7 ignored（含 ctext/B站/历史评测 live 测试，`--ignored` 手动通过）；
- 包 8,714,113 B（8.31 MiB）/ exe 21,882,880 B（20.87 MiB）。

## 截图

| 截图 | 验收点 |
| --- | --- |
| [01-facts-history-area.png](01-facts-history-area.png) | 事实库分区：历史事实 1 / 其他事实 16 |
| [02-general-fact-no-sources.png](02-general-fact-no-sources.png) | 通用事实详情没有「史料」区，只有网络证据 |
| [03-history-fact-sources.png](03-history-fact-sources.png) | 历史事实详情有「史料」、自动找史料、归类、移除引用 |
| [04-sources-read-only.png](04-sources-read-only.png) | 史料库只读 + 轻量「补录离线史料」 |
| [05-source-detail-read-only.png](05-source-detail-read-only.png) | 史料详情只读，没有加原文/引用按钮 |
| [06-history-note-rail.png](06-history-note-rail.png) | 笔记右栏显示「有史料 / 历史 / 1 条史料」 |
| [07-history-filter.png](07-history-filter.png) | 历史事实筛选 |
| [08-auto-find-network-fail.png](08-auto-find-network-fail.png) | 自动找史料失败：只显示原因，不伪造结果 |
| [09-domain-general-hidden.png](09-domain-general-hidden.png) | 归到其他事实后史料区隐藏 |
| [10-manual-offline-import.png](10-manual-offline-import.png) | 离线/纸本补录弹窗 |
| [11-history-search-candidates.png](11-history-search-candidates.png) | 查询规划 + 候选摘要 |
| [12-history-candidates-expanded.png](12-history-candidates-expanded.png) | 候选展开：provider/authority/status/reason/外链 |
| [13-ctext-key-setting.png](13-ctext-key-setting.png) | ctext API Key 设置 |
| [14-candidate-paste-dialog.png](14-candidate-paste-dialog.png) | 候选补录原文弹窗 |
| [15-candidate-paste-cited.png](15-candidate-paste-cited.png) | 补录成功、候选标 cited（测试后已清理） |

## 诚实边界

- **没有 live 自动 citation**：本机 Wikisource/IA 直连超时；ctext 无 Key 只能拿书目；citation 仍只有早前人工《明史》1 条。
- **候选补录是人工最后一步**：用户要打开候选页复制原文；程序负责切 passage/挑句/逐字校验。粘贴到同文异版时 quote 会过，版本仍需人工确认。
- ctext 网页抓取被明确禁止（会故意返回损坏数据），只走官方 API；没有 Key 只保留候选。
- 唯一《明史》记录是早前人工验证数据，不是自动命中。
- `domain` 由模型标注，可能误判；现在有「归类」可人工纠正。新的 history/general prompt 只验证了 parser 和迁移，没有跑真实模型完整 trace；缺字段时默认 general。
- Wikisource/IA 抓取器未 live 验证；查询规划真机成功，但没有 golden set，无法量化错书率。
- 候选可能包含相关但非同一版本的书（明史纪事本末、明史演义）；只作候选，不自动引用。
- 模型选 quote 的 prompt 没有在最终构建上跑过真实模型；非逐字、越界、过短的 quote 会被拒绝。
- 确定性兜底不做繁简转换；繁简不同的主张可能只能依赖模型选句。
- 自动 source 没有专门校对 UI；选错时可在事实详情移除引用后重试。
