// Preview or backfill saved Jiangshan map contexts for historical notes.
//
// Default is a read-only candidate scan: it lists historical notes that do not
// already have 地图* front matter, without calling the model.
// `--preview` calls the model and prints candidates; `--apply` writes only when
// the model returns both a year and a place at/above --min-confidence.
//
// usage:
//   CDP_PORT=9333 node tools/map-backfill.mjs
//   CDP_PORT=9333 node tools/map-backfill.mjs --preview --limit=5
//   CDP_PORT=9333 node tools/map-backfill.mjs --apply --limit=5 --min-confidence=0.8
//   CDP_PORT=9333 node tools/map-backfill.mjs --apply --only=<noteId>

const args = process.argv.slice(2)
const apply = args.includes('--apply')
const preview = args.includes('--preview')
const limitArg = args.find((arg) => arg.startsWith('--limit='))
const minArg = args.find((arg) => arg.startsWith('--min-confidence='))
const onlyArg = args.find((arg) => arg.startsWith('--only='))
const limit = limitArg ? Math.max(0, Number(limitArg.split('=')[1]) || 0) : 20
const minConfidence = minArg ? Math.max(0, Math.min(1, Number(minArg.split('=')[1]) || 0)) : 0.7
const only = onlyArg ? onlyArg.slice('--only='.length) : ''
const port = process.env.CDP_PORT || '9222'

if (apply && preview) {
  console.error('--apply and --preview are mutually exclusive')
  process.exit(2)
}

const runner = async (options) => {
  const invoke = window.__TAURI_INTERNALS__.invoke
  const notes = await invoke('list_notes')
  const result = {
    mode: options.apply ? 'apply' : options.preview ? 'preview' : 'scan',
    candidates: 0,
    checked: 0,
    applied: 0,
    skipped: 0,
    failed: 0,
    promptTokens: 0,
    completionTokens: 0,
    results: [],
  }

  for (const note of notes) {
    if (options.only && note.id !== options.only) continue
    const historical =
      (note.tags || []).includes('领域/历史') || (note.periods || []).length > 0
    if (!historical) continue
    let existing = null
    try {
      existing = await invoke('map_context', { noteId: note.id, cachedOnly: true })
    } catch {
      // No saved context and no cached AI result.
    }
    if (existing && existing.source === 'note') continue
    result.candidates += 1
    if (!options.run) {
      result.results.push({ status: 'candidate', noteId: note.id, title: note.title })
      continue
    }
    if (options.limit > 0 && result.checked >= options.limit) continue
    result.checked += 1
    try {
      const context = await invoke('map_context', {
        noteId: note.id,
        cachedOnly: false,
      })
      result.promptTokens += context.promptTokens || 0
      result.completionTokens += context.completionTokens || 0
      const eligible =
        Boolean(context.year) &&
        Boolean(context.place) &&
        context.confidence >= options.minConfidence
      if (!eligible) {
        result.skipped += 1
        result.results.push({
          status: 'skip',
          noteId: note.id,
          title: note.title,
          year: context.year,
          place: context.place,
          confidence: context.confidence,
          reason: context.reason,
          why: !context.year
            ? 'no-year'
            : !context.place
              ? 'no-place'
              : 'low-confidence',
        })
        continue
      }
      if (options.apply) {
        await invoke('map_context_save', {
          noteId: note.id,
          context: {
            year: context.year,
            yearText: context.yearText || String(context.year ?? ''),
            place: context.place,
            person: context.person,
            event: context.event,
            source: 'ai',
            confidence: context.confidence,
            reason: context.reason,
          },
        })
        result.applied += 1
      }
      result.results.push({
        status: options.apply ? 'applied' : 'preview',
        noteId: note.id,
        title: note.title,
        year: context.year,
        yearText: context.yearText || String(context.year ?? ''),
        place: context.place,
        person: context.person,
        event: context.event,
        confidence: context.confidence,
        reason: context.reason,
        cached: context.cached,
        promptTokens: context.promptTokens,
        completionTokens: context.completionTokens,
      })
    } catch (error) {
      result.failed += 1
      result.results.push({
        status: 'failed',
        noteId: note.id,
        title: note.title,
        error: String(error?.message || error),
      })
    }
  }
  return result
}

const targets = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json()
const page = targets.find(
  (item) => item.type === 'page' && (!process.env.CDP_TARGET || item.url.includes(process.env.CDP_TARGET)),
)
if (!page) throw new Error('no CDP page target')

const ws = new WebSocket(page.webSocketDebuggerUrl)
await new Promise((resolve, reject) => {
  ws.addEventListener('open', resolve, { once: true })
  ws.addEventListener('error', reject, { once: true })
})
let nextId = 0
const pending = new Map()
ws.addEventListener('message', (event) => {
  const message = JSON.parse(String(event.data))
  const entry = pending.get(message.id)
  if (!entry) return
  pending.delete(message.id)
  entry(message)
})
function send(method, params = {}) {
  const id = ++nextId
  ws.send(JSON.stringify({ id, method, params }))
  return new Promise((resolve, reject) => {
    pending.set(id, (message) => {
      if (message.error) reject(new Error(JSON.stringify(message.error)))
      else resolve(message.result)
    })
  })
}

const options = {
  apply,
  preview,
  run: apply || preview,
  limit,
  minConfidence,
  only,
}
const expression = `(${runner.toString()})(${JSON.stringify(options)})`
const response = await send('Runtime.evaluate', {
  expression,
  awaitPromise: true,
  returnByValue: true,
})
ws.close()
const report = response.result?.value
if (!report) {
  console.error(JSON.stringify(response, null, 2))
  process.exit(1)
}
console.log(JSON.stringify(report, null, 2))
if (report.mode === 'scan' && report.candidates) {
  console.error(
    `\n${report.candidates} historical note(s) without saved map context. ` +
      'Use --preview to call the model without writing, or --apply to write high-confidence results.',
  )
}
