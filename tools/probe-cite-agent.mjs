// Probe for the minimal citation agent.
// usage: node tools/probe-cite-agent.mjs "<note title substring>" [outdir]
//
// Runs production_cite_note on an existing history note, then prints/saves the
// note citations. It writes only note_citations; it never writes facts.

import fs from 'node:fs'
import path from 'node:path'

const query = (process.argv[2] || '').trim()
if (!query) {
  console.error('usage: node tools/probe-cite-agent.mjs "<note title substring>" [outdir]')
  process.exit(2)
}
const outDir = process.argv[3] || path.join(process.env.TEMP || '.', 'cite-agent', `${Date.now()}`)
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

const started = Date.now()
const report = await invoke('production_cite_note', {
  input: { noteId: note.id, title: note.title, body: detail.body, maxCitations: 6 },
})
const durationMs = Date.now() - started
const citations = await invoke('production_note_citations', { noteId: note.id })

fs.writeFileSync(path.join(outDir, 'report.json'), JSON.stringify(report, null, 2))
fs.writeFileSync(path.join(outDir, 'citations.json'), JSON.stringify(citations, null, 2))
fs.writeFileSync(path.join(outDir, 'note.md'), detail.body)

const lines = [
  `# Citation agent probe`,
  '',
  `- note: ${note.title} (${note.id})`,
  `- status: ${report.status}`,
  `- summary: ${report.summary}`,
  `- citations: ${report.citations.length}`,
  `- duration: ${(durationMs / 1000).toFixed(1)}s`,
  `- usage: ${report.usage.promptTokens} in / ${report.usage.completionTokens} out / ¥${(report.usage.estimatedCost ?? 0).toFixed(4)}`,
  '',
  '## Citations',
  '',
]
for (const citation of citations) {
  lines.push(`### [${citation.status}] ${citation.noteQuote}`)
  if (citation.workTitle) lines.push(`- 出处：${citation.workTitle}${citation.volume ? ` · ${citation.volume}` : ''}${citation.page ? ` · ${citation.page}` : ''}`)
  if (citation.author) lines.push(`- 作者：${citation.author}`)
  if (citation.locator) lines.push(`- 位置：${citation.locator}`)
  if (citation.originalText) lines.push(`- 原文：${citation.originalText}`)
  if (citation.url) lines.push(`- 链接：${citation.url}`)
  if (citation.snippet) lines.push(`- 候选摘要：${citation.snippet}`)
  lines.push('')
}
lines.push('## Trajectory', '')
for (const turn of report.turns) {
  if (turn.phase === 'model') {
    const content = (turn.content || '').replace(/\s+/g, ' ').slice(0, 260)
    if (content) lines.push(`${turn.index}. model: ${content}`)
  } else {
    lines.push(`${turn.index}. ${turn.tool}(${JSON.stringify(turn.args ?? {}).slice(0, 240)})`)
    lines.push(`   → ${(turn.observation || '').replace(/\s+/g, ' ').slice(0, 300)}`)
  }
}
fs.writeFileSync(path.join(outDir, 'report.md'), lines.join('\n'))

console.log(`status ${report.status} · ${report.citations.length} citations · ${(durationMs / 1000).toFixed(1)}s · ¥${(report.usage.estimatedCost ?? 0).toFixed(4)}`)
for (const citation of citations) {
  console.log(`- [${citation.status}] ${citation.noteQuote.slice(0, 50)} → ${citation.workTitle || '?'}${citation.volume ? ` ${citation.volume}` : ''}`)
}
console.log('saved', outDir)
ws.close()
