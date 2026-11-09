# 史料检索系统：调研与 M1 实现

> 状态：**M1 已实现并安装验收**（2026-11-08）
> 目标：为历史类事实做一套**独立的、以史料质量为目标**的检索系统，而不是把通用网页搜索换个调用。
> 实现：`src-tauri/src/production/history_search.rs` + `history_ingest.rs`；候选 UI 在事实详情。
> 结论：通用搜索只能做候选发现，不能做史料质量判定；ctext 网页抓取被明确禁止，必须走官方 API。

---

## 0. 调研结论

| 源 | 访问方式 | 自动引用 | 本机实测 |
| --- | --- | --- | --- |
| Wikisource | MediaWiki API：`action=query&list=search` 搜索；`action=parse&prop=text` 取正文 | 可（转录全文） | 直连请求超时；未 live 命中 |
| Internet Archive | `advancedsearch.php` 搜索；`/metadata/{id}` 找 `_djvu.txt`；`/download/{id}/...` 取 OCR 全文 | 可（OCR 全文） | 直连请求超时；未 live 命中 |
| ctext | 官方 JSON API `https://api.ctext.org`；`searchtexts?title=` 可匿名检索书目；`gettext?urn=&apikey=` 取全文 | **仅 API Key** | `searchtexts?title=明史` live 成功；`gettext` 无 Key 返回 `ERR_REQUIRES_AUTHENTICATION` |
| NLC 中华古籍资源库 | 网页目录/影像，需按书名检索 | 否，只作候选 | 未抓取 |
| 一史馆 | 明清档案目录，公开全文有限 | 否，只作候选 | 未抓取 |
| HathiTrust | 公版 Full view 可读；检索页可生成链接 | 否，只作候选 | 未抓取 |
| DeepSeek Search / 通用搜索 | 网页相关性检索 | **不用于史料质量层** | 不作为本系统 provider |

### 0.1 ctext 的边界（重要）

实测 `https://api.ctext.org/searchtexts?title=明史` 返回：

```json
{"books":[{"title":"明史","urn":"ctp:wb410835"}]}
```

匿名可以拿到书目，但 `gettext` 即使请求单章也返回：

```json
{"error":{"code":"ERR_REQUIRES_AUTHENTICATION","description":"...registered IP address or supply a valid API key."}}
```

ctext 的网页搜索页会返回反爬提示，并明确说明：**网页抓取违反条款、会间歇性返回故意损坏的数据**。  
所以本系统：

- 不抓 ctext 网页；
- 只用官方 `api.ctext.org`；
- 没有 API Key 时只保留书目候选链接，不写 citation；
- 有 Key 时才尝试 `gettext` 全文，仍然要过逐字 quote 校验。

### 0.2 为什么不用 DeepSeek Search 直接替代

通用搜索的 unit 是网页，史料搜索的 unit 是 work / edition / passage。  
通用搜索会受 SEO、时效、摘要长度影响；把 snippet 写进 citation 会重演“网页证据污染史料库”。  
因此本系统把通用搜索排除在自动引用链之外，最多作为未来的候选发现层，且必须标记 candidate。

---

## 1. 架构

```text
历史主张
  → 查询规划（模型 + registry 兜底）
      work / section / people / era / queries
  → 多源发现
      ctext searchtexts（书目，可匿名）
      Wikisource search（全文）
      Internet Archive advancedsearch（OCR 全文）
      NLC / 一史馆 / HathiTrust（只生成候选链接）
  → 候选排序
      authority tier + work match + section match + query overlap
  → 全文抓取（仅 authority 允许且 provider 允许）
      Wikisource parse API
      ctext gettext API（需要 Key）
      IA metadata + _djvu.txt
  → passage 切分 + 模型/确定性选句 + 逐字子串校验
  → 高置信写 source/passage/citation
  → 低置信/外链写 candidate（不引用）
```

### 1.1 Authority 分层

| 层级 | 来源 | 自动引用 |
| --- | --- | --- |
| `primary` | 一手全文 + 版本/卷页/扫描页 | 是 |
| `transcription` | Wikisource / ctext 转录全文 | 是，标“未核对原书扫描页” |
| `ocr` | Internet Archive OCR 全文 | 是，标“OCR 未核对扫描页” |
| `secondary` | 二手研究/学术页 | 否，只候选 |
| `reference` | 工具书/百科 | 否，只候选 |
| `link_only` | NLC / 一史馆 / HathiTrust / ctext 无 Key | 否，只候选 |

自动引用的硬条件：

1. 来源在 registry 或 allowlist；
2. 抓到全文，不是 snippet；
3. quote 是 passage 的逐字子串；
4. 至少有一组 query/title 实体重合；
5. 写入 provider、URL、edition、authority、抓取时间、candidate id。

### 1.2 查询规划

`history_search::plan_query`：

- 有 AI 时，让模型输出 `work/section/people/era/queries`；
- 模型失败/无 Key 时，用静态 registry（18 部核心史书）+ 现有 claim queries 兜底；
- 查询不是整句主张，而是“书名/篇名/人物/原文关键词”的组合。

真机例：严嵩事实的规划结果是：

```text
明史 · 卷三百八·奸臣传·严嵩
```

这不是把整句主张丢给搜索，而是先定位到《明史·严嵩传》。

### 1.3 Registry

`history_search.rs` 内置 18 部核心史料：明史、明实录、明史纪事本末、国榷、清史稿、清实录、史记、汉书、后汉书、三国志、资治通鉴、续资治通鉴长编、宋史、元史、旧唐书、新唐书、明会典、日知录。

第一版是编译期静态表，不做 UI 编辑；后续可以迁到 SQLite，支持用户自定义书单。

---

## 2. 数据与审计

### 2.1 `history_search_candidates`

每条候选记录：

```text
id, fact_id, provider, title, url, snippet,
authority, status (candidate/cited/rejected), reason,
work_title, section_title, source_id, citation_id, created_at
```

用途：

- 让用户看到“检索到了什么、为什么没有自动引用”；
- 失败不是静默的；
- `cited` 才对应 citation；
- 外链源不会伪装成史料。

### 2.2 `history_page_cache`

```text
key, provider, url, title, text, fetched_at
```

- 抓取成功才缓存；
- TTL 7 天；
- 避免重复抓同一页；
- 也便于审计 quote 是否真的来自该页。

### 2.3 UI

事实详情「史料」区：

- 有 citation 时显示引用；
- 有候选时折叠显示「来源候选 N 条」；
- 每条候选显示 provider、authority、status、reason、外链；
- 没有 citation 时默认展开候选，避免“点完自动找史料什么也没发生”。

设置页「联网检索」新增 **ctext API Key（可选）**。没有 Key 时 ctext 只保留书目候选，不自动引用。

---

## 3. 质量闸门

### 3.1 自动引用

只对以下 source 尝试抓全文并写 citation：

- Wikisource 转录全文；
- ctext 官方 API 全文（有 Key）；
- Internet Archive `_djvu.txt` OCR。

### 3.2 只做候选

- NLC / 一史馆 / HathiTrust：目录/外链，不抓；
- ctext 无 Key：书目候选；
- 二手/工具书：不自动引用；
- 通用搜索/新闻/自媒体：不进入本系统。

### 3.3 Quote 校验

- 模型只能从抓取到的 passage 里“挑一句”；
- 输出必须是 passage 的逐字子串；
- 不是子串、过短、越界 → 拒绝；
- 抓不到全文/没有 quote → 0 条 citation。

---

## 4. 实测（2026-11-08）

### 4.1 已通过

- `ctext searchtexts?title=明史`：**live 成功**，返回 `明史` / `ctp:wb410835` 等书目；
- 严嵩事实点「自动找史料」：
  - 规划：`明史 · 卷三百八·奸臣传·严嵩`；
  - 候选：7 条（ctext 明史、HathiTrust、NLC、ctext 台湾文献、明史演义、明史纪事本末、一史馆）；
  - 自动抓取：0（ctext 无 Key；Wikisource 直连失败）；
  - citation 仍为 1（早前人工验证的《明史》），没有被失败请求污染；
- `cargo test`：**147 passed / 7 ignored**；
- 包：**8,587,010 B（8.19 MiB）** / exe **21,518,336 B（20.51 MiB）**。

### 4.2 候选补录（无 Key / 网络失败时的最后一步）

候选行有「补录原文」：打开候选页面，复制正文/相关段落，粘贴回事实详情。程序：

1. 用候选的 provider/title/url/work/section 预填 source；
2. 对粘贴文本切 passage；
3. 确定性/模型挑句；
4. quote 必须是粘贴原文的逐字子串；
5. 成功后写 source/passage/citation，候选标 `cited`；
6. 失败则候选标 `rejected`，0 条 citation。

真机验收：

- ctext《明史》候选 → 粘贴「严嵩无他才略，唯一意媚上，窃权罔利。」→ 成功写入 1 条 citation，候选标 cited，fact citation 从 1 变 2；
- 测试后移除 citation、删除测试 source/passage、把候选恢复为 candidate；库回到 1/1/1；
- 这段来源的 authenticity note 是「用户从 ctext 候选页粘贴；未自动抓取，未核对原书扫描页」。

### 4.3 截图

- `shots/domain/11-history-search-candidates.png`：规划 + 候选摘要；
- `shots/domain/12-history-candidates-expanded.png`：候选展开，authority/status/reason/外链；
- `shots/domain/13-ctext-key-setting.png`：ctext API Key 设置；
- `shots/domain/14-candidate-paste-dialog.png`：候选补录弹窗；
- `shots/domain/15-candidate-paste-cited.png`：补录成功，候选标 cited（测试后已清理）。

---

## 5. 诚实边界

1. **没有 live 自动 citation**：Wikisource/IA 在本机直连超时；ctext 无 Key 只能拿书目。  
   自动引用代码路径已实现，但最终写入仍只有早前人工《明史》一条；**候选补录**真机写入过 1 条测试 citation，测试后已清理。
2. **ctext 全文未 live 验证**：`gettext` 需要订阅 Key；当前只验证了匿名 `searchtexts` 和错误分支。
3. **Wikisource/IA 抓取器未 live 验证**：解析器用 fixture/单测覆盖；live 直连失败。
4. **候选补录是人工最后一步**：用户仍要打开候选页并粘贴原文；程序负责对齐和校验，不负责判断页面是否可靠。粘贴错页/错版本时，quote 校验会拒绝，但用户仍可能粘贴到同文的错误版本。
5. **Internet Archive OCR 有错字**：即使将来命中，也必须标“OCR 未核对扫描页”。
6. **繁简/异体字**：确定性兜底不做繁简转换；繁简差异大时只能依赖模型选句。
7. **ctext 只返回书目时不会递归章节**：M1 不递归 `subsections`；如果 `gettext` 只返回目录，会拒绝并保留候选。
8. **没有 OCR/扫描页对齐**：扫描页只作为版本证据，不自动转文本。
9. **候选列表可能包含相关但非同一版本的书**（如《明史纪事本末》《明史演义》）；它们只作候选，不会自动引用。
10. **查询规划可能错书**：模型可能把主张规划到相近史书；候选 reason/authority 让错误可见，但还没有人工“换一本书”操作。
11. **没有权威 ID**（Wikidata/CBDB/CHGIS）；事件/人物别名仍靠查询规划。
12. **ctext API Key 明文存本地 SQLite**，和现有模型/检索 Key 的存储方式一致；还没有系统凭据/KMS。

---

## 6. 下一阶段（M2）

1. ctext API Key 真实全文验证；递归章节/URN 解析。
2. Wikisource/IA live 命中与抓取验证（换网络/代理环境）。
3. 候选“换源/重试”操作：只对候选做一次选择，不回到手工录入。
4. 版本/卷页/扫描页字段；同一 work 多 edition 去重。
5. 查询规划 golden set：20–30 条真实历史主张，报告规划命中率与错书率。
6. 史料引用质量指标：假引用率必须为 0；自动引用/候选/拒绝比例；provider 命中率。
