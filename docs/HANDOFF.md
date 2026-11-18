# 当前交接卡（2026-11-19）

> 状态快照，不是永久规范。新会话以 `git status/log` 和当前文件为准；数字冲突以最新实测为准。
> 永久流程看 [`SESSION_START.md`](SESSION_START.md)，主题/配色看 [`THEMES.md`](THEMES.md)、[`COLOR_RESEARCH.md`](COLOR_RESEARCH.md)。

## 1. 当前状态

- 分支：`feat/native-production-facts-ui`；本批 4 个 commit（Rust / 前端 / 工具 / 文档）已推送，工作区干净。
- 最近构建：`cargo test --lib` **212 passed / 7 ignored**；`tools/test-agent-gate.mjs` **25 assertions**；`npx vue-tsc -b` exit 0。
- **本批没有重打 tauri 包**：安装包仍是 2026-11-17 的 `8,797,820 B`，exe `22,126,592 B`；要更新安装版必须 `npm run tauri build`。
- 制作台可见模板 6 个：视频笔记、文稿笔记、**技术调研笔记**、**智能体笔记**、速览与溯源、技术小册子；HTML 作者版暂时从界面隐藏（代码保留）。
- `sample-vault/` 下未跟踪的个人笔记和 `.yebian/` 运行产物不是代码改动，不要提交。

## 2. 本批新增（2026-11-18 ~ 19）

### 技术调研笔记 `research-note`（严格版）

- 来源：`docs/research/super-agent/PROMPT.md` 的施工图规格。
- 流程：`plan`（章节计划 JSON）→ 成稿 → 确定性结构校验 → 不合格只重写一次 → 入库。
- 校验：1 个 H1、8–12 个 H2、2,000–5,000 CJK、禁用「速览/弦外之音/速查」小节名、正文无表格、图 kind 白名单。
- 这是“固定结构”的对照模板；不喜欢固定结构时用 `agent-note`。

### 智能体笔记 `agent-note`（一句话版）

- 默认 `minimal`：目标只有「把素材生成一份美观的笔记」+「只使用素材里的信息、不编造」；工具 `write_note / patch_section / finish`；6 回合；40k 上下文；门禁只有「有标题 + ≥200 字」。
- 素材和**当前整篇草稿**都在上下文里；`write_note` / `patch_section` 的观察会自动附一行结构状态（H1/H2/字数/图问题/问题列表），模型不用额外调用 audit 就知道是否写完。
- 审计上限是 **20 节 / 6,000 字**（旧 12 节 / 5,000 字会逼 ZCode 那种 14–18 节的厚笔记压缩）；`audit_note` 仍可主动调用看完整小节清单。
- 四个探针档：`minimal / plan / audit / free`，通过任务 `options.prompt` 选择；**UI 不暴露**，只有探针脚本用。`free` 是“中等 brief + 强制 audit”，`plan` 是“+todo_write”。
- 失败也记账：worker 把 run JSON（usage/turns/draft）传回，Rust 在失败前把 token 计入任务用量，并落 `agent-run.json` + 失败草稿 `note.md`。
- 窗口必须保持打开；关窗后任务停在等待，30 分钟超时。

### 运行页轨迹

- `agent_note / agent / author` 节点默认打开「轨迹」：回合数、模型/工具总耗时、输入/输出/推理 token；每回合的工具、模型耗时、tokens、观察全文，可展开工具参数、模型推理、模型原始输出。
- 日志页对 agent 节点隐藏重复的 `#n` 回合摘要；时间线把模型段（灰）和工具段（蓝）分开画。
- 截图 `shots/audit/276-agent-trajectory.png`。

### Super Agent 对照（15 次真实运行）

- 素材：`run_54c3d3ec…/transcribe/transcript-01.md`（12,050 B / 3,342 CJK）；5 组 × 3 次。
- 汇总（修复前）：minimal 2/3、2,520 字、¥0.0324；plan 2/3、2,605 字、¥0.0557；audit 2/3、2,323 字、¥0.0392；free 3/3、2,988 字、¥0.0379；ZCode 3/3、3,696 字、147,867 输入 token。
- 修复后 minimal 3/3（2,419–2,875 字，2–4 回合，¥0.021–0.0497）；受控失败验证：3,044/2,988 tok、¥0.0162，`agent-run.json` 33.5 KB、失败草稿 10.2 KB。
- 报告与样例：[`docs/research/super-agent/eval/REPORT.md`](research/super-agent/eval/REPORT.md)。

## 3. 本批结论（不要重新推导）

- 写作规则这一轴已经饱和：所有成功产物都覆盖 12 个要点，长度差来自展开和图示，不是漏事实。
- 瓶颈是 **JSON-in-text 协议**：长 `write_note` 的 JSON 可能解析失败（audit-2 的失败原因）；根治要原生 tool calling。
- `plan` 档不值：成本最高、2/3 成功、覆盖率不升。
- `free` 是修复前最稳的档（3/3）；`minimal` 加上自动结构状态后也 3/3，且更便宜，所以默认保持 `minimal`。
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

- JSON-in-text 长写入仍可能解析失败；原生 tool calling 是下一项。
- 运行页轨迹依赖 `agent-run.json`：实时回合在窗口内存里，运行页中途刷新要等落盘；失败轨迹现在也会落盘。
- 运行页只做了本次的 1440 档实拍；1040 / 暗色下轨迹面板没有逐页审计。
- 本批未跑 `npm run tauri build`，安装版不是这批代码。
- 自动结构状态里的上限（20 节 / 6,000 字）是启发式，不是审美判断。

## 7. 新会话开场模板

```text
按 docs/SESSION_START.md 开始，先读 docs/HANDOFF.md。
这次只做：<一句话任务>。
先复述当前分支/未提交状态、相关限制和复测命令，再动代码。
```
