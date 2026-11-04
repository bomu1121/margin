# 提交约定

提交信息用 Conventional Commits，主题用中文，一句话说清改了什么：

```
<type>(<scope>): <主题>
```

`type` 只用这几种：

- `feat`：用户能感知的新能力
- `fix`：修正行为
- `refactor`：不改变行为的整理
- `perf`：性能
- `docs`：文档、截图、验收记录
- `test`：测试与基准
- `chore`：构建、依赖、脚本

`scope` 用现有模块名：`note` / `tags` / `graph` / `search` / `annotations` / `desktop` / `db` / `tools`。

写法：

- 主题不超过 40 个汉字，不加句号，不用 emoji。
- 一个提交只做一件事。功能、重构、文档不要混在一起。
- 正文只在需要解释“为什么”时写，两三行说清取舍，不要复述 diff。
- `dist/`、`src-tauri/target/`、`node_modules/` 不入库。

示例：

```
feat(graph): 重做全库关系图的聚焦与面板
fix(db): 按 notes.rowid 删除 FTS，避免二次索引 O(N²)
docs: 补充最终 release 的启动与内存数据
```

以后新增的提交也按这个来。
