// Audits the claims block of AI-note outputs that already exist in the vault.
// usage: node tools/audit-ai-claims.mjs
//
// Read-only: parses <!-- yebian-claims ... --> from note bodies, counts usable
// claims (史载/转述 with a work), and joins note_citations by note. Requires
// CDP on 9222.

import fs from 'node:fs'
import path from 'node:path'

const port = process.env.CDP_PORT || '9222'
const list = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json()
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

const CITABLE = new Set(['史载', '史實', '史实', '转述', '轉述'])
const notes = await invoke('list_notes')
const rows = []
for (const note of notes) {
  const detail = await invoke('get_note', { id: note.id })
  const body = detail?.body || ''
  const match = body.match(/<!-- yebian-claims([\s\S]*?)-->/)
  if (!match) continue
  let claims = []
  let parseError = ''
  try {
    claims = JSON.parse(match[1].trim())
  } catch (error) {
    parseError = String(error)
  }
  const citations = await invoke('production_note_citations', { noteId: note.id })
  const kinds = {}
  for (const claim of claims) {
    const kind = claim.kind || '?'
    kinds[kind] = (kinds[kind] || 0) + 1
  }
  const usable = claims.filter(
    (claim) => CITABLE.has(claim.kind) && String(claim.work || '').trim().length > 0,
  )
  rows.push({
    id: note.id,
    title: note.title,
    bodyChars: body.length,
    claims: claims.length,
    parseError,
    kinds,
    withWork: claims.filter((claim) => String(claim.work || '').trim()).length,
    withOriginal: claims.filter((claim) => String(claim.original || '').trim()).length,
    withUrl: claims.filter((claim) => String(claim.url || '').trim()).length,
    usable: usable.length,
    citations: citations.length,
    found: citations.filter((item) => item.status === 'found').length,
    candidates: citations.filter((item) => item.status === 'candidate').length,
  })
}

const total = rows.reduce(
  (acc, row) => ({
    notes: acc.notes + 1,
    claims: acc.claims + row.claims,
    usable: acc.usable + row.usable,
    withOriginal: acc.withOriginal + row.withOriginal,
    citations: acc.citations + row.citations,
  }),
  { notes: 0, claims: 0, usable: 0, withOriginal: 0, citations: 0 },
)
const result = {
  measuredAt: new Date().toISOString(),
  total,
  notesWithUsableClaims: rows.filter((row) => row.usable > 0).length,
  notesAllPending: rows.filter((row) => row.claims > 0 && row.usable === 0).length,
  note: 'Usable = kind is 史载/转述 and work is non-empty. 待查 is intentionally not citable.',
  rows,
}
const outPath = path.resolve('docs/ai-claims-audit.json')
fs.mkdirSync(path.dirname(outPath), { recursive: true })
fs.writeFileSync(outPath, JSON.stringify(result, null, 2))

console.log(JSON.stringify({ total, notesWithUsableClaims: result.notesWithUsableClaims, notesAllPending: result.notesAllPending }, null, 2))
for (const row of rows) {
  console.log(`${row.usable}/${row.claims} usable  found=${row.found} cand=${row.candidates}  ${row.title}`)
}
console.log('saved', outPath)
ws.close()
