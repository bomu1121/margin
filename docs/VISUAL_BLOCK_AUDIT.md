# 视觉块回归测试（可复用）

> 适用：改 `note_engine.rs` 的视觉提示词、`src/lib/visualBlock.ts`、`markdown.ts` / `richText.ts`、主题变量、搜索/字数时。
> 第一次成文：2026-11-20（P159），5 份现成转写稿 + 1 次重跑，发现“720px 设计宽度”错误。

## 1. 素材：用现成转写，不重新转写

```text
sample-vault/.yebian/runs/<runId>/transcribe/transcript-01.md
```

- 按 CJK 选不同长度：约 1.9k / 2.3k / 3.3k / 6.5k。
- 题材尽量分开（科技解释、新闻、经济数据、面试口语、概念对比）。
- brief 放 **`%TEMP%/visual-briefs/*.md`**，不要放 vault，否则会被当成笔记索引。
- 默认行为：brief 只写“把素材写成一篇给同事看的笔记。”
- 点状测试：brief 明确说“画一张时间线 / 排名图 / 四路线对比 / 2×2 卡片”。

## 2. 跑

```bash
NOTE_TASK="$TEMP/visual-briefs/<brief>.md" \
  node --no-warnings tools/probe-ai-note.mjs \
  "sample-vault/.yebian/runs/<runId>/transcribe/transcript-01.md" \
  "visual-<label>" "$TEMP/visual-runs/<label>"
```

- 每个 label 一个任务；结果落 `待整理`。
- 5 份约 3–5 分钟；单份 15–60 秒，长稿更久。
- probe 输出的 `visuals` 是 ` ```html ` 围栏数，不再是旧 `figure`。

## 3. 找任务和笔记

```js
const jobs = await window.__TAURI_INTERNALS__.invoke("production_list")
jobs.filter(j => j.title.startsWith("AI 成稿对照 · visual-"))
// 取 j.noteId / j.noteRelPath
```

也可以 `list_notes()` 按标题找。注意：同一来源重跑可能复用同一个 noteId、覆盖原笔记；测试时先记下路径。

## 4. 量（CDP 9222）

1440×900 下：

- 笔记正文列宽 **575px**；任务弹窗结果列 680px（iframe `max-width:600px`）。
- 1024 模拟笔记列只有 **346px**（低于 app `minWidth:1040`，只作边界参考）。
- 每个 `iframe.yb-visual-frame` 读：
  - `data-yb-visual-height`：内容高度
  - `data-yb-visual-overflow`：`1` = 横向溢出
  - `data-yb-visual-width`：`scrollWidth/clientWidth`
  - `data-yb-visual-clipped`：高度超过 4000px 被截
- 从 `srcdoc` 里取 `<div class="yb-visual">` 到 `</div><script>` 之间的片段，检查：
  - `#hex` / `rgb(` / `hsl(` 数量（中性色应被归一；强调色可以保留）
  - `position: fixed/sticky`、`100vh`、`@import`
  - `<a` / `<img` / `<button` / `<table` / `<svg` / `<style`
- 截图：对每个 frame `scrollIntoView({block:"center"})`，浅色 + 深色各一张；1024 只截最高/最窄的块。

**红灯**：

- `overflow=1` 或 `clipped=1`
- 高度 > 1000px（不一定错，但要看是否该拆）
- SVG `viewBox` 宽 > 700 且字号 < 12（575 宽下会缩到看不清）
- 硬编码中性色（尤其 `rgb(255,255,255)`/`#fff`/浅灰背景）
- 两列卡片行里只剩一张卡（2+1 空洞）
- 明确的链接/图片/按钮（会被 sanitizer 去掉，视觉上缺一块）

## 5. 判

- “好不好看”交给独立评审读 PNG，不给自己的输出打分。
- 区分 **必须修** 和 **建议**；必须修修完要复检。
- 默认任务应该能自然出图，但不要为了配图而配图。
- 结构问题优先改 **设计系统/渲染器**，不要又往提示词里塞“3 卡/4 卡必须几列”这种特例。

## 6. 清理

1. `production_delete` 所有 `AI 成稿对照 · visual-*` 任务。
2. 删除笔记文件：`sample-vault/待整理/<标题>.md`。
3. `reindex`；用 `list_notes` 搜测试标题，确认 `testLeft=[]`。
4. 截图保留在 `shots/audit/`，测试笔记不要留。

## 7. 当前基线（2026-11-20，P159）

- 5 份转写稿：default 2,397 CJK/2 图/10 表；gdp 1,084/1 图；timeline 1,635/1 图；interview 2,697/1 图/19 表；capitalism 1,883/1 图。
- 全部 success；1440 无横向溢出/裁切；默认任务也主动出 2 图。
- 已修：提示词设计宽度 720→575；`rgb()` 近灰度归一；iframe `resize` 重算；任务弹窗 `max-width:600px`。
- 已知未修：3 卡 2+1 空洞；`rgb()` 强调色不归一（深色脊线偏亮）；1024 笔记列 346px（低于 minWidth，不是真实窗口）；打印/其他主题未验收；安装包未重打。
- 证据：`docs/experiment-log.txt` P159、`docs/measured.json` 的 `visualBlock20261120`；截图 `shots/audit/540-555`。
