// Probe for the experimental trace agent.
// usage: node tools/probe-trace-agent.mjs "<note title substring>" [outdir]
//
// Finds a note in the current vault, runs production_trace_agent_probe on it,
// prints the full trajectory, and writes report.json / report.md / facts.json.
// This spends real model + search calls; it does not create a production job.

import fs from 'node:fs'
import path from 'node:path'

const query = (process.argv[2] || '').trim()
if (!query) {
  console.error('usage: node tools/probe-trace-agent.mjs "<note title substring>" [outdir]')
  process.exit(2)
}
const outDir = process.argv[3] || path.join(process.env.TEMP || '.', 'trace-agent-probe', `${Date.now()}`)
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
const exact = notes.find((note) => note.title === query)
const partial = notes.find((note) => note.title.includes(query))
const note = exact || partial
if (!note) {
  console.error(`note not found: ${query}`)
  console.error(notes.slice(0, 20).map((item) => `- ${item.title}`).join('\n'))
  process.exit(1)
}
const detail = await invoke('get_note', { id: note.id })
if (!detail?.body?.trim()) throw new Error(`note body is empty: ${note.title}`)
console.log('note', note.id, note.relPath, `${detail.body.length} chars`)

const started = Date.now()
const report = await invoke('production_trace_agent_probe', {
  input: {
    noteId: note.id,
    notePath: note.relPath,
    title: note.title,
    body: detail.body,
    maxClaims: 8,
    maxRounds: 14,
  },
})
const durationMs = Date.now() - started
const facts = await invoke('production_note_facts', { noteId: note.id, runId: null })

fs.writeFileSync(path.join(outDir, 'report.json'), JSON.stringify(report, null, 2))
fs.writeFileSync(path.join(outDir, 'facts.json'), JSON.stringify(facts, null, 2))
fs.writeFileSync(path.join(outDir, 'note.md'), detail.body)

const lines = [
  `# Trace agent probe`,
  '',
  `- note: ${note.title} (${note.id})`,
  `- status: ${report.status} / ${report.stopReason}`,
  `- summary: ${report.summary}`,
  `- claims: ${report.claims.length}`,
  `- searches: ${report.searchCount}, fetches: ${report.fetchCount}`,
  `- duration: ${(durationMs / 1000).toFixed(1)}s`,
  `- usage: ${report.usage.promptTokens} in / ${report.usage.completionTokens} out / ¥${(report.usage.estimatedCost ?? 0).toFixed(4)}`,
  '',
  '## Claims',
  '',
]
for (const claim of report.claims) {
  lines.push(`- **${claim.verdict}** ${claim.claim}`)
  if (claim.source) lines.push(`  - ${claim.source}`)
  if (claim.quote) lines.push(`  - “${claim.quote}”`)
}
lines.push('', '## Trajectory', '')
for (const turn of report.turns) {
  if (turn.phase === 'model') {
    const content = (turn.content || '').replace(/\s+/g, ' ').slice(0, 300)
    if (content) lines.push(`${turn.index}. model: ${content}`)
  } else {
    const args = turn.args ? JSON.stringify(turn.args).slice(0, 240) : ''
    lines.push(`${turn.index}. ${turn.tool}(${args})`)
    lines.push(`   → ${(turn.observation || '').replace(/\s+/g, ' ').slice(0, 320)}`)
  }
}
lines.push('', '## Facts for this note', '')
for (const fact of facts) {
  lines.push(`- [${fact.status}] ${fact.text}`)
  for (const evidence of fact.evidence || []) {
    lines.push(`  - ${evidence.url}`)
    if (evidence.quote) lines.push(`    “${evidence.quote}”`)
  }
}
fs.writeFileSync(path.join(outDir, 'report.md'), lines.join('\n'))

console.log(`status ${report.status} / ${report.stopReason} · ${report.claims.length} claims · ${report.searchCount} searches · ${report.fetchCount} fetches · ${(durationMs / 1000).toFixed(1)}s · ¥${(report.usage.estimatedCost ?? 0).toFixed(4)}`)
console.log(`saved ${outDir}`)
ws.close()
