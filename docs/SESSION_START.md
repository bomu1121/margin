# 新会话启动流程

## 1. 打开正确的工作区，先看分支状态

打开仓库根目录：

- 本机：`D:\Develop\yebian`
- 家里：`%USERPROFILE%\margin`

不要打开 `D:\Develop`、上层目录或 `draft-*` 会话目录，否则项目级 `AGENTS.md` 不会加载。

开新会话先确认：

```bash
git status -sb
git log --oneline -5
```

如果当前不是 `main`：

- 先看 `git log --oneline main..HEAD`，确认这个分支上积了什么；
- 不要自动 `pull` / `merge` / `rebase` / 切回 `main`；
- 只有存在 upstream 时才谈 `git pull --ff-only`；本地功能分支没有 upstream 是正常的。

`sample-vault/ScribeFlow/`、`sample-vault/.yebian/` 是运行时产物，已被 `.gitignore` 忽略。`git status` 里没有它们，不代表功能没做。

当前 `sample-vault` 采用一级话题目录：`历史 / 时事 / 科技 / 社会 / 游戏 / 开发测试`；自动制作的新笔记先落顶层 `待整理/`，由用户再移入话题目录。

## 2. 第一轮只读基线，不改代码

把下面这段直接发给新会话：

```text
先不要改代码。

先读：
- 项目根 AGENTS.md、CONTRIBUTING.md、docs/SESSION_START.md
- README.md 顶部数字 +「8. 没做完的 / 已知限制」
- docs/measured.json（结构化实测）
- docs/experiment-log.txt 的最后 2–3 条（最新改动与限制）

再按这次任务选读最小集合，不要一次全读：
- 制作 / 订阅 / 模型 / 续跑：
  docs/PRODUCTION.md、docs/PRODUCTION_BACKLOG.md、docs/SUBSCRIPTION_DESIGN.md、
  docs/MODEL_PROVIDER_DESIGN.md、docs/RESUME_DESIGN.md
- 笔记 / 阅读 / 搜索 / 左右栏 / 视觉：
  docs/NOTE_READING_AUDIT.md、docs/SEARCH_DESIGN.md、docs/PANEL_INTERACTION.md、
  docs/UI_AUDIT.md、docs/SIDEBAR.md、DESKTOP.md 第 7 节
- 事实 / 史料 / 候选 / 整理 / 拆分：
  docs/FACTS_DESIGN.md、docs/FACT_CURATION_DESIGN.md、docs/FACT_SPLIT_DESIGN.md、
  docs/HISTORY_SOURCES_DESIGN.md、docs/HISTORY_SEARCH_DESIGN.md、
  docs/HISTORY_EVAL.md、docs/TRACE_SEARCH_DESIGN.md、docs/SOURCE_QUEUE_DESIGN.md
- 索引 / 数据层：
  docs/INDEXING.md、docs/INDEXING-SPLIT.md，以及 README 的本地数据一节
- 旧 ScribeFlow 协议 / 历史：
  docs/SCRIBEFLOW.md、docs/SCRIBEFLOW_FEATURE_AUDIT.md

然后复述：
1. 当前基线数字和测量口径（测试数、安装包、笔记/事实数量，写清是实测还是估算）；
2. 当前分支和未提交状态；哪些改动已经在分支上、哪些还没提交；
3. 哪些限制是刻意设的，不能当 bug 改；
4. 这次只改哪一个变量，以及改完用哪条命令复测。

确认后再动代码。
```

这一轮的目的不是让 AI 表演读过文件，而是让它先暴露自己理解错了什么。基线复述不对，后面的数字都不可信。

## 3. 文档地图（按任务取最小集合）

| 任务 | 先读 |
| --- | --- |
| 总览、最终数字、已知限制 | `README.md` 顶部 + 第 8 节、`docs/measured.json` |
| 制作台、订阅、模型、ASR、续跑 | `docs/PRODUCTION.md`、`PRODUCTION_BACKLOG.md`、`SUBSCRIPTION_DESIGN.md`、`MODEL_PROVIDER_DESIGN.md`、`RESUME_DESIGN.md` |
| 笔记阅读、标题/侧栏密度、搜索、标注 | `docs/NOTE_READING_AUDIT.md`、`docs/SEARCH_DESIGN.md`、`docs/SIDEBAR.md`、`docs/ANNOTATION_REDESIGN.md` |
| 左右栏、浮层滚动条、响应式 | `docs/PANEL_INTERACTION.md`、`docs/UI_AUDIT.md` |
| 事实、来源、引用、候选、整理、拆分 | `docs/FACTS_DESIGN.md`、`FACT_CURATION_DESIGN.md`、`FACT_SPLIT_DESIGN.md`、`SOURCE_QUEUE_DESIGN.md`、`FACT_AUDIT.md`（争议状态口径） |
| 历史检索、史料质量、自动溯源 | `docs/HISTORY_SOURCES_DESIGN.md`、`HISTORY_SEARCH_DESIGN.md`、`HISTORY_EVAL.md`、`TRACE_SEARCH_DESIGN.md` |
| 索引、FTS、切库 | `docs/INDEXING.md`、`docs/INDEXING-SPLIT.md` |
| 桌面壳、性能、打包 | `DESKTOP.md` 第 7 节、`README.md` 的安装包/启动数字 |
| 旧协议、迁移背景 | `docs/SCRIBEFLOW.md`、`docs/SCRIBEFLOW_FEATURE_AUDIT.md` |

`docs/experiment-log.txt` 是时间线，不要从头读到尾；只看最后几条和与任务相关的 P 编号。`docs/measured.json` 是结构化数据，数字冲突时以它和最新日志为准。

## 4. 每一轮改动

- 一次只改一个变量。功能、重构、样式不要混在一轮里。
- 改之前先跑基线命令；改完用同一条命令复测。
- 结论必须带数字和来源：`docs/measured.json` 或 `docs/experiment-log.txt`。
- 视觉改动导出 PNG，交给独立的 `visual-judge` 验收，不要自己看完就算过。
- 改完更新 README 的已知限制；新增数字写进 `docs/experiment-log.txt`。
- 发现测量工具本身有假通过/假失败，先修工具，再信结果。

## 5. 收尾

- 跑一次前端构建：`npm run build`
- 动过 Rust、窗口、打包：跑 `npm run tauri build`，并在原生窗口验收
- 按 `CONTRIBUTING.md` 分组提交：Rust 后端、前端页面、图谱、工具、文档分开；不把无关改动塞进一个 commit
- 只有用户明确要求才 push；push 后确认 `git status -sb` 干净
- 提交前先看 `git diff --stat` 和 `git status --short`，不要把生成物、临时脚本、运行时 vault 带进去

## 6. 不要做的事

- 不要在新会话里重新推导已经写在 README 里的结论。
- 不要因为代码“看起来能跑”就省掉实测。
- 不要把 `dist/`、`src-tauri/target/`、`node_modules/` 提交进去。
- 不要把 `AGENTS.md` 里的个人规则和本机路径复制到公开仓库。
- 不要把 `sample-vault/ScribeFlow/` 的运行时笔记改动当成代码改动提交。
- 不要在功能分支上自动 merge 到 `main` 或 force push。
