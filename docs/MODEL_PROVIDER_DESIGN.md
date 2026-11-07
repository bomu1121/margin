# 模型接入与计价监测：调研与改造方案

> 状态：**2026-11-06 P0-06 已实现首版**。AI 设置已改为「provider 预设 → 填 Key → 拉模型 / 目录回退 →
> 自动带出参考价」；手填 AI 单价不再是主路径，只有目录没收录或自定义 provider 时才可选填。
> 已定产品口径：**CNY（按用户设置汇率折算）、预算只提醒、首版 13 个 provider**。
> 验证：Rust `80 passed, 0 failed, 2 ignored`；手动跑通 models.dev 实时目录测试（11 个有目录的 provider、636 个模型）；
> 前端 build 通过；真机截图 `shots/production-05-settings.png`、`production-06-settings-usage.png`、`production-16-model-sort-copy.png`、`production-17-manual-price.png`。

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

## 3. 改造前的问题（已修复）

改造前 `Settings → AI 模型`：

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
[DeepSeek ▾]  [获取模型 / 测试连接]  [查询余额]  [保存设置]  [获取 Key / 文档]
API Key  [••••••••]  已保存

模型  [deepseek-v4-pro]  ¥4.75 / ¥14.26
搜索模型 ID 或名称…
  deepseek-flash             1M 上下文  ¥1.08 / ¥4.32
  deepseek-v4-pro            1M 上下文  ¥4.75 / ¥14.26
  …

参考价  输入 ¥4.75 · 输出 ¥14.26 · 缓存读 ¥0.16 · 缓存写 ¥4.75（按输入价）
来源 models.dev · 2026-11-06 · 1 USD = 7.2 CNY · 估算，实际账单以服务商为准
[高级] 接口地址 / 协议（自定义才可选）/ 汇率
```

- 选内置 provider 后只显示 key；base URL 收进高级。
- 「获取模型」调 provider 的 `/models`；失败时回退到目录里的该 provider 模型，不阻塞。
- 模型行显示上下文和输入/输出价；没有公开价显示「未收录，仅记 token」。
- 不再有「输入单价/输出单价/币种」三个必填框。
- 自定义 provider 才显示协议、base URL、手动价目。

### 4.2 设置页：用量与余额

```text
本月用量
¥0.00 / ¥20.00         [进度条]
0 tokens · 1 个任务 · 0 个有价任务
按模型：deepseek-v4-pro 0 tokens ¥0.00

余额（DeepSeek）  ¥42.80    [刷新]
注意：余额是账户级，不是本次任务账单。
```

- 任务详情保留每次任务的 tokens/估算；新增「账单」标签仅当 provider 返回实际 cost。
- 月度汇总从本地 usage 表算，不请求厂商账单。
- 预算只做提醒，默认不硬拦截；hard cap 作为后续可选项。

### 4.3 Provider 预设（首版范围）

| Provider | 协议 | 模型发现 | 计价来源 | 余额 |
| --- | --- | --- | --- | --- |
| DeepSeek | OpenAI-compatible | `/models`（失败回退目录） | models.dev（`deepseek`） | `/user/balance` |
| OpenAI | OpenAI-compatible | `/models` | models.dev（`openai`） | 仅 Admin Key，首版不接 |
| Anthropic | Anthropic Messages | `/v1/models` | models.dev（`anthropic`） | 仅 Admin Key，首版不接 |
| Google Gemini | Gemini generateContent | `/v1beta/models` | models.dev（`google`） | 不接 |
| OpenRouter | OpenAI-compatible | `/models`（带 pricing） | OpenRouter 实时报价（明确标注转售） | `/credits`（management key） |
| Moonshot / Kimi | OpenAI-compatible | `/models` | models.dev（`moonshotai-cn`） | `/users/me/balance` |
| 智谱 BigModel | OpenAI-compatible | `/models`（如可用） | models.dev（`zhipuai`） | 官方无 API，明确提示 |
| SiliconFlow | OpenAI-compatible | `/models` | models.dev（`siliconflow`） | 不接 |
| 阿里百炼 | OpenAI-compatible | `/models` | models.dev（`alibaba`） | 不接 |
| 火山方舟 | OpenAI-compatible | `/models` | models.dev（`volcengine`） | 不接 |
| Ollama | OpenAI-compatible | `/v1/models` | 无目录价，只记 token | 不接 |
| LM Studio | OpenAI-compatible | `/v1/models` | models.dev 的 `lmstudio` 条目 | 不接 |
| 自定义 | OpenAI-compatible | 可选 `/models` | 目录无价则只记 token | 不接 |

> 不在首版接「Claude Code / ChatGPT 订阅 OAuth」：那是订阅账号，不是官方 API；第三方客户端复用登录态有 ToS/稳定性风险。
> 只支持官方 API key（以及本地模型无 key）。

### 4.4 价格目录

**首版实际数据源：models.dev `api.json`**
- 按 provider 分层，cost 是每百万 USD；有 input/output/cache_read/cache_write/context/canonical_model_id。
- 运行期拉取 `https://models.dev/api.json`，缓存到 `<appdata>/production/models-dev-catalog.json` 24h；离线用上次缓存。
- 匹配顺序：模型 ID 精确 → 去掉日期后缀（`-YYYY-MM-DD` / `-YYYYMMDD`）→ `canonical_model_id` 结尾精确匹配；不做模糊 contains。
- 没有价目 → 不估算，不填 0；界面显示「未收录价格」。
- 2026-11-06 实测：11 个有目录的 provider、636 个模型；DeepSeek V4 Pro 输入 ¥4.752 / 输出 ¥14.256（USD 0.66 / 1.98 × 7.2）。

**OpenRouter 实时价（已实现）**
- 仅用于 OpenRouter provider；`/api/v1/models` 的 `pricing` 是 per-token USD，换算成每百万。
- 界面标注「OpenRouter 转售报价」；响应 `usage.cost` 单独记为服务商实报，不混入估算。

**未实现：LiteLLM 作为主/备价目。** 本文最初把它列为首选，但首版验证下来 models.dev 的字段、
provider 分层和 24h 缓存已经够用；再引入一个 4,700 条的价目源会增加匹配歧义和维护面。等出现
models.dev 明确缺价、且用户实际受影响的模型时再接。

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

- 缓存价缺失时按输入价，UI 标「按输入价」；`PriceQuote.cache_read_priced` / `cache_write_priced` 记录该价是否来自目录。
- 推理 token 已包含在 output 的不重复加；只有 provider 单独计价时才加。首版只记录，不单独计费。
- 阶梯价/峰谷价：首版不做阶梯选择，按目录基础档估算；DeepSeek 峰谷价在 provider 说明里明确标注。
- 金额统一 CNY 展示：目录美元价 × 用户设置的汇率（默认 7.2）；余额按厂商返回币种单独显示，不换汇。

### 4.6 余额

首版只接有公开 API 的：

- DeepSeek `GET /user/balance`，API key；
- Moonshot `GET /v1/users/me/balance`，API key；
- OpenRouter `GET /api/v1/credits`，management key；
- 其他 provider：明确「该渠道没有公开余额接口」，不逆向、不抓网页。

余额只用于账户级对账，不能分摊到任务。首版只验证了「无 key / 不支持渠道」的错误路径；
三个真实余额接口没有用有效 key 验收。

## 5. 数据与兼容

首版实际落库：

```text
production_jobs
  + provider TEXT
  + model TEXT
  + usage_json: Usage {
      prompt_tokens, uncached_input_tokens, completion_tokens,
      cached_tokens, cache_write_tokens, reasoning_tokens,
      audio_seconds, estimated_cost, currency, priced,
      provider_cost, provider_currency
    }

settings
  production.ai.provider / protocol / base_url / model
  production.ai.exchange_rate / monthly_budget_cny
  production.ai.api_key.<provider>   ← 每个 provider 独立存 key
```

**没有新增 `production_usage` 明细表**：首版把 usage 聚合在任务行上，月度汇总从
`production_jobs.usage_json` 按 `created_at` 过滤本月计算。这样迁移面小、旧任务直接兼容；
代价是无法按步骤/单次调用拆账。等需要「一个任务里哪一步最贵」时再拆表。

旧数据：
- 已有任务的 provider/model 为空时显示「未知模型」，不崩溃、不编金额。
- 旧的通用 key `production.ai.api_key` 在打开数据库时迁移到当前 provider 的
  `production.ai.api_key.<provider>`，然后删除旧键；避免切换 provider 时把 A 的 key 发给 B。
- 旧的手填单价字段不再读写；主界面没有手动价目。自定义 provider 目录未命中时只记 token。

## 6. 分阶段实现

**P0-06 模型接入与自动计价（首版已完成）**
1. ✅ Rust provider registry：13 个预设 + 3 种协议（OpenAI-compatible / Anthropic / Gemini）。
2. ✅ 模型发现 `/models` + models.dev 目录回退；24h 缓存 + stale 回退代码路径。
3. ✅ models.dev 价目、OpenRouter 实时价；来源/更新时间；日期后缀与 canonical 匹配。
4. ✅ 设置页重做：provider / key / 获取模型 / 搜索 / 价格 / 保存；移除手填 AI 单价。
5. ✅ usage 归一化 + 成本估算 + 任务/月度汇总。
6. ✅ DeepSeek/Moonshot/OpenRouter 余额接口；其他 provider 明确提示不支持。
7. ✅ 单测：价格匹配、无价不估算、CNY 换算、成本逐类 token、缓存缺失回退、key 隔离与迁移。

**P1-13 体验收尾（完成）**
- ✅ 模型搜索、错误文案、保存时刷新月度汇总、provider 切换清空 Key。
- ✅ 任务详情「估算 / 服务商账单」分开显示。
- ✅ 预算提醒、80% 金色警告。
- ✅ 上下文/输入价/输出价排序；无价/无上下文排最后，选中项被挤出前 80 时固定「当前选择」。
- ✅ 复制模型 ID；走 Tauri clipboard 插件，不触发 WebView2 权限框。
- ✅ 自定义/未收录模型的手动 CNY 价目，按 provider + model 存储；优先级 `手动 > 目录 > 不估算`。
- ✅ 校对/合并/属性抽取的 usage 已计入；精确 mock：1M+1M = ¥80，带抽取 3M+3M = ¥120。

**P2-09 计价精度**
- 阶梯价/峰谷价/缓存写价按 provider 实测。
- ASR/检索成本自动计价。
- ✅ 汇率手动设置与展示（默认 7.2，不抓实时汇率）。

**P3 商业化**
- key 迁系统凭据/KMS。
- 团队/多租户/配额/账单。
- 云端 usage 汇总。

## 7. 验收结果（2026-11-06）

| # | 标准 | 结果 | 证据 |
| --- | --- | --- | --- |
| 1 | 选 DeepSeek 不填单价也能看到模型和价格 | **通过** | 真机目录 4 个模型；`deepseek-v4-pro` 显示 ¥4.75 / ¥14.26；截图 `production-05-settings.png` |
| 2 | 目录失败时用缓存；无目录只记 token | **部分通过** | 24h 缓存 + stale 回退代码路径；未做断网实测；无价模型显示「未收录价格」 |
| 3 | 任务详情显示输入/输出/缓存/推理与估算 | **代码完成，未真实跑模型** | UI tab 已改；旧任务无 token 显示 0；没有用真实 key 跑一次新任务 |
| 4 | OpenRouter `usage.cost` 显示服务商实报 | **代码完成，未真实调用** | 解析 + `providerCost` 独立字段；任务详情有「服务商账单」行 |
| 5 | DeepSeek 余额能查；其他明确不支持 | **部分通过** | 无 key 错误路径真机通过（「还没有配置 API Key」）；不支持渠道文案已写；未用真实 key 查余额 |
| 6 | 自定义 provider 无价不产生假金额 | **通过** | 价格为空 → 「未收录价格」；成本 0 且 `priced=false`；有单测 |
| 7 | 旧任务/旧设置不崩 | **通过** | 旧任务显示「未知模型 / ¥0.00」；旧 key 迁移有单测 |

**没验证的：** 真实 provider key 的 `/models`、chat、余额；断网缓存回退；OpenRouter 实报；
真实任务的 usage 归一化（只做了单测和 mock）。不要把「代码路径存在」当成「已验收」。

## 8. 最终产品选择（已定）

- **币种**：目录美元价按用户设置汇率折算 CNY；默认 7.2；余额按厂商币种。
- **预算**：只提醒，不硬拦截；80% 以上变金色，任务不阻断。
- **provider 范围**：13 个预设，首版全上；OpenAI/Anthropic/Gemini 用原生协议。
- **订阅 OAuth**：不做；只支持官方 API key 与本地模型。
- **价目源**：models.dev 主，OpenRouter 实时价仅用于 OpenRouter；LiteLLM 暂不接。
- **key 存储**：每个 provider 独立 key；仍明文 SQLite，P3 迁系统凭据。
