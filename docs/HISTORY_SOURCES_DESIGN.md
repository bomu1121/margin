# 史料与历史事件：完整开发方案

> 状态：**M1/M2 已实现，M3 自动入库首版已接（live 未命中）**（2026-11-08）
> 目标：让历史类笔记不只是“事实被标成有争议/未找到”，而是能找到**具体史料、具体版本、具体卷页和原文**，并把“事件—事实—史料—引文—笔记”连成可追溯的链。
> **产品裁剪（2026-11-08）**：当前主流程只暴露“事实 → 一条史料引文”；事件实体、6 种历史状态从 UI 移除（后端事件表保留为实验，不再导航/展示）。本文保留完整设计，但不要把事件页/状态矩阵再加回主流程。
> **产品修正（2026-11-08，第二次）**：史料库不是手工资料柜，而是自动溯源的结果存储。UI 已移除「新增史料 / 找史料 / 加原文段落 / 引用到事实」这些手工主入口；事实按 `domain` 分成「历史事实 / 其他事实」，只有历史事实显示「史料」区。
> **自动入库（2026-11-08，第三次）**：已实现 `history claim → 查询规划 → 多源史料检索 → 抓全文 → 按句切 passage → 确定性/模型选逐字原文 → 子串校验 → source/passage/citation`。独立检索系统见 [`HISTORY_SEARCH_DESIGN.md`](HISTORY_SEARCH_DESIGN.md)：Wikisource / Internet Archive / ctext 官方 API / NLC / 一史馆 / HathiTrust，带 authority 分层和候选池。**ctext 网页抓取被明确禁止，只走官方 API；当前本机 Wikisource/IA 直连超时，ctext 无 Key 只能拿书目候选，未拿到 live 自动 citation。**
> 末尾 §15 记录实际落地的表、命令、UI、真机数据和未完成项。
> 末尾 §15 记录实际落地的表、命令、UI、真机数据和未完成项。

## 0. 结论先行

1. **史料必须是单独的一等实体**，不能做成 `facts` 的一个状态或字段。  
   事实回答“这个主张是否成立”；史料回答“哪本书、哪一版、哪一卷、哪一页、原文怎么说”。
2. **历史事件是事实的一个领域/类型**，不是另一个“事实库”。  
   事件是稳定节点（赤壁之战、鸿门宴、玄武门之变），关于事件的主张才是事实（兵力、日期、因果、参与者），事实再引用史料。
3. **历史模式不能复用普通网页事实核查的判定语义**。  
   网页搜不到不等于史实不存在；历史事实应有“有直接史料 / 有间接史料 / 史料互异 / 待考 / 今人研究”等状态。
4. **搜索要 source-first（先找出处），不是 claim-first（拿整条主张搜网页）**。  
   最有效的路径是：二手研究脚注 → 一手史料书名/卷次 → 古籍全文库/扫描库 → 逐字核对原文 → 记录版本和页码。
5. **史料引文必须落在 passage（段落/引文）上，而不是只存一个 URL。**  
   一条史料可以有多个版本、多个段落；一条事实可以引用多个 passage；一条 passage 可以支持多条事实。

一句话：

> **Event 是“发生了什么”，Fact 是“关于它的一條主张”，Source 是“哪本书/哪份档案”，Passage 是“原文”，Citation 是“这条原文和这条主张的关系”。**

---

## 1. 现状与问题

当前事实库已经支持：

- `facts` / `note_fact_mentions` / `fact_sources` / `fact_evidence` / `fact_checks` / `fact_relations`；
- 事实详情页、引用笔记、证据、核验历史、人工合并/撤销；
- 溯源时用模型做“新 claim 是否归入已有事实”；
- 图谱的笔记—事实—来源层。

但对历史类笔记不够：

| 问题 | 现状 | 历史场景的要求 |
| --- | --- | --- |
| 证据只有 URL/domain/quote | `fact_sources` 以 URL 唯一 | 史料常常没有 URL，只有书名、版本、卷页、馆藏 |
| 事实状态是网页核查语义 | verified/disputed/unsupported | 需要“直接史料/间接史料/史料互异/待考” |
| 搜索以网页为主 | Search API + 网页抓取 | 需要古籍全文库、扫描库、目录、报刊库、档案目录 |
| 事件没有独立身份 | 事实文本是自由文本 | 同一事件的不同写法要能匹配；事件要有别名/时间/人物/地点 |
| 证据只到句子 | quote 存在 `fact_evidence` | 需要 edition/volume/page/scan_url，能回到原书影像 |
| 史料和事实混在一起 | 一条 URL 一条 evidence | 一条史料可支持多条事实；一条事实可引多条史料 |

---

## 2. 领域模型

### 2.1 Event（历史事件）

事件是稳定的历史节点，不是“真相已经确定”的同义词。它允许别名、时间范围和争议。

```text
Event
  id
  canonical_name        # 赤壁之战
  aliases               # 赤壁、曹操南征、建安十三年之战
  start_date / end_date # 208；允许只到年/月，或“约”
  date_note             # 建安十三年；公历换算说明
  places                # 赤壁（历史地名 + 今地）
  actors                # 曹操、孙权、刘备、周瑜
  authority_ids         # Wikidata QID / CBDB / CHGIS / 自定义
  summary               # 一段中性概述，不是事实判定
```

事件不是事实的替代品：

- 事件：赤壁之战；
- 事实：曹操军“号称八十万”；
- 事实：周瑜判断“不过十五六万”；
- 史料：《三国志·吴书·周瑜传》裴注引《江表传》；
- 引文：具体原文段落；
- 关系：这条史料支持/异文/反驳哪条事实。

### 2.2 Fact（关于事件的主张）

事实仍然沿用当前 `facts`，增加：

- `domain`：`general` / `history` / `science` / ...
- `event_id`：可空；历史事实建议挂事件；
- `historical_status`：历史模式专用状态；
- `claim_kind`：`date` / `number` / `causal` / `actor` / `place` / `event_fact` / `interpretation`。

**不要因为事件固定就自动合并事实。**  
“曹操军八十万”和“曹操军十五六万”可以属于同一事件，但它们是不同主张，可能是异文/争议，不能被同一个事实节点吞掉。

### 2.3 Source（史料/文献）

Source 是“具体的文献记录”，建议把“著作”和“版本”分开考虑，但首版可以先合成一张表：

```text
Source
  id
  work_title            # 三国志
  section_title         # 吴书·周瑜传
  author                # 陈寿
  compiler              # 裴松之（注）
  era                   # 西晋/南朝宋
  source_type           # primary / secondary / archive / newspaper / reference
  edition               # 中华书局1959点校本 / 百衲本 / 某抄本
  publisher
  publish_year
  volume                # 卷五十四
  repository            # 国家图书馆 / 哈佛燕京 / 某档案馆
  url                   # 全文页（可空）
  scan_url              # 扫描影像（可空）
  call_number           # 索书号（可空）
  language
  authenticity_note     # 真伪/辑佚/传抄说明
  created_at / updated_at
```

后续如果版本很多，可拆成 `source_works` + `source_editions`；首版不要过度设计，先保证“一本书的某个版本”能记录清楚。

### 2.4 Passage（引文/段落）

Passage 是真正被引用、被比对、被展示的原文单位。没有 passage，只有 source，无法逐字校验。

```text
Passage
  id
  source_id
  volume / chapter / page
  original_text         # 原文，保留异体字/标点
  normalized_text       # 去标点/繁简统一后的检索键
  translation           # 今译/英译（可选）
  variant_of            # 异文指向另一 passage
  scan_url              # 对应扫描页
  locator               # “卷五十四，第1262页”或“第12叶a”
  created_at
```

### 2.5 Citation（引用关系）

Citation 把 Fact 和 Passage 连起来：

```text
Citation
  id
  fact_id
  passage_id
  relation              # supports / contradicts / variant / context
  quote                 # 必须是 passage.original_text 的子串
  locator               # 冗余保存当时的卷页定位
  confidence            # 检索/对齐置信度，不是“事实概率”
  checked_at
  created_at
```

约束：

- `quote` 必须是 `passage.original_text` 的逐字子串；不满足就不能标 `supports`；
- `source` 必须是候选来源之一；
- 同一条事实可以有多条 citation；
- 同一条 passage 可以支持多条事实；
- 删除 source 时不能静默删 citation，要有引用保护/确认。

---

## 3. 数据表方案

> 下面是首版建议。可以复用现有 `fact_sources`/`fact_evidence` 做迁移，但不要让新史料系统继续依赖 `url UNIQUE`。

### 3.1 新表

```sql
CREATE TABLE history_events (
  id TEXT PRIMARY KEY,
  canonical_name TEXT NOT NULL,
  aliases_json TEXT NOT NULL DEFAULT '[]',
  start_date TEXT NOT NULL DEFAULT '',
  end_date TEXT NOT NULL DEFAULT '',
  date_note TEXT NOT NULL DEFAULT '',
  places_json TEXT NOT NULL DEFAULT '[]',
  actors_json TEXT NOT NULL DEFAULT '[]',
  authority_json TEXT NOT NULL DEFAULT '{}',
  summary TEXT NOT NULL DEFAULT '',
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);

CREATE TABLE sources (
  id TEXT PRIMARY KEY,
  work_title TEXT NOT NULL,
  section_title TEXT NOT NULL DEFAULT '',
  author TEXT NOT NULL DEFAULT '',
  compiler TEXT NOT NULL DEFAULT '',
  era TEXT NOT NULL DEFAULT '',
  source_type TEXT NOT NULL DEFAULT 'primary',
  edition TEXT NOT NULL DEFAULT '',
  publisher TEXT NOT NULL DEFAULT '',
  publish_year TEXT NOT NULL DEFAULT '',
  volume TEXT NOT NULL DEFAULT '',
  repository TEXT NOT NULL DEFAULT '',
  url TEXT NOT NULL DEFAULT '',
  scan_url TEXT NOT NULL DEFAULT '',
  call_number TEXT NOT NULL DEFAULT '',
  language TEXT NOT NULL DEFAULT 'zh',
  authenticity_note TEXT NOT NULL DEFAULT '',
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);

CREATE TABLE source_passages (
  id TEXT PRIMARY KEY,
  source_id TEXT NOT NULL,
  volume TEXT NOT NULL DEFAULT '',
  chapter TEXT NOT NULL DEFAULT '',
  page TEXT NOT NULL DEFAULT '',
  original_text TEXT NOT NULL,
  normalized_text TEXT NOT NULL,
  translation TEXT NOT NULL DEFAULT '',
  variant_of TEXT NOT NULL DEFAULT '',
  scan_url TEXT NOT NULL DEFAULT '',
  locator TEXT NOT NULL DEFAULT '',
  created_at INTEGER NOT NULL
);

CREATE TABLE citations (
  id TEXT PRIMARY KEY,
  fact_id TEXT NOT NULL,
  passage_id TEXT NOT NULL,
  relation TEXT NOT NULL,
  quote TEXT NOT NULL DEFAULT '',
  locator TEXT NOT NULL DEFAULT '',
  confidence REAL NOT NULL DEFAULT 0,
  checked_at INTEGER NOT NULL,
  created_at INTEGER NOT NULL,
  UNIQUE(fact_id, passage_id, quote)
);

ALTER TABLE facts ADD COLUMN domain TEXT NOT NULL DEFAULT 'general';
ALTER TABLE facts ADD COLUMN event_id TEXT NOT NULL DEFAULT '';
ALTER TABLE facts ADD COLUMN historical_status TEXT NOT NULL DEFAULT '';
ALTER TABLE facts ADD COLUMN claim_kind TEXT NOT NULL DEFAULT '';
```

索引：

```sql
CREATE INDEX idx_sources_title ON sources(work_title, edition);
CREATE INDEX idx_passages_source ON source_passages(source_id);
CREATE INDEX idx_citations_fact ON citations(fact_id);
CREATE INDEX idx_citations_passage ON citations(passage_id);
CREATE INDEX idx_facts_event ON facts(event_id);
```

### 3.2 旧数据迁移

现有 `fact_sources` + `fact_evidence` 是“网页证据”：

- 每条 `fact_sources` → 一条 `sources`，`source_type='web'`，`work_title=title or domain`，`url` 保留；
- 每条 `fact_evidence` → 一条 `source_passages`（`original_text=quote`，`locator=''`）+ 一条 `citations`；
- 旧的 `fact_evidence.relation` 映射到 `citations.relation`；
- 保留旧表只读，避免迁移失败时丢数据；
- 迁移版本号写 `fact_meta.source_version=1`。

**不要**把旧 URL 证据硬塞成“古籍版本”；它们的 `source_type` 必须是 `web`，不要伪装成 primary source。

### 3.3 与现有事实状态的关系

保留 `facts.status`（candidate/verified/disputed/unsupported）作为通用状态；历史模式新增 `historical_status`：

| historical_status | 含义 |
| --- | --- |
| `direct_source` | 有一手史料直接记载，且引文已对齐 |
| `indirect_source` | 只有后世编纂/转述/二手研究 |
| `source_variant` | 多条史料互异，无法简单判真伪 |
| `needs_research` | 暂未找到，但**不等于不存在** |
| `modern_scholarship` | 只有今人研究/论文观点 |
| `disputed_interpretation` | 史料相同、解释不同 |

历史模式不要用“未找到外部出处”作为最终状态；用 `needs_research`，并显示“已查过哪些库、哪些没访问权限”。

---

## 4. 历史溯源流水线

```text
笔记正文
  ↓ 1 事件/主张抽取
  ↓ 2 事件解析（别名、时间、人物、地点、权威 ID）
  ↓ 3 史料查询规划（书名/篇名/人物/年号/异名）
  ↓ 4 多源发现（古籍全文、扫描、目录、报刊、学术）
  ↓ 5 获取原文（抓取/手动录入/扫描页）
  ↓ 6 passage 对齐（逐字/模糊）
  ↓ 7 引文判定（直接/间接/异文/矛盾/待考）
  ↓ 8 入库（source/passage/citation）+ 事实状态
```

### 4.1 事件/主张抽取

输入：笔记标题、正文、已有事实。

输出：

```json
{
  "events": [
    {
      "name": "赤壁之战",
      "aliases": ["赤壁", "曹操南征"],
      "date": "建安十三年（208）",
      "people": ["曹操", "孙权", "刘备", "周瑜"],
      "places": ["赤壁"],
      "claims": [
        {"text": "曹操军号称八十万", "kind": "number"},
        {"text": "周瑜判断曹军不过十五六万", "kind": "number"}
      ]
    }
  ]
}
```

约束：

- 事件名、别名、时间、人物必须能在正文/标题中 grounding；
- 不要把模型知道的“外部历史知识”写成笔记主张；
- 主张仍然逐条进入事实库，不能用一个事件节点替代所有主张。

### 4.2 事件解析

- 先查本地 `history_events` 的 canonical/aliases；
- 可选查 Wikidata/CBDB/CHGIS 做权威 ID；
- 多个候选时保留 `event_candidates`，让用户确认，不静默合并；
- 同一事件的不同写法（赤壁 / 赤壁之战 / 建安十三年曹操南征）可以归到同一 event，但事实仍分开。

### 4.3 查询规划

对每个事件/事实生成查询组：

1. **事件名 + 别名**（简繁、异体）；
2. **人物 + 事件**（字号、谥号、庙号）；
3. **书名 + 篇名/卷次**；
4. **年号 + 干支 + 公历**；
5. **英文/拼音**（世界史或海外研究）；
6. **原文精确短语**（引号）；
7. **“原文/全文/点校本/影印/卷/页”** 等检索限定词。

查询不要只有一条“赤壁之战 真相”。  
历史搜索的关键是**先找到可能的书名/篇名，再进全文库核对**。

### 4.4 多源发现

按来源类型分层：

| 层 | 目标 | 例子 |
| --- | --- | --- |
| A 一手史料全文 | 原文 | ctext、Wikisource、古籍全文库 |
| B 一手史料影像 | 版本/卷页 | 中华古籍资源库、书格、馆藏扫描 |
| C 档案/报刊 | 近代史料 | 一史馆、全国报刊索引 |
| D 二手研究 | 引文链/学术争论 | CNKI、NCPSSD、JSTOR、Google Scholar |
| E 工具书/权威 | 事件/人物/地名 | Wikidata、CBDB、CHGIS、TGAZ |

排序：

- 一手 > 点校本/影印本 > 后世编纂 > 二手研究 > 百科/网页；
- 有扫描页 > 只有转录 > 只有摘要；
- 版本/卷页齐全 > 只有书名；
- 同分保留上游顺序，不凭模型“感觉”判断权威。

### 4.5 获取原文

三种模式：

1. **自动抓取**：Wikisource/ctext/Internet Archive/Google Books 等公开页面，复用现有 `web_fetch`；
2. **手动录入**：付费库、登录库、实体书、档案馆；用户粘贴书目、卷页、原文、扫描链接；
3. **只登记线索**：知道书名但拿不到原文，登记为 `needs_research`，不假装已核验。

**不要自动抓取付费库/需要登录的库，也不要用网页摘要冒充原文。**

### 4.6 Passage 对齐

- 把抓取正文/用户粘贴文本切句；
- 先用 normalized 精确匹配，再用编辑距离/字符重叠做候选；
- 模型只负责“从候选段落中选最相关的一段”，不能生成原文；
- 最终 `citation.quote` 必须是 `passage.original_text` 的子串；
- 记录 `locator`（卷/篇/页/叶）和 `scan_url`；
- OCR 错字要保留 OCR 原文和人工校正两个字段，不能静默改原文。

### 4.7 引文判定

| relation | 含义 | 例子 |
| --- | --- | --- |
| `supports` | 原文明确支持 | 原文写“十五六万”，事实写“约十五六万” |
| `contradicts` | 原文明确不同 | 事实写“八十万”，原文写“十五六万” |
| `variant` | 版本/异文/不同记载 | 裴注引另一书说法不同 |
| `context` | 只提供背景，不直接判定 | 原文提到赤壁但没谈兵力 |
| `not_found` | 查过但没有可用原文 | 只作为“待考”，不是“不存在” |

判定仍由模型做，但必须受程序约束：

- source 必须在候选 source 列表；
- quote 必须在 passage；
- 没有 passage 不允许 `supports/contradicts`；
- 没有访问权限时写 `needs_research`，不写 `unsupported`。

---

## 5. 搜索渠道与自动化边界

### 5.1 中文古籍/史料

| 渠道 | 内容 | 自动化建议 | 注意 |
| --- | --- | --- | --- |
| [ctext.org](https://ctext.org/) | 先秦—明清典籍全文、扫描对照、检索 | 可做 HTML/API 适配（不稳定，需降级） | 不是权威点校本；引用前核对扫描页 |
| [中华古籍资源库](https://read.nlc.cn/) | 国图善本、方志、家谱、拓片等影像 | 首版**手动/半自动**；检索页跳转 | 主要是书目检索 + 影像，不等于全文 OCR |
| [书格](https://www.shuge.org/) | 高清古籍 PDF、舆图、拓本 | 手动/链接登记 | 适合看影像，不是全文检索库 |
| [Wikisource](https://zh.wikisource.org/) | 已转录古籍/文献 | MediaWiki API 可自动 | 转录可能有错，必须对扫描 |
| [中国第一历史档案馆](https://fhac.com.cn/) | 明清档案目录；清实录/清会典全文 | 手动/链接跳转 | 公开全文有限，更多需到馆/申请 |
| [全国报刊索引](https://www.cnbksy.com/) | 晚清、民国报刊全文 | 手动/机构订阅跳转 | 不要绕过登录/付费墙 |
| [国家哲学社会科学文献中心](https://www.ncpssd.org/) | 学术论文/研究文献 | 手动/链接跳转 | 用来找二手研究脚注 |

### 5.2 外文/世界史

| 渠道 | 内容 | 自动化建议 |
| --- | --- | --- |
| [HathiTrust](https://www.hathitrust.org/) | 千万级卷册，全球全文检索 | 先做链接/手动；全文页受版权限制 |
| [Internet Archive](https://archive.org/) | 扫描书、microfilm、档案 | API 可查元数据；全文检索偏条目级 |
| [Google Books](https://books.google.com/) | 绝版书、预览、片段 | Google Books API 可做发现 |
| [JSTOR](https://www.jstor.org/) / [Project MUSE](https://muse.jhu.edu/) | 二手研究/书评/学术争论 | 手动/机构访问 |
| [Perseus](https://www.perseus.tufts.edu/hopper/) / [Loeb](https://www.loebclassics.com/) | 希腊罗马原典/英译 | 公开页面可适配 |

### 5.3 权威数据（可选，不阻塞首版）

- [Wikidata](https://www.wikidata.org/)：事件/人物/地点 QID；
- [CBDB](https://cbdb.fas.harvard.edu/)：中国历史人物 ID；
- [CHGIS](https://chgis.org/) / TGAZ：历史地名、时间范围；
- 这些用于**消歧和匹配**，不作为史料原文来源。

### 5.4 自动化分级

| 级别 | 行为 | 适用 |
| --- | --- | --- |
| L1 自动 | API/HTML 抓取 + passage 对齐 | Wikisource、ctext、Google Books、Internet Archive |
| L2 半自动 | 生成检索链接/检索式，用户打开并粘贴 | NLC、一史馆、全国报刊索引、HathiTrust |
| L3 手动 | 用户录入书目/卷页/原文/扫描链接 | 实体书、档案馆、付费库 |
| L0 禁止 | 绕过登录、批量抓付费库、伪造原文 | 全部 |

首版建议只做 **L1 + L3**，L2 先做“打开检索页 + 粘贴导入”。不要一开始就追所有库的自动化。

---

## 6. 查询策略：怎么高效找到原文

### 6.1 已知书名/篇名

1. `书名 + 篇名 + 人物` 搜 ctext/Wikisource；
2. 找到卷次/段落；
3. 对照扫描页（ctext 的 image/transcription view、NLC 影像）；
4. 记录 edition/volume/page/scan_url；
5. 如果只有转录，标 `transcription_only`，不标“已核对原书”。

### 6.2 只知道事件/人物

1. 先用二手研究（CNKI/NCPSSD/JSTOR/Google Scholar）找脚注；
2. 脚注里的书名/卷次就是下一跳；
3. 再进古籍全文库/扫描库；
4. 不要拿百科/自媒体当史料出处。

### 6.3 不知道出处

1. 事件别名 + 人物 + 关键词“原文/记载/史料”；
2. 在 ctext 全文检索中用**精确短语**；
3. 同时试繁体/简体/异体字；
4. 用 CBDB/CHGIS/Wikidata 消歧人物/地点/事件；
5. 找到候选后仍然回到“已知书名/篇名”流程核对。

### 6.4 版本与异文

- 同一书不同版本可能句读/字句不同；
- 记录 `edition`、`publisher`、`year`、`volume/page`；
- 异文用 `variant_of` 连接，不覆盖原文；
- 引文保留原字形；normalized 只用于搜索，不用于展示。

### 6.5 不能做的事

- 不把网页摘要当“史料原文”；
- 不把 ctext/Wikisource 转录当“已核对原书”；
- 不自动抓取付费/登录库；
- 不把“搜不到”判成“历史事实不存在”；
- 不把不同版本/不同说法强行合并成一条“唯一事实”。

---

## 7. UI 方案

### 7.1 路由

```text
/facts              现有事实库
/facts/:id          现有事实详情（历史模式增加事件/史料区）
/events/:id         事件页（时间线、人物、地点、相关事实/史料/笔记）
/sources            史料库（按年代/类型/版本筛选）
/sources/:id        史料详情（书目、卷页、原文、引用它的事实/笔记）
```

### 7.2 事实详情（历史模式）

在现有“引用笔记 / 证据 / 相关 / 核验历史”基础上增加：

- **事件卡**：事件名、别名、时间、地点、人物、权威 ID；
- **史料**：按 source 分组，显示书名/篇名/版本/卷页/原文/扫描链接；
- **异文/别说**：同一事实的不同史料说法并列，不合并；
- **研究史**（可选）：二手研究观点；
- **状态**：`有直接史料 / 有间接史料 / 史料互异 / 待考 / 今人研究`，不再用网页式“未找到”。

### 7.3 史料详情

- 书目信息：书名、作者、成书年代、版本、出版社、藏地、索书号；
- 原文 passages：卷/篇/页、原文、今译、扫描页；
- 引用它的事实；
- 引用它的笔记；
- 外部链接：全文页、扫描页、馆藏目录；
- 版本/异文关系。

### 7.4 史料库

- 筛选：年代、史料类型（一手/二手/档案/报刊/工具书）、版本、是否有扫描、是否已核对；
- 搜索：书名、作者、篇名、原文关键词；
- 新增：手动录入书目 + 粘贴原文 + 扫描链接；
- 不把“事实”混进史料列表；二者互相链接。

### 7.5 笔记右栏

首版不必再加一个“史料”tab。现有「事实」tab 点进事实详情即可看到史料区；  
如果历史笔记的史料很多，再增加一个「史料」tab（只列本笔记引用/提及的史料），不要把史料塞进事实行。

### 7.6 历史搜索面板

在“速览与溯源”或单独的“历史研究”模板中：

- 事件/人物/地点/时间识别结果；
- 查询变体（简繁/别名/年号）；
- 渠道选择：ctext / Wikisource / Google Books / Internet Archive / 手动；
- 结果分“原文候选 / 书目候选 / 需要手动打开”；
- 每个候选显示来源类型、版本、是否有扫描、是否已对齐；
- 一键“录入为史料”。

---

## 8. API 与模块

### 8.1 Rust 模块

```text
src-tauri/src/production/
  history.rs          # 事件/历史状态/迁移
  sources.rs          # source/passage/citation CRUD、搜索、对齐
  source_search.rs    # provider adapters、query planner
```

或在 `production/history/` 下拆：

```text
history/
  mod.rs
  events.rs
  sources.rs
  passages.rs
  citations.rs
  search/
    mod.rs
    ctext.rs
    wikisource.rs
    google_books.rs
    internet_archive.rs
    manual.rs
```

### 8.2 Tauri commands（建议）

```text
production_history_event(id)                  # 读事件
production_history_events(query, limit)       # 查事件
production_history_resolve(text)              # 事件候选/别名
production_sources(query, filters, limit)     # 史料库
production_source(id)                         # 史料详情
production_source_save(input)                 # 手动新增/编辑
production_source_passages(source_id)         # 原文段落
production_source_import(input)               # 粘贴/导入原文
production_fact_citations(fact_id)            # 事实的史料引用
production_citation_save(input)               # 关联 passage -> fact
production_history_search(input)              # 历史搜索（provider/query）
production_history_fetch(input)               # 抓取/登记来源
production_history_align(input)               # passage 对齐
```

现有 `production_facts` / `production_fact` / `production_verify_fact` 保留；历史模式新增独立命令，不把古籍搜索硬塞进 `production_verify_fact`。

### 8.3 前端

```text
src/views/SourcesView.vue
src/views/SourceDetailView.vue
src/views/EventDetailView.vue
src/components/history/SourcePicker.vue
src/components/history/CitationEditor.vue
src/components/history/HistorySearchPanel.vue
src/components/history/EventCard.vue
```

### 8.4 设置

- 历史搜索渠道开关；
- 每次搜索最大候选数/抓取数；
- 是否使用权威数据（Wikidata/CBDB/CHGIS）；
- 是否允许自动抓取公开页面；
- 付费库只生成跳转链接，不存凭证；
- 数据许可/版权提示。

---

## 9. 判定与状态语义

### 9.1 事实状态

保留通用 `facts.status` 兼容现有 UI；历史模式主要读 `historical_status`：

```text
direct_source          有直接史料，引文已对齐
indirect_source        只有间接/后世记载
source_variant         史料互异
needs_research         查过但没有可用原文/权限
modern_scholarship     只有今人研究
disputed_interpretation 史料相同、解释不同
```

### 9.2 Citation 关系

```text
supports     原文支持
contradicts  原文与主张冲突
variant      异文/不同版本/不同记载
context      只提供背景
```

### 9.3 不要用的词

- 不要把“史料互异”叫“事实错误”；
- 不要把“没找到”叫“不存在”；
- 不要把“ctext 转录”叫“原书原文”；
- 不要把“二手论文”叫“一手史料”。

---

## 10. 测试与验收

### 10.1 单元测试

- `source_id`/`passage_id`/`citation_id` 稳定性；
- 同一 source 多 passage、同一 passage 多 fact；
- citation.quote 必须是 passage 原文子串；
- 错误 quote 被拒绝；
- 版本/异文不覆盖原文；
- event alias/日期解析；
- 历史状态映射；
- 旧 `fact_sources/fact_evidence` 迁移到 sources/passages/citations；
- 删除 source 的引用保护。

### 10.2 固定 fixtures（离线）

- 《三国志·吴书·周瑜传》裴注引《江表传》段落；
- 《史记·项羽本纪》鸿门宴段落；
- 一条世界史原典（Perseus/Loeb 公版段落）；
- 每条都写“原文/版本/卷页/预期关系”。

### 10.3 Live tests（ignored）

- Wikisource MediaWiki API；
- ctext 检索页/扫描页；
- Google Books API；
- Internet Archive metadata；
- 现有 `web_fetch` 抓 HTML/PDF；
- 不测付费库自动化。

### 10.4 E2E

1. 新建一篇历史笔记，正文包含一个事件和 2-3 条主张；
2. 历史模式抽取事件/主张；
3. 生成查询变体；
4. 从 Wikisource/ctext 找到一个候选；
5. 抓取/粘贴原文；
6. 对齐 passage；
7. 事实详情显示“史料/原文/卷页/异文”；
8. 从事实反查 source，从 source 反查事实/笔记；
9. 断网/无权限时显示“待考”，不是“未找到”；
10. 旧事实库数据不丢。

### 10.5 UI 验收

- 1040 宽、明/暗；
- 史料详情长原文可读；
- 卷页/扫描链接可点；
- 事实详情“史料”区不压过“引用笔记”；
- 史料库筛选/搜索/新增；
- 每页 0 个 <12px 文本、0 低对比（沿用现有审计）。

---

## 11. 分期实施

> 下面是原计划。**M1 的“手动新增史料”已不再作为产品主路径**；当前实现保留了数据模型，但 UI 改成自动溯源结果索引。阅读时把 M1 理解为“数据契约验证”，不要照着重加手动入口。

### M1：史料库 + 手动引用（先可用）

- `sources` / `source_passages` / `citations` 表；
- 手动新增史料、粘贴原文、卷页/版本/扫描链接；
- 事实详情增加“史料”区；
- 史料详情反查事实/笔记；
- 迁移旧 web evidence；
- 不做外部搜索。

**价值**：用户已经可以把自己知道的史料原文录进来，形成可追溯引用。

### M2：历史事件 + 历史状态

- `history_events`；
- 抽取事件/别名/时间/人物/地点；
- 事实 `domain/event_id/historical_status`；
- 事件页；
- 历史模式 UI 文案/状态；
- 不做自动搜索。

**价值**：同一事件的多篇笔记/多条事实能聚合，不再只按文本相似度。

### M3：历史搜索（公开库）

- query planner（别名/简繁/年号/人物/书名）；
- Wikisource/ctext/Google Books/Internet Archive adapter；
- 候选 source/passage 展示；
- 手动确认后入库；
- 检索/抓取 checkpoint；
- 不做付费库自动抓取。

**价值**：从“用户自己录入”升级到“能找到候选原文”。

### M4：半自动/导入/质量

- NLC/一史馆/全国报刊索引：生成检索链接 + 粘贴导入；
- 扫描页/版本/异文 UI；
- 历史状态/研究史；
- 权限/版权/robots 限制；
- 真实历史笔记 E2E。

### M5：可选增强

- Wikidata/CBDB/CHGIS 权威解析；
- 自动繁简/年号转换；
- 事件图谱；
- 史料 OCR（仅公版/用户上传）；
- 引用导出（CSL/Chicago/GB/T 7714）。

---

## 12. 风险与诚实边界

- **ctext/Wikisource 转录不等于原书**：必须对扫描页；OCR 有错字。
- **中华古籍资源库以影像为主**：不是全文 OCR，自动检索能力有限。
- **一史馆公开全文有限**：更多是目录/到馆；不能承诺线上一键全文。
- **全国报刊索引/CNKI/JSTOR 有付费墙**：只能链接/机构访问，不绕过。
- **版权**：只长期保存公版/用户提供/合理引用片段；不批量存付费全文。
- **版本差异**：同一书不同版本字句/句读不同；异文必须并列。
- **事件身份有争议**：事件名/日期/地点也可能有异说；事件节点是工作锚点，不是终审。
- **模型会编引文**：quote 必须程序化校验为 passage 子串；source 必须在候选内。
- **搜不到不等于不存在**：历史模式用 `needs_research`，不轻易 `unsupported`。
- **权威数据不完整**：CBDB/CHGIS/Wikidata 覆盖有限，不能强制所有事件都有 ID。
- **首版不做 OCR**：扫描件需要人工录入/后续 OCR；扫描 PDF 文本层不可靠。
- **搜索渠道会变**：适配器要可降级；不能把某个站的 HTML 当稳定 API。

---

## 13. 明确不建议做的事

1. 不要把史料做成 `facts` 的一个 `type` 或状态；
2. 不要因为事件固定就自动合并所有相关主张；
3. 不要用网页摘要/百科当史料原文；
4. 不要自动抓取付费/登录库；
5. 不要把 ctext/Wikisource 转录直接标“已核对原书”；
6. 不要把“搜不到”判成“历史事实不存在”；
7. 不要把所有版本/异文强行归一；
8. 不要在没有 passage 的情况下让模型写 quote。

---

## 14. 早期 M1 的选择（已被产品修正推翻）

> 这一节记录的是 2026-11-08 第一版为什么先做手动史料库。**现在的产品方向已经改为自动溯源，手动入口从主 UI 移除。** 保留这段是为了说明当时的技术判断，不是当前实现建议。

当时如果只做一件事：**先做 M1 的手动史料库 + 事实引用**。

原因：

- 历史研究里大量核心史料本来就不在公开 API 里，手动录入是必需能力，不是退路；
- 它不依赖外部站点稳定性，能先验证数据模型和 UI；
- 等 M1 的 source/passage/citation 稳定后，再把搜索接进来，不会返工；
- 事件/历史状态（M2）可以在 M1 的数据上自然生长。

**为什么后来推翻：** 手动录入能验证数据模型，但它不是产品主路径。一个 14 字段的新增史料表单没人会维护；史料库如果依赖用户手工建库，就永远只有验证数据。正确顺序是：自动检索/抓取/对齐先成为主路径，人工只保留“纠错/离线补录”的隐藏能力。当前代码已经按这个方向收掉主 UI，并接入了自动入库（见 §15.2）。

首版验收标准（当时）：

- 一条历史事实能挂 1-3 条史料；
- 每条史料有书名/版本/卷页/原文/扫描链接（可空但显式）；
- 事实详情能展示原文和“异文/待考”；
- 史料详情能反查事实和笔记；
- 旧事实库不丢；
- 不依赖任何付费库或 API Key 也能跑通一次完整引用。


## 15. 实现记录（M1/M2 + 领域分区 + 自动入库）

### 15.1 已落地

- **表**：`sources`、`source_passages`、`citations`、`history_events`、`history_meta`；
- **旧数据**：`fact_sources` + `fact_evidence` 是网页事实核查证据，**不再迁入史料库**；`source_version=3` 会清理已有的 `source_type='web'` 副本；旧 `fact_evidence` 保留，事实详情「网络证据」继续可读；
- **facts 扩展**：`domain`、`event_id`、`historical_status`、`claim_kind`；
- **事实领域**：主张抽取 prompt 新增 `domain`（`history` / `general`）；`record_candidate_with_domain` 落库；已有引用的事实由 `domain_version=1` 迁移标成 `history`；同一事实后续被判定为历史时只升不降；事实详情新增「归类」按钮，可人工改成历史/其他事实；
- **自动史料入库**（`history_ingest.rs` + `history_search.rs`）：
  - 先由查询规划器输出 `work/section/people/era/queries`；无 AI 时用 18 部核心史书的静态 registry + claim queries 兜底；
  - 多源发现：ctext 官方 API（`searchtexts?title=`，可匿名拿书目）、Wikisource（全文）、Internet Archive（metadata + `_djvu.txt` OCR）；NLC/一史馆/HathiTrust 只生成候选链接；
  - 候选按 authority tier + work/section 匹配 + query overlap 排序；
  - 只有 `primary` / `transcription` / `ocr` 且抓到全文的候选才尝试写 citation；`link_only` / `secondary` / `reference` 只写候选；
  - 抓全文后按句号/分号/换行切 passage；先用确定性最长公共子串兜底，有 AI 时让模型从候选 passage 里“挑一句”；
  - 模型输出必须通过 `quote` 是 `passage` 逐字子串的校验；失败就丢弃；
  - 只有拿到逐字 quote 才写 `source/passage/citation`；找不到、网络失败、候选不是全文库时不写；
  - 候选写入 `history_search_candidates`，抓取文本写入 `history_page_cache`（7 天 TTL）；
  - 溯源流水线内对最多 3 条历史主张自动跑；事实详情也有「自动找史料」按钮（force）；
- **API**：sources 列表/详情/保存、passage 保存/删除、citation 保存/删除、事件列表/详情/保存、事实关联事件、历史状态设置、史料检索（后端保留）、`production_fact_domain_set`、`production_fact_history_ingest`、`production_fact_history_candidates`、`production_fact_history_candidate_import`；
- **UI**：
  - `/facts` 事实库：顶部「全部事实 / 历史事实 / 其他事实」分区，列表按「历史事实」「其他事实」分组；历史事实带「历史」标记；
  - `/facts/:id` 事实详情：只有 `domain=history` 才显示「史料」区；其他事实只显示「网络证据」「相关与冲突」「核验历史」；史料区有「自动找史料」；每条引用可移除（不删史料原文）；候选行有「补录原文」：打开候选页 → 粘贴正文 → 程序切 passage/挑句/子串校验 → 写 citation 或拒绝；
  - `/sources` 史料库：只读索引，移除「新增史料 / 找史料」；保留一个很轻的「补录离线史料」入口（书名/篇名/作者/版本/原文/定位/可选历史事实）；
  - `/sources/:id` 史料详情：只读，展示书目、原文、今译、扫描链接、引用它的事实；移除「加原文段落 / 引用到事实」；
  - 顶栏保留「史料」入口；事件页/历史状态选择已从 UI 移除。
- **检索助手**：`production_history_search` 后端仍可查 Wikisource + Internet Archive；同时返回 ctext/NLC/书格/一史馆/全国报刊索引/HathiTrust 外部链接。结果只是候选，不自动入库。**当前 UI 不再暴露这个入口。**
- **安全边界**：自动入库不会把网页摘要当史料；`source_type='web'` 仍被排除；模型只能从抓取到的原文里挑 quote，不能生成原文。

### 15.2 真机数据（2026-11-08，自动入库版）

- 当轮库：**17 条事实** = **1 条历史事实** + **16 条通用事实**（后续历史语料已扩到 59 条：43 历史 + 16 通用）；
- 历史事实：`fact_cc798cf03ac6efb10e3e7ef2`「严嵩无他才略，唯一意媚上，窃权罔利。」，`domain=history`；
- 史料：**1 份**《明史·严嵩传》、**1 段原文**、**1 条引用**；事件表 **0**；
- **自动找史料真机**：点「自动找史料」后，查询规划返回 `明史 · 卷三百八·奸臣传·严嵩`；发现 **7 条候选**（ctext 明史、HathiTrust、NLC、ctext 台湾文献、明史演义、明史纪事本末、一史馆）；ctext 无 API Key、Wikisource 直连失败，**自动抓取 0 条，citation 仍为 1**，候选全部保留并显示原因，没有污染史料库；
- **ctext live**：`searchtexts?title=明史` 真实返回书目，`gettext` 无 Key 返回 `ERR_REQUIRES_AUTHENTICATION`；网页抓取被 ctext 明确禁止，本系统只走官方 API；
- **候选补录真机**：ctext《明史》候选 → 粘贴「严嵩无他才略，唯一意媚上，窃权罔利。」→ 写入 1 条 citation，候选标 cited，fact citation 1→2；随后移除 citation、删除测试 source/passage、候选恢复 candidate，库回到 1/1/1；
- **domain 纠正真机**：把严嵩事实切到「其他事实」，史料区隐藏、状态回到未核验；再切回「历史事实」，史料区恢复；DB `domain` 实测 general → history；
- **离线补录真机**：/sources 打开「补录离线史料」，提交「测试离线史料」+ 测试原文 + 引用到严嵩事实；sources/citations 从 1/1 变 2/2；在事实详情移除测试 citation 后 citations 回到 1；停应用后删除测试 source/passage，库回到 1/1/1；未留测试数据。保存时先建 source，再建 passage，再可选 citation；失败会尽力回滚 source；
- 页面验收：
  - 事实库显示「历史事实 1 / 其他事实 16」，历史区只有严嵩事实；
  - 通用事实详情（如「标题声称民主党在中期选举中大胜。」）只有「网络证据」，没有「史料」；
  - 历史事实详情显示「历史事实」和「史料」，网页证据为空时提示「没有补充的网页证据；这条历史事实以史料为准」；
  - 嘉靖笔记右栏显示「有史料 / 历史 / 1 条史料」；
  - 史料库只剩《明史》一条，没有「新增史料」按钮，只有轻量「补录离线史料」；
  - 史料详情只读，不再有手工加段落/引用入口；
- 截图目录：`shots/domain/`：
  - `01-facts-history-area.png`：事实库分区；
  - `02-general-fact-no-sources.png`：通用事实无史料栏；
  - `03-history-fact-sources.png`：历史事实有史料栏 + 自动找史料 + 归类 + 移除引用；
  - `04-sources-read-only.png`：史料库只读 + 补录入口；
  - `05-source-detail-read-only.png`：史料详情只读；
  - `06-history-note-rail.png`：历史笔记右栏；
  - `07-history-filter.png`：历史事实筛选；
  - `08-auto-find-network-fail.png`：自动找史料失败，不伪造结果；
  - `09-domain-general-hidden.png`：人工归到其他事实后史料区隐藏；
  - `10-manual-offline-import.png`：离线/纸本补录弹窗；
  - `11-history-search-candidates.png`：查询规划 + 候选摘要；
  - `12-history-candidates-expanded.png`：候选展开（authority/status/reason/外链）；
  - `13-ctext-key-setting.png`：ctext API Key 设置；
  - `14-candidate-paste-dialog.png`：候选补录原文弹窗；
  - `15-candidate-paste-cited.png`：补录成功、候选标 cited（测试后已清理）。
- `cargo test`：**165 passed / 7 ignored**（7 个 ignored 包含 ctext / B站 / 历史评测 live 测试）；
- 包：**8,714,113 B（8.31 MiB）** / exe **21,882,880 B（20.87 MiB）**。

### 15.3 未完成 / 诚实边界

- **自动入库链路已实现，但没有 live 自动 citation**：本机 Wikisource/IA 直连超时；ctext 无 Key 只能拿书目候选；最终 citation 仍只有早前人工验证的 1 条《明史》。**候选补录**真机写入过 1 条测试 citation（ctext 明史），测试后已清理。ctext `searchtexts` 已 live 成功；`gettext` 需要订阅 Key，未 live 成功。
- **候选补录是人工最后一步**：用户要打开候选页、复制正文/段落并粘贴；程序负责切 passage、挑句、逐字校验和写 citation。它不是 14 字段手填，但仍依赖用户判断页面是否是对应版本；粘贴到同文异版时，quote 校验可能通过，版本仍需人工确认。
- **ctext 网页抓取被明确禁止**：官方页面会返回反爬提示并声明会故意返回损坏数据；本系统只走 `api.ctext.org`，没有 Key 时只保留候选链接，绝不抓网页。
- **Wikisource/IA 抓取器未 live 验证**：MediaWiki API 与 IA `_djvu.txt` 解析器有 fixture/单测，但本机无法直连。
- **模型选 quote 的 prompt 没有在最终构建上跑过真实模型**：只测了 `parse_quote_reply`（越界、非子串、过短都拒绝）和确定性兜底；模型漏字段/返回非逐字文本时会被丢弃。
- **查询规划真机成功，但没有 golden set**：严嵩事实规划到 `明史 · 卷三百八·奸臣传·严嵩`；尚无 20–30 条标注集来衡量错书率。
- **确定性兜底要求繁简/标点基本一致**：最长公共子串不做繁简转换；繁简不同的主张可能只能依赖模型选句，模型失败就找不到。
- **ctext 只返回书目时不会递归章节**：M1 不递归 `subsections`；如果 `gettext` 只返回目录，会拒绝并保留候选。
- **候选里可能有相关但非同一版本的书**（如《明史纪事本末》《明史演义》）；它们只作候选，不会自动引用。
- **domain 仍是模型分类**：可能把时政评论里的历史引用标成 history，或漏标历史主张；现在有手动「归类」可以纠正，但不会自动发现错误。
- **新的 `history/general` prompt 没有跑过真实模型的完整 trace**：只验证了 JSON parser（显式 domain / 缺省 general）和真实库迁移；没有在最终构建上用真实 DeepSeek 跑一篇历史笔记，确认模型稳定返回 `history`。模型漏字段时会落成 `general`。
- 没有 OCR；扫描版 PDF/影像不能自动转 passage。
- 没有权威 ID（Wikidata/CBDB/CHGIS）解析；事件别名需手填。
- 历史状态/事件实体已从 UI 移除；当前只以是否有 `citations` 显示「有史料」。
- 没有自动从笔记抽取事件。
- 旧网页证据不再进入史料库；它们属于事实的「网络证据」，不冒充一手史料。
- 自动入库的 source 没有人工校对 UI；如果 quote 选错，只能在事实详情移除 citation 后重试，或后续补校对模式。

### 15.4 设计风险（不要为了简化而做错）

1. **不要把 `sources` 合并进 `facts`。** 一条史料可支持多条事实，一条事实可引多条史料；合并后无法表达版本、卷页、异文和反查。
2. **不要把网页证据直接当史料。** 之前的 `source_type='web'` 污染已经证明：URL/摘要不是“哪本书哪一版哪一页”。自动入库必须先拿到原文并做逐字校验。
3. **不要从检索 snippet 直接生成 citation。** snippet 不是 passage，模型可能补字；必须以抓取到的全文为 passage，quote 必须是子串。
4. **不要因为自动溯源没接完就恢复 14 字段手动表单。** 正确做法是补自动链路；人工只做隐藏的纠错/离线补录。
5. **不要把 `domain` 当成绝对真理。** 它只是路由：决定显示史料区还是网页证据区；现在有手动纠正，但模型误判仍可能发生。
6. **自动入库宁缺毋滥。** 抓不到全文、quote 不是子串、候选不是全文库时，必须不写 citation；否则会重演“网页摘要冒充史料”的污染。
7. **不要抓 ctext 网页。** ctext 明确禁止爬取，并声明会故意返回损坏数据；只能走官方 `api.ctext.org`，没有 Key 就只保留候选。
8. **不要把 link_only/候选当 citation。** NLC/一史馆/HathiTrust/ctext 无 Key 只是目录线索；候选池与史料引用必须分开。
