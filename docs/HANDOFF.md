# 当前交接卡（2026-11-19）

> 状态快照，不是永久规范。新会话以 `git status/log` 和当前文件为准；数字冲突以最新实测为准。
> 永久流程看 [`SESSION_START.md`](SESSION_START.md)，主题/配色看 [`THEMES.md`](THEMES.md)、[`COLOR_RESEARCH.md`](COLOR_RESEARCH.md)。

## 1. 当前状态

- 分支：`feat/native-production-facts-ui`；P137–P147 已推送（`0f62cec`），P148–P159 已提交并推送（HEAD 见 `git log`）；P160 速览 vs AI 成稿同源对比已记录。**P161–P166 已提交**：后端 `9b642f6`、前端 `44c6177`、工具 `68714a3`；文档 `7053590`、P167 文档 `d486dfb`。**P168 已提交**：主导航移除“事实”（`c03d96a`）、第三篇陈寿笔记 4/4 补原文、安装包体积（`b2f5650` / `d9e8beb`）。**P169 已做**：兼容性对照；**P170 已做**：AI 成稿 claims 协议 + cite_agent 确定性读取 + 出处栏状态标签。正文曾短暂校正两处，按用户要求已还原；数据在 vault DB。`sample-vault/` 下的个人内容不要提交。
- 最近构建：`cargo test --lib` **223 passed / 7 ignored**；`npm run build` 通过；P166 视觉 607/608、P167 视觉 609–612、P168 视觉 613–616、P170 视觉 619/620 均 pass。三篇旧链路共 **15/15** 条出处有原文；AI 成稿 claims 小样 1 found + 1 candidate。
- **P170 已重打进安装包**：NSIS 安装包 `8,985,040 B`（8.57 MiB），主程序 `22,877,184 B`（21.82 MiB），路径 `src-tauri/target/release/bundle/nsis/页边笔记_0.1.0_x64-setup.exe`。NSIS 缓存已通过镜像补齐。
- 制作台可见模板 **4 个**：视频笔记、文稿笔记、**AI 成稿**、速览与溯源；技术小册子、HTML 作者版、前端 agent runtime、技术调研笔记、智能体笔记已删除。
- `sample-vault/` 下未跟踪的个人笔记和 `.yebian/` 运行产物不是代码改动，不要提交。

## 2. 本批新增（2026-11-18 ~ 19）

### AI 成稿 `ai-note`（当前唯一的 AI 笔记模板）

- 入口：制作 → 新建制作（或模板库）→ **AI 成稿**。步骤是 `source →（转写/校对）→ agent_note → publish`。
- 文本源本来就是校对稿：`steps_for("ai-note","text")` 跳过 transcribe 和 refine，直接 `source → agent_note → publish`；视频源才保留转写和 AI 校对。
- 默认 brief 是引擎内置的一句话版；向导里的「写作要求」会整段替换它。**P152 起默认没有 `NoteGateSpec`、没有 repair、没有 anti-thrash**；模型不调工具即结束，只有“必须产出 note.md”这一条硬要求。
- 2026-11-19 冒烟（`run_59536b40…`，895 B 文稿）：4 回合、5,703/645 tok、310 CJK、5 H2、0 图、通过；文本源确实只跑了 `source → agent_note → publish`。截图 `shots/audit/284-ai-note-templates.png`、`285-ai-note-run.png`。
- **图**：P152 时系统提示只给输出介质语法（figure 块支持 `flow / layers / bar / quote / compare`），不规定何时画、画几张、怎么画；需要图就在「写作要求」里说。**P159 起改为 ` ```html ` 视觉块，旧 figure 语法已删除，见本文件 P159 一节。** P152 的两次运行里，默认一句话任务 0 图，明确写“至少两张”后出 2 图。
- 2026-11-19 三次图测试（历史，规则版）：`run_d09db15…` 2 图 / 685 CJK；`run_983469e4…` 2 图 / 807 CJK；`run_8b599b63…` 3 图 / 1,250 CJK / 6 H2 / 5 表格 / 门禁通过。这些是 P143–P145 规则版的记录；P152 已删除 gate/anti-thrash，不要拿它们当当前行为。
- **图的视觉**：`figures.mjs` / `book.css` 重做。标题和图注统一到图框下方（短蓝线 + 粗标题 + 宋体说明）；去掉面板/卡片/带子的内层描边；compare 改成双色面板 + 色条 + 40px `vs` 圆标；flow 节点 48px、编号与标题同行、首蓝尾绿；layers 去掉白色内条，改成标签 + 措施文本 + 竖向脊线。独立视觉评审（模型读 PNG）6.5 → 7.4 → **8.3**，最后一轮 0 条必须修；截图 `shots/audit/286-288`。
- **已删除**：`research-note` / `agent-note` 两个模板定义、`NoteGateSpec::research_note()`、`prompts::research_note_system/plan/repair`、旧探针 `probe-research-note.mjs`。旧任务仍可打开，但不再有这两个可选模板。

### AI 成稿运行页（agent 任务）

- **P147 时曾去掉画布 + 瀑布时间线**。AI 成稿中间是一个黑箱 agent，没有可控中间阶段；页面顶部只留一条紧凑路径：`校对稿 →（转写 → 校对）→ AI 成稿 → 已保存`，没有“读取来源”假步骤，也没有把 publish 画成节点。**P148 已用 RunFlow 加回真实步骤流程条，见 §2.8。**
- 主区是宽对话（1440 下容器 1030px，正文 760px），右侧是 340px 产出卡：笔记标题、字数/H2/图、门禁、打开笔记、目录、来源路径。
- 对话是一个用户气泡 + 一个 AI 运行块（左侧竖轨 + 回合圆点 + 思考/工具/文本）；工具行无边框浅灰、结果和参数折叠；`浏览目录 .` 显示“当前目录”。
- 页面头部对 agent 任务不再重复放“门禁/打开笔记/跟随”；≤1024 头部恢复“打开笔记”，980 单页滚动、tab 在对话上方，产出卡在折线下但入口在首屏。
- 非 agent 任务仍保留旧的 graph/timeline 兜底布局；本轮没有重做。
- 独立评审：第一轮 7.0（结构成立，执行有重复/滚动问题）→ 第二轮 **9.0**，0 条必须修。截图 `shots/audit/293-295`。

### Super Agent 对照（15 次真实运行）

- 素材：`run_54c3d3ec…/transcribe/transcript-01.md`（12,050 B / 3,342 CJK）；5 组 × 3 次。
- 汇总（修复前）：minimal 2/3、2,520 字、¥0.0324；plan 2/3、2,605 字、¥0.0557；audit 2/3、2,323 字、¥0.0392；free 3/3、2,988 字、¥0.0379；ZCode 3/3、3,696 字、147,867 输入 token。
- 修复后 minimal 3/3（2,419–2,875 字，2–4 回合，¥0.021–0.0497）；受控失败验证：3,044/2,988 tok、¥0.0162，`agent-run.json` 33.5 KB、失败草稿 10.2 KB。
- 报告与样例：[`docs/research/super-agent/eval/REPORT.md`](research/super-agent/eval/REPORT.md)。

### 2026-11-19 增量：research-note 真跑、盲评、上下文修复（未提交）

- **research-note 首次真跑**：Super Agent 转写稿 3,348 CJK，deepseek-v4-flash；10 节 / 4,011 CJK / 3 图 / 0 表格，一次通过（无重写），97.3s、¥0.0757；0 条超素材数字。样例 `docs/research/super-agent/eval/research-1.md`。
- **5 份匿名盲评**：两份独立评审排名完全一致 **NOTE > research-1 > free-1 > zcode-1 > minimal-5**。默认 minimal 虽然 12/12 要点覆盖，但决策价值排末位；research-1 是唯一 12/12 覆盖 + 7/7 决策标记的新产物。详见 [`docs/research/super-agent/eval/BLIND.md`](research/super-agent/eval/BLIND.md)。
- **上下文修复**：`buildContext` 之前会在预算不足时整段 drop 素材（40k CJK @ 40k 预算）；现在素材/草稿为 required，截断不 drop，并记录「上下文裁剪」；note 模式预算 40k→60k。`tools/test-agent-context.mjs` 6 assertions，修复前红、修复后绿。
- **未提交文件**：`src/lib/agent/context.ts`、`runtime.ts`、`worker.ts`、`tools/test-agent-context.mjs`、`docs/research/super-agent/eval/research-1.md`、`BLIND.md`、`README.md`、`docs/experiment-log.txt`、本文件。
- 安装包仍是 2026-11-17 的 `8,797,820 B`；这批未重打。

### 2026-11-19 增量：ZCode 单次引擎（Phase 0–2，未提交）

- **Phase 0 事实**：ZCode 写笔记用的是 general-purpose 子代理（短系统提示、`tools: ["*"]`、文件产出、原生 tool calls、无 finish），不是主会话 110KB 提示。规格见 [`docs/ZCODE_ENGINE_SPEC.md`](ZCODE_ENGINE_SPEC.md)。
- **实现**：`clients.rs` 加 `chat_tools`（原生 tools/tool_calls）；`production/note_engine.rs` 工作区 + `read_file/write_file/edit_file/list_dir` + 原生循环 + 确定性门禁 + 失败轨迹；`pipeline.rs` 的 `agent_note` 不再排队等窗口 worker；`JobOptions.note_task` 可带写作 brief。
- **实测**（同一 3,348 CJK 转写稿）：一句话 3 回合/18.3s/¥0.0208/3,221 字；`PROMPT.md` brief 修复前 37 回合/385.8s/2,039,109 tok/¥0.4200，加入「写入后回结构状态 + 满足即停 + 连续 3 次 <2% 润色停止 + 600k token 预算」后 3 回合/60.8s/19,964 tok/¥0.0554，10 节/4,450 字。
- **盲评**：评审 1 `NOTE > 引擎 > research-1 > minimal > 一句版`；评审 2 `引擎 > research-1 > NOTE > minimal > 一句版`。引擎 + brief 进第一梯队；NOTE 因无法复核的仓库数字被两份评审扣分。
- **Phase 3/4 已做**：`research-note` 步骤改成 `source → agent_note → publish`，brief 用 `prompts::research_note_system()`（该模板/提示词已在 2026-11-19 模板收敛中删除）；前端 `worker.ts` 的 note 模式删除；随后把小册子/HTML 作者/前端 agent runtime/`production_agent_tasks`/旧 research 管线整体删除（`booklet/htmlbook/render/research_note/agent_shell` 模块、pipeline 分支、命令、模板、UI）。清理后 research-note 冒烟：4 回合 / 56.3s / ¥0.0495 / 4,725 字 / 10 节 / 0 图。
- **P140 收口**：research brief 的严格门禁（2,500–4,500 字、8–12 H2、无表格、图 kind 白名单、禁用小节）落进引擎；超限提示给「一次删减至少 N 字」；模型结束时门禁不过最多修 2 轮。同素材：修复前 20 回合 / ¥0.1264 / 4,499 字，修复后 3 回合 / ¥0.0466 / 4,481 字，首稿通过。引擎只支持 OpenAI 兼容协议（DeepSeek 属于这一类），pipeline 在 ASR 前预检；运行页从 Rust 日志生成 live 轨迹；向导新增「写作要求」。剩余：失败路径 mock、live/写作要求视觉走查、安装包。
- **未提交文件**：`src-tauri/src/production/clients.rs`、`note_engine.rs`、`pipeline.rs`、`mod.rs`、`templates.rs`、`store.rs`、`publish.rs`、`src-tauri/src/lib.rs`、`tools/probe-ai-note.mjs`、`src/views/NoteView.vue`、`src/views/ProductionRunView.vue`、`src/components/production/*`、`src/lib/api.ts`、`src/types.ts`、`src/lib/runModel.ts`、`docs/ZCODE_ENGINE_SPEC.md`、`docs/research/super-agent/eval/engine-*.md`、`research-engine-1/2.md`、`docs/experiment-log.txt`、`docs/measured.json`、`README.md`、本文件；另有多个旧文件被删除（见 `git status`）。
- 安装包仍是 2026-11-17 的 `8,797,820 B`；这批未重打。

### 2026-11-19 增量：运行页重构 + 取消中断（未提交）

- **布局**：1440 画布固定 300px（窗口高 ≤800px 时 210px），时间线吃剩余高度；1040 起检查器与主列并排（主列 614px、检查器 390px），≤980 才用覆盖式 drawer；drawer 打开时隐藏「收起回合」按钮并加浅遮罩。1040 实测轴末刻度右缘 x=549、检查器左缘 x=638，无重叠；无横向溢出。
- **信息层级**：概览首行结论（门禁/字数/回合/写入/节点耗时）→ 结构门禁 5 格 → 任务 → 可折叠 brief；时序/用量移到「指标」；轨迹回合头只留模型耗时 + 精确 token，正文只留输入/输出；工具行单行 + ▸/▾，观察全文限高 260px 内滚；指标恢复开始/结束并补最慢回合。
- **数值闭合**：秒统一 2 位小数；头/时间线/检查器分别标「总耗时 / 跨度 / 节点」；时间线底部「构成 = 节点 + 步骤 + 衔接」。实测 57.93s 模型 + 6ms 工具 + 14ms 节点其他 = 57.95s 节点；57.95s + 33ms 步骤 + 44ms 衔接 = 58.027s ≈ 58.03s 跨度。token 精确值：20,345 + 9,530 = 29,875；回合 1,909 + 13,850 + 14,116 = 29,875。
- **取消根因**：旧 `send_chat_request` 只对 `request.send()` 做取消竞速，`response.text()` 不在 cancel 分支；取消后引擎仍等完整响应（旧任务 run_faf86b 的 70.9s 回合、artifact 比 finishedAt 晚 63,924ms）。现在 body 读取也进 `tokio::select!`；3 次探针 cancel→agent-run.json 分别 4ms / 6ms / 7ms。
- **终点时长**：`buildRunNodes` 对 terminal 任务把所有已启动节点的 endedAt 补成 jobEnd；画布/时间线/检查器用 finishedAt 兜底，不再对已结束任务用 Date.now 计时。
- **画布**：阶段带统一底色；节点 208→180px、列距 72→48px，1040 适配从 71% 提到 85%，节点副文本约 10px。
- **验收**：运行页重构时 cargo 211 passed / 7 ignored；模板收敛删掉 2 个旧测试后 **209 passed / 7 ignored**；vue-tsc exit 0；npm run build 通过；独立视觉评审（模型读 PNG，非用户研究）5.5 → 7.5 → 8.2 → 8.6 → 9.0，最后一轮 0 条必须修。详细几何/取消数据见 `docs/measured.json` 的 `runPage20261119` 和 `docs/experiment-log.txt` P141。
- **边界**：≤980 drawer 仍会盖住时间线右半；暗色未逐页审计；失败态只覆盖「用户取消」，最大回合/token 预算/门禁耗尽未 mock；修复前的旧取消任务仍保留 70.9s 历史轨迹；≤800px 高时 390px 检查器隐藏 token 脚注（同一句在推理格 tooltip）；安装包未重打。

### 2026-11-19 增量：制作台视觉收敛与运行画布（P148–P153）

- **外壳**：制作台导航固定 232px（≤1180 为 208px），不再沿用笔记侧栏 440px；隐藏 studio 的侧栏拖拽/收起/顶栏计数；侧栏只在有进行中任务时显示该段，删除「最近完成」和底部重复引擎/路径信息。
- **页面**：工作台删除重复的「引擎」横幅和三处 section 副标题，模板卡 3+1 改 2×2；任务删除「已入库」空副行/行尾箭头，行高 75→54px（修 `.prod-task-main` 与任务弹窗类名冲突），筛选标签改「失败 / 取消 / 中断」，980 隐藏来源列；模板卡去掉「N 个步骤」；提示词删「内置/自定义」列；订阅删重复订阅数与健康徽章，ScribeFlow 文案改「待整理」。
- **任务弹窗**：导出/重命名/删除改图标按钮；删结果区 frontmatter 条和右栏重复「状态」、noteId 提示；结果列 680px + 固定表格，1040 下 `scrollWidth=clientWidth=704`。
- **运行页**：新增 `RunFlow.vue` 替代 `RunGraph.vue` 和 AI 成稿的 chip 路径条：单行 208×92 步骤卡（阶段/序号/状态点/耗时/token），active 连接线移动点，>6 节点横向滚动、选中自动滚入；AI 成稿显示真实 `source → … → agent_note → publish`；非 agent 用同一组件。事实行去掉常驻「跟随中」/「累计」，刷新改图标；产出卡区分已保存/未产出；检查器删重复模板/标题卡，改「这一步」摘要。`RunGraph.vue`（600 行）与 `runModel.ts` 的 layout 代码已删。
- **Markdown**：AI 对话改渲染 Markdown；新增 `src/lib/richText.ts` 预转 `**…**`，修全角标点导致 marked 不认、星号原样漏出（任务预览 38 个 `<strong>`、0 个字面 `**`）。
- **publish 改名（P149）**：最后一步显示名从「入库页边笔记」改为「保存笔记」；`templates.rs` 11 处 step 标签、向导「完成后自动保存到笔记库」、完成通知同步；旧任务 DB 里的历史 label 不回填，由 `runModel.stepLabel()` 在运行页/任务弹窗显示层统一映射。流程 key 仍是 `publish`；「入库」只作为阶段名和模板 pipeline chip。
- **任务弹窗版本页（P150）**：版本详情默认 Markdown 预览并剥离 YAML frontmatter，新增「预览 / 源码」切换；版本页填满弹窗、预览内部滚动，并隐藏右侧任务信息栏；详情宽 1024 下 717px、1440 下 774px。右侧「入库笔记」改「笔记文件」。边界：不做逐版本 diff；源码保留 frontmatter。
- **同一来源再次制作（P151）**：`conflict_check` 现在对任何已存在的同源笔记都返回 `ConflictInfo`（新增 `edited` 区分手改），向导弹窗默认「保留两份」走 `keep_both` 生成新 noteId，发布为「标题 (2).md」；「更新原笔记」先 snapshot 到 `.yebian/versions`。实测用 run_59536b40 同文稿再次开始，弹窗出现（1,879/1,878 字、revision 6、未手改），取消后无任务。边界：任务详情的「重新制作/继续制作」仍更新同一 noteId；缺 outbox 生成稿的旧笔记仍可能不提示。
- **AI 成稿去枷锁（P152）**：删除 `NoteGateSpec`、门禁 repair、anti-thrash（稳定写/连续微调提示/一次自检）、20 回合/600k 预算/32k max_tokens 的默认收缩；改为 ZCode 模式：general-purpose system prompt + 用户任务 + 工作区路径，`max_rounds=100`、`max_prompt_tokens=2,000,000` 仅作安全护栏，`max_tokens=None` 用 provider 默认。工具从 4 个文件工具扩到 6 个：`read_file / write_file / edit_file / list_dir / bash / fetch_url`；未接 Glob/Grep/WebSearch/MCP。系统提示只保留 figure 输出语法，不再有“何时画/画几张”的规则。
- P152 实测（同一 895 B 校对稿）：默认一句话任务 `run_0fd5f85c…` 4 回合 / 380 CJK / 5 H2 / 5 表格 / 0 图；在 `noteTask` 里写“至少两张 figure”后 `run_45853b42…` 9 轮 / 831 CJK / 2 图 / 5 H2，均成功。没有门禁后，运行页/产出卡不再出现“门禁通过”；历史任务仍按旧 observation 显示。
- P152 边界：bash 未做进程超时沙箱，fetch_url 未做正文抽取（只截断 20k 字符）；未接 Glob/Grep/WebSearch/MCP；没有门禁意味着结构/字数完全靠任务 brief；P152 未重打安装包。
- **向导选项页（P153）**：新增 `TagInput.vue`（回车/逗号添加、Backspace 删末尾、chip × 删除、粘贴拆分、已有标签建议、IME 保护）；「抽取属性」改 `aria-pressed` chip，设置页共用；删除单独「输出」段，确认页增加「标签」行。CDP 验证输入「财」→建议「领域/财经」、Enter 加 chip、点「时期」`aria-pressed=true`；视觉评审 2 页 9 分 pass。边界：标签无重命名/合并/数量上限；截图 391/392 是评审版本，之后删了重复提示并关掉 textarea resize。
- **验收**：`cargo test --lib` 209/7；`npm run build` 通过；截图 `shots/audit/300`–`379`；独立视觉评审首轮 21 页 17 pass / 4 fail，修复后 9/9 pass、0 条必须修。
- **边界**：未重打 tauri 包；RunFlow 无缩放/平移，10+ 节点未测；暗色只截 video run 1440；失败态只覆盖取消/中断；笔记阅读页未纳入；视觉评审只评静态图，动效未验证。

### 2026-11-19 增量：任务详情右栏重做（P154）

- **问题**：右栏 5 条全宽 hairline、每段统一 14px padding、运行区 2 列网格把 tokens 留在空半格、11–14px 混 2 个字族、48×36 封面居中贴在三行文字旁。
- **改法**：背景 `var(--rail)` → `color-mix(40% rail, surface)`（neutral 浅色 #f8f9fb）；删掉 5 条分割线，改 20px 组间距 + 6px 标签→值；栏内统一 sans + 12px 下限；运行区改 3 行 42px 标签列的 key-value；封面 64×40 顶对齐；笔记未入库不显示「打开」，目录显示 `待整理/`；任务 ID 缩成 `前14…后6`，完整值在 title。
- **实测**：1440 内容高 795 → 668px（栏 667.58，刚好放下，原需滚 127px）；1024 内容 625 / 栏 496（仍需滚 129px，原 300+）；scrollWidth=clientWidth=279，无横向溢出；组间实测 19–21px。对比度（精确计算）：浅色 ink-3 5.18:1、ink-2 6.07:1、blue 4.93:1；深色 5.74 / 8.99 / 5.69。
- **验收**：`npx vue-tsc -b` exit 0；`npm run build` 通过；独立视觉评审 3 轮，最终 1440 9 / 1024 8 / full 9，pending、dark 各 9，0 must-fix。截图 `shots/audit/400-401`（before）、`414`（pending）、`415`（dark）、`416-417`（最终 1440/1024）。
- **边界**：pending 截图是把内存 job 字段改成未入库形状，不是真实运行；1024 及以下仍滚动；只审 neutral 明暗，其他主题只截浅色；未重打安装包。

### 2026-11-20 增量：结果页渲染图 / 滚动条 / 重复按钮（P155）

- **图**：结果页、版本页、AI 对话原来走 `richText.ts`，不认识 figure 围栏；现在 `markdown.ts` 抽出 `renderFigureBlock()`，`richText.ts` 在 marked 前把 figure 围栏替换成同一段 HTML。笔记页 / 结果页 / 版本页 / 对话四处同源。实测同一篇笔记：结果页 `.fig` 3 个、`<pre>` 0 个（改前 0 / 3）。
- **flow 裁切**：`renderFlow` 返回 naturalWidth，`renderFigure` 用 `max(宽度, naturalWidth)` 做 viewBox。本篇 5 节点内容宽 878，原 viewBox 724 会裁掉末尾 164px；现在 viewBox 889，最右节点 879，05 完整。
- **layers 占位**：没有值的行不再画「（无）」，只留平台名（内容本身没写每层措施）。
- **滚动条**：`.prod-task-panel` 加进 overlay 选择器；它的 rightEdge 对齐右栏左边界（右栏隐藏时回退到面板右缘），并在 nearEdge 里补了 gutter 命中兜底。实测 overlay left=949、宽 10、右栏左边界 959，thumb 237px（panel 626 / scroll 1619）。
- **按钮**：删除结果正文末尾「打开笔记 / 导出 Markdown」；打开走右栏「打开」，导出走头部图标。正文末尾 0 按钮。
- **验收**：`npx vue-tsc -b` exit 0；`npm run build` 通过；独立视觉评审 6 页全 pass（8–9 分），layers 补拍 9 分，0 must-fix。截图 `shots/audit/419`（改前）、`428-433`（最终）。
- **边界**：5 节点宽 flow 在 680px 预览列里标签缩到约 9.5px（1024 约 9.2px），评审认为可读但明显小于正文；笔记页列更宽；layers 只显示平台名是内容缺失，不是渲染丢字；版本页/对话未单独截图；只审 neutral 明暗；未重打安装包。

### 2026-11-20 增量：内置模板卡片重做（P156）

- **问题**：卡片内 5 层盒子（边框、25px 图标框、蓝色“基础”徽章、每个流程步骤一个描边 chip、卡内“用此模板”按钮），标题/描述/流程/来源/按钮全是 12px，像后台表单。
- **改法**：新增 `TemplateCard.vue`，首页和模板库共用；整张卡改 `<button>`（`aria-label="<名>，使用此模板"`），删卡内按钮，右上角只留 12px「使用 →」，hover 标题/CTA 变蓝、箭头右移 2px、边框 line→line-2。标题 17px Newsreader，裸 16px 图标，描述 12.5px，来源/流程两行固定 30px 标签列（12px），流程改成箭头路径，来源用「B站 · 收藏 · 本地文件 · 文稿」。去掉卡上“基础/可选”徽章（徽章仍用于向导/设置）。
- **实测**：卡片 1440 下 413×143（改前 414×140）、1024 下 373×143；卡内 button/chip/徽章 0（改前 1 按钮 + 4–5 chip + 1 徽章）；标题 17、描述 12.5、标签/值/CTA 12，无 <12px。
- **验收**：`npx vue-tsc -b` exit 0；`npm run build` 通过；点击「AI 成稿」卡打开向导；独立视觉评审两轮，最终 5 页 pass（9/8/9/8/7），0 must-fix。截图 `shots/audit/440-441`（改前）、`455-459`（最终）、`460-462`（其它主题浅色）。
- **边界**：「从什么开始」来源卡仍是方块图标；向导模板步骤仍用 chip/徽章；模板库页 4 张卡下方留白约一半；只截 neutral 明暗 + 3 套浅色；未重打安装包。

### 2026-11-20 增量：任务详情去重（P157）

- **问题**：同一屏多处重复：头部 60% 与运行卡片「60% · 已完成 3/5 个步骤」；头部「视频笔记 · B站」与右栏「模板：视频笔记 / 5 步 / 来源→转写→校对→加工→入库」「来源：B站」；未入库时右栏「笔记文件 / 等待入库 / 任务完成后写入」与运行状态；已完成时正文 H1 与弹窗标题只差句末标点；右栏「相关笔记」与正文末尾「## 相关笔记」；步骤页进度头和「执行步骤」小标题各显示一次 3/5。
- **改法**：头部 meta 去进度；运行卡片右侧只留「3 / 5 步」；右栏删模板段与相关笔记段，来源改「UP 主 / 收藏 / 文件位置 / 来源摘录」，笔记文件仅在 `job.noteId` 存在时显示、路径改「位于 待整理/」，任务 ID 贴底小号 footer + 1px 上边框；结果 kicker 去模板名、改「已保存」，H1 去尾标点后与任务标题相同则隐藏；步骤页小标题去重复步数；运行标题改「正在 AI 加工」。
- **实测**：右栏已完成 1440 **724/724**、1024 **584/584**、980 **588/588**（P154 记录 1024 内容 625 / 栏 496，需滚 129px，现在无滚动）；运行态（内存模拟）1440/1024 均 **510/510**；运行卡片 124px 高；1440 主列 758 / 右栏 279，1024 701 / 279，980 678 / 259；三档 scrollWidth=clientWidth。760×800 起右栏落到正文下方，随整页滚动（rail 303 / scroll 500）。
- **验收**：`npx vue-tsc -b` exit 0；`npm run build` 通过；独立视觉评审 6 页首轮 6/6 pass 但列 3 个必须修（「正在AI 加工」缺空格、右栏「来源」与头部同名不同义、「待整理/」像文件名一部分）；修复后复检 4 页全 pass、0 must-fix；kicker 对比度 3.44:1 → 5.19:1。复检新增建议「1440 右栏中部空白、footer 孤立」已加上边框，未再送审。截图 `shots/audit/470-484`。
- **边界**：运行态截图是把内存 job 改成 running（用 `detailLoading` 挡掉轮询），不是真实任务，耗时数值不可信；只截 neutral 明/暗，warm/graphite/noblue 和「文件位置 / 来源摘录」两种来源标签未截；760 以下右栏落到正文下方，需整页滚动；正文底部仍是滚动硬裁，没有渐隐；未重打安装包。

### 2026-11-20 增量：运行页单视图（P158）

- **问题**：非 agent 运行页顶部 5 张 `RunFlow` 流程卡、时间线、检查器三处重复同一步和耗时；卡 01 写 `147s`、时间线写 `1.47s`（100× 矛盾）；两块面板被拉满（1440 下各 580px 高），时间线内容只到 y=484、检查器到 y=604，下半屏全空；检查器默认选最没信息的「保存笔记」（35ms），概览只重复「完成 · 35ms」。
- **改法**：删除 `RunFlow.vue`（458 行）；时间线按 素材/生成/入库 分组，每行显示 步骤 + 摘要 + 条形 + 耗时 + tokens + 成本；选中行 10% 蓝底 + 2px 左蓝条；初始打开已完成任务默认选 token 最多的节点（本任务 AI 加工），运行中完成保持原选中不跳；非 agent 检查器去掉「概览」，只留 日志/产物/指标，默认指标，指标卡含状态/耗时/tokens/成本/开始/结束；宽度 1440 358px、1024 300px，≤980 默认收起；面板按内容高度收起；底部加产出条（字数/H2/图/路径）；页脚补「节点外」token/成本差额；「回到当前节点」只在运行中显示。
- **实测**：1440 时间线 **1030×399**（改版前 960×580，内容只到 y=484）、检查器 **358×271**（改前 420×580）、产出条 1030×46；1024 时间线 **686×399**（改前 627×399，检查器 357）、检查器 **298×357**；980/760 检查器默认收起，时间线 **954/734×399**；四档 `scrollWidth=clientWidth`，无横向溢出。时间线内容 399px 贴合 5 步 + 3 个阶段头，无内部空白。
- **数字闭合**：任务 38,659 tok / ¥0.138558；节点合计 34,315 tok / ¥0.126625；节点外 **4,344 tok / ¥0.011933**（页脚显示 4 位，title 给 6 位）；步骤 176.079s + 衔接 13.398s = 189.477s ≈ 3m09s。
- **验收**：`npx vue-tsc -b` exit 0；`npm run build` 通过；独立视觉评审首轮 6 页：基线 500 pass，515（1440）/516（1024）/518（暗色）fail，3 个 must-fix（选中行 1.03:1 不可辨、1024 右栏 357px 压扁图表、tokens/cost 对不上），980/760 pass；修复后复检 5 页（1440/1024/980/760/暗色）**全 pass、0 must-fix**。复检后又按建议改了 760 页头重排（528）、「节点外调用」文案、去掉 tokens 双单位、1024 指标值右对齐；这些是文案/对齐改动，未再送审。截图 `shots/audit/500`、`515-516`、`522-525`、`528-531`、`524`。
- **边界**：运行态没有重新截屏；「节点外」token 归因为属性抽取是推断，未逐条核对；只审 neutral 明暗；agent 运行页（对话/产出）未重审；时间线在 >10 节点时靠 `max-height: min(560px, 100vh-240px)` 内滚，未用 10+ 节点实测；未重打安装包。

### 2026-11-20 增量：HTML 视觉块替代 figure 模板（P159）

> 可复用的多稿回归流程见 [`VISUAL_BLOCK_AUDIT.md`](VISUAL_BLOCK_AUDIT.md)：现成转写稿、probe、iframe 指标、红线和清理步骤。

- **问题**：旧 figure 只有 flow/layers/bar/quote/compare 五个固定 SVG 模板，模型只能填字段，布局/颜色/字号都被模板写死；同样内容在 ZCode 里直接写 HTML/CSS 会好看得多。限制的是渲染层，不是模型。
- **改法**：`note.md` 的 ` ```html ` 围栏渲染成 sandbox iframe（`src/lib/visualBlock.ts`）：DOMPurify 白名单（禁 script/iframe/form/input/link/a 等）、CSP `default-src 'none'`、注入主题 CSS 变量、ResizeObserver 回传高度、无外链；常见中性 hex 归一成 `--surface/--paper/--line/--ink/--ink-3`，深色不再变成亮岛。笔记页、任务结果预览、版本页、AI 对话共用。
- **删除**：`figures.mjs` / `figures.d.mts`、`renderFigureBlock`、`lang === 'figure'` 分支、`book.css`/`production.css` 的 `.fig*` 样式。旧 figure 笔记现在按普通代码块显示，没有兜底（用户明确要求）。
- **提示词（P159 收口）**：`## 输出格式` 只保留环境约束：自包含片段、任意布局/强调色、中性色优先变量、无 script/外链/表单、inline SVG、文字不溢出、不要为配图而配图。删掉 3/4 卡布局、720px 列数、禁止 hex/rgb、不要自指标题这四条过度约束（都是评审个案后加的）；示例改为单卡，只示意写法。
- **字数/FTS**：`vault::plain_text` 只对 ` ```html ` 围栏去 style/script/标签，其他 Markdown（含普通 code fence）保留；`ParsedNote.chars` 和 FTS `search_text` 改用它。新增 2 条单测。
- **实测**：手写测试块在笔记页 1440 下 iframe 高 **361px**，明暗主题跟随；agent 实测生成 5,683 B 笔记（451 CJK / 4 H2），HTML 块在任务弹窗约 680px 预览里 **4 卡一行**、高度自动；srcdoc 检查：模型 `alert`/`onerror` 被剥离，只剩 App 自己的高度脚本。**重启新二进制 + 重建索引后复跑同一 brief**：4 回合 / 395 CJK / 1 个视觉块；模型改用**四行竖排时间线**（不是 4 卡），块内没有自指标题；笔记页显示 **515 字**（`plain_text` 生效）。搜索 `radius / padding / display` 均 0 命中，`读取来源` 仍命中（fallback LIKE 也走 plain text）。
- **多稿实测（5 份现成转写稿，P159 收口后）**：default（Super Agent，3.3k 转写）2,397 CJK / 2 图 / 10 表；gdp（日本，2.3k）1,084 / 1 图（SVG 排名图）；timeline（美国故障，2.4k）1,635 / 1 图（竖向时间线 + 脊线）；interview（6.5k 面试稿）2,697 / 1 图 / 19 表；capitalism（ITIF，1.9k）1,883 / 1 图（四条路线堆叠）。全部 success；1440 下 **无横向溢出、无裁切**；默认任务也主动出了 2 图。
- **多稿实测发现的坑**：①**720px 设计宽度是错的**——笔记列在 1440 只有 **575px**，模型按 720 设计会把 SVG 字缩到 ~10.8px、4 卡变 3+1。已把提示词改成 **575px（弹窗 680 / 窄屏 350）**；重跑 interview 后改为竖向 HTML 对比（viewBox 520、11/13px），575 下约 12–14px。②`rgb()` 灰度不经过 hex 归一；已加 `rgb()` 灰度映射（强调色不动）。③iframe 宽度变化后高度/宽度指标不重算；已加 `resize` 监听。④任务弹窗视觉块加 `max-width:600px`，与 575 笔记列更接近。⑤默认 3 卡出现 2+1 空洞，是自由布局的代价，没有再加规则。
- **验收**：`cargo test --lib` **211 passed / 7 ignored**；`npx vue-tsc -b` exit 0；`npm run build` 通过；截图 `540`（手写浅色）、`541`（手写深色）、`542`（agent 浅色）、`543`（agent 深色，未归一前）、`544`（agent 深色，归一后）、`545`（任务弹窗预览）。独立视觉评审 6 页：540/541/542/543/544 fail、545 pass，3 个 must-fix（4 张卡在 573px 笔记页 3+1/第 4 张通栏、预览 678px 与笔记 573px 版式不一致、块标题“四张卡片”自指且与 H2 重复）；方向值得继续，543→544 证明主题归一解决了亮岛。提示词已改（4 张 2×2、不要排版自指标题），重启后复跑改用竖排时间线；复跑截图 `546`（浅）/`547`（深）/`548`（任务弹窗），独立复检 **3 页全 pass、0 must-fix**：3 个 must-fix 全部消除（4 行竖排无 3+1、573/678 两宽同构、块内无自指标题）。复检建议（未实现）：编号圆之间加 1px 竖脊线、时间颜色统一或按耗时做同色深浅、行标题 14px/500、弹窗 680px 下给块加约 600px max-width。
- **边界**：中性色归一只是常见 hex 值映射，不是 CSS 解析；模型仍可能写硬编码强调色/复杂布局；旧 figure 笔记会显示源码；FTS/字数要重建索引才更新；**新二进制已重启并重建索引**，复跑输出为竖排时间线，3 个 must-fix 复检全 pass；复检的“竖脊线/时间色语义/标题字重/max-width”建议未实现；未跑 `npm run tauri build`。
- **多稿边界**：1024 模拟下笔记列只有 346px（低于 app `minWidth:1040`），capitalism 块 1172px 高但无横向溢出；timeline 6 处 `rgb()` 强调色保留（深色可读，脊线偏亮）；默认 3 卡 2+1 空洞未修；5 个测试任务/笔记已删除，截图 `550-555` 保留。

### 2026-11-21 增量：溯源 Agent 实验版（P161，未提交）

- **定位**：只有 `production_trace_agent_probe` 命令 + `tools/probe-trace-agent.mjs`，没有任务类型/UI/模板；与 AI 成稿隔离（独立 prompt、工具、上下文、输出契约），只共享事实库/provider/usage。
- **工具**：`find_facts / search_web / fetch_source / report_claim / finish`；预算 14 回合、8 主张、10 搜索、6 抓取。程序校验 URL 必须来自本轮检索、quote 必须逐字连续、外部可印证→supports、存在争议→contradicts、可印证/争议前必须 fetch 正文；只写 facts/evidence/mentions。
- **三次真跑**：① 开源工具笔记 8 主张/8 搜索/2 抓取/63.0s/¥0.0669，4 verified 但用了别的项目当证据；② 胡塞 PIF 笔记 8/7/6/114.6s/¥0.0893，3 verified + 3 disputed + 1 candidate + 1 unsupported，查出「10月19日」应为 11月19日、「击中」实为拦截后碎片、「140 家企业」指 KAFD 区而非楼内，385 米/THE LINE 日期可印证；③ 开源笔记加主语规则后 7/3/0/20.2s/¥0.0236，明确「主语无法定位」，7 条全 unsupported，不再挪用别的项目。
- **规则修正**：运行 ② 第一条 verdict=存在争议 但 relation 空，落成 candidate；已加 verdict/relation 一致校验，未复跑。运行 ① 的 4 条类别性 verified 仍在事实库，运行 ③ 没覆盖。
- **工具修复**：`web_fetch` 之前把 GBK 当 UTF-8，中文页面抓成乱码，引文无法校验；已加 header/meta charset + GB2312→GB18030 解码和单测。
- **验收**：`cargo test --lib` **216 passed / 7 ignored**；未跑前端（无前端改动）；未重打安装包。
- **边界**：实验事实写进两篇真实笔记，没有删除命令；历史/ctext 未接入；8 条预算导致覆盖不全；GBK 修复未在真实页面复跑；未做提示注入/恶意页面/登录源对抗测试。详见 `docs/TRACE_AGENT_PROBE.md`。

### 2026-11-21 增量：同一视频笔记的溯源对照（P162，未提交）

- **素材**：BV1xfypzTEv3（14:34），库内已有 6,070 字笔记；旧流水线先跑 note-trace 抽出 10 条 claim，hybrid 用同一清单 + `ignoreFacts=true` 独立查证。
- **结果**：旧 10 条 / 219.6s / ¥0.1820 / 4 可印证 + 4 争议 + 2 未找到；hybrid 10 条 / 0 missing / 196.9s / ¥0.1386 / 20 搜索 + 10 抓取 / 3 可印证 + 0 争议 + 6 弱源 + 1 未找到。**9/10 判定不同**。
- **主要发现**：01–03 是复合主张，两边都只覆盖一部分；单循环把 07（空管）和 08（“这就是生活”）的引文串位；旧流水线把 01/02/03/10 的细节差异判成争议；hybrid 对 04/05/09/10 选到弱源。结论是“先拆原子主张 + 每条独立 agent + 共用 verdict 口径”，不是纯 agent 替代。
- **证据**：对照图 `docs/trace-agent/compare-bili.png`（1800×1280，2x）、数据 `docs/trace-agent/runs/compare-bili/comparison.json`、生成器 `tools/build-trace-compare-chart.mjs`。独立视觉评审 8 分 pass，修掉小字/行 04/标签键后复检。
- **边界**：单视频、10 条、无人工盲评；hybrid v1 是单循环，不是最终 per-claim agent；旧事实在库里但 hybrid 忽略；安装包未重打。

### 2026-11-21 增量：出处系统（P163–P170，已提交）

- **产品降级**：历史学习只需要“笔记原句 → 书名/原文”，不再做事实/判定/合并/拆分/候选分级。旧事实与史料页保留为档案，不删数据。
- **新模型**：`note_citations`（history.rs）存 note_id、逐字 note_quote、work_title/author/volume/page/locator、original_text、status found/candidate、passage/source/url/snippet；复用 sources/source_passages/history_search_candidates。
- **新 agent**：`cite_agent.rs` 只做 find_candidates / list_sources / read_source / attach_citation / attach_candidate / finish；不写 facts、不判真假；note_quote 必须逐字来自正文。
- **新 UI**：笔记右栏“事实”tab 换成“出处”；5 条卡片显示句子→书名→原文/待补；pending 卡有“补原文”+“打开来源”，补原文后直接显示原文；工具栏“5 条出处 · N 条待补原文”+“重新找出处”。旧“事实”页仍在顶部导航，作为档案。
- **实测**：华佗笔记 5 条出处 / 31.8s / ¥0.0361；手动补 4 条（《三国志·华佗传》《关羽传》《后汉书·五行志》）并换成可读链接，5 条出处 · 1 条待补（曹操高陵报告）；第二篇北魏 6 条候选 / 28.4s / ¥0.0289。`production_history_search` 实测 Wikisource/IA `error sending request`，0 hits，只有目录链接；补了「加出处」手动兜底（`production_note_citation_add`）。独立视觉评审：补原文入口后 1440/1024 pass；`cargo test --lib` 217 passed / 7 ignored。
- **P165 修复**：候选检索把“宋书 薛安都”当整串匹配，复跑 0；已拆词+按命中数排序，第二篇复跑 6 条 / 29.0s / ¥0.0294，ctext 书目页；详细见 experiment-log P165。
- **P166 管理闭环**：出处卡片增加“改原文 / 删除”，删除带确认、计数即时更新；`cite_agent` 达到 6 条立即结束，不再空转到 12 回合。真实 UI 探针（`tools/probe-cite-ui.mjs`）在华佗笔记临时新增 1 条 → 6 条，改原文保存“测试原文二”，删除后回到 5 条并清理临时记录；`cargo test --lib` 221 passed / 7 ignored；视觉 607/608 两页 pass。
- **P167 全量补原文**：华佗 **5/5**、北魏 **6/6**，共 **11/11 found**。北魏 6 条 = 2《魏書》+1《通鑑紀事本末·宋明帝北伐》+3《資治通鑑》。改正 2 条误归《宋書》（十五万、萧道成对白）和 1 条误归《曹操高陵考古发掘报告》（曹操龋齿—头风，改为潘伟斌文）。链接换成可打开的识典古籍/南博/ctext/华西都市报，8 个唯一 URL 均 HTTP 200；视觉 609–612 四页 pass。
- **P168 入口收敛 + 第三篇（正文保持原文）**：
  - 主导航移除「事实」，只留「笔记 / 图谱 / 标签 / 史料 / 制作」；旧事实页收进设置 → 关于「旧事实档案」，数据不删；视觉 615/616 pass。
  - 正文：曾把北魏「十之八九」改成「十之七八」（3 处）、华佗「头骨上的蛀牙」改成「下颌骨牙齿上的蛀牙」；按用户要求已全部还原，对应 citation 的 note_quote 同步还原。正文仍为「十之八九」「头骨上的蛀牙」，差异只保留在出处卡片的 `original_text`（“什七八”“多颗牙齿”）。
  - 陈寿笔记 1,821 字、原 0 条：`production_cite_note` **4 条 / 15.9s / ¥0.0179 / success**（自然结束），4 条全候选（《三国志》×2、《晋书》×2）；人工补成 **4/4**（《三国志·评曰》2、《晋书·陈寿传》1、《说文解字·髟部》1），把「髡刑定义」从《晋书》改正到《说文解字》。视觉 613/614 pass。
- **P169 AI 成稿兼容性**：三篇验证笔记都是旧链路（北魏 `trace`、华佗 `note-trace`、陈寿 `trace`）；现有 2 篇 `ai-note` 是 APEC/GitHub，非历史。用北魏正文作素材生成 AI 成稿历史笔记 **84.7s / 2,810 CJK / 9 H2 / 4 视觉块 / ¥0.0905**；出处 agent **6 条 / 30.5s / ¥0.0421 / 0 found**，旧北魏 **6 条 / 29.0s / ¥0.0294 / 0 found**（后手工 6/6）。AI 成稿 6 条 URL **全是 HathiTrust/NLC 检索/馆藏入口，0 直达原文**，至少 3 条书名/卷次不对。结论：**能跑，但契合度不高**；AI 成稿明确“不引用史书文言原文”并按【史载】/【素材判断】/【本笔记判断】分层，出处 agent 不读这些标记，也不要求原句。
- **P170 claims 协议**：`note_engine.rs` 系统提示要求 AI 成稿输出 `yebian-claims` HTML 注释 JSON（quote/kind/work/section/original/url），只列史载/转述，判断不列，没有来源写 `待查`。`cite_agent::run_claims_block` 在 API Key 检查前读取 claims：只处理史载/转述，跳过判断/待查，quote 必须逐字，`work` 为空一律跳过，有 original 直接 found，否则 candidate；**不调用模型**。旧笔记无 claims 时继续走原模型流程。出处栏增加 `已找到/待补原文` 状态标签和条件提示。
- **P170 实测**：合成探针 **7ms / 0 token / 1 found**；小素材 AI 成稿 **29s / 265 CJK / ¥0.0216**，claims 1 史载 + 1 转述，`production_cite_note` **6ms / 0 token / 1 found + 1 candidate**；无来源小素材 4 条 `待查`，不落出处。同一篇北魏长稿改后复跑 **84s / 3,088 CJK / 12 H2 / 2 视觉块 / ¥0.0574**，claims 8 条全 `待查`，出处 **7ms / 0 token / no_sources / 0 citations**；正文结构把“史事/解释/空白”拆开。`cargo test --lib` **223 passed / 7 ignored**；619/620 视觉 pass（617/618 首轮 fail：提示承诺不存在的打开来源、状态不可辨）。
- **边界**：正文未改；两处差异保留在出处卡片；「十五万」存在「五万」版本异文；关羽传链接是 ctext 书目页；陈寿「评价最好的几个人之一」是作者判断，《三国志·评曰》只支持高评价、不是字面排名；完整北魏长稿用第一版 claims prompt 时模型卡在回合、被取消，收紧后复跑成功但 claims 全 `待查`、0 出处；安装包已重打（`8,985,040 B`）。详见 `docs/CITATIONS.md`。

## 3. 本批结论（不要重新推导）

- **速览配方可由 AI 成稿复现**：同源日本 GDP（AI 测试用已有转写稿，速览 job 含转写/校对，比较的是写作阶段）单次对比，速览模板 3,035 CJK / 13 H2 / 0 表 / 0 图 / ¥0.1406 / 198.5s；AI 成稿 + 速览 brief 3,262 CJK / 11 H2 / 0 表 / 0 图 / ¥0.0932 / 86.8s；结构都含速览/判断句 H2/弦外之音/速查。单源单次、未盲评质量，不能据此说“更好”。**速览与溯源仍独占 `trace` / 事实入库**；AI 默认是短稿 + 表格/图，不是速览。- 写作规则这一轴已经饱和：所有成功产物都覆盖 12 个要点，长度差来自展开和图示，不是漏事实。**但 12 要点覆盖不是决策质量**：2026-11-19 盲评里 NOTE 10/12 排第一，minimal 12/12 排末位。
- 瓶颈是 **JSON-in-text 协议**（可靠性）：长 `write_note` 的 JSON 可能解析失败（audit-2 的失败原因）。**2026-11-19 已修**：`agent_note` 改走 Rust 原生 tool calling（P137），不再有 JSON-in-text；旧前端 runtime 已删除。质量差距在 genre 和 brief，不在协议。
- `plan` 档不值：成本最高、2/3 成功、覆盖率不升。
- `free` 是修复前最稳的档（3/3）；`minimal` 加上自动结构状态后也 3/3，且更便宜。**2026-11-19 模板收敛**：`research-note` 和 `agent-note` 两个模板已删除，只留一个 `ai-note`（AI 成稿）；历史盲评仍可作为“固定结构 vs 自由结构”的证据，但不再是可选模板。
- 失败运行现在会计账并留轨迹；历史三次失败（修复前）的 0 账单没有回填。
- 自动结构状态是软提示，模型可以不理；门禁仍只有标题 + ≥200 字。

## 4. 已定下的旧决策（仍然有效）

- 四套主题 `neutral / warm / graphite / noblue`，模式 `system / light / dark`，默认 `neutral + system`；主题是设备本地偏好，不云同步。
- 设置页路由 `/settings/:section?`，默认 `/settings/appearance`；新增设置只加 `settingsSections.ts` 的 section + 面板。
- 索引页宽 `1120px`、详情页宽 `980px`、左右 padding `30px`；不要各页写死宽度。
- 事实状态：candidate 无来源只留「未核验」，有来源显示「N 条来源待核验」；订阅标题「共 N 个」；任务左栏徽标只显示进行中。
- 图谱仍是深色数据画布；标签台两个搜索、任务表列密度是刻意保留，不是 bug。

## 5. 硬约束

- 不主动 commit/push，除非用户明确要求（本批已获授权并推送）。
- `ctext` 只走 `api.ctext.org`，不抓网页；不申请 ctext key。
- 不打印/存储 Bilibili cookies、API Key；不把 `sample-vault/` 个人内容、`dist/`、`src-tauri/target/` 提交。
- 一次只改一个变量；视觉改动导出 PNG 后交独立 judge；数字必须标来源（实测 / 精确计算 / 估算）。
- 改代码前先看 README「没做完的 / 已知限制」，不要把手动上限当 bug。

## 6. 已知坑 / 未完成

- 旧前端 agent runtime 和 `production_agent_tasks` 已删除；旧任务若有 `agent`/`author` 步骤不再可跑。`agent_note` 只走 Rust 引擎。
- **模板收敛（2026-11-19）**：`research-note` / `agent-note` 已从代码和可见模板中删除；旧任务还能打开，但重跑会因模板不存在而失败，页头可能显示原始 template id。当前唯一 AI 笔记模板是 `ai-note`（AI 成稿）。
- **AI 成稿默认门禁很宽**：只有 1 个 H1 + ≥200 CJK；不再有固定 8–12 节、无表格、禁用小节等严格规则。需要结构时靠向导里的「写作要求」，它是软 brief，不是确定性校验。
- **HTML 视觉块（P159）**：中性色归一只是常见 hex 值映射，模型仍可能写硬编码强调色/复杂布局；旧 figure 笔记按普通代码块显示；FTS/字数要重建索引；已重启新二进制并重建索引，复跑输出为竖排时间线，复检 3 页 pass；fallback LIKE 也过滤 CSS 词；复检的竖脊线/时间色/标题字重/max-width 建议未实现；多稿实测发现 720→575 设计宽度错误已修，`rgb()` 灰度也已归一，`resize` 会重算高度；未重打安装包。
- **溯源 Agent（P161/P162）**：实验版，已被 P163 出处系统替代；旧事实页仍保留为档案。
- **出处系统（P163–P170）**：三篇旧链路 15/15 有原文；P170 已落 AI 成稿 claims 确定性读取（0 token）；长稿改后 8 条 `待查`、0 出处；`work` 为空跳过；`待查` 不落出处；正文未改，差异保留在出处卡片；补原文只保存文本、不自动对齐；自动收尾代码已加但未真实模型复跑；陈寿「评价最好」是作者判断；旧事实页已从主导航移除、数据未删；安装包已重打。
- 运行页轨迹：Rust 引擎每回合写 log + 最终 `agent-run.json`；实时回合不再依赖窗口内存。
- 运行页已实拍 1440×900、1040×760、980×760、1440×760；**暗色和真失败态（门禁耗尽/超 token/超回合）仍未逐页审计**；≤980 drawer 会盖住时间线右半，是有意取舍。
- 取消现在会中断 in-flight 模型请求（body 读取进 `tokio::select!`）；修复前的旧取消任务仍保留 70.9s 历史轨迹，不回填。
- 本批未跑 `npm run tauri build`，安装版不是这批代码。
- 自动结构状态里的上限（20 节 / 6,000 字）是启发式，不是审美判断。
- **模板卡片（P156）**：「从什么开始」四张来源卡仍是 44px 方块图标，与模板卡的裸图标是两套语言；向导「模板」步骤仍是 chip + 徽章；模板库页只有 4 张卡，2 列网格下方约一半留白。
- **结果页（P155）**：5 节点宽 flow 在 680px 预览列里标签缩到约 9.5px（1024 约 9.2px），评审认为可读但明显小于正文；layers 只显示平台名是笔记内容没有每层措施；正文末尾的打开/导出按钮已删除，唯一入口是右栏「打开」和头部导出图标；版本页/对话共用新的图渲染但未单独截图。
- **任务详情右栏（P154 / P157）**：P157 后 1440/1024/980 右栏不再滚动；760 以下右栏落到正文下方。运行态截图是内存模拟，不是真实任务；只审了 neutral 的明/暗，warm/graphite/noblue 只截浅色。
- **P148 视觉收敛的边界**：`RunFlow` 只有横向排布，没有缩放/平移；>6 节点靠横向滚动，10+ 节点未测；暗色只截了 video run 1440；失败态只覆盖取消/中断，门禁耗尽/超 token/超回合仍未 mock；任务弹窗结果预览和 AI 对话用了新的 `richText.ts` CJK 强调预处理，笔记阅读页未统一，仍可能出现 `**…（全角标点）…**` 原样星号。
- **P148 未重打安装包**：安装版仍是 2026-11-17 的 8,797,820 B，不含这批样式和 `RunFlow`。

## 7. 新会话开场模板

```text
按 docs/SESSION_START.md 开始，先读 docs/HANDOFF.md。
这次只做：<一句话任务>。
先复述当前分支/未提交状态、相关限制和复测命令，再动代码。
```
