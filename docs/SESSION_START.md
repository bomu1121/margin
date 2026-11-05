# 新会话启动流程

## 1. 打开正确的工作区

打开仓库根目录：

- 本机：`D:\Develop\yebian`
- 家里：`%USERPROFILE%\margin`

不要打开 `D:\Develop`、上层目录或 `draft-*` 会话目录，否则项目级 `AGENTS.md` 不会加载。

开新会话前先确认：

```bash
git status -sb
git pull --ff-only
```

## 2. 第一轮只读基线，不改代码

把下面这段直接发给新会话：

```text
先不要改代码。

读项目根的 AGENTS.md、CONTRIBUTING.md、README.md 的「没做完的 / 已知限制」、
DESKTOP.md 第 7 节、docs/measured.json 和 docs/SCRIBEFLOW.md（ScribeFlow 集成状态矩阵与 SF-xxx backlog）。

然后复述：
1. 当前基线数字和测量口径；
2. 哪些限制是刻意设的，不能当 bug 改；
3. 这次只改哪一个变量，以及改完用哪条命令复测。

确认后再动代码。
```

这一轮的目的不是让 AI 表演读过文件，而是让它先暴露自己理解错了什么。基线复述不对，后面的数字都不可信。

## 3. 每一轮改动

- 一次只改一个变量。功能、重构、样式不要混在一轮里。
- 改之前先跑基线命令；改完用同一条命令复测。
- 结论必须带数字和来源：`docs/measured.json` 或 `docs/experiment-log.txt`。
- 视觉改动导出 PNG，交给 `visual-judge`，不要自己看完就算过。
- 改完更新 README 的已知限制；新增数字写进 `docs/experiment-log.txt`。

## 4. 收尾

- 跑一次前端构建：`npm run build`
- 动过 Rust、窗口、打包：跑 `npm run tauri build`，并在原生窗口验收
- 按 `CONTRIBUTING.md` 分组提交，不把无关改动塞进一个 commit
- 推送后确认 `git status -sb` 干净

## 5. 不要做的事

- 不要在新会话里重新推导已经写在 README 里的结论。
- 不要因为代码“看起来能跑”就省掉实测。
- 不要把 `dist/`、`src-tauri/target/`、`node_modules/` 提交进去。
- 不要把 `AGENTS.md` 里的个人规则和本机路径复制到公开仓库。
