// Measures the source-quality gate used by the citation fill agent.
// usage: node --no-warnings tools/bench-cite-source-quality.mjs
//
// Runs five real DeepSeek Search queries (count=5) and reports whether the
// top-1 hit is a primary full-text source. It is a measurement tool, not part
// of the app: the classifier is self-checked against known host mappings before
// any query runs. Search API cost is not priced by the app catalog, so this
// script reports latency only, not money.

import fs from 'node:fs'
import path from 'node:path'
import os from 'node:os'
import { DatabaseSync } from 'node:sqlite'

const PRIMARY = ['zh.wikisource.org', 'ctext.org', 'shidianguji.com', 'njmuseum.org.cn']
const CATALOG = ['read.nlc.cn', 'hathitrust.org', 'fhac.com.cn']

function sourceQuality(url) {
  const value = String(url || '').toLowerCase()
  if (PRIMARY.some((host) => value.includes(host))) return 'primary'
  if (CATALOG.some((host) => value.includes(host))) return 'catalog'
  return 'secondary'
}

// The measurement tool must agree with cite_agent::source_quality on known
// hosts before it is allowed to classify live search results.
const selfCheck = [
  ['https://zh.wikisource.org/wiki/三国志', 'primary'],
  ['https://ctext.org/sanguozhi', 'primary'],
  ['https://www.shidianguji.com/zh/book/1', 'primary'],
  ['https://www.njmuseum.org.cn/houhanshu', 'primary'],
  ['https://read.nlc.cn/', 'catalog'],
  ['https://babel.hathitrust.org/cgi/ls', 'catalog'],
  ['https://fhac.com.cn/archive', 'catalog'],
  ['https://example.com/paper', 'secondary'],
]
for (const [url, expected] of selfCheck) {
  const actual = sourceQuality(url)
  if (actual !== expected) throw new Error(`classifier self-check failed: ${url} => ${actual}, expected ${expected}`)
}

const dbPath = path.join(os.homedir(), 'AppData', 'Roaming', 'com.yebian.notes', 'yebian.sqlite')
const db = new DatabaseSync(dbPath, { readOnly: true })
const key =
  process.env.DEEPSEEK_API_KEY ||
  db.prepare("select value from settings where key = 'production.ai.api_key.deepseek'").get()?.value ||
  ''
db.close()
if (!key.trim()) throw new Error('no DeepSeek key in env or settings')

const queries = [
  '三国志 武帝纪 公至延津 绍众大溃',
  '后汉书 五行志 建安二十二年 大疫',
  '资治通鉴 泰始二年 泗水冰合 弃船步走',
  '三国志 华佗传 麻沸散 断肠湔洗 缝腹膏摩',
  '宋书 薛安都 遣使 建康 归顺 泰始',
]
const suffix = process.env.CITE_BENCH_SUFFIX || ''

const rows = []
for (const query of queries) {
  const searchQuery = `${query}${suffix}`.slice(0, 128)
  const started = Date.now()
  const response = await fetch('https://api.deepseek.com/v1/tools/search', {
    method: 'POST',
    headers: {
      Authorization: `Bearer ${key}`,
      'Content-Type': 'application/json',
    },
    body: JSON.stringify({ query: searchQuery, count: 5 }),
  })
  const text = await response.text()
  const latencyMs = Date.now() - started
  if (!response.ok) throw new Error(`search ${response.status}: ${text.slice(0, 300)}`)
  const value = JSON.parse(text)
  const list = value.results || value.search_result || []
  const hits = list
    .map((item) => ({
      title: item.title || '',
      url: item.url || item.link || '',
      quality: sourceQuality(item.url || item.link || ''),
    }))
    .filter((item) => item.url)
  if (!hits.length) throw new Error(`no url hits for query: ${query}`)
  // The product ranks primary before secondary before catalog, so the
  // decision-relevant top-1 is the first primary hit (if any), not the raw
  // search-engine first result.
  const ranked = hits.find((item) => item.quality === 'primary') || hits.find((item) => item.quality === 'secondary') || hits[0]
  rows.push({
    query,
    searchQuery,
    latencyMs,
    top1: hits[0],
    rankedTop1: ranked,
    counts: {
      primary: hits.filter((item) => item.quality === 'primary').length,
      secondary: hits.filter((item) => item.quality === 'secondary').length,
      catalog: hits.filter((item) => item.quality === 'catalog').length,
    },
    hits,
  })
  await new Promise((resolve) => setTimeout(resolve, 300))
}

const rawTop1Primary = rows.filter((row) => row.top1.quality === 'primary').length
const rankedTop1Primary = rows.filter((row) => row.rankedTop1.quality === 'primary').length
const gate = 'ranked top-1 primary >= 4/5'
const verdict = rankedTop1Primary >= 4 ? 'pass' : 'fail'
const costCny = rows.length * 0.005
const result = {
  measuredAt: new Date().toISOString(),
  provider: 'deepseek',
  suffix,
  count: 5,
  queries: queries.length,
  rawTop1Primary,
  rankedTop1Primary,
  verdict,
  rawVerdict: rawTop1Primary >= 4 ? 'pass' : 'fail',
  gate,
  costCny,
  costBasis: 'DeepSeek Search CNY 5 / 1,000 successful calls = CNY 0.005 per call; 5 successful calls',
  note:
    'Raw top-1 is the search engine first hit; ranked top-1 applies cite_agent source_quality ordering (primary > secondary > catalog), which is what the app actually uses. Cost is priced from the official per-successful-call price, not a billing export.',
  rows,
}

const outPath = path.resolve(process.env.CITE_BENCH_OUT || 'docs/source-quality-bench.json')
fs.mkdirSync(path.dirname(outPath), { recursive: true })
fs.writeFileSync(outPath, JSON.stringify(result, null, 2))

console.log(JSON.stringify({ rawTop1Primary, rankedTop1Primary, verdict, gate, costCny }, null, 2))
for (const row of rows) {
  console.log(
    `${row.rankedTop1.quality.padEnd(9)} ${String(row.latencyMs).padStart(4)}ms  ${row.rankedTop1.title.slice(0, 28)}  ${row.rankedTop1.url}`,
  )
}
console.log('saved', outPath)
