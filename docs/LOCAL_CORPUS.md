# 本地全文库

## 当前数据

本轮导入了三本公版史书，存放在用户的 `yebian.sqlite` 里（不打包进安装包）：

| 书 | 作者 | 段落 | 来源 |
| --- | --- | ---: | --- |
| 资治通鉴 | 司马光 | 258,436 | [Ancient-China-Books/zizhitongjian](https://github.com/Ancient-China-Books/zizhitongjian) |
| 后汉书 | 范晔 | 67,160 | [Ancient-China-Books/houhanshu](https://github.com/Ancient-China-Books/houhanshu) |
| 三国志 | 陈寿 | 35,955 | [Ancient-China-Books/sanguozhi](https://github.com/Ancient-China-Books/sanguozhi) |

合计 **361,551 段**；`yebian.sqlite` 约 **165 MiB**（173,174,784 B）。原始仓库是公版 EPUB；后汉书仓库 README 注明基于「漢川草廬」重製。

## 怎么导入

### 应用内

1. 打开「史料」页。
2. 点右上「导入全文」。
3. 填书名/作者/朝代，选择一个或多个 `txt` / `md` / `html` 文件。
4. 点「导入并切分」。应用会去掉 HTML 标签，按句号/分号/换行切 passage，并写入 `sources` / `source_passages`。

### 命令行（开发用）

```text
node tools/import-local-corpus.mjs <目录> <书名> [作者] [朝代]
```

脚本会递归读取目录下的 html/txt/md，按文件名排序，逐个调用 `production_local_corpus_import`。应用需要带 CDP 9222 运行。

## 检索与回填

- 补原文时，如果候选没有 URL，或 URL 是目录页，先查本地 passage；本地没有命中才走联网检索。
- 本地检索用 4 字重叠评分，完全包含原句时加 1000 分；最多取 3 段。
- `normalized_text` 用 OpenCC `TSCharacters.txt`（Apache-2.0）把繁体归一成简体；`original_text` 保留原字。这样简体笔记能搜到繁体原文，卡片仍显示原字。
- 命中后仍要过逐字校验：模型只能从 passage 里挑连续原文，不能自己写。成功写回 `source_id` / `passage_id` / `provider = local-corpus`，卡片显示「本地库」。

## 实测（2026-11-23）

同一组 5 条无 URL 候选：

| 指标 | P183 只有联网 | P185 本地优先 |
| --- | ---: | ---: |
| 成功 | 4/5 | **5/5** |
| 本地命中 | 0 | **4/5** |
| 可信来源（本地或原文库） | 4/5 | **5/5** |
| 总耗时 | 22.4s | **15.6s** |
| 模型成本 | ¥0.011640 | ¥0.0090 |
| 检索成本 | ¥0.025 | **¥0.005** |
| 总成本 | ¥0.036640 | **¥0.013996** |

成本下降 **61.8%**，本地命中 **4/5**，达到计划里的验收线（本地 ≥3/5、成本 ≥30%、0 条假原文）。

逐条：

- 三国志·武帝纪：本地，2,985ms，¥0.0018，原文「紹衆大潰，紹及譚棄軍走，渡河。」
- 后汉书·五行志：本地，509ms，¥0，原文「建安二十二年，大疫。」
- 资治通鉴·泰始二年：本地，2,029ms，¥0.0010，原文「泗水冰合，永等棄船步走」
- 三国志·华佗传：本地，1,464ms，¥0.0012，原文「病若在腸中，便斷腸湔洗，縫腹膏摩…」
- 宋书·薛安都传：未导入，回退联网，8,655ms，¥0.0100（含检索 ¥0.0050）

原始数据：[`docs/local-corpus.json`](local-corpus.json)、[`docs/cite-fill-bench.json`](cite-fill-bench.json)。

## 已知限制

- 只导入了三本，不是二十四史全量。
- 检索是 4 字重叠 + 扫描 `LIKE`，不是 FTS/向量检索；大库再扩时需要建索引。
- 繁简归一能覆盖常见字，但异体字/避讳字仍可能漏；漏了会回退联网。
- 导入是按文件顺序拼接，没有保留原书的卷/篇结构；`chapter` 目前统一写「全卷」。
- 安装包不包含语料，换机器需要重新导入。
- 本地原文保留繁体，和正文简体不一致时会在出处卡片里表现为差异；这是有意保留原字。
