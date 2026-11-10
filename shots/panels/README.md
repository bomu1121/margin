# 左右栏与浮层滚动条截图（2026-11-09）

设计/实测：[`docs/PANEL_INTERACTION.md`](../../docs/PANEL_INTERACTION.md)

| 截图 | 内容 |
| --- | --- |
| [01-scrollbar-hover.png](01-scrollbar-hover.png) | 光标靠近左侧笔记树右缘，浮层 thumb 淡入；未靠近时不显示 |
| [02-doc-scrollbar-hover.png](02-doc-scrollbar-hover.png) | 正文右缘的浮层 thumb；右栏边界保持干净 |
| [03-resized-panels.png](03-resized-panels.png) | 左栏 240→300、右栏 380→430 后，正文仍可读 |
| [04-sidebar-collapsed.png](04-sidebar-collapsed.png) | 左栏收起，正文占满；顶栏开关保留 |
| [05-rail-collapsed.png](05-rail-collapsed.png) | 右栏收起为 28px 边缘条，正文变宽 |
| [07-note-1040.png](07-note-1040.png) | 1040×760：顶栏不溢出，左右栏与正文都在 |

## 实测

- 左栏拖动 240→300，localStorage 写入 300；
- 左栏收起后 `.main` 1518px，正文可见；
- 右栏拖动 380→430，localStorage 写入 430；
- 右栏收起/展开通过；
- 滚动条 hover 出现，900ms 后淡出；快速划过不出现；
- thumb 拖动让笔记树 scrollTop 0→120；
- 滚动条可见时右栏 380→475，浮层从 1131 跟到 1031，没有旧位置残留；
- 包 8,722,874 B（8.32 MiB）/ exe 21,914,112 B（20.90 MiB）。

## 边界

- 没有真实触屏验收；
- 横向滚动没有自定义浮层；
- 浮层只覆盖已列出的滚动容器选择器；
- 1040 只验了笔记页，没有逐页重拍事实/史料/队列的新宽度组合。
