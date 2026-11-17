# 当前交接卡（2026-11-17）

> 这是**状态快照**，不是永久规范。新会话以 `git status/log` 和当前文件为准；数字冲突时以最新实测为准。
> 永久工作流看 [`SESSION_START.md`](SESSION_START.md)，配色依据看 [`THEMES.md`](THEMES.md)、[`COLOR_RESEARCH.md`](COLOR_RESEARCH.md)。

## 1. 当前状态

- 分支：`feat/native-production-facts-ui`
- 最后已推送：`63e55f0`（`docs: 记录配色调研、主题系统与 P2 收尾`）
- 工作区有**未提交**内容：
  - 右键菜单裁剪修复：`src/components/NoteList.vue`、`tools/verify-context-menu.mjs`
  - 侧栏滚动条贴边：`src/lib/overlayScrollbars.ts`
  - 对应文档/截图：`docs/SIDEBAR.md`、`docs/measured.json`、`docs/experiment-log.txt`、`shots/audit/README.md`、`shots/audit/260-*`、`261-*`
- 最近构建：安装包 `8,797,820 B`，exe `22,126,592 B`；安装版已重启。
- `sample-vault/` 下未跟踪的个人笔记**不是代码改动**，不要提交。

## 2. 已经定下、不要重新推导的事

### 配色与主题

- 四套内置主题：`neutral` / `warm` / `graphite` / `noblue`。
- 模式：`system` / `light` / `dark`，默认 `neutral + system`。
- 主题是**设备本地偏好**（localStorage），不参与云同步。
- 设计规则：
  - 一套中性表面 + 一个主色；
  - `--blue-soft` 是中性选中底，不做大面积淡蓝；
  - `--gold` 只做 warning，不再表示历史/档案；
  - 历史/史料分类用中性 chip；
  - 状态色只用于真正的状态，必须配合文字/形状。
- 不要重开“土黄 + 淡蓝”的调研；调研结论和四套 demo 在 `docs/COLOR_RESEARCH.md`、`docs/THEMES.md`。

### 设置页

- 路由：`/settings/:section?`，默认 `/settings/appearance`。
- 顶栏「设置」进入；设置页隐藏笔记侧栏。
- 设置页使用注册表 `src/lib/settingsSections.ts`；新增设置只加 section + 面板，不改导航结构。
- 主题卡预览必须用主题**自己的固定色**，不能跟当前主题 token 变。
- 明暗模式单选自动保存；主题卡支持键盘方向键。
- 「制作与模型」直接跳制作台设置，不复制表单。

### 布局

- 索引页宽度 `--index-width: 1120px`；详情页 `--detail-width: 980px`；左右 padding `--page-pad-x: 30px`。
- 知识库列表/详情同层同宽；不要退回各页写死 1000/1120/900/980。

### 已收尾的 P2

- 事实详情状态行去重：candidate 无来源只留「未核验」，有来源显示「N 条来源待核验」。
- 订阅标题「共 N 个」，核清 9 vs 1 行是首屏截断。
- 任务左栏徽标只显示进行中；长 BV 有 tooltip。
- 设置 subnav 分 3 组，AI 面板只保留 1 个保存按钮。

仍在边界里、不要当成 bug：

- 标签台两个搜索并排（信息架构选择）。
- 任务表列密度没有重排。
- 图谱是刻意保留的深色数据画布；少数旧光晕 `rgba` 未清。
- 标签台/制作台/任务详情/图谱没有逐页跑 8 组合主题审计。

## 3. 硬约束

- `ctext` 网页抓取被禁止，只能使用 `api.ctext.org`；暂时不申请 ctext key。
- 只保留联网搜索、自动溯源、候选粘贴队列；不做复杂的本地/离线史料手工录入。
- 非历史事实不得显示「史料」区。
- 不得在文档/日志中打印或存储 Bilibili cookies。
- 不主动 commit/push，除非用户明确要求。
- 公开仓库：不要把个人路径、密钥、cookies、`sample-vault/` 个人内容提交进去。
- 改代码前先看 README 的「没做完的 / 已知限制」，不要把手动上限当 bug 改掉。
- 一次只改一个变量；视觉改动导出 PNG 后交给独立 `visual-judge` 验收。
- 数字必须标来源：实测 / 精确计算 / 模型估算。

## 4. 当前已知坑

- 右键菜单：`.tree-window` 的 `will-change: transform` 会成为 `position: fixed` 的包含块；菜单必须 Teleport 到 `body`，不能只改 `z-index`。
- 滚动条：`.tree-scroll` 的内滚动框比侧栏可见右缘少 5px；侧栏 overlay 要按 `.sidebar` 右缘定位，hover fallback 也要覆盖侧栏 padding。
- 设置页：不要再给 shell 加 `settings-mode` 类，会和模式分段控件的 `.settings-mode` 冲突；当前 shell 用 `settings-shell`。
- 主题闪烁：`index.html` 的同步脚本必须在 module 之前设置 `data-theme/data-mode/color-scheme`；不要退回只靠 `prefers-color-scheme`。
- 主题 token 在 `src/styles/themes.css`，导入顺序在 `panels.css` 之后；旧 `app.css` 的暗色媒体块仍存在，但主题选择器优先级更高。
- 构建/安装：`npm run build` 只验前端；要更新安装版必须 `npm run tauri build`，关闭旧进程后替换 `%LOCALAPPDATA%\页边笔记\yebian.exe` 再启动。
- 右键菜单回归需要 app 带 CDP 9222：`node --no-warnings tools/verify-context-menu.mjs`。

## 5. 新会话开场模板

最短可用：

```text
按 docs/SESSION_START.md 开始，先读 docs/HANDOFF.md。
这次只做：<一句话任务>。
先复述当前分支/未提交状态、相关限制和复测命令，再动代码。
```

如果需要更高效率，再补一句范围：

```text
只改 <文件/页面>；不要重做主题调研，不要动 sample-vault，不要自动 commit/push。
```

不要只说“按 docs/SESSION_START.md 开始”就期待它知道当前做到哪：`SESSION_START` 是流程，`HANDOFF` 是状态，两者一起才够。
