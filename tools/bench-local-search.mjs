// Benchmarks the local full-text search used by the source reader.
// usage: node tools/bench-local-search.mjs "<work title>" [queries]
//
// Samples 4-char CJK phrases from the first two reader pages, runs them through
// production_source_reader, and reports p50/p95. Requires CDP on 9222.

import fs from 'node:fs'
import path from 'node:path'

const workTitle = (process.argv[2] || '资治通鉴').trim()
const wanted = Number(process.argv[3] || 20)

const list = await (await fetch('http://127.0.0.1:9222/json/list')).json()
const page = list.find((item) => item.type === 'page' && item.url.startsWith('http'))
if (!page) throw new Error('no CDP page target')
const ws = new WebSocket(page.webSocketDebuggerUrl)
await new Promise((resolve, reject) => {
  ws.addEventListener('open', resolve, { once: true })
  ws.addEventListener('error', reject, { once: true })
})
let id = 0
const pending = new Map()
ws.addEventListener('message', (event) => {
  const message = JSON.parse(String(event.data))
  if (!message.id) return
  const entry = pending.get(message.id)
  if (!entry) return
  pending.delete(message.id)
  entry(message)
})
function send(method, params = {}) {
  const callId = ++id
  ws.send(JSON.stringify({ id: callId, method, params }))
  return new Promise((resolve, reject) => {
    pending.set(callId, (message) => {
      if (message.error) reject(new Error(JSON.stringify(message.error)))
      else resolve(message.result)
    })
  })
}
async function evaluate(expression) {
  const result = await send('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true })
  if (result.exceptionDetails) throw new Error(result.exceptionDetails.text || 'page error')
  return result.result?.value
}
const invoke = (command, args = {}) =>
  evaluate(`window.__TAURI_INTERNALS__.invoke(${JSON.stringify(command)}, ${JSON.stringify(args)})`)

const sources = await invoke('production_sources', { query: workTitle, sourceType: '', limit: 20 })
const source = sources.find((item) => item.workTitle === workTitle) || sources[0]
if (!source) throw new Error(`source not found: ${workTitle}`)

const paragraphs = []
for (const pageNo of [1, 2]) {
  const page = await invoke('production_source_reader', { id: source.id, query: '', page: pageNo, pageSize: 200 })
  paragraphs.push(...page.paragraphs)
}
const queries = []
for (const paragraph of paragraphs) {
  const chars = [...paragraph].filter((char) => /[\u4e00-\u9fff]/.test(char)).join('')
  for (let index = 0; index + 4 <= chars.length && queries.length < wanted; index += 37) {
    queries.push(chars.slice(index, index + 4))
  }
  if (queries.length >= wanted) break
}
if (queries.length < wanted) throw new Error(`only sampled ${queries.length} queries`)

const rows = []
for (const query of queries) {
  const started = performance.now()
  const result = await invoke('production_source_reader', {
    id: source.id,
    query,
    page: 1,
    pageSize: 200,
  })
  rows.push({
    query,
    ms: Math.round(performance.now() - started),
    matches: result.matchCount,
    page: result.page,
  })
}
const sorted = rows.map((row) => row.ms).sort((left, right) => left - right)
const percentile = (value) => sorted[Math.min(sorted.length - 1, Math.ceil(sorted.length * value) - 1)]
const result = {
  measuredAt: new Date().toISOString(),
  workTitle: source.workTitle,
  sourceId: source.id,
  queries: rows.length,
  p50Ms: percentile(0.5),
  p95Ms: percentile(0.95),
  maxMs: sorted.at(-1),
  rows,
}
const outPath = path.resolve('docs/local-search-bench.json')
fs.mkdirSync(path.dirname(outPath), { recursive: true })
fs.writeFileSync(outPath, JSON.stringify(result, null, 2))
console.log(JSON.stringify({ workTitle: result.workTitle, queries: result.queries, p50Ms: result.p50Ms, p95Ms: result.p95Ms, maxMs: result.maxMs }, null, 2))
console.log('saved', outPath)
ws.close()
