# 标注系统重做：调研、交互方案与三个 demo（2026-11-10）

> 结论先说：当前标注不是“体验不够好”，是交互模型选错了——它把每个正文块当成一个独立的“可标注单元”，在每块右侧放 `+`，点击后把人送到右侧栏写一张表单。读到哪、想标哪，和“入口在哪、表单在哪”是分离的。
>
> 建议采用 **“选区即动作，页边即结果”**：拖选文字后才出现动作；高亮直接落；带笔记时在页边写，保存后成为页边卡片。三个 demo 已做，A 已选定并落地 M1（见 §7）。

## 1. 当前实现审计

| 位置 | 现状 | 问题 |
| --- | --- | --- |
| `NoteBlock.vue` | 每个 `.unit` 右侧一个 `.add`（`+`），hover 出现 | 每一段都多一个入口；用户先找 `+`，再回右侧写，动作不在注意力处 |
| `NoteBlock.vue` | 点击 `+` 传 `block.id` | 只能标整段，无法标一句话/一个词；引用是整段 `block.text` |
| `NoteView.vue` | `composer` 固定在右栏「标注」tab | 选中文字后没有就地动作；眼睛要从正文跳到右栏 |
| `NoteView` | `markMap` 每个 block 只对应一个 mark | 同一段不能有多条独立标注；标注重叠/多段选择没有模型 |
| `NoteBlock` | 标注高亮只是一个数字 mark，不是文本范围 | 用户看不出“到底标的是哪几个字” |
| 数据 | `Annotation { blockId, quote }`，`quote` 是整块 | 没有字符偏移，无法精确恢复一句话的选区 |

这不是“再加一个颜色选择器”能解决的；需要把**选择范围**变成一等公民。

## 2. 调研：成熟产品怎么做

| 产品/方案 | 交互事实 | 可借用 | 不照搬 |
| --- | --- | --- | --- |
| Readwise Reader | 键盘上下移动段落焦点，`H` 高亮、`N` 笔记；宽屏时注释作为 **Tufte 式页边**出现；高亮颜色有语义（黄/珊瑚/蓝/灰） | 页边留白、键盘流、颜色有含义、默认高亮 | 段落级焦点仍是“单元”；我们优先文本选区 |
| Hypothesis | 选中文字 → 选区上方小工具条（Annotate / Highlight）；右侧栏卡片与正文位置对应；点高亮打开卡片 | 选区工具条、点高亮回看、高亮与卡片双向定位 | 全量侧栏在长文里会抢注意力；用户抱怨“点一下整栏都跳” |
| Google Docs | 选中文字 → 浮动“添加评论”；选中范围高亮；评论线程在右侧页边与文本对齐 | 选区→评论、持久高亮、页边线程、resolve 状态 | 正式评审批注感太重；我们需要更轻的“读书记号” |
| Obsidian 标注插件（Marginalia MD / Note Annotations / Reader Margins / Marglow） | 选中文字高亮，评论以页边卡/悬浮编辑器出现；窄栏自动降级；点高亮编辑 | 文本范围高亮、页边卡、窄栏降级、点击编辑 | 插件多依赖 Markdown 语法/侧车文件；我们的存储已是 SQLite |
| Tufte sidenotes | 注释放在页边，读者用余光判断是否要看；不打断正文 | 页边是“第二阅读通道”，不是表单面板 | 纯静态 sidenote 不解决“怎么创建” |
| 设计原则（多篇 UX 分析） | 选区在操作期间必须保持可见；动作不能盖住选区；先用最轻的 affordance；颜色若要分类必须命名；Esc/Cancel/保存的边界要明确 | 浮动工具条不遮选区、渐进披露、键盘可达 | 不要为了“创新”隐藏基本可发现性 |

参考：
- [Readwise: Highlights, Tags, and Notes](https://docs.readwise.io/reader/docs/faqs/highlights-tags-notes)
- [Andy Matuschak: Few digital reading environments support marginal notes](https://notes.andymatuschak.org/zGsRWkonFv1KGAsWwiYA3he)
- [Hypothesis annotation guide](http://experiments.coara.org/hypothesis-annotation-guide.html)
- [Hypothesis keyboard-only annotation](https://dev-web.hypothes.is/help/creating-annotations-using-only-your-keyboard/)
- [Hypothesis UX backlog #446](https://github.com/hypothesis/product-backlog/issues/446)
- [Tufte CSS](https://edwardtufte.github.io/tufte-css/)

### 调研收敛出的 7 条原则

1. **动作跟着选区走**：先有文本范围，再有工具；没有选区时不存在“标注入口”。
2. **选区不被遮挡**：工具条在选区上方/下方翻转；输入框不能盖住正在写的句子。
3. **默认动作要一键完成**：高亮不该先选颜色、再选类型、再保存；默认色 + Enter 足够。
4. **笔记是可选加深**：只想要记号的人不应被迫写正文；写笔记是渐进披露。
5. **结果留在页边**：保存后不要跳走；卡片与原文段落同高，读者用余光看到。
6. **颜色不是装饰**：若要颜色分类，给名字/含义；否则统一墨色更安静。
7. **锚点必须可恢复**：存 `blockId + start/end + quote`，重排/重生成时能逐字校验；跨段选择要存多段锚点。

## 3. 三个 demo

会话可视化目录：`annotation-lab.html`（可直接在对话里点）。

### A. Margin ink（页边落笔）

- 拖选文字 → 页边出现一张临时卡片；输入框已聚焦，直接写。生产 M1 用右侧栏作为页边（同一列、同一套卡片），没有真实排到段落同高；demo 里是理想形态。
- 三个颜色点：点颜色立即保存为“只高亮”；输入 + Enter 保存为带笔记。
- 保存后卡片留在页边，和段落同高，有一条细线连回正文；正文是淡墨高亮。
- 没有 `+`、没有独立“标注单元”、没有右栏表单。
- 适合：想把标注当“读书记号 + 页边批注”的用户；最接近纸质书。

### B. Underline studio（选区工具条 + 文内落笔）

- 拖选文字 → 选区上方出现一个小胶囊：3 个颜色点 + “写笔记”。
- 点颜色 = 立即高亮；点“写笔记” = 在选区旁弹出输入框，`Enter` 保存。
- 保存后的卡片仍落在页边（和 A 共用页边层），所以不是“写完就丢进右栏”。
- 适合：快速高亮为主、偶尔写一句的用户；动作路径最短。

### C. Ink thread（页边线/点，最安静）

- 拖选文字 → 不出现工具条；选区到页边拉出一条细线/一个点，页边出现一条极窄的批注线。
- 输入框就在页边线上；颜色/类型是三个小字按钮，不抢视线。
- 已保存的标注在页边成为“点 + 细线 + 一段小字”，hover/点击才展开全文。
- 适合：长文深度阅读、希望正文几乎无干扰的用户；可发现性最低，需要键盘提示。

## 4. 推荐：B 的动作 + A 的结果

不是三选一。推荐把两者合成：

1. **选区上方**出现极轻工具条（B）：默认色 + “写笔记” + Esc。
2. 点颜色 → 立刻高亮，不打开任何面板。
3. 点“写笔记” → **就在选区旁**写（B），不跳右栏；`Enter` 保存。
4. 保存后 → **页边卡片与段落对齐**（A），不是右栏列表；点卡片回到原文。

这样同时满足：
- 所过即所求：动作在选区；
- 结果在余光：卡片在页边；
- 不强迫单元：选区可以是一句话、一个词，甚至跨段；
- 不增加入口：没有选区就没有按钮。

C 作为“专注阅读模式”的候选：可在设置里切换“页边显示卡片 / 只显示点线”。

## 5. 生产实现方案（选定后执行）

### 5.1 数据模型

```ts
interface AnnotationSegment {
  blockId: string
  start: number   // 块内纯文本 UTF-16 偏移
  end: number
}
interface Annotation {
  id: string
  noteId: string
  segments: AnnotationSegment[]  // 跨段选择 = 多段
  quote: string                 // 逐字原文，用于校验/重定位
  body: string
  kind: '标记' | '存疑' | '待办'
  color: 'ink' | 'gold' | 'blue' | 'rose'
  createdAt: number
}
```

- SQLite `annotations` 增 `segments_json` 与 `color`；旧数据迁移为 `[{blockId, start:0, end: blockText.length}]`。
- 保留 `quote` 和 `blockId` 兼容旧 API；`quote` 是锚点校验的最终依据。

### 5.2 选区 → 锚点

- `mouseup`/`keyup` 后读 `Selection.getRangeAt(0)`；按块计算 `start/end`（TreeWalker 统计文本节点）。
- 跨段选择生成多个 segment；单段则一个。
- 保存前检查与已有 segment 是否重叠；重叠时提示“并入已有标注/换一段”，不静默覆盖。
- 重排/重生成时用 `quote` + `blockId` 逐字校验；失败进入“锚点失效”区，不静默挂错。

### 5.3 渲染与点击（M1 实际实现）

- `Range` 按 `blockId + start/end` 精确映射；用 `splitText` 把目标文字包进 `<mark class="anno-hl">`，`data-anno` 支持点击回看。**实际没有用 CSS Custom Highlight**：本机 WebView2 里 `CSS.highlights` 存在但 `::highlight` 不上色，已改为 mark 包裹并保留清晰的高亮语义。
- 点高亮 → 选中对应页边卡片并滚动到原文；点卡片 → 滚回原文并加深高亮。`caretPositionFromPoint` 作为无 mark 的兜底。
- 页边卡片按 `blockOrder` 排序、编号；M1 在右侧栏内渲染，未做段落同高绝对定位。窄屏仍是同一套 rail 卡片。

### 5.4 交互状态

- `idle`：只有正文。
- `selected`：工具条/页边临时卡；选区保持可见；`Esc` 取消。
- `composing`：输入框聚焦；`Enter` 保存，`Shift+Enter` 换行，`Esc` 取消。
- `saved`：页边卡片 + 正文高亮；点卡片进入 `editing`，可改正文/类型/颜色/删除。
- 键盘：`H` 默认高亮，`N` 写笔记，`Esc` 取消，`Cmd/Ctrl+Enter` 保存；焦点不离开选区上下文。**这是设计目标，不是全部落地**；A 的输入框在选区后已经聚焦，`H/N` 会和输入字母冲突，M2 明确不采用，实际只有 `Enter` / `Esc`。

### 5.5 删除/兼容

- 删除旧 `NoteBlock` 的 `.add` 按钮和 `@annotate` 事件；旧 `startAnnotate` 改为“接收一个选区”。
- 右栏「标注」tab 只保留索引/筛选/失效区，不再放 composer；避免表单与页边重复。
- 旧标注全部迁移；迁移脚本在测试库先跑，校验 quote 逐字一致。

## 6. Demo 的边界

- Demo 是交互原型，不是生产实现；标注只存在内存，刷新丢失。
- Demo 的跨段选择已支持多 segment 保存；真实鼠标选区/键盘选区还要做边界测试。
- 颜色目前是视觉分类，没有给颜色命名；生产版要么命名（黄=重要/蓝=定义/珊瑚=反驳），要么只用墨色。
- 没有移动端布局细化；窄屏在 demo 里降级为卡片列表。
- 没有实现 anchor relocation/失败区；生产版必须复用现有标注重定位。

## 7. 已落地：A · M1

用户选定 A。当前生产实现：

- `NoteBlock.vue` 删掉每段 `.add` 和 `@annotate`；正文只剩 `.unit[data-block]`。
- 拖选正文（支持跨段）→ 右栏「标注」出现临时卡片：引文、3 个颜色、标记/存疑/待办、单行输入；`Enter` 保存，`Esc` 取消。
- 保存写 `segments`（blockId + UTF-16 start/end）和 `color`；正文用 `<mark class="anno-hl is-color" data-anno>` 精确高亮，点高亮回看。
- 打开有标注的笔记自动切到「标注」；卡片按正文顺序编号，点卡片定位原文。
- 旧标注 `segments_json=[]` 时回退到整块高亮；`relocate_annotations` 会同步改 `segments_json` 的 blockId。
- 回归（M1 交付时）：`cargo test --lib` 167 passed / 7 ignored；`npm run build` / `npm run tauri build` 通过；包 8,722,874 B（8.32 MiB）/ exe 21,914,112 B（20.90 MiB）。
- 截图：`shots/audit/58-annotation-default-margin.png`、`60-annotation-selection-composer.png`。

### M1 的边界（M2 已解决其中两项）

- 右侧栏是页边，但卡片**没有排到与段落同高**；这是 M2 的布局项，**M2 已完成**。
- 没有 hover 预览、没有键盘 `H/N`、没有多选/批量；**hover 预览 M2 已做**；`H/N` 因与输入模型冲突明确不做；移动端选区没有逐项验收。
- 跨段标注已能保存多个 segment，但页边卡片只按第一段定位；跨段高亮/卡片定位需要再验。
- 旧标注的整块回退是迁移兜底，不是精确范围。
- 颜色尚无命名语义；删除是直接删除，没有撤销。

## 8. 已落地：A · M2 右栏即页边（2026-11-11）

第一版 M2 做错了：在正文右侧又开了 200px 页边列，同时保留 378px 右栏。结果正文列从 764 缩到 649，右栏只剩一句“标注已排到页边”的空话。用户指出“可视区域变小、右侧空缺”——这个判断成立。

现在改成：**不新开页边列，直接把右栏「标注」tab 当页边。**

- 删除 `.doc-margin` 第二列；正文恢复单列，`.note-layout .doc` 的最大宽度从 860 恢复为 1000。1662 窗口实际 `.doc` 961、正文列 865（上一版 649）。
- 右栏「标注」tab 变成页边画布：`.rail-body` 在 notes tab 下 `position:relative; overflow:hidden`，卡片/临时卡绝对定位在 `.rail-anno-layer` 里。
- 卡片 top = 选区首行的 `Range.getBoundingClientRect().top` − 右栏 body 的 top；同一段多条按 12px 依次下推。滚动正文时用 `requestAnimationFrame` 逐帧重排，卡片和正文同一行。
- 鼠标滚轮在页边卡片区域滚动的是正文（实测 deltaY 120 → docScroll 120），不是滚一个看起来空着的右栏。
- 右栏收起/展开、拖宽后由 `ResizeObserver` 重排；收起后正文可以到 1000（正文列 904），展开后卡片回到原位。
- 卡片宽度跟着右栏：1662 时 345，1500 时 345，1040 时 297；不再是上一版挤在 185px 里。
- 临时卡会被底部裁切时，`revealComposer()` 把正文向下滚所需的 56px，让输入框完整出现；实测 composer bottom 1188 < body bottom 1204。
- 锚点失效的卡片收进右栏底部抽屉（`max-height:50%` 内部滚动），不参与锚定。
- hover 双向联动保留：悬停卡片 → 对应正文高亮加轮廓；悬停正文高亮 → 对应卡片加边框。

实测（安装版，CDP）：

| 窗口 | 正文容器 / 正文列 / 右栏 / 卡片宽 | 卡片 top vs 高亮 top |
| --- | --- | --- |
| 1662×1204 | 961 / 865 / 378 / 345 | 744/744、955/955 |
| 1500×900（模拟） | 799 / 703 / 378 / 345 | 941/941、1143/1144（±1px 取整） |
| 1040×760（模拟） | 430 / 362 / 330 / 297 | 1230/1231、1534/1535（±1px 取整） |

其他实测：

- 选中“青黄不接”：composer top 709 = 选区 top 709，输入框已聚焦。
- 选中页面底部的“对官员来说”：正文自动滚 56px，composer bottom 1188 < body bottom 1204，完整可见。
- 保存/删除：2 条 → 3 条 → 2 条；新增卡片 top 1039，对应高亮 top 1040（自动滚 56px 后）。
- 正文滚 200px：卡片 top 744 → 544，同步 −200px。
- 滚轮在页边滚动：docScroll 0 → 120。
- 右栏收起：doc 1000 / 正文列 904；展开后卡片回到 744/955。
- 1662 / 1500 / 1040 三档 `document.documentElement.scrollWidth <= innerWidth`，无横向溢出。
- hover：卡片→高亮、高亮→卡片都命中。

回归：`cargo test --lib` 167 passed / 7 ignored；`npm run build` / `npm run tauri build` 通过；包 8,725,071 B（8.32 MiB）/ exe 21,914,112 B（20.90 MiB）。截图：`shots/audit/70-annotation-cards-compact.png`、`75-annotation-composer-refined.png`。

### M2 的边界与刻意没做

- **不做全局 `H/N`**：A 在拖选后直接聚焦输入框，`H/N` 会和用户输入的字母冲突。Readwise 的 `H/N` 建立在“段落焦点”模型上，和我们的“文本选区”模型不兼容；保留 `Enter` 保存、`Esc` 取消。
- **颜色点是预选，不是“点一下立即入库”**：避免误触写库；只留高亮时直接 `Enter`（输入框为空）也是一键。
- 删除没有撤销；锚点失效仍收在右栏底部抽屉。
- 跨段标注卡片只按第一段定位；正文高亮跨段正常。
- 只验了桌面鼠标选区；触屏/移动端未验。
- 右栏是固定宽度面板：标注很少时，卡片只出现在对应行，下面仍是空的——这是页边，不是列表；可以用右栏拖宽/收起调整，收起后正文列可到 904。
- 页面底部选区的自动滚动会移动 56px，取消标注后不会自动滚回原位。
- 没有额外连接线；右栏自身的左边框就是页边边界。

## 9. 已落地：M3 去掉类型标签，压缩卡片（2026-11-11）

用户明确不用“标记 / 存疑 / 待办”，而且这三个按钮让临时卡多一行、在 345px 里很挤。处理：

- 从 `AnnotationComposer` 和 `AnnotationCard` 删除类型选择/显示；`Annotation.kind` 字段保留（旧数据兼容），新标注写 `kind: ''`，不再显示。没有做数据库迁移，也不清空旧 kind。
- 卡片从“顶部类型行 + 引文 + 正文 + 底部动作”改成“引文 + 正文 + 一行元信息”：左侧 `编号 · 时间`，右侧 `删除`；整张卡片点击定位，`定位` 按钮删除。没有顶部行，没有 kind。实测无正文卡高 **73**、带正文 **97**（上一版 99/125）。
- 临时卡从“引文 + 颜色/类型行 + 输入 + 底栏”改成“引文 + 输入 + 底栏（颜色 / Enter·Esc / 保存）”。实测宽度 345 时高 **115**，底部颜色点、提示和保存按钮同一行。
- 引文仍是左侧 2px 蓝线、12px serif；正文 13px sans；元信息与动作 10.5/11px。层级只靠字号、字体和一条细线，不再靠类型色。
- 编号改为按正文顺序（block order + 同块 segment start），不再只按 block；截图时 3 条标注在页边从上到下就是 01 / 02 / 03。
- 验证：页面 DOM 中“标记 / 存疑 / 待办”文本 **0 处**；截图时 3 张卡片没有类型节点；临时卡 top 709 = 选区 top 709，输入框聚焦。底部选区自动滚动从 56px 降到 **22px**（临时卡变矮），composer bottom 1188 < body bottom 1204。
- 回归：`cargo test --lib` 167 passed / 7 ignored；`npm run build` / `npm run tauri build` 通过；包 8,725,071 B（8.32 MiB）/ exe 21,914,112 B（20.90 MiB）。截图：`shots/audit/70-annotation-cards-compact.png`、`75-annotation-composer-refined.png`。

### M3 的边界

- 旧标注的 `kind` 仍在 SQLite 里，只是 UI 不展示；以后真要删列需要迁移。
- 同一段有多条标注时，卡片按 12px 下推；当前数据里同段第 2 条把第 1 条推后了 74px，独立行仍精确对齐（955/955）。
- 删除仍无撤销；颜色只是视觉分类，没有命名语义。

## 10. 视觉打磨：从“选中框”到“页边墨迹”（2026-11-11）

用户指出四个问题：提示文字、点击效果、段落上的深蓝外框、标记上的浅蓝框。功能不动，只重做视觉。

### 调研

- **WCAG / Gutenberg 高亮讨论**：高亮底色保持固定低透明度；hover / focus / active 用不透明下划线表达，而不是把底色加深。加深底色会降低文字与背景的对比度，Gutenberg 的实测里 4.54:1 的正文在 tint 加深后掉到 2.67:1。[Gutenberg PR #80544](https://github.com/WordPress/gutenberg/pull/80544)、[WCAG G183](https://www.w3.org/WAI/WCAG22/Techniques/general/G183)
- **Readium / Liseur**：填充式高亮和 underline 是两种合法语义；underline 更安静，尤其暗色主题下“填色是页面上最响的东西”。[Readium changelog](https://pub.dev/packages/flutter_readium/changelog)、[Liseur ADR 0008](https://github.com/chmouel/liseur/blob/8e9ca4d2931d72d38d5da79f12568aebd591e109/docs/adr/0008-highlight-styles.md)
- **Hypothesis**：标注的 focus affordance 本来就该克制；键盘 `:focus-visible` 才需要明确 focus ring，鼠标选中不需要大外框。[Hypothesis issue #2527](https://github.com/hypothesis/client/issues/2527)
- **Readwise Reader**：暖色荧光笔 + 页边注；颜色是纸上的记号，不是 UI 控件的高亮边框。[Highlights, Tags, and Notes](https://docs.readwise.io/reader/docs/faqs/highlights-tags-notes)

### 落地

- **标记**：底色固定 `18%`（gold）/ `14%`（blue/rose），active **不加深底色**；hover 只出现 58% 同色下划线，active 变成 1.5px、72% 同色下划线；删掉所有 outline。跨行标记用 `box-decoration-break: clone`。
- **待保存标记**：虚线金色下划线，和已保存的实线下划线区分。
- **点击定位**：删除整段深蓝 outline，改为段落左侧 2px 金色竖线，1.2s 淡出；只有区块不在视口舒适区（上下各 72px）才滚动，目标位置是视口 28%，不再强行居中。
- **卡片 active**：不再蓝底蓝边；用标注色的 6% 暖底 + 边框 + 2px 内嵌左线；hover 2.5% 暖底。编号用 82% 标注色 + 18% ink（gold 在暖底上对比度 5.35），正文/时间/操作保持 ink-2。
- **提示**：编号/时间/操作 11px、tabular-nums；操作 hover 才出现下划线，键盘 focus 用金色 outline；临时卡提示从 ink-3 提到 ink-2。
- **临时卡**：整体改成暖金纸片（gold-line 边框），input focus 用 3px gold-soft 光晕；颜色点选中的环用自身颜色，不再用灰色 radio ring；空态去掉虚线框。

### 实测

- active 与 inactive 标记的 background 计算值完全相同（gold 18%）；active 的 `outline` 为 `none`，只有 `text-decoration` 1.5px。
- 文字对比度：ink 在 gold/blue/rose 标记底上 **13.46 / 13.48 / 13.29**；卡片正文 ink-2 在 6% 暖底上 **7.44**；提示 ink-2 在 paper 上 **7.46**；编号 5.35。
- 点击已可见的卡片：`scrollTop` 保持 **0**；滚到 900 后点同一张卡：**900 → 325**（目标 28%），flash 竖线存在。
- 页面文本 `标记 / 存疑 / 待办`：**0 处**。
- 回归：`cargo test --lib` 167 passed / 7 ignored；`npm run build` / `npm run tauri build` 通过；包 8,725,071 B（8.32 MiB）/ exe 21,914,112 B（20.90 MiB）。截图：`shots/audit/73-annotation-active.png`、`74-annotation-flash.png`、`75-annotation-composer-refined.png`。

### 边界

- 暗色模式没有逐项截屏验收；`color-mix` 在 WebView2 157 上可用，但暗色下的 tint 对比度只做了计算，没有真机目检。
- 没做色盲模拟/Windows 高对比模式。
- 标记和卡片仍不是键盘可聚焦元素；只有临时卡 input 有 `:focus`，操作按钮有 `:focus-visible`。
- 下划线依赖字体 metrics；不同字体下 offset 可能差 1px。
- 减少动效时 flash 竖线改为静态显示 1.2s，不播放 scale/fade。

## 11. M5 删除保护（2026-11-12）

标注删除原来是一击即删，误触就会丢标注。补了一个轻量二次确认：

- 第一次点「删除」只进入待确认，卡片底部变为「删除这条标注？」+「取消 / 确认删除」；4 秒不操作自动复位。
- 第二次点「确认删除」才调用删除；「取消」立即恢复。
- 锚点失效卡片的删除同样走两步。
- 实测：待确认时卡片数不变；取消后恢复「删除」；4.3 秒自动复位；确认后卡片数 6 → 5。截图 `shots/audit/81-delete-confirm.png`。

边界：这是前端二次确认，不是服务端软删除；确认后没有撤销。删除后想恢复只能从笔记历史/数据库备份，不在本轮范围。

## 12. 决策记录

A（页边落笔）已选。B/C 的 demo 保留作后续“选区工具条/专注阅读模式”的参考，不再作为主线。
