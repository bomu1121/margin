// Compare the fixed trace pipeline with the hybrid trace agent on one note.
// usage: node tools/probe-trace-compare.mjs "<note title substring>" [outdir]
//
// 1. runs production_trace_note (the previous fixed pipeline)
// 2. takes its claim inventory
// 3. runs production_trace_agent_probe with the fixed inventory and ignoreFacts
// 4. saves comparison.json with both reports and metrics
// This spends real model/search calls and writes facts to the note.

import fs from 'node:fs'
import path from 'node:path'

const query = (process.argv[2] || '').trim()
if (!query) {
  console.error('usage: node tools/probe-trace-compare.mjs "<note title substring>" [outdir]')
  process.exit(2)
}
const outDir = process.argv[3] || path.join(process.env.TEMP || '.', 'trace-compare', `${Date.now()}`)
fs.mkdirSync(outDir, { recursive: true })

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

const notes = await invoke('list_notes')
const note = notes.find((item) => item.title === query) || notes.find((item) => item.title.includes(query))
if (!note) throw new Error(`note not found: ${query}`)
const detail = await invoke('get_note', { id: note.id })
if (!detail?.body?.trim()) throw new Error(`note body is empty: ${note.title}`)
console.log('note', note.id, note.relPath, `${detail.body.length} chars`)

// 1. previous fixed pipeline
const startedOld = Date.now()
const job = await invoke('production_trace_note', {
  input: {
    noteId: note.id,
    notePath: note.relPath,
    title: note.title,
    body: detail.body,
    traceClaims: 10,
    traceMax: 5,
  },
})
console.log('old pipeline job', job.id)
let oldDetail = null
const deadline = Date.now() + 30 * 60 * 1000
let lastLine = ''
while (Date.now() < deadline) {
  oldDetail = await invoke('production_get', { id: job.id })
  const current = oldDetail?.job
  if (!current) throw new Error('old job disappeared')
  const line = `${current.status} ${current.progress}% ${current.currentStep || ''}`
  if (line !== lastLine) {
    console.log(new Date().toISOString().slice(11, 19), line)
    lastLine = line
  }
  if (['success', 'error', 'cancelled', 'interrupted'].includes(current.status)) break
  await new Promise((resolve) => setTimeout(resolve, 2000))
}
const oldMs = Date.now() - startedOld
const oldJob = oldDetail?.job
if (!oldJob) throw new Error('old job missing')
const oldResult = oldJob.result || {}
const oldClaims = oldResult.trace || []
console.log('old pipeline done', oldJob.status, oldClaims.length, 'claims', `${(oldMs / 1000).toFixed(1)}s`)

if (!oldClaims.length) {
  fs.writeFileSync(path.join(outDir, 'comparison.json'), JSON.stringify({ note, old: { job: oldJob, ms: oldMs, claims: oldClaims } }, null, 2))
  throw new Error('old pipeline returned no claims')
}

// 2. hybrid: fixed inventory from the old pipeline + agent investigation
console.log('hybrid claims', oldClaims.length)
const startedNew = Date.now()
const hybrid = await invoke('production_trace_agent_probe', {
  input: {
    noteId: note.id,
    notePath: note.relPath,
    title: note.title,
    body: detail.body,
    claims: oldClaims.map((claim) => claim.claim),
    ignoreFacts: true,
  },
})
const hybridMs = Date.now() - startedNew
const facts = await invoke('production_note_facts', { noteId: note.id, runId: null })

const comparison = {
  note: { id: note.id, title: note.title, relPath: note.relPath, chars: detail.body.length },
  old: {
    engine: 'trace.rs fixed pipeline (note-trace)',
    jobId: job.id,
    status: oldJob.status,
    durationMs: oldMs,
    usage: oldJob.usage,
    claims: oldClaims,
    summary: oldResult.summary,
  },
  hybrid: {
    engine: 'fixed inventory + trace agent (ignoreFacts)',
    status: hybrid.status,
    stopReason: hybrid.stopReason,
    durationMs: hybridMs,
    usage: hybrid.usage,
    summary: hybrid.summary,
    claims: hybrid.claims,
    inventory: hybrid.inventory,
    missing: hybrid.missing,
    searches: hybrid.searchCount,
    fetches: hybrid.fetchCount,
  },
  facts,
}
fs.writeFileSync(path.join(outDir, 'comparison.json'), JSON.stringify(comparison, null, 2))
fs.writeFileSync(path.join(outDir, 'note.md'), detail.body)

const oldCost = oldJob.usage?.estimatedCost ?? 0
const newCost = hybrid.usage?.estimatedCost ?? 0
console.log(`old: ${oldClaims.length} claims / ${(oldMs / 1000).toFixed(1)}s / ¥${oldCost.toFixed(4)}`)
console.log(`new: ${hybrid.claims?.length ?? 0} claims + ${hybrid.missing?.length ?? 0} missing / ${(hybridMs / 1000).toFixed(1)}s / ¥${newCost.toFixed(4)} / ${hybrid.searchCount} searches / ${hybrid.fetchCount} fetches`)
console.log('saved', outDir)
ws.close()
