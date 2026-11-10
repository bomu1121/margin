# 侧栏搜索与结果面板（2026-11-09）

> 目标：一屏只留一个搜索入口；结果不再用 380px 小下拉，而是 Obsidian 式的侧栏结果面板。
> 范围：全局搜索入口、侧栏搜索框、结果列表、`Ctrl+K` 聚焦、空态。不包含全文高亮/正则/筛选语法。

## 1. 问题

用户截图里一屏同时有两个搜索框：

1. 顶栏 `搜索笔记 / 标签 / 人物…`：全局 FTS，结果是一个 380px 下拉，行里只有领域 + 标题。
2. 左侧栏 `筛选笔记…`：只在当前树里按标题/路径过滤；focus 后出现黑色实心圆点 + 粗蓝双环。

问题不是“搜索功能重复”这么简单：两个入口的**结果形态**也不一致，一个下拉、一个过滤树，用户得先判断该用哪一个。侧栏输入框的 active 态尤其明显：搜索图标没有 `fill:none`，默认填充成黑点；focus 同时设置 `border-color` + `outline 2px` + `outline-offset:1px`，变成双层粗环。

## 2. 设计

### 2.1 只留侧栏一个入口

- 删除顶栏 `.search-wrap` 和它的下拉结果。
- 侧栏输入框改成 `搜索笔记…`，成为唯一的搜索入口。
- `Ctrl+K` 不再聚焦顶栏，而是向 `NoteList` 派发 `yb-focus-search`，聚焦侧栏输入框并全选。
- 应用最小窗口宽度是 1040，侧栏在支持范围内始终可见；不存在“窄窗搜索入口消失”的回归。

### 2.2 搜索走现有 FTS，不再只过滤树

- 输入 140ms 防抖后调用 `api.searchNotes(text)`，复用已有 FTS（标题/正文/标签等）。
- 有 query 时：隐藏收藏和文件夹树，显示结果面板。
- 清空 query：结果消失，恢复原来的树和折叠状态。
- 结果行：标题（13.5px serif）+ 路径（11.5px sans）；标题/路径都不含 query 时标 `正文命中`，避免“这条为什么会出现”的困惑。

### 2.3 Obsidian 式结果面板

```text
[搜索图标] 特朗普                          [清除]
6 篇笔记
中期选举零分交卷：伊朗和乌克兰都要为特朗普的失败负责
时事/中期选举零分交卷：….md
特朗普中期选举惨败：通胀、ICE与烂候选人如何把深红州翻蓝
时事/特朗普中期选举惨败：….md
民主党大胜中期选举，然后呢？
正文命中 · 时事/民主党大胜中期选举，然后呢？.md
```

- 结果计数在头部；清除只保留输入框右侧的 X，头部不再重复「清除」。
- 结果行两行：标题 + 路径；`正文命中` 用金色小字，和标题/路径区分。
- 结果列表在原来的 `.tree-scroll` 容器里滚动，不另起滚动条。
- 点击结果打开笔记；query 保留，方便连续打开多条结果。

### 2.4 输入框样式

- 高度 30px，背景 `--surface`，1px `--line` 边框，5px 圆角。
- 搜索图标 14px，显式 `fill:none; stroke:currentColor; stroke-width:1.6`，不再出现黑点。
- focus：`outline:none; border-color:var(--blue); box-shadow:0 0 0 2px var(--blue-soft)`。只有一层轻环，不再是 border + outline 双环。
- hover 只加深边框；清除按钮 24×24，键盘可达。

### 2.5 顺手修掉窄窗标题裁切

1040 下长标题的第二个语义块（如「伊朗和乌克兰都要为特朗普的失败负责」）超过正文宽度，原来 `white-space:nowrap` 会直接溢出到右栏并被裁掉。现在每个标题块 `min-width:0; white-space:normal`：能放下时仍按标点分块，放不下时在块内换行，不再横向溢出。

## 3. 实测

- 搜索 `特朗普`：6 篇笔记；结果顺序与 FTS 返回一致；其中 3 条标题不含 query，显示 `正文命中`。
- 顶栏 `.search-wrap` 数量为 **0**；侧栏 `.side-filter` 是唯一搜索输入。
- 搜索图标 computed `fill: none`，`stroke: rgb(100, 108, 119)`。
- focus computed `outline: none`，`box-shadow: rgb(238, 242, 255) 0 0 0 2px`。
- `Ctrl+K` 后 `document.activeElement` 是 `.side-filter`；侧栏收起时先展开再聚焦（等待一个 tick）；`Esc` 清空 query，树恢复。
- 点击第一条结果：笔记标题切到「中期选举零分交卷…」，query 仍为 `特朗普`，结果仍 6 条。
- 1040 下长标题不再被右栏裁切；标题变成 3 行，最后一行「失败负责」。
- `cargo test --lib` 165 passed / 7 ignored；`npm run tauri build` 通过；安装包 8,714,113 B / exe 21,882,880 B。

## 4. 诚实清单

- 结果只有标题 + 路径 + `正文命中` 标记，**没有正文片段/高亮**；要显示片段需要后端 FTS snippet，这轮没做。
- 没有大小写、正则、标签/路径过滤语法；没有搜索历史/保存查询。
- 只测了约 30 篇笔记的样例库；没有测 5 万篇下的结果滚动性能。
- 没有复检暗色、125%/150% 缩放、真实触屏。
- `Ctrl+K` 只在 `NoteList` 挂载时有效（制作台侧栏不是 NoteList）；制作台内没有搜索。
- 标题块内换行后不再保证每行都是完整语义；只保证不裁切、不出现单字孤行。

## 5. 相关文件

- `src/components/NoteList.vue`：query 防抖、FTS 结果、结果面板、Ctrl+K。
- `src/App.vue`：删除顶栏搜索，`Ctrl+K` 派发 `yb-focus-search`。
- `src/styles/sidebar.css`：输入框、focus、结果行、`正文命中`。
- `src/styles/panels.css`：长标题块允许内部换行。
- `shots/audit/44-search-default.png`、`45-search-active.png`、`46-search-results.png`、`47-search-1040.png`、`48-search-1440.png`。
