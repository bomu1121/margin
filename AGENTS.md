# AGENTS.md

项目级约定，与其他说明冲突时以本文件为准：

- 新会话先按 [docs/SESSION_START.md](docs/SESSION_START.md) 做基线复述，再动代码。
- 提交信息遵循 [CONTRIBUTING.md](CONTRIBUTING.md)。
- 提交按层分组：Rust 后端、前端页面、图谱、工具、文档分开；不把无关改动塞进同一个 commit。
- 改代码前先看 README 的「没做完的 / 已知限制」，不要把手动设的上限当成 bug 直接改掉。
