// End-to-end acceptance for the citation fill path.
// usage: node tools/bench-cite-fill.mjs
//
// Creates five temporary candidate citations with no URL, runs the real
// search -> fetch -> model -> verbatim-check path, and records success rate,
// source quality, duration and cost. Temporary rows are deleted afterwards.
// Requires the app running with CDP on 9222 and a configured AI/search key.

import fs from 'node:fs'
import path from 'node:path'

const claims = [
  { workTitle: '三国志·武帝纪', noteQuote: '公至延津，绍众大溃。' },
  { workTitle: '后汉书·五行志', noteQuote: '建安二十二年，大疫。' },
  { workTitle: '资治通鉴·泰始二年', noteQuote: '泗水冰合，弃船步走。' },
  { workTitle: '三国志·华佗传', noteQuote: '断肠湔洗，缝腹膏摩。' },
  { workTitle: '宋书·薛安都传', noteQuote: '薛安都遣使建康归顺。' },
]
const NOTE_ID = 'yn-656d47976f46b7051343fddf'

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

const rows = []
for (const claim of claims) {
  const created = await invoke('production_note_citation_add', {
    input: {
      noteId: NOTE_ID,
      noteQuote: claim.noteQuote,
      workTitle: claim.workTitle,
      status: 'candidate',
    },
  })
  try {
    const started = Date.now()
    let report
    let thrown = ''
    try {
      report = await invoke('production_cite_fill_original', { id: created.id })
    } catch (error) {
      thrown = String(error)
    }
    rows.push({
      ...claim,
      wallMs: Date.now() - started,
      ok: Boolean(report?.citation),
      error: thrown || report?.error || '',
      sourceQuality: report?.sourceQuality || '',
      original: report?.citation?.originalText || '',
      url: report?.citation?.url || '',
      durationMs: report?.durationMs ?? 0,
      modelCost: report?.usage?.estimatedCost ?? 0,
      searchCost: report?.searchUsage?.estimatedCost ?? 0,
      cost: (report?.usage?.estimatedCost ?? 0) + (report?.searchUsage?.estimatedCost ?? 0),
      searchCalls: report?.searchUsage?.successfulCalls ?? 0,
      promptTokens: report?.usage?.promptTokens ?? 0,
      completionTokens: report?.usage?.completionTokens ?? 0,
    })
  } finally {
    await invoke('production_note_citation_delete', { id: created.id }).catch(() => {})
  }
}

const ok = rows.filter((row) => row.ok)
const local = rows.filter((row) => row.ok && row.sourceQuality === 'local')
const primary = rows.filter((row) => row.ok && row.sourceQuality === 'primary')
const trusted = rows.filter((row) => row.ok && ['local', 'primary'].includes(row.sourceQuality))
const result = {
  measuredAt: new Date().toISOString(),
  claims: rows.length,
  ok: ok.length,
  local: local.length,
  primary: primary.length,
  trusted: trusted.length,
  modelCost: rows.reduce((sum, row) => sum + row.modelCost, 0),
  searchCost: rows.reduce((sum, row) => sum + row.searchCost, 0),
  cost: rows.reduce((sum, row) => sum + row.cost, 0),
  durationMs: rows.reduce((sum, row) => sum + row.durationMs, 0),
  note:
    'Temporary citations were deleted; the note body and existing citations were not changed. local = filled from an imported full text; trusted = local or primary. searchCost uses DeepSeek Search CNY 5 / 1,000 successful calls.',
  rows,
}
const outPath = path.resolve('docs/cite-fill-bench.json')
fs.mkdirSync(path.dirname(outPath), { recursive: true })
fs.writeFileSync(outPath, JSON.stringify(result, null, 2))

console.log(
  JSON.stringify(
    {
      claims: result.claims,
      ok: result.ok,
      local: result.local,
      primary: result.primary,
      trusted: result.trusted,
      modelCost: result.modelCost,
      searchCost: result.searchCost,
      cost: result.cost,
      durationMs: result.durationMs,
    },
    null,
    2,
  ),
)
for (const row of rows) {
  console.log(`${row.ok ? 'OK ' : 'FAIL'} ${row.sourceQuality.padEnd(9)} ${String(row.durationMs).padStart(5)}ms ¥${row.cost.toFixed(4)} (检索 ¥${row.searchCost.toFixed(4)}) ${row.workTitle} :: ${row.original || row.error}`)
}
console.log('saved', outPath)
ws.close()
