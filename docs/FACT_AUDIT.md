# 事实库争议状态全量审计（2026-11-10）

> 范围：`%APPDATA%\com.yebian.notes\yebian.sqlite` 里全部 **59 条事实**、55 条核验记录、59 条证据、63 条笔记提及。
> 结论先说：**主要是我们的问题，不是你的误读。** 22 条 `disputed` 里，**11 条是证据明明支持却被标成“存在争议”**；1 条是复合事实只覆盖了一半；剩下 10 条才是真的数值/身份/解释冲突或历史异说。

## 1. 总数与口径

| 状态 | 数量 |
| --- | ---: |
| `verified` | 29 |
| `disputed` | 22 |
| `unsupported` | 6 |
| `candidate` | 2 |
| **合计** | **59** |

- 55 条 `fact_checks`；7 条事实没有 check 记录（其中 4 条 `disputed`、2 条 `verified`、1 条 `candidate`）。
- 两条事实各有两个 check（托育补贴、2026 中期选举耗资），所以按 check 行数会出现 24 个 disputed 行；按事实去重是 **22**。
- 证据引用来自 `fact_evidence`，与 `fact_checks.evidence_json` 同源；本次逐条把 quote 与 fact 文本做了对照。

## 2. 22 条 disputed 的分类

### A. 误判：quote 支持事实，却被标成“存在争议”（11 条）

| 事实 | 证据 quote | 为什么会误判 |
| --- | --- | --- |
| 小布什政府以伊拉克 WMD 虚假情报为依据卷入战争 | “claims about the Iraqi threat turned out to be false” | quote 明确支持；无 check，旧数据直接带着 `contradicts` |
| 李渊晚年奢侈享乐、沉迷后宫、放权 | “李渊晚年宠幸的嫔妃越来越多” | quote 支持“沉迷后宫”；模型把“只覆盖一部分”当成争议 |
| 2015 皮尤：60% 共和党支持供武，民主党不到 39% | “60% of Republicans would send arms… only 39% of Democrats agree” | 数字逐字吻合；无 check，仍被标 disputed |
| 萧道成反对刘彧北上，刘彧不听 | “宋明帝不听萧道成之言……张永等人在彭城大败” | quote 支持“反对/不听”；模型误判成冲突 |
| 尉元与张永相持、断粮道，467 年正月张永南撤 | “次年 467 年正月……张永等……弃船步走撤还” | quote 支持时间与撤退；模型误判 |
| 义嘉之乱后薛安都、沈文秀、崔道固遣使归顺 | “文秀与崔道固俱以州降，请师接应” | quote 支持归顺；模型误判 |
| 尉元命薛安都追击、自己拦截，宋军大败，张永/沈攸之逃淮阴 | “攸之免官……还屯淮阴” | quote 至少支持“逃到淮阴”；模型把局部覆盖当冲突 |
| 玄武门时李世民可调动的不止东宫 | “当时的李世民已经掌握了大部分禁军” | quote 直接支持“兵力不止东宫” |
| 玄武门首因是李渊持续任用李世民 | “他收复山西，威望陡增……巩固军权” | quote 支持“任用→军权/威望”的因果方向 |
| 辽亡时能数出的抵抗者只有耶律大石，后西去 | “遂殺蕭乙薛坡里括自立爲王西去” | quote 支持“西去”；模型误判 |
| 466 年 11 月刘彧派张永、沈攸之率十五万北上 | “张永、沈攸之率甲士 15 万往迎” | 数字与事实逐字吻合 |

这 11 条里，2 条没有 `fact_checks`（小布什 WMD、皮尤），但 `fact_evidence.relation` 仍是 `contradicts`；其余 9 条 check verdict 被模型写成“存在争议”，代码又把“争议”直接映射成 `contradicts`。

### B. 复合事实只覆盖一半（1 条）

| 事实 | 证据 | 判断 |
| --- | --- | --- |
| 美国 400 万条含铅管道、持续几十年、50 多万儿童铅中毒 | “美国约有 50 万 1-5 岁儿童血铅超标” | quote 只支持“50 万儿童”，不支持“400 万条管道”；而 400 万和 50 万在库里各有一条 `verified` 的原子事实。它是**复合/重复**，不是“来源与事实矛盾” |

### C. 真实的数值/身份/解释冲突（10 条）

| 事实 | 证据 | 真实问题 |
| --- | --- | --- |
| 青苗法半年利息两成 | “青苗钱是不可能半年息两分的” | 利率口径冲突（两成/两分） |
| 每年 120 亿美元、两三年换完全部含铅水管 | “must be removed by 2037” | 时间口径被事实夸大 |
| 2022 托育补贴：1180 万符合、120 万领取、16% | “1.8 million children received subsidies…16%” | 百分比对，绝对人数 120 万 vs 180 万冲突 |
| 小布什祖父是摩根兄弟合伙人 | “partner in Brown Brothers Harriman” | 机构名冲突（摩根兄弟 vs 布朗兄弟哈里曼） |
| 陈寿父亲被诸葛亮处以髡刑 | “陈寿的父亲历来争议不断，未见史籍有准确记载” | 史料本身有异说；事实把传闻写成确定 |
| 2026 中期选举耗资“超过”120 亿美元 | “耗资近 120 亿美元” | “超过” vs “近”冲突 |
| 孙权早年信任周瑜/鲁肃/吕蒙/陆逊/张昭 | “孙权其实对自己的手下都不太信任” | 解释性冲突（早年/晚年、信任程度） |
| 宋军冻死“超过一半、十之八九冻掉手脚” | “士卒冻死者太半，手足断者什七八” | 太半/什七八 vs 超过一半/十之八九，比例被夸大 |
| 慕容白曜率大军进入青州 | “白曜到瑕丘，遇患……不得前进。曰曜遂不行” | 是否真正进入青州有冲突 |
| 小布什家族复合事实（祖父摩根、外祖父默克创始人） | 只支持“曾祖父钢铁石油大王” | 复合事实含错误/未支持子句；与上条祖父事实重叠 |

## 3. 根因

1. **核验 prompt 没有定义“存在争议”**
   `src-tauri/src/production/prompts.rs` 只给四个 verdict 名称，没有说“只有明确不同数值/日期/身份或明确反驳才算争议”。模型把“只覆盖一部分”“二手来源”“措辞不同”都写成“存在争议”。

2. **代码把“争议”直接当成“反驳”**
   `trace.rs:99` / `trace.rs:254`：
   ```rust
   relation: if item.verdict.contains("可印证") { "supports" }
             else if item.verdict.contains("争议") { "contradicts" }
             else { "mention" }
   ```
   这等于假设每个“存在争议”的 quote 都在反驳；实际上 quote 可能完全支持。

3. **另一条导入路径把非“可印证”一律写成 contradicts**
   `facts.rs:2288`：
   ```rust
   if verdict.contains("可印证") { "supports" } else { "contradicts" }
   ```
   “未找到”“仅非权威来源”“存在争议”都会变成 contradicts，包括没有 check 的旧数据。

4. **status 只看 verdict，不看 evidence relation**
   `status_for_verdict` 把含“争议”的 verdict 直接映射成 `disputed`；没有检查是否存在真正的 `contradicts` 证据。

5. **复合事实没有拆分/逐句支持检查**
   400 万管道 + 50 万儿童、Bush 家族四代人，都是“一句事实里多个断言”；一条 quote 只覆盖其中一句，就被判成争议或直接算 verified，口径不稳定。

## 4. 修复（已执行）

### 4.1 当前数据

已备份数据库到 `%APPDATA%\com.yebian.notes\yebian.sqlite.fact-audit-20261110.bak`，然后执行 11 条误判修复：`fact_evidence.relation` 改 `supports`，`fact_checks.verdict` 改 `外部可印证`，`facts.status` 改 `verified`。
- 1 条复合（400 万 + 50 万）：保留 `disputed`；它是复合/重复，不是“来源矛盾”，后续拆成两条原子事实。
- 10 条真实冲突：保留 `disputed`；UI 文案下一批从“来源与事实表述不一致”改成更准确的“数值/身份/解释冲突”。
- 修复后：`verified 40 / disputed 11 / unsupported 6 / candidate 2`；事实库状态行显示 `有争议 11`，不是 22。

### 4.2 未来核验（已落地）

- `TraceItem` 增加 `relation` 字段；`prompts.rs::trace_system` 用严格口径要求 `relation` 与 verdict 一致：
  - `外部可印证`：quote 与主体/时间/数值口径一致，能直接支持；只是措辞不同或只覆盖核心的一部分、但不冲突，仍算可印证。
  - `存在争议`：quote 明确给出不同数值/日期/身份，或明确反驳；不能因为“只覆盖一部分/二手来源”就写争议。
  - `仅非权威来源`：quote 支持，但来源是自媒体/论坛/营销页。
  - `未找到外部出处`：没有可用 quote。
- `record_verification` 的 status 由 **evidence relation** 决定，而不是 verdict 字符串：
  - 有 `contradicts` → disputed；
  - 有 `supports` 且无 contradicts → verified（`仅非权威` 除外）；
  - 只有 context/没有证据 → candidate/unsupported。
- 两条导入路径都优先读 `relation`，缺失才回退到 verdict 映射。
- 新增单测 `supporting_evidence_wins_over_a_disputed_verdict`：verdict 写“存在争议”但 relation=supports 时 status 必须是 verified；真实 contradicts 才 disputed。
- `cargo test --lib` 165 passed / 7 ignored；包 8,714,113 B（8.31 MiB）/ exe 21,882,880 B（20.87 MiB）。

## 5. 审计边界

- 本次是**逐条人工对照 quote 与 fact**，不是重新联网检索；判断依据是库里已有的 quote。
- 10 条“真实冲突”里，有些是历史异说/解释差异（孙权、陈寿），不一定是“事实错误”，但确实不能算“外部可印证”。
- 只审计当前 `sample-vault` 对应的库；其他 vault 未查。
- 没有重新运行 AI 核验；数据修复是确定性的 SQL 更新，修复前会备份。
- 复合事实（Bush 家族、400 万管道 + 50 万儿童）没有拆分；这是下一步“事实拆分/原子化”的输入。
