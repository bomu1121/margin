# 当前交接卡（2026-11-19）

> 状态快照，不是永久规范。新会话以 `git status/log` 和当前文件为准；数字冲突以最新实测为准。
> 永久流程看 [`SESSION_START.md`](SESSION_START.md)，主题/配色看 [`THEMES.md`](THEMES.md)、[`COLOR_RESEARCH.md`](COLOR_RESEARCH.md)。

## 1. 当前状态

- 分支：`feat/native-production-facts-ui`；P137–P147 已推送（`0f62cec`），2026-11-19 晚的 P148–P153（视觉收敛、RunFlow、冲突提示、AI 成稿去枷锁、向导标签）已提交并推送。工作区只剩 `sample-vault/` 个人内容。
- 最近构建：`cargo test --lib` **209 passed / 7 ignored**（P148 后复测）；`npx vue-tsc -b` exit 0；`npm run build` 通过；运行页 1440/1040/980、任务/模板/订阅/任务弹窗/向导已用 CDP 截图和独立视觉评审（模型读 PNG）验收：首轮 21 页 17 pass / 4 fail，修复后 9/9 pass、0 条必须修。
- **本批没有重打 tauri 包**：安装包仍是 2026-11-17 的 `8,797,820 B`，exe `22,126,592 B`；要更新安装版必须 `npm run tauri build`。
- 制作台可见模板 **4 个**：视频笔记、文稿笔记、**AI 成稿**、速览与溯源；技术小册子、HTML 作者版、前端 agent runtime、技术调研笔记、智能体笔记已删除。
- `sample-vault/` 下未跟踪的个人笔记和 `.yebian/` 运行产物不是代码改动，不要提交。

## 2. 本批新增（2026-11-18 ~ 19）

### AI 成稿 `ai-note`（当前唯一的 AI 笔记模板）

- 入口：制作 → 新建制作（或模板库）→ **AI 成稿**。步骤是 `source →（转写/校对）→ agent_note → publish`。
- 文本源本来就是校对稿：`steps_for("ai-note","text")` 跳过 transcribe 和 refine，直接 `source → agent_note → publish`；视频源才保留转写和 AI 校对。
- 默认 brief 是引擎内置的一句话版；向导里的「写作要求」会整段替换它。**P152 起默认没有 `NoteGateSpec`、没有 repair、没有 anti-thrash**；模型不调工具即结束，只有“必须产出 note.md”这一条硬要求。
- 2026-11-19 冒烟（`run_59536b40…`，895 B 文稿）：4 回合、5,703/645 tok、310 CJK、5 H2、0 图、通过；文本源确实只跑了 `source → agent_note → publish`。截图 `shots/audit/284-ai-note-templates.png`、`285-ai-note-run.png`。
- **图**：系统提示只给输出介质语法（figure 块支持 `flow / layers / bar / quote / compare`），不规定何时画、画几张、怎么画；需要图就在「写作要求」里说。P152 的两次运行里，默认一句话任务 0 图，明确写“至少两张”后出 2 图。
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

## 3. 本批结论（不要重新推导）

- 写作规则这一轴已经饱和：所有成功产物都覆盖 12 个要点，长度差来自展开和图示，不是漏事实。**但 12 要点覆盖不是决策质量**：2026-11-19 盲评里 NOTE 10/12 排第一，minimal 12/12 排末位。
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
- 运行页轨迹：Rust 引擎每回合写 log + 最终 `agent-run.json`；实时回合不再依赖窗口内存。
- 运行页已实拍 1440×900、1040×760、980×760、1440×760；**暗色和真失败态（门禁耗尽/超 token/超回合）仍未逐页审计**；≤980 drawer 会盖住时间线右半，是有意取舍。
- 取消现在会中断 in-flight 模型请求（body 读取进 `tokio::select!`）；修复前的旧取消任务仍保留 70.9s 历史轨迹，不回填。
- 本批未跑 `npm run tauri build`，安装版不是这批代码。
- 自动结构状态里的上限（20 节 / 6,000 字）是启发式，不是审美判断。
- **P148 视觉收敛的边界**：`RunFlow` 只有横向排布，没有缩放/平移；>6 节点靠横向滚动，10+ 节点未测；暗色只截了 video run 1440；失败态只覆盖取消/中断，门禁耗尽/超 token/超回合仍未 mock；任务弹窗结果预览和 AI 对话用了新的 `richText.ts` CJK 强调预处理，笔记阅读页未统一，仍可能出现 `**…（全角标点）…**` 原样星号。
- **P148 未重打安装包**：安装版仍是 2026-11-17 的 8,797,820 B，不含这批样式和 `RunFlow`。

## 7. 新会话开场模板

```text
按 docs/SESSION_START.md 开始，先读 docs/HANDOFF.md。
这次只做：<一句话任务>。
先复述当前分支/未提交状态、相关限制和复测命令，再动代码。
```
