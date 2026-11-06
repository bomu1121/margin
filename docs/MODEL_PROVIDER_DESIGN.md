# 模型接入与计价监测：调研与改造方案

> 状态：**调研完成，待实现**。当前正式版 AI 设置仍是「选 provider + 手填模型 + 手填单价」，
> 这是内部工具思路，不适合作为产品默认路径。
> 日期：2026-11-06。实现前先按本文定 provider 范围、计价口径和界面。

## 1. 结论先行

1. **成熟产品不要求用户手填每百万 token 单价。** 它们做的是：
   **provider 预设 → 填 key → 拉模型列表 → 选模型 → 价格从模型目录自动带出**。
   手填价格只出现在「自定义 OpenAI-compatible / 私有部署」这种确实没有公开价目的场景。
2. **价格不是一个数，必须带口径。** 同一模型可能有输入/输出/缓存读/缓存写/推理/音频、峰谷价、长上下文阶梯价；
   官方价、网关转售价、订阅套餐价不是一回事。产品只能承诺「估算」，能拿到厂商实报金额时单独标注「账单」。
3. **页边笔记应该把「模型目录 + 价目表」当作数据，不当作设置。** 目录可刷新、可缓存、可离线回退；
   用户不需要知道 LiteLLM、OpenRouter 这些名字，界面只显示「参考价来源/更新时间」。
4. **单价手填从主界面移除。** 已知模型全部自动查价；查不到时只记 token，不编金额；
   自定义 provider 才在「高级」里允许覆盖价目。
5. **计价监测的最小闭环**是：每次调用归一化 usage（含缓存/推理/音频）→ 按目录价估算 →
   任务/月度汇总 → 可选余额对账。余额是账户级，不能假装是单次运行账单。

## 2. 主流产品怎么做的

| 产品 | 接入方式 | 模型选择 | 计价/监测 | 可借鉴/不可借鉴 |
| --- | --- | --- | --- | --- |
| **Cursor** | Settings → Models，内置 OpenAI/Anthropic/Google/Azure/Bedrock 预设，填 key 即可；provider 直接向用户收费 | 保存 key 后模型出现在 picker；BYOK 只覆盖 chat，Tab 等仍用 Cursor 自有模型 | Cursor 自己按 token rate 计团队费用；个人 BYOK 由 provider 账单负责 | 借：provider 预设 + key 后自动出模型；不借：Cursor 的 token rate 是它的商业模型，不是我们的 |
| **Cherry Studio** | 内置 provider 通常只填 key；base URL 默认；支持多 key 轮换 | 「获取模型列表」→ 勾选加入；显示真实 model ID；有 Test 连接 | 文档没有把「手填单价」作为主流程 | 借：预设、获取模型、测试连接、真实 ID；不借：多 key 轮换对本地单用户过重 |
| **Cline** | 主流 provider 预设；自定义 OpenAI-compatible 填 base URL/key/model ID | 模型列表/自定义 ID | 自定义 provider 仍有 input/output price 手填；主流 provider 用内置 model info 计成本 | 借：主流 provider 自动；不借：把自定义价格暴露在主界面 |
| **Continue** | `config.yaml` 配 provider/model/roles，key 用环境变量 | 配置式，无统一模型目录 | 文档没有成本面板；偏开发者配置 | 借：provider 抽象；不借：YAML 配置对普通用户太重 |
| **OpenRouter** | 一个 key 接多模型；`/api/v1/models` 返回模型、上下文、价格；`/credits` 查余额 | 统一模型 ID（author/slug） | 响应自带 `usage.cost`（账户扣费）、缓存/推理 token；`/generation` 可后查；价格是 USD/credits | 借：模型目录带价、每请求成本、余额；注意：它的价是转售价，不能拿来算官方 provider 的账 |
| **LiteLLM** | 代理/网关；模型 cost map + provider 配置 | 模型列表/路由 | 模型价目表（per-token、缓存、推理、音频、阶梯价）；spend logs；预算/按 key 归属 | 借：价目表结构与 spend 归因；不借：代理部署复杂度 |
| **LibreChat** | `librechat.yaml` 配 endpoint/model | 管理员配模型 | tokenConfig 手填 prompt/completion/context；transactions 记 token；余额是站内 credits | 借：token 事务记录、上下文仪表；不借：管理员手填价格 |
| **models.dev** | 开源模型目录 API | provider/model 目录 | 每百万 token 的 input/output/cache_read/cache_write、上下文、能力、官方 doc 链接；provider 可覆盖模型 | 借：provider-specific 目录 + 溯源；注意：字段/价格会变，必须缓存和校验 |

### 2.1 共同结论

- **预设是入口，不是模型列表的硬编码。** 预设只提供 base URL、协议、默认模型/文档；模型列表应尽量从 provider 拉。
- **模型 ID 必须展示真实值。** 同名不同日期/版本/供应商不能合并；否则用户选错模型。
- **价格是目录属性，不是用户设置。** 目录要能刷新，且显示来源。
- **成本必须带「估算/账单」标签。** 只有网关类 provider 能在响应里报实际扣费；官方 API 只能 token × 目录价。
- **余额是账户级对账，不是单次账单。** 有接口就查，没有就明确说没有；不要逆向内部接口。

## 3. 现有实现的问题

当前 `Settings → AI 模型`：

- provider 只有 DeepSeek / OpenAI / 自定义；
- 模型是自由文本，默认 `deepseek-chat` 已经可能是旧模型；
- 单价必须手填，默认 0；
- 没有模型列表、没有测试连接、没有余额、没有月度汇总；
- `Usage` 只有 prompt/completion/audio，缓存/推理字段没有归一化；
- 价格按设置里的手填值现算，目录价、峰谷价、缓存价全部丢失；
- 用户不知道「0 元」是免费还是没填。

这会让第一次配置就卡住，也会让成本数字不可信。

## 4. 建议的产品形态

### 4.1 设置页：模型连接

```text
AI 模型
[DeepSeek ▾]  [获取模型]  [测试连接]
API Key  [••••••••]  已保存

模型  [搜索模型… ▾]
  deepseek-v4-pro      1M context   $0.66 / $1.98 每 1M
  deepseek-flash       1M context   $0.15 / $0.60 每 1M
  …

参考价：LiteLLM / 2026-11-06；实际账单以厂商为准
[高级] 自定义 Base URL / 协议 / 手动价目（仅自定义/私有模型）
```

- 选内置 provider 后只显示 key；base URL 收进高级。
- 「获取模型」调 provider 的 `/models`；失败时回退到目录里的该 provider 模型，不阻塞。
- 模型行显示上下文和输入/输出价；没有公开价显示「未收录，仅记 token」。
- 不再有「输入单价/输出单价/币种」三个必填框。
- 自定义 provider 才显示协议、base URL、手动价目。

### 4.2 设置页：用量与余额

```text
本月用量
$1.86 / 预算 $20      [进度条]
输入 182.4k · 输出 41.2k · 缓存 120.0k · 推理 8.1k
按模型：deepseek-v4-pro $1.42 / deepseek-flash $0.44

余额（DeepSeek）  ¥ 42.80    [刷新]
注意：余额是账户级，不是本次任务账单。
```

- 任务详情保留每次任务的 tokens/估算；新增「账单」标签仅当 provider 返回实际 cost。
- 月度汇总从本地 usage 表算，不请求厂商账单。
- 预算只做提醒，默认不硬拦截；hard cap 作为后续可选项。

### 4.3 Provider 预设（首版范围）

| Provider | 协议 | 模型发现 | 计价来源 | 余额 |
| --- | --- | --- | --- | --- |
| DeepSeek | OpenAI-compatible | `/models`（失败回退目录） | LiteLLM（官方源链接） | `/user/balance` |
| OpenAI | OpenAI-compatible | `/models` | LiteLLM | 仅 Admin Key，首版不接 |
| Anthropic | Anthropic Messages | `/v1/models` | LiteLLM | 仅 Admin Key，首版不接 |
| Google Gemini | Gemini generateContent | `/v1beta/models` | LiteLLM | 不接 |
| OpenRouter | OpenAI-compatible | `/models`（带 pricing） | OpenRouter 报价（明确标注转售） | `/credits`（management key） |
| Moonshot / Kimi | OpenAI-compatible | `/models` | LiteLLM | `/users/me/balance` |
| 智谱 BigModel | OpenAI-compatible | `/models`（如可用） | LiteLLM/models.dev | 官方无 API，明确提示 |
| SiliconFlow | OpenAI-compatible | `/models` | LiteLLM/models.dev | 待验证 |
| 阿里百炼 | OpenAI-compatible | `/models` | LiteLLM/models.dev | 待验证 |
| 火山方舟 | OpenAI-compatible | `/models` | LiteLLM/models.dev | 待验证 |
| Ollama / LM Studio | OpenAI-compatible | `/v1/models` | 本地 0 元 | 不接 |
| 自定义 | OpenAI-compatible | 可选 `/models` | 目录无价则只记 token；高级可覆盖 | 不接 |

> 不在首版接「Claude Code / ChatGPT 订阅 OAuth」：那是订阅账号，不是官方 API；第三方客户端复用登录态有 ToS/稳定性风险。
> 只支持官方 API key（以及本地模型无 key）。

### 4.4 价格目录

**首选数据源：LiteLLM `model_prices_and_context_window.json`**
- 约 4,700 条，每条有 `input_cost_per_token` / `output_cost_per_token` / `cache_read_input_token_cost` / `cache_creation_input_token_cost` / `output_cost_per_reasoning_token` / `litellm_provider` / `source`。
- 运行期拉取，缓存到 app data 24h；带 `source` 和 `fetchedAt`；离线用上次缓存。
- 匹配顺序：`provider/model` 精确 → provider 前缀 → 叶子名 + `litellm_provider` 对齐；不做模糊 contains。
- 没有价目 → 不估算，不填 0。

**次选：OpenRouter `/api/v1/models`**
- 仅用于 OpenRouter provider；价格是转售价，界面必须写「OpenRouter 报价」。
- 有 `pricing.prompt/completion/input_cache_read/input_cache_write/internal_reasoning`，per-token USD。
- 响应 `usage.cost` 是账户扣费，优先于估算。

**第三：models.dev `api.json`**
- 按 provider 分层，cost 是每百万 USD；有 cache_read/cache_write/canonical_model_id。
- 用作 LiteLLM 未命中时的补充，不覆盖官方 provider 的已知价。

**绝不做：**
- 抓官方定价网页当运行时接口（JS 渲染、403、峰谷价）；
- 用 OpenRouter 价算 DeepSeek/OpenAI 官方 key 的账；
- 拿一个「平均价」糊所有模型；
- 价格缺失时填 0 当作免费。

### 4.5 Usage 归一化

每个 provider 返回的 usage 字段不同，进入账本前统一成：

```text
UsageRecord
  provider, model, jobId, step, at
  uncachedInputTokens
  cachedInputTokens
  cacheWriteTokens
  outputTokens
  reasoningTokens       # 仅记录；是否已含在 output 由 provider 说明
  audioSeconds
  estimatedCost, currency
  providerCost?, providerCurrency?   # 网关实报
```

归一口径（待实现时逐 provider 验证）：

| Provider | 输入 | 缓存读 | 缓存写 | 输出 |
| --- | --- | --- | --- | --- |
| OpenAI-compatible | `prompt_tokens - cached` | `prompt_tokens_details.cached_tokens` | `cache_write_tokens`（如有） | `completion_tokens` |
| DeepSeek | `prompt_cache_miss_tokens` | `prompt_cache_hit_tokens` | 0 | `completion_tokens` |
| Anthropic | `input_tokens` | `cache_read_input_tokens` | `cache_creation_input_tokens` | `output_tokens` |
| Gemini | `promptTokenCount - cachedContentTokenCount` | `cachedContentTokenCount` | 0 | `candidatesTokenCount`（`thoughtsTokenCount` 只记录） |
| OpenRouter | `prompt_tokens - cached` | `cached_tokens` | `cache_write_tokens` | `completion_tokens` |
| ASR | — | — | — | `usage.seconds` / duration |

成本公式：

```text
cost = uncachedInput * inputPrice
     + cachedInput * cacheReadPrice
     + cacheWrite * cacheWritePrice
     + output * outputPrice
     + audioSeconds * audioPrice
```

- 缓存价缺失时按输入价；但要在 UI 标「按输入价估算」。
- 推理 token 已包含在 output 的不重复加；provider 单独计价时才加。
- 阶梯价/峰谷价：目录条目有 tiers 时用当前请求的 prompt 长度/时间选档；无法判断时用基础档并标「估算」。
- 金额 currency 统一 USD 估算；余额按厂商返回币种单独显示，不做隐式汇率换算。

### 4.6 余额

只接有公开 API 的：

- DeepSeek `GET /user/balance`，API key；
- Moonshot `GET /v1/users/me/balance`，API key；
- OpenRouter `GET /api/v1/credits`，management key；
- 其他 provider：明确「该渠道没有公开余额接口」，不逆向、不抓网页。

余额只用于账户级对账，不能分摊到任务。

## 5. 数据与兼容

新增表/字段：

```text
production_usage
  id, job_id, step, provider, model,
  uncached_input, cached_input, cache_write, output, reasoning,
  audio_seconds, estimated_cost, currency,
  provider_cost, provider_currency,
  created_at

production_jobs
  + provider, model, pricing_source, pricing_version
```

旧数据：
- 已有任务没有 usage 明细 → 保留原 token/价，列表标「旧口径」。
- 旧的手填单价/币种字段保留在数据库，但主界面不再展示；仅当 provider=自定义且目录未命中时作为高级覆盖。
- 迁移不删除旧字段，避免历史记录打不开。

## 6. 分阶段实现

**P0-06 模型接入与自动计价（先做）**
1. Rust provider registry + protocol adapters（OpenAI-compatible / Anthropic / Gemini）。
2. 模型发现 `/models` + 目录回退；缓存。
3. LiteLLM/OpenRouter/models.dev 价目抓取、缓存、匹配、来源/时间。
4. 设置页重做：provider/key/获取模型/搜索/价格/测试连接；移除手填单价。
5. usage 归一化 + 成本估算 + 任务/月度汇总。
6. DeepSeek/Moonshot/OpenRouter 余额。
7. 单测：价格匹配、usage 归一化、成本公式、无价不估算。

**P1-13 体验收尾**
- 模型搜索、上下文/价格排序、复制模型 ID、错误文案。
- 任务详情「估算/账单」标签。
- 预算提醒、80% 警告。
- 自定义 provider 高级价目折叠。

**P2-09 计价精度**
- 阶梯价/峰谷价/缓存写价按 provider 实测。
- ASR/检索成本。
- 汇率显示（可选，用户显式设置）。

**P3 商业化**
- key 迁系统凭据/KMS。
- 团队/多租户/配额/账单。
- 云端 usage 汇总。

## 7. 验收标准

1. 新用户选 DeepSeek，只填 key，点「获取模型」能看到模型；选中后价格自动显示，**不需要手填单价**。
2. 断网/目录接口失败时，已缓存目录仍可用；无目录时只显示 token，不显示 0 元。
3. 文本任务运行后，任务详情显示输入/输出/缓存/推理 token 与估算金额；金额带来源和「估算」标签。
4. OpenRouter 请求返回 `usage.cost` 时显示「账单」而不是估算。
5. DeepSeek 余额能查；智谱明确显示不支持。
6. 自定义 provider 不填价目时，不产生假金额；高级里可覆盖。
7. 旧任务/旧设置不崩；旧口径有标记。

## 8. 需要先定的产品选择

推荐默认：

- **币种**：目录估算统一 USD；余额按厂商币种单独显示；不自动换汇。
- **预算**：只提醒，不硬拦截。
- **provider 范围**：上表 12 个；Anthropic/Gemini 用原生协议，其余 OpenAI-compatible。
- **订阅 OAuth**：不做；只支持官方 API key 与本地模型。
- **价目源**：LiteLLM 主，OpenRouter 只用于 OpenRouter，models.dev 补充；显示来源和时间。

如果这三项认可，下一步按 P0-06 实现；这是「设置页 + 引擎 usage + 账本」的联动改动，不是单改一个输入框。
