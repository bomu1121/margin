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
| — | [43-note-one-line-info.png](43-note-one-line-info.png) 旧「信息」：B站 · UP、创建/发布/运行/生成、打开原视频 |
| — | [80-info-panel-redesign.png](80-info-panel-redesign.png) 2026-11-12 信息栏去噪：来源/作者/发布日期 + 打开原视频/制作记录；文件/文件夹/复制/入库日期 |

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
| [83-rail-fact-states.png](83-rail-fact-states.png) | 右栏事实去大字：异常状态只在 meta 以小色点 + 10.5px「有不同记载/暂无出处」显示；全历史笔记不再逐卡重复「历史」 |

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

## 标注页边、紧凑卡片与视觉打磨（M2 + M3 + M4）

| 截图 | 状态 |
| --- | --- |
| [70-annotation-cards-compact.png](70-annotation-cards-compact.png) | 1662×1204：正文列 865 / 右栏 378 / 卡片 345；去掉类型行后无正文卡高 73、带正文 97；同段多条按 12px 下推，独立行仍与高亮同高（955/955） |
| [73-annotation-active.png](73-annotation-active.png) | 点击正文高亮：底色不变，active 只加深色下划线；卡片 active 为 6% 标注色暖底 + 2px 左线，不再是蓝底蓝框 |
| [74-annotation-flash.png](74-annotation-flash.png) | 点击卡片定位：段落左侧 2px 金色竖线淡出，无深蓝 outline；可见区块不滚动 |
| [75-annotation-composer-refined.png](75-annotation-composer-refined.png) | 临时卡暖金纸片：引文 + 输入 + 底栏；input focus 金色光晕，色点自身色环，高 115 |
| [81-delete-confirm.png](81-delete-confirm.png) | 标注删除二次确认：第一次点删除只进入「删除这条标注？」+ 取消/确认删除，4 秒自动复位；确认后 6→5 |

## 标签台：UP 主派生标签（2026-11-12）

| 截图 | 状态 |
| --- | --- |
| [82-up-tag-board.png](82-up-tag-board.png) | 索引从 `作者` 派生 `UP主/<作者>`：UP 主组 12 个；点「小丑朱迪」命中 7 篇，卡片行显示 `UP主/小丑朱迪` |

## 笔记标题区（2026-11-16）

| 截图 | 状态 |
| --- | --- |
| [112-note-header-no-category.png](112-note-header-no-category.png) | 标题上方不再显示 `笔记 / 未分类`；标题直接作为阅读起点 |

## 事实详情重设计（2026-11-16）

| 截图 | 状态 |
| --- | --- |
| [110-fact-detail-redesign.png](110-fact-detail-redesign.png) | 通用事实：单列；状态一句话；来源前置；相关事实去技术百分比；核验折叠 |
| [111-fact-detail-history-redesign.png](111-fact-detail-history-redesign.png) | 历史事实：史料/候选/网页补充分层；无右侧重复栏；领域切换在 meta 行 |

## 笔记事实溯源（2026-11-16）

| 截图 | 状态 |
| --- | --- |
| [107-facts-trace-cta.png](107-facts-trace-cta.png) | 事实为 0：空态和工具栏都提供「跑溯源」，说明会消耗模型/检索且不改写正文 |
| [108-facts-after-trace.png](108-facts-after-trace.png) | 实跑 `笔记溯源` 任务后：7 条事实、5 条有网页证据、2 条待关注；任务只更新事实库 |
| [109-trace-task-detail.png](109-trace-task-detail.png) | 任务详情：模板「笔记溯源」，结果写「事实已更新」，关联现有笔记，正文未改写 |

## 侧栏图形与收藏位置（2026-11-16）

| 截图 | 状态 |
| --- | --- |
| [102-sidebar-folder-icons.png](102-sidebar-folder-icons.png) | 文件夹有展开/收起图标；笔记行不再有文档图标；未收藏行没有 lead 占位 |
| [103-sidebar-pinned-lead.png](103-sidebar-pinned-lead.png) | 收藏后 pin 出现在行左侧 lead；收藏区同样用 pin；右侧不再预留星标槽 |

## 制作台模板白名单与新建向导（2026-11-13）

| 截图 | 状态 |
| --- | --- |
| [84-production-templates-filtered.png](84-production-templates-filtered.png) | 首页只显示 3 个模板；「查看全部 3 个」 |
| [85-production-prompts-filtered.png](85-production-prompts-filtered.png) | 提示词库内置风格只剩「速览版」1 个 |
| [86-subscription-trace-hint.png](86-subscription-trace-hint.png) | 订阅编辑：模板 2 项、提示词 1 项；视频笔记下提示不跑联网溯源 |
| [87-wizard-step1-fixed.png](87-wizard-step1-fixed.png) | 向导底部提示按状态变化；未粘贴链接时禁用下一步并说明原因 |
| [88-wizard-favorites-fixed.png](88-wizard-favorites-fixed.png) | 98 条收藏时正文独立滚动，footer 固定在视口内 |
| [90-wizard-parsed-hint.png](90-wizard-parsed-hint.png) | 链接解析后提示「已解析；可以下一步」 |
| [91-wizard-favorites-fixed-final.png](91-wizard-favorites-fixed-final.png) | 收藏列表 final：正文 client/scroll 481/6765 时 footer 仍可见 |
| [92-wizard-stepper-step1.png](92-wizard-stepper-step1.png) | 步骤条：数字圆点 + 连线，当前步骤蓝点光环，无胶囊 chip |
| [93-wizard-stepper-step2.png](93-wizard-stepper-step2.png) | 完成步骤绿点 + 绿线，当前步骤蓝点 |
| [94-wizard-stepper-digits-centered.png](94-wizard-stepper-digits-centered.png) | 序号 `line-height:1` 后圆心/字形中心误差 0.00003px |
| [95-production-home-badges-fixed.png](95-production-home-badges-fixed.png) | 模板徽标 26.7→20.1px，流程 chip 不被撑高 |
| [96-wizard-step2-badges-fixed.png](96-wizard-step2-badges-fixed.png) | 模板卡徽标居中，卡片高度 92.9px |
| [97-wizard-step2-820x680.png](97-wizard-step2-820x680.png) | 820×680：步骤条和 footer 不溢出 |
| [98-wizard-step2-520x680.png](98-wizard-step2-520x680.png) | 520×680 截图（仅截图，未跑全量几何审计） |
| [99-production-tasks-badges-fixed.png](99-production-tasks-badges-fixed.png) | 任务表：自动徽标 18.1px，不撑高任务行 |
| [100-follow-list-show-added.png](100-follow-list-show-added.png) | 取消隐藏已添加：9 个已订阅 UP 排在底部，置灰 + 勾 +「已添加」，不可再选 |
| [101-follow-list-hide-added.png](101-follow-list-hide-added.png) | 默认隐藏已添加：50 条已加载关注中显示 41 条可添加项；可勾选恢复 |

几何审计：`npm run audit:wizard`，7 个状态 0 失败，报告 [`docs/wizard-audit.json`](../../docs/wizard-audit.json)。

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

## 配色层级（2026-11-16）

- `113-color-light.png`：默认浅色。正文阅读面 `--surface #fffdf8` 最亮，左右栏 `--rail #ece8de` 退后，顶部/画布 `--paper #f4f1e8`；正文与右栏对比 1.204（旧 1.045）。
- `114-color-dark.png`：暗色。正文 `#211d16` 高于左右栏 `#12100d`，画布 `#0c0b09`；正文/右栏对比 1.132。
- 旧配色侧栏纯白 + 正文 `#f6f7f4` + 右栏 `#f1f2ef`，正文/右栏仅 1.045，层级反了。

## 阅读页视觉复核（2026-11-16）

| 截图 | 内容 |
| --- | --- |
| `115-note-review-after.png` | 改后浅色：正文 720px / 18px（约 40 字/行）、署名含作者、“新”点、顶栏三段 |
| `116-note-review-before-after.png` | 113 vs 115 全页并排 |
| `117-note-review-detail-rail.png` | 右栏首两张卡 1:1 对比：域徽章中性、动作固定右侧、卡片 14px |
| `119-note-review-after-dark.png` | 改后暗色 |
| `120-note-review-dark-before-after.png` | 114 vs 119 暗色并排 |
| `121-note-1040-after.png` | 1040×760 改后；顶栏无溢出，正文 390 / 17px，标题 28px |

备注：113/114 与 115/119 之间库内新增 1 篇笔记（62→63），顶栏/左栏计数不是同一时刻；右栏宽度两轮均为 302。

## 标签台视觉重排（2026-11-16）

| 截图 | 内容 |
| --- | --- |
| `122-tags-before.png` | 改前浅色：标签板 282 个、结果行 115px、无标签搜索 |
| `123-tags-after.png` | **V1（弃用）** 标签板 `--rail`：太深；仅留作对照 |
| `124-tags-before-after.png` | 122 vs 123；V1 全页对照 |
| `125-tags-detail-board.png` | V1 标签板 1:1：标签板 rail；已弃用 |
| `126-tags-detail-rows.png` | 结果行 1:1：115 → 72px，标签并入日期行、去掉“未分类”；两版共用 |
| `127-tags-before-dark.png` | 改前暗色 |
| `128-tags-after-dark.png` | **V1（弃用）** 暗色标签板 rail |
| `129-tags-dark-before-after.png` | 127 vs 128；V1 暗色对照 |
| `130-tags-1040-after.png` | V1 1040（标签板 rail）；仅留作对照 |
| `131-tags-surface-after.png` | **V2 最终** 浅色：标签板与结果同为 surface，左栏 rail |
| `132-tags-surface-before-after.png` | 122 vs 131；V2 最终浅色对照 |
| `133-tags-surface-after-dark.png` | **V2 最终** 暗色 |
| `134-tags-surface-dark-before-after.png` | 127 vs 133；V2 最终暗色对照 |
| `135-tags-three-up.png` | 改前 / V1 rail / V2 surface 三态 |
| `136-tags-dark-three-up.png` | 改前暗 / V1 rail / V2 surface 三态 |
| `137-tags-1040-surface-after.png` | V2 1040×760：board 300 / main 460，无横向溢出 |
| `138-tags-detail-board-v2.png` | V2 标签板 1:1：surface + 搜索 + 21/261 + 折叠 |

备注：122/127 与 V2 131/133 之间点过一条结果，笔记从未读变已读，左栏“新”27→26；顶栏/结果仍 63。V1 把标签板压到 `--rail`，用户反馈侧边栏太深，V2 已退回 `--surface`。

## 侧栏 rail 提亮（2026-11-16 二次）

| 截图 | 内容 |
| --- | --- |
| `139-note-rail-before.png` | 笔记页旧 rail `#ece8de`；用临时样式回到旧 token，事实 tab，和 140 同状态 |
| `140-note-rail-paper.png` | 笔记页新 rail `#f4f1e8`（= paper），正文仍 `#fffdf8` |
| `141-note-rail-before-after.png` | 139 vs 140 全页并排 |
| `142-note-rail-detail.png` | 左栏 + 正文 1:1：米色侧栏 → 纸色侧栏 |
| `143-tags-rail-paper-after.png` | 标签页新 rail：左栏 paper，标签板/结果 surface |
| `144-tags-rail-before-after.png` | 131（rail #ece8de）vs 143（rail #f4f1e8） |
| `145-note-rail-dark-before.png` | 暗色旧 rail `#12100d` |
| `146-note-rail-dark-after.png` | 暗色新 rail `#1a1712` |
| `147-note-rail-dark-before-after.png` | 145 vs 146 暗色并排 |

备注：浅色 surface/rail 1.204→1.111；暗色 1.132→1.065；`audit-text` 笔记页 176 / 标签页 407，均 0/0/0。浅色侧栏与顶栏同色，边界靠 1px `--line`。

## 全页走查（2026-11-16）

报告：[`docs/PAGE_AUDIT_2026-11-16.md`](../../docs/PAGE_AUDIT_2026-11-16.md)；尺寸/计数：[`docs/page-audit-2026-11-16.json`](../../docs/page-audit-2026-11-16.json)；脚本 `tools/capture-page-audit.mjs`。1440×900 @1x，浅色（图谱页本身深色）。

| 截图 | 页面 | 一句话 |
| --- | --- | --- |
| `150-note.png` | 笔记（事实 tab） | 基准页；右栏事实 9、正文 702/18 |
| `151-graph.png` | 图谱 | 默认 54%，258 标签最小 9.5px；legend/filter 重复 |
| `152-tags.png` | 标签台 | board 340 / main 760；21 标签 + 264 属性；结果 64×72 |
| `153-facts.png` | 事实库 | 68 行；48 行重复“历史”；筛选墙 |
| `154-facts-curate.png` | 事实整理 | 0 空 section 在复合 2 / 无来源 4 前 |
| `155-fact-detail-1.png` | 事实详情（第一条） | h1 36；史料 0 在候选 9 前 |
| `156-fact-detail-general.png` | 事实详情（通用） | 说法不一致 vs 与事实不一致，两色 |
| `157-sources.png` | 史料库 | 1 条史料 + 399 待补录，动作重复 |
| `158-source-detail.png` | 史料详情 | 1 段原文 + 1 条事实；typo 待补补册页 |
| `159-sources-pending.png` | 待补录候选 | 399/96 组，首组 12，8 个黑色补录原文 |
| `160-production-home.png` | 制作台首页 | 两个黑色“新建制作”，62 重复 |
| `161-production-tasks.png` | 制作台任务 | nav/filter/rail 三处 62；标题截断 |
| `162-production-templates.png` | 制作台模板 | 3 卡占半屏；右侧大留白 |
| `163-production-prompts.png` | 制作台提示词 | 1 行内容 + 整页空白 |
| `164-production-subscriptions.png` | 制作台订阅 | 订阅 9 vs 1 行 UP；两张状态卡 |
| `165-production-settings.png` | 制作台设置 | 四列；引擎 rail 挤占表单 |

## 全页修正（2026-11-17）

报告：[`docs/PAGE_FIX_2026-11-17.md`](../../docs/PAGE_FIX_2026-11-17.md)。改后原图 `170-185`；逐页 before/after：

| 对比 | 页面与改动 |
| --- | --- |
| `190-note-topbar-before-after.png` | 顶栏计数：“64 篇笔记”→“笔记库 64 篇”，≤1180 隐藏 |
| `191-graph-before-after.png` | 范围分段；legend/filter 合并；标签 9.5→12px |
| `192-facts-before-after.png` | 去 48 个“历史”chip；状态筛选折叠；行尾来源计数 |
| `193-facts-curate-before-after.png` | 空 section 收一行；0 统计降权 |
| `194-fact-detail-1-before-after.png` | h1 36→28；候选优先并展开 |
| `195-fact-detail-general-before-after.png` | h1 28；冲突统一金色 |
| `196-sources-before-after.png` | 待补录 399 只出现一次 |
| `197-sources-pending-before-after.png` | 候选动作 outline；360→动态措辞；0 统计降权 |
| `203-source-detail-before-after.png` | 史料详情：header 1120→900 对齐、引文去重、12px |
| `198-production-home-before-after.png` | 单个“新建制作”；最近完成 3；rail 去 62 |
| `199-production-tasks-before-after.png` | 单个 CTA；标题 tooltip |
| `200-production-templates-before-after.png` | 3 卡单排；页头去 CTA |
| `201-production-prompts-before-after.png` | 增加自定义空态 |
| `202-production-settings-before-after.png` | 去引擎 rail；表单 800→1100；补 h1 |
| `205-tags-1040-final.png` | 1040 顶栏无溢出 |

## P2 收尾（2026-11-17 续）

| 截图 | 内容 |
| --- | --- |
| `207-settings-grouped-1440.png` | 设置 subnav 3 组；AI 面板顶部重复保存已删 |
| `208-subscriptions-count-1440.png` | 订阅标题「共 9 个」；DOM 9 行 |
| `209-tasks-density-1440.png` | 左栏任务徽标只显示进行中；来源列 tooltip |
| `210-settings-grouped-1040.png` | 1040：nav 150 / panel 852，无横向溢出 |
| `211-fact-status-pending-source-1440.png` | candidate 有 1 条 supports：`未核验 · 1 条来源待核验 · 看来源` |
| `212-tasks-1040.png` | 1040：63 行，来源省略但无横向溢出 |
| `213-subscriptions-1040.png` | 1040：订阅标题计数不换行、无横向溢出 |

备注：`audit-text` 设置 116 / 订阅 218 / 任务 633 / 事实详情 109 节点，浅/暗均 0/0/0；“订阅 9 vs 1 行”是首屏只截到第 1 行的误读，DB 和 DOM 都是 9。

## 知识库宽度统一（2026-11-17）

| 截图 | 内容 |
| --- | --- |
| `214-facts-width-1440.png` | 事实库 1120px；与史料库同宽 |
| `215-sources-width-1440.png` | 史料库 1120px；与事实库同宽 |
| `216-source-detail-width-1440.png` | 史料详情 980px；与事实详情同宽 |
| `217-fact-detail-width-1440.png` | 事实详情 980px；与史料详情同宽 |

备注：索引页统一 1120、阅读页统一 980、左右内边距统一 30；1660×1202 实测事实库/史料库头部 x=390/w=1120，两详情页 x=460/w=980，四页均无横向溢出。

## 四套主题与设置页（2026-11-17）

| 截图 | 内容 |
| --- | --- |
| `230-settings-neutral-light-1440.png` | 设置页：3 组模式、4 张固定预览主题卡 |
| `231-settings-warm-dark-1440.png` | 设置页 warm + 深色 |
| `233-note-<theme>-<mode>-1440.png` | 笔记页 4 主题 × 明暗（8 张） |
| `234-settings-neutral-1040.png` | 设置页 1040：单列收窄，无横向溢出 |
| `235-settings-neutral-600.png` | 设置页 600：分类与主题卡单列 |
| `240-facts-neutral-light.png` / `241-facts-noblue-dark.png` | 事实库：历史分类/选中筛选去金色 |
| `242-sources-neutral-light.png` / `243-sources-noblue-dark.png` | 史料库：来源行左线改中性 |
| `250-graph-neutral-light.png` | 图谱：深色画布，节点/边用主题 accent |
| `251-note-noblue-dark-relation.png` | 笔记局部关系图：陶土 accent |

备注：`audit-text` 笔记页 189、设置页 76、事实库 382 节点，四套主题 × 明暗全部 0 个 <12px、0 个低对比；设置页 1040/600 均无横向溢出。

## 右键菜单视口约束（2026-11-17）

| 截图 | 内容 |
| --- | --- |
| `260-context-menu-teleported.png` | 右键菜单挂到 body；靠近右缘时向左翻，快捷键列不再被侧栏裁掉 |

备注：`tools/verify-context-menu.mjs` PASS；菜单 `parentElement === body`，2051×1202 下 `left/right=1853/2043`、`top/bottom=218/357`，完整在视口内。

## 滚动条贴边（2026-11-17）

| 截图 | 内容 |
| --- | --- |
| `261-sidebar-scrollbar-right.png` | 侧栏 overlay 滚动条贴到侧栏右缘；thumb 距侧栏右缘 2px |
