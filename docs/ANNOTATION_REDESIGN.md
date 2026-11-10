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
- 键盘：`H` 默认高亮，`N` 写笔记，`Esc` 取消，`Cmd/Ctrl+Enter` 保存；焦点不离开选区上下文。

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
- 回归：`cargo test --lib` 166 passed / 7 ignored；`npm run build` / `npm run tauri build` 通过；包 8,722,874 B（8.32 MiB）/ exe 21,914,112 B（20.90 MiB）。
- 截图：`shots/audit/58-annotation-default-margin.png`、`60-annotation-selection-composer.png`。

### M1 的边界

- 右侧栏是页边，但卡片**没有排到与段落同高**；这是 M2 的布局项。
- 没有 hover 预览、没有键盘 `H/N`、没有多选/批量；移动端选区没有逐项验收。
- 跨段标注已能保存多个 segment，但页边卡片只按第一段定位；跨段高亮/卡片定位需要再验。
- 旧标注的整块回退是迁移兜底，不是精确范围。
- 颜色尚无命名语义；删除是直接删除，没有撤销。

## 8. 决策记录

A（页边落笔）已选。B/C 的 demo 保留作后续“选区工具条/专注阅读模式”的参考，不再作为主线。
