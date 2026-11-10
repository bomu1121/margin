# 新增页面 UI 走查截图（2026-11-09）

走查报告：[`docs/UI_AUDIT.md`](../../docs/UI_AUDIT.md)

## 最终构建（整改后）

| 截图 | 页面 |
| --- | --- |
| [01-facts-1440.png](01-facts-1440.png) | 事实库 · 1440；范围/状态分组、行内 meta 去噪 |
| [02-facts-1040.png](02-facts-1040.png) | 事实库 · 1040；动作不孤行、搜索独立一行 |
| [03-curation-1440.png](03-curation-1440.png) | 事实整理 · 1440；4 个可行动统计卡 + 空 section 一行 |
| [04-curation-composite-1440.png](04-curation-composite-1440.png) | 事实整理 · 复合事实卡；拆分主按钮 + ⋯ |
| [05-curation-1040.png](05-curation-1040.png) | 事实整理 · 1040 |
| [06-sources-1440.png](06-sources-1440.png) | 史料库；无手工入口、tab count、三步引导 |
| [06b-sources-1040.png](06b-sources-1040.png) | 史料库 · 1040；动作不孤行、三步引导三列 |
| [07-source-detail-1440.png](07-source-detail-1440.png) | 史料详情 |
| [07b-source-detail-1040.png](07b-source-detail-1040.png) | 史料详情 · 1040 |
| [08-queue-1440.png](08-queue-1440.png) | 待补录队列 · 1440；折叠组 + 紧凑候选行 + 主按钮 |
| [09-queue-1040.png](09-queue-1040.png) | 待补录队列 · 1040 |
| [10-history-fact-1440.png](10-history-fact-1440.png) | 历史事实详情；锚点、候选独立 section、空 section 收起 |
| [12-general-fact-1440.png](12-general-fact-1440.png) | 通用事实详情；定性冲突不再显示空数字表 |
| [13-general-fact-1040.png](13-general-fact-1040.png) | 通用事实详情 · 1040 |

## 笔记阅读页（单独走查）

报告：[`docs/NOTE_READING_AUDIT.md`](../../docs/NOTE_READING_AUDIT.md)

| 改前 | 改后 |
| --- | --- |
| [14-note-1440.png](14-note-1440.png) 标题「机/会」拆行、UP 署名、两条元信息、侧栏两行、速览双栏 | [41-note-one-line-1440.png](41-note-one-line-1440.png) 标题按冒号两行、字数/原视频一行、侧栏单行、速览单栏 |
| [15-note-1040.png](15-note-1040.png) 标题三行、UP 署名、来源条三行、侧栏两行、速览双栏 | [42-note-one-line-1040.png](42-note-one-line-1040.png) 标题 28px 两行、侧栏单行、原视频一行、速览单栏 |
| — | [40-note-one-line-actual.png](40-note-one-line-actual.png) 实际 1865 窗口：左 440 / 右 378 / 正文 860；侧栏每篇 30px 单行 |
| — | [43-note-one-line-info.png](43-note-one-line-info.png) 右栏「信息」：B站 · UP、创建/发布/运行/生成、打开原视频 |

DOM 实测：1865 正文 860 / 标题 39px；1440 正文 622 / 标题 28px；1040 正文 430 / 左 280 / 右 330。三档标题均为 2 个视觉行，署名均 1 行 20px、横向溢出 0；侧栏笔记行 48→30px，只显示标题和可选标注数。

## 侧栏搜索

报告：[`docs/SEARCH_DESIGN.md`](../../docs/SEARCH_DESIGN.md)

| 截图 | 状态 |
| --- | --- |
| [44-search-default.png](44-search-default.png) | 唯一入口：侧栏 `搜索笔记…`；顶栏搜索已移除 |
| [45-search-active.png](45-search-active.png) | focus：1px 蓝边 + 2px 淡蓝光晕，无黑点、无双层粗环 |
| [46-search-results.png](46-search-results.png) | 搜索「特朗普」：6 篇笔记，标题 + 路径，正文命中单独标 |
| [47-search-1040.png](47-search-1040.png) | 1040：结果面板与长标题块内换行，不裁切 |
| [48-search-1440.png](48-search-1440.png) | 1440：侧栏结果面板 |
| [49-sidebar-no-crumb.png](49-sidebar-no-crumb.png) | 侧栏去掉 `根 › ScribeFlow › 待整理` 面包屑，搜索框下直接进入树 |
| [50-sidebar-expand-no-bar.png](50-sidebar-expand-no-bar.png) | 点击「展开所有文件夹」后侧栏没有原生滚动条闪现；`.yb-scroll` 在收起时就已保留 |
| [53-facts-after-audit.png](53-facts-after-audit.png) | 事实审计修复后：有争议 11、有证据支持 40、未找到 6 |

复检：`.search-wrap` 数量 0；图标 `fill:none`；focus `outline:none`、`box-shadow 2px`；`Ctrl+K` 聚焦 `.side-filter`；`Esc` 清空恢复树；点击结果后 query 保留。

## 笔记库一级目录

| 截图 | 状态 |
| --- | --- |
| [51-vault-topics.png](51-vault-topics.png) | 29 篇按话题整理：历史 10 / 开发测试 1 / 待整理 空 / 时事 7 / 游戏 2 / 社会 2 / 科技 7 |
| [52-vault-topics-final.png](52-vault-topics-final.png) | 最终包重启后的一级目录与计数；无 `ScribeFlow`、无空 `新建文件夹` |

## 标注选区交互（M1）

| 截图 | 状态 |
| --- | --- |
| [58-annotation-default-margin.png](58-annotation-default-margin.png) | 有标注的笔记默认打开「标注」页边；2 条 mark 高亮 + 2 张卡片，无 `.unit .add` |
| [60-annotation-selection-composer.png](60-annotation-selection-composer.png) | 拖选「逼着你借」→ 页边临时卡片（引文/3 色/类型/输入）出现，正文 pending 高亮 |

## 整改前标注

`annotated/*.webp` 是走查时在旧构建上截的标注图，红框编号对应 `docs/UI_AUDIT.md` 的问题列表，用于前后对照：

- `annotated/01-facts-1440.webp`
- `annotated/02-facts-1040.webp`
- `annotated/05-curation-1040.webp`
- `annotated/06-sources-1440.webp`
- `annotated/08-queue-1440.webp`
- `annotated/11-history-sources-1440.webp`
- `annotated/12-general-fact-1440.webp`

## 复检结论

- 队列折叠/展开：12 候选 → 0 → 12，43 组不变；
- 史料库 `补录离线史料` 不存在；tabs 1/1/0/0/0/0；
- 严嵩详情无「拆分…」；小布什详情有；
- 小布什冲突显示“来源与事实表述不一致”，无空数字行；
- 拆分/撤销回归：59 facts / 63 mentions / 0 reviews / 0 active splits；
- `cargo test --lib` 166 passed / 7 ignored；包 8,722,874 B（8.32 MiB）/ exe 21,914,112 B（20.90 MiB）。

## 边界

- 只截 1440×900 / 1040×760；没有 900 宽、125%/150% 缩放、暗色模式。
- 笔记页另外测了实际 1865×1271 窗口；只覆盖一篇 2,461 字中文笔记，无超长/英文/无标点标题样本。
- 队列没有逐条执行补录；补录弹窗逻辑未改。
- P2（子事实父链接、右栏子事实 badge、拆分弹窗主语提示、事实库父子组）未做。
