# 制作任务断点续跑：调研与实现方案

> 状态：**调研完成，尚未实现**（2026-11-07）。
> 目标编号：P2-02。
> 结论先行：不引入 Temporal/Inngest 这类外部引擎；在现有 SQLite + 本地文件上做
> **单元级检查点 + 从顶部重放（memoized replay）**。已完成且输入没变的调用直接复用，
> 失败只重跑缺失单元；发布单独做幂等。这样既不改本机离线模型，也能把“重试整条模板”
> 变成“继续未完成的部分”。

---

## 1. 结论

1. **检查点粒度不能只到步骤。** 先做步骤级只能避免“重跑整个模板”，但在一个步骤内
   （例如 18 分钟视频的 2 段 ASR、3 段校对、两次 AI 加工）失败仍会重复计费。
   要做 **每个外部调用 / 每个 ASR 分段一个检查点**。
2. **恢复方式采用“从顶部重放 + 单元级 memoization”**，不额外维护“从第 N 步进入”的
   分支代码。`run()` 仍从第一行开始，已完成的单元返回持久化结果；这与 Inngest
   `step.run()` 的模型一致，也比手写“从中途继续”更难出错。
3. **检查点必须带输入哈希。** 只存一个 `done` 标记会在提示词、模型、源文本变化后
   复用错误结果。哈希不匹配就重算；下游自然因为上游输出变化而重算。
4. **发布必须单独做幂等。** AI 全部复用后，`publish` 仍可能因为“已写入 outbox 但没
   finish”而再次发布，生成重复 revision。续跑前要先认领已经存在的 package/registry。
5. **任务要绑定创建时的笔记库和执行配置。** 现在 job 没有存 vault 路径、base URL、
   ASR 配置；切换库或改设置后重试可能把任务写到另一个库，或让续跑结果与已缓存的
   上游不一致。
6. **失败任务的 usage 要落盘。** 当前 `fail()` 不保存已发生的调用，实测第一次失败时
   job usage 是 0，但模型已经计费。检查点行要同时存 usage，失败后至少能显示已知成本。

---

## 2. 当前实现审计

### 2.1 现有阶段与外部调用

| 阶段 | 外部/副作用 | 当前可复用产物 | 断点粒度 |
| --- | --- | --- | --- |
| `source` | B站 preview、音频 URL、下载；收藏夹逐个下载 | 临时音频文件 | 每个视频/每个下载 |
| `transcribe` | ffmpeg 预处理；MiMo 10 分钟分段；OpenAI ASR | 分段文本 | **每个 ASR 分段** |
| `refine` | 每 6000 字一次 chat | 每段校对文本 | 每段 |
| `prompt` | 每 12000 字一次 chat；多风格；速览版额外弦外之音；多段 merge | 每个 style/part、merge、弦外之音 | 每次调用 |
| `chapter` / `merge` / `drill` / `trace` | 每次 1 次或多次 chat/search | 调用结果 | 每次调用 |
| `extract` | 1 次 chat | 实体 JSON | 每次调用 |
| `publish` | 写 outbox、drain、registry、笔记 | note path/revision | package/registry 幂等 |

当前 `pipeline::run` 是线性函数：任一 `?` 直接冒泡，`ProductionServiceHandle::run`
只调用 `store.fail(id, error)`，不保存中间状态。`production_steps` 只记录状态和
一段 detail，没有 payload；`production_jobs` 只在 `finish` 时写 `result_json` 和
`usage_json`。

### 2.2 失败实测：重试整条链路会重复付费

用本地 mock（每个成功 chat 固定返回 `1M prompt + 1M completion`），手动价
¥10/M 输入、¥30/M 输出，设置 `retries=0`；第 2 次调用注入 HTTP 500：

| 指标 | 第一次失败 | 点“重试”后 |
| --- | ---: | ---: |
| mock 收到请求 | 2 | 2 |
| 成功调用 | 1（校对） | 2（校对 + 加工） |
| job 记录 usage | 0 | 2M / 2M |
| job 记录成本 | ¥0 | ¥80 |
| 实际成功调用总额 | **3M / 3M** | **3M / 3M** |
| 实际应计成本 | **¥120** | **¥120** |

结论（实测）：
- 一条本来只需要 2 次调用的文本任务，失败一次后共发出 **4 次请求、成功 3 次**。
- 第一次成功的校对 1M/1M 被完全丢掉；重试又从校对开始。
- job 最终显示 ¥80，实际 mock 侧成功调用为 3 组 1M/1M，按手动价应 **¥120**。
- 失败瞬间 job usage 是 0，用户看不到已经花掉的钱。

这不是模拟的用户故事，而是本机 mock 的调用计数；失败注入和临时库已在测试后清理。

### 2.3 代码审计发现的其他风险

1. **ASR 分段结果不落盘**：`transcribe_mimo_segments` 在临时目录里逐段调用，任何
   一段失败会删除整个临时目录；前几段已经计费但没有任何记录。
2. **publish 的重入窗口**：`publish()` 先创建 revision package，再 `drain`，最后
   `finish`。如果进程在 `drain` 成功后、`finish` 前退出，续跑会再次 `publish`，
   `next_revision` 会生成新的 revision；现有 outbox 的 ACK 逻辑只能防止重复消费，
   不能阻止“又创建一个新 package”。
3. **job 没有绑定 vault**：`production_retry` 使用当前 `vault_root`。用户切换笔记库后
   重试旧任务，结果会写到新库。
4. **job 没有执行配置快照**：`job.provider/model` 有快照，但 base URL、协议、ASR
   engine/model、检索 provider、默认提示词都取当前 settings；续跑时这些值可能已经变。
5. **步骤状态没有“已复用”语义**：`set_step` 只有 pending/running/done/error/skipped；
   续跑后无法在界面上区分“这次真的跑了”和“从检查点恢复”。
6. **`clear_temp` 会无条件删除临时音频**：如果以后把音频路径写进检查点，清理临时文件
   会让续跑失效；必须先定义保留策略。
7. **失败 usage 丢失**：`fail()` 只写错误，不接收已经累积的 usage；这与 2.2 的实测一致。

---

## 3. 外部方案调研

### 3.1 Temporal

- Activity **至少执行一次**；进程在“外部副作用已发生、结果未记录”之间崩溃时会重试，
  所以副作用必须幂等，idempotency key 必须由业务数据稳定派生，不能每次尝试生成新 UUID。
- 长 Activity 用 `heartbeat(progress)` 保存检查点，下一次尝试通过 heartbeat details 从
  检查点继续；heartbeat 也用于取消和检测卡死。
- 重试策略按阶段区分：网络瞬时错误短退避，限流长退避，永久错误标记 non-retryable；
  需要 dead-letter 而不是无限重试。
- Workflow 本身不应直接做 I/O；副作用放到 Activity，便于恢复。

对本项目的启示：**检查点是数据，不是“状态文案”**；每个单元要有稳定 ID、输入哈希、
结果和 usage；永久错误不要伪装成可重试。

### 3.2 Inngest

- `step.run()` 的结果会 memoize：函数因重试/崩溃被重新调用时，已完成的 step 直接返回
  保存的结果，未完成的才执行。
- handler 从顶部重新执行；**step 外面的代码会再跑一遍**，所以副作用必须放进 step。
- step ID 必须稳定，不能带时间戳/随机值；循环里可以复用，但引擎用计数器区分。
- 官方也提醒：请求成功但结果记录前失败，仍可能重复调用；外部副作用仍需幂等键。
- step result 计入持久化状态，大文件应存外部存储，step 只传引用。

对本项目的启示：**“从顶部重放 + 单元 memoization”** 比“从中途入口继续”简单可靠；
本地纯计算（拼字符串、解析）重跑无害，不需要为它们建检查点。

### 3.3 LangGraph

- checkpointer 保存 super-step 的状态和 pending writes；恢复时已完成的节点不重跑，
  失败节点之后的节点继续执行。
- durability 有 `exit` / `async` / `sync`：同步写最稳但慢；异步写有“崩溃时丢最后一个
  checkpoint”的窗口；只在退出时写则无法从中途崩溃恢复。
- 第一次 checkpoint 落盘前崩溃，线程看起来像从未启动；需要在 checkpointer 外记录
  “已接收”。
- checkpoint 会无限增长，需要清理策略。

对本项目的启示：**落盘时机要明确**；每次外部调用成功后立即同步写 SQLite，而不是等
步骤结束；同时要处理“第一次调用前就崩溃”的空任务。

### 3.4 n8n

- 重试会从保存的执行状态继续，已完成节点不重跑；**失败节点会重新处理它的全部输入项**，
  所以一个节点内已经发生的部分副作用可能重复。
- 节点级 `Retry On Fail` 只重跑该节点；完整 workflow 重试仍会重放失败节点。
- 社区确认没有原生“从任意节点继续”；需要节点幂等、错误 workflow 和状态表。
- 子工作流重试会从子工作流开头开始。

对本项目的启示：**步骤级重试不够细**；一个步骤内的多个 chunk 必须拆成独立单元，否则
就是 n8n 的“失败节点重跑全部输入项”。

### 3.5 Dify

- 节点有重试次数/间隔、默认值、fail branch；运行和节点记录持久化，重试历史单独记录。
- pause/resume 会保存变量池、运行状态，从暂停点继续；但**没有自动崩溃恢复**，也没有
  面向用户的“从失败节点继续”入口（社区有需求，未成为公开能力）。

对本项目的启示：产品上必须把“继续”做成明确的一等按钮；不能假设底层会自动恢复；
暂停/中断/失败三种状态要分开处理。

### 3.6 AWS Step Functions / durable execution 通用原则

- 默认 at-least-once；需要 at-most-once 的操作要单独设计，不能靠重试。
- 幂等键应由执行 ID + 步骤名稳定派生；键在步骤内部生成，重放时不能变。
- 数据库用条件写/唯一键/事务；避免无条件自增、重复 INSERT。
- **缓存 LLM 输出本身**，而不只是“done”标记；重试直接复用结果，避免重新生成和重复计费。
- 大结果存外部存储，状态里只放引用；失败要区分可重试/永久，并限制重试次数。

### 3.7 长音频转写

- 长音频按 30–600 秒或 API 限制分段，每段独立请求；失败只重试该段。
- 每段结果在进入下一段前落盘；重启从最后提交的段继续。
- 分段输入要有音频哈希 + 参数（模型、采样率、分段边界），否则改了预处理会复用旧文本。
- 失败段要显式记录，不能静默丢弃；全部段成功后才拼成最终 transcript。

---

## 4. 推荐设计

### 4.1 运行模型：从顶部重放，单元级 memoization

`pipeline::run` 保持线性，但每次外部调用都经过 `RunContext`：

```text
run(resume=true):
  source item 0      -> checkpoint hit? return payload : fetch + save
  ASR part 0         -> hit? return : call + save
  ASR part 1         -> miss -> call + save (失败则停在这里)
  refine part 0      -> hit? return : call + save
  prompt style 0 p0  -> ...
  publish            -> idempotent claim
```

- 纯本地计算（split、join、parse、进度）重跑，不建检查点。
- 有网络/计费/文件副作用的调用才建检查点。
- 失败时，已成功的调用已经落盘；下一次 `resume` 从顶部走，遇到命中直接跳过。
- 不需要维护“从第 N 步进入”的多个入口，减少分支和状态错位。

### 4.2 检查点单元

| 步骤 | `unit_key` 示例 | payload | 输入哈希必须包含 |
| --- | --- | --- | --- |
| source | `source:item:0` | item 元数据、音频相对路径、音频 SHA-256 | source spec、解析后的 bvid/cid、文件 size/mtime/SHA、pipeline version |
| ASR | `asr:item:0:part:0` | 该段文本 | 音频 SHA、ASR engine/model/base、分段边界 |
| ASR 汇总 | `asr:item:0` | 拼接文本、audioSeconds | 各段 payload 哈希 |
| 校对 | `refine:item:0:part:2` | 该段校对文本 | chunk 文本、校对 system hash、模型/provider/base |
| 加工 | `prompt:item:0:style:0:part:1` | 该段生成文本 | system hash、title、chunk、模型/provider/base |
| 多段合并 | `prompt:item:0:style:0:merge` | 合并文本 | parts 哈希、merge system hash、模型 |
| 弦外之音 | `prompt:item:0:style:0:xianyin` | section 文本 | body、xianyin system hash、源文本 |
| 章节 | `chapter` | 章节文本 | 源文本、chapter 参数、system hash、模型 |
| 合并 | `merge` | 合并文本 | item notes 哈希、system hash、模型 |
| 练一练 | `drill` | JSON + 解析结果 | body、题数、system hash、模型 |
| 溯源 | `trace:search` / `trace:chat` | hits / 解析结果 | query、provider、max、body、hits、模型 |
| 抽取 | `extract` | 实体 JSON | body、fields、system hash、模型 |
| 发布 | `publish:package` | revision、note path、body SHA、status | body、tags、entities、options、noteId、vault |

`unit_key` 只使用稳定的序号/步骤名，**不得包含时间戳、随机 UUID、重试次数**。
重试次数只用于日志，不参与 key。

### 4.3 SQLite 表

```sql
CREATE TABLE IF NOT EXISTS production_checkpoints (
  job_id TEXT NOT NULL,
  unit_key TEXT NOT NULL,
  step_key TEXT NOT NULL,
  kind TEXT NOT NULL,              -- text / chat / asr / source / publish
  input_hash TEXT NOT NULL,        -- sha256(canonical inputs + pipeline_version)
  payload_json TEXT NOT NULL,
  usage_json TEXT NOT NULL DEFAULT '{}',
  payload_bytes INTEGER NOT NULL DEFAULT 0,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  PRIMARY KEY (job_id, unit_key)
);
CREATE INDEX IF NOT EXISTS idx_production_checkpoints_step
  ON production_checkpoints(job_id, step_key, created_at);
```

- 文本/JSON 直接进 SQLite；音频只存临时目录相对路径 + SHA，不进数据库。
- 单行 payload 限 1 MiB（当前 transcript 4,438 字、note 4,024 字，远低于上限；
  这是**估算**，不是库中所有任务的上限）。
- 写入与 usage 一起在事务里提交；成功调用先落盘再进入下一步。
- `production_steps` 增加 `reused_units` / `total_units` 两个字段，用于 UI 显示
  “已复用 3/5 段”，不把检查点混进日志。
- `production_jobs` 增加：
  - `vault_path TEXT NOT NULL DEFAULT ''`：任务创建时的笔记库；
  - `config_json TEXT NOT NULL DEFAULT ''`：非密钥执行配置快照；
  - `pipeline_version INTEGER NOT NULL DEFAULT 0`；
  - `checkpoint_at INTEGER NOT NULL DEFAULT 0`。

旧任务的 `vault_path/config_json` 为空：**不允许 resume**，只能“重跑全部”；重跑时
使用当前配置并显示旧任务未绑定提示。

### 4.4 输入哈希与失效

统一函数：

```text
input_hash = sha256(canonical_json({
  pipeline_version,
  step,
  inputs...,
  model/provider/protocol/base_url,
  system_prompt_hash,
  relevant_options
}))
```

- 哈希不匹配：视为 miss，重算并覆盖该 unit；不删除旧行，后续查询只认当前哈希。
- 提示词改动：`system_prompt_hash` 变，`prompt` 及其下游全部 miss；`refine/ASR`
  仍可复用。
- 模型/接口/协议改动：所有 AI 单元 miss；`source`/下载仍可复用。
- 源文本/标题改动：`refine` 和后续 miss；ASR 可复用（如果音频没变）。
- `fields`/`tags` 改动：只影响 `extract/publish`，不重跑生成。
- `PIPELINE_VERSION` 在 prompt 组装、分段算法、解析逻辑变化时递增；防止“代码变了
  但输入哈希碰巧一样”。
- 两个自定义提示词同名、数据库顺序变化：按实际 system 文本哈希判断，不按名字。
- 源文件：本地文件把 size + mtime + SHA-256 写入 source 哈希；B站音频把下载后的
  SHA-256 写入 ASR 哈希。文件被替换后不能复用旧转写。
- `resume_info` 只统计当前哈希匹配的检查点；旧哈希行保留但不计入“可复用”，
  避免 UI 把过期结果说成还能用。

### 4.5 执行配置与 vault 快照

- 创建 job 时写入 `vault_path`（绝对路径 canonicalize）和 `config_json`：
  provider/model/protocol/baseUrl、ASR engine/model/baseUrl/ffmpegPath、search provider、
  defaultPrompt、exchangeRate、retries、fields、pipelineVersion，以及创建时的
  价格快照（目录价或手动价，含时间戳）。
- **不存 API Key**；运行时按 provider 从 SQLite 取当前 key（key 轮换后仍可继续）。
- 续跑沿用创建时价格快照计算成本，避免中途改手动价导致同一个任务前后两半口径不同；
  界面标明“创建时价目”。
- `resume` / `retry` 校验 vault；不一致返回“这个任务属于另一个笔记库”，不写入。
- 续跑使用 **job 快照配置**，不跟随用户后来改的设置；设置变更时提示“本次继续使用
  创建时配置；要按新配置请重跑全部”。
- 旧任务没有快照：只允许 full retry，使用当前配置，并在 UI 明确说明。

### 4.6 状态机

```text
queued -> running -> success
                    -> error
running -> cancelled
running/queued --(app exit)--> interrupted
error/interrupted/cancelled --resume--> queued -> running
error/interrupted/cancelled --retry_all--> queued -> running (clear checkpoints)
```

- `resume` 的数据库更新必须带条件：
  `WHERE id=? AND status IN ('error','interrupted','cancelled')`；影响 0 行就拒绝，
  防止双击/两个窗口同时启动。
- job 行本身就是“已接收”记录：即使第一次外部调用前崩溃，任务仍存在，续跑会从
  第一个单元开始，不会出现“线程不存在”；只是没有可复用内容。
- 续跑开始时把 `usage_json` 归零，按检查点重新累加，避免把上一次运行和本次复用重复算。
- 成功步骤保持 `done`；失败步骤在进入时改 `running`；未到达步骤保持 pending。
- `recover_interrupted` 只改 job 状态和 running 步骤，**不删除检查点**。
- 取消语义：取消不删除检查点；用户可显式“继续”，但不自动恢复，避免违背取消意图。

### 4.7 发布幂等

`publish` 增加一个“认领”流程：

1. 计算 `body_sha256`（原始 note body，不含 front matter 渲染）。
2. 查 `publish:package` 检查点：
   - 有且 hash 相同，且 package/registry 有效：直接返回 note path；
   - 有但 package 缺 READY/ACK：先 `drain`，验证 registry，再返回；
3. 没有检查点时，扫描 `.yebian/outbox/<noteId>/*/artifact.json`：
   `run.run_id == job.id` 且 `note.md` 的 SHA 匹配：复用该 package，不新建 revision。
4. 再查 registry：如果 `runId == job.id` 且 note 文件存在，直接写检查点并返回。
5. 都不存在才创建新 revision；创建后检查点记录 revision/body hash，再 drain、ACK。
6. `finish` 成功后才删除临时目录/检查点；崩溃在 finish 前，下一次续跑走认领路径。

这样即使崩溃在“package ready → drain → finish”之间，也不会生成重复笔记或第二条 revision。
现有 `outbox::drain` 的“registry revision 已存在则只 ACK”逻辑继续作为第二层保护。

### 4.8 成本与 usage

- 每个检查点行携带该次调用的 usage；成功调用写入时同时更新当前 job 的已知 usage。
- 失败时 `fail_with_usage(id, error, usage)` 写入已发生成本；用户至少能看到“这次失败
  已经花了多少”。
- 续跑从 0 开始，但读取命中单元时把其 usage 加回；最终 `finish` 只写一次总和。
- 同一个 unit 不会因为重试而重复计入：命中只加一次；miss 调用成功才写一次。
- 请求发出但响应丢失、无法知道服务商是否计费的情况，仍按“未记录”处理；这是
  本地应用无法消除的 at-least-once 成本。UI 标注“已知成本”而不是“绝对账单”。
- 旧任务（本功能之前）没有检查点，只有旧的最终 usage；不伪造补账。

### 4.9 临时文件与清理

- 音频/分段放 `.yebian/production-tmp/<jobId>/`，检查点只存相对路径 + SHA。
- MiMo 分段改成确定性路径：`production-tmp/<jobId>/asr-parts/item-<i>/part-<j>.mp3`；
  不再使用随机 `std::env::temp_dir()` 目录，否则失败时无法定位已生成的分段。
- ASR 分段目录在成功/删除任务时清理；失败/中断时保留，由续跑复用或用户清理。
- 成功：`finish` → 删除检查点和 temp（检查点不再需要）。
- 失败/中断/取消：保留 temp 和检查点，供继续；数据页显示每个任务的检查点大小。
- `clear_temp` 默认跳过“有检查点的未完成任务”，或弹出明确提示“清理会让这些任务
  无法续跑”；不能静默删除。
- 删除任务：删除 job/steps/logs/checkpoints/temp；已入库笔记不动（与现有语义一致）。
- `clear_finished` 同步删除检查点；不自动按时间清理（用户可能过几天才继续）。
- 建议在数据页加“可续跑任务占用”一栏，避免 temp 无限增长。

### 4.10 UI

失败/中断/取消的任务卡片：

```text
制作失败
AI 接口 500 ...
已保存 7 个中间结果（最近 14:32），继续会复用转写和校对，从「加工」第 2 段开始。
[继续制作] [重跑全部] [导出日志]
```

- “继续制作”是主按钮，仅当 `resumeInfo.canResume`。
- “重跑全部”二次确认：会清空检查点、重新调用模型；不会删除已入库笔记，但可能生成新 revision。
- 步骤 tab：每步显示 `已复用 3 段` / `本次执行 2 段` / `未开始`；日志增加
  “复用检查点 refine:item:0:part:0”。缓存命中的日志只写一行摘要，不重复粘贴整段
  文本，避免日志膨胀。
- 若任务已发布但未 finish：显示“笔记已入库，继续只会完成任务状态，不会重复生成”。
- vault 不一致：禁用继续，提示切换回原库。
- 配置变化：提示“已保存的上游仍可复用；加工会按创建时配置继续”；提供“按当前配置重跑全部”。
- 任务列表：error/interrupted 且 checkpoints>0 显示“可继续”徽标。
- 不显示原始检查点 JSON；只显示数量和步骤摘要，避免界面噪声。

### 4.11 API

```text
production_resume_info(id) -> {
  canResume, reason, checkpointCount, checkpointBytes,
  reusableSteps: [{key,label,units,tokens,costCny}],
  savedCalls, savedTokens, savedCostCny,
  published, vaultMatches, configChanged
}
production_resume(id) -> Job
production_retry(id) -> Job        // 语义改为“全部重跑”，先清检查点
```

- `resume_info` 每次打开任务详情时读取；不要在前端自己推断。
- `resume` 与 `retry` 都在后端做状态校验；前端按钮不是安全边界。
- 旧前端调用 `production_retry` 仍可用，但现在是 full retry；新 UI 明确区分两个按钮。

---

## 5. 失败语义与边界

| 情况 | 行为 |
| --- | --- |
| 网络 5xx/超时/429 | 调用层按现有 retries 退避；失败后保留检查点，可继续 |
| 401/403/404/模型不存在 | 视为永久错误，不自动无限重试；用户修正 key/模型后继续 |
| ffmpeg 失败 | 不写 ASR 检查点；保留输入音频；修复 ffmpeg 后继续 |
| ASR 第 2 段失败 | 第 1 段已落盘，继续只请求第 2 段 |
| 加工第 2 个 style 失败 | 第 1 个 style 已落盘，继续只跑第 2 个 |
| 发布失败 | 已有 body 检查点，继续只重试 publish，不重跑 AI |
| 发布成功但 finish 前崩溃 | registry/outbox 认领，不新增 revision |
| 应用退出 | `interrupted`；检查点保留；可继续 |
| 用户取消 | 不自动继续；保留检查点，可手动继续 |
| 切换笔记库 | 拒绝 resume；提示回原库 |
| 提示词/模型变化 | 对应单元 miss；上游可复用；UI 提示 |
| 旧任务无检查点 | 只能 full retry；不显示“可继续” |
| 双击继续 | 条件状态更新只允许一个请求进入 running |
| 删除任务 | 检查点/temp 一并删除；已入库笔记不删除 |

---

## 6. 验收方案

### 6.1 必测矩阵

1. **文本任务，第 2 次 chat 失败**：第一次 1 次成功；继续后只补第 2 次。总成功调用 2，
   job usage 2M/2M，成本按手动价 ¥80；不再出现 4 次请求。
2. **ASR 两段，第二段失败**：第一段落盘；继续只调用第二段；最终 transcript 与全量
   重跑一致，audioSeconds 不重复。
3. **多风格，第二风格失败**：第一风格复用；继续只生成第二风格；variants 不重复。
4. **publish 失败**：继续 0 次 AI 调用；只重试入库；revision 只增加 1。
5. **publish 成功后崩溃**：模拟 finish 前退出；继续不新建 package，笔记只有一条，
   revision 不增加。
6. **应用重启**：running → interrupted；继续成功；步骤/usage 正确。
7. **提示词改动**：refine/ASR 复用，prompt 及下游重跑；不会复用旧 prompt 输出。
8. **源文本/标题改动**：对应下游失效；ASR 仍复用。
9. **双击继续/并发继续**：只有一个执行实例；第二个请求得到明确错误。
10. **删除/清理**：删除任务清 checkpoints/temp；clear_temp 不会静默破坏可续跑任务。
11. **旧任务**：没有检查点，只能 full retry；不误报可继续。
12. **vault 切换**：resume/retry 被拒绝或要求确认，不写错库。

### 6.2 量化指标

- mock 请求计数：续跑后只增加缺失单元数。
- usage：job 最终 = 命中单元 usage + 本次新增；与 mock 成功调用总额一致。
- revision：发布续跑前后只增加 0 或 1 次，不能重复。
- 检查点大小：一个 18 分钟视频（4,438 字 transcript、4,024 字 note）预计 <100KB
  JSON；音频另计约 3–4MB/段（**估算**，实际以任务目录为准）。
- UI：失败卡显示可复用单元数；步骤 tab 的复用/本次执行数量相加等于步骤总单元数。
- 可读性：沿用 12px / 4.5:1 审计门槛。

### 6.3 工具

- `tools/mock-ai.mjs`：可配置“第 N 次调用失败/成功”、记录调用次数和 usage。
- `tools/verify-resume.mjs`：CDP 驱动打包版，在临时 vault 中建任务、断言失败、
  点继续、读 job usage/revision/note 数量，清理临时目录并恢复 vault/settings。
- Rust 单测：checkpoint store 的 upsert/hash/cascade、状态机条件更新、
  publish 认领逻辑、usage 汇总。
- 端到端必须用临时 vault；不得写入 `sample-vault`。

---

## 7. 实施顺序

1. **E-01**：mock server + 失败注入；先把“现在会重复调用”固化成基线。
2. **Schema + 快照**：`production_checkpoints`、`vault_path`、`config_json`、
   `pipeline_version`；创建 job 时写入；旧任务迁移为不可 resume。
3. **RunContext**：checkpoint 读写、输入哈希、usage、同步落盘；先接
   `source/ASR/refine`，用 mock 单测验证。
4. **接完整 pipeline**：prompt/chapter/merge/drill/trace/extract；每次调用一个 unit。
5. **publish 幂等**：package/registry 认领；补“发布后崩溃”测试。
6. **resume API + UI**：失败卡、步骤复用标记、全量重跑确认、vault 错误。
7. **验收与文档**：6.1 矩阵、截图、measured/experiment-log、诚实清单。
8. 每一层单独提交：Rust 存储/引擎 → 前端 → 文档；不把 schema、UI、文案混在一个 commit。

---

## 8. 诚实清单

- 2.2 的基线是**文本任务 + 本地 mock**，不是 B站/ASR 真实账单；ASR 分段丢失是代码审计结论。
- 检查点大小 <100KB 是**按已有 4,438 字 transcript / 4,024 字 note 估算**，不是全库统计。
- 检查点存的是完整转写/笔记文本，敏感度与笔记相同；它们在本机 `%APPDATA%` 的 SQLite
  中，不在 vault 内，不随 vault 同步。
- “at-least-once”意味着请求已发出但响应丢失时仍可能重复计费；本方案只能减少可观测的重复，
  不能保证服务商侧 exactly-once。
- 发布幂等依赖本地 outbox/registry 未被手工删除；用户手动删 package/note 后，续跑会按
  当前文件状态重新发布。
- 不做跨设备/跨 vault 续跑；任务绑定创建时 vault。
- 不做自动过期清理；检查点会占本地 SQLite，数据页只展示，不自动删。
- 不做“从任意历史 checkpoint 回滚”或分支工作流；只做当前任务向前续跑。
- 旧任务（本功能前创建）没有输入快照，不允许 resume；只保留 full retry。
- 本方案尚未实现；上面所有“会/将”是设计，不是已验收行为。

---

## 9. 参考

- Temporal Activities（至少一次、heartbeat、重试策略）：
  <https://docs.temporal.io/activities>
- Temporal 独立 Activity / 幂等与重试：
  <https://learn.temporal.io/tutorials/typescript/standalone-activities/>
- Inngest Durable Endpoints（step memoization）：
  <https://www.inngest.com/docs/examples/durable-endpoints>
- Inngest Durable Execution 最佳实践（step ID、副作用、结果记录窗口）：
  <https://github.com/inngest/website/blob/9645465201ccacaed41792565b58b7f6c822e517/pages/docs/durable-execution/best-practices/index.mdx>
- LangGraph Checkpointers（pending writes、durability、首次 checkpoint）：
  <https://docs.langchain.com/oss/python/langgraph/checkpointers>
- n8n 执行重试（已完成节点跳过、失败节点重跑全部输入）：
  <https://docs.n8n.io/build/understand-workflows/understand-executions/debug-executions/>
- Dify 节点错误处理（重试、fail branch、运行记录）：
  <https://enterprise-docs.dify.ai/en/3.13.x/use/build/predefined-error-handling-logic>
- Dify pause/resume 与运行状态（内部检查点、没有自动崩溃恢复）：
  <https://doc.holiday/library/dify/pause-resume-and-run-state/>
