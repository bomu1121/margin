// Exercise the quick-production button against the dev webview, then cancel and
// delete the job immediately. Requires CDP on 127.0.0.1:9222.
import fs from 'node:fs'

const port = process.env.CDP_PORT || '9222'
const list = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json()
const page = list.find((item) => item.type === 'page' && (!process.env.CDP_TARGET || item.url.includes(process.env.CDP_TARGET)))
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

const expression = `(async () => {
  const invoke = (command, args = {}) => window.__TAURI_INTERNALS__.invoke(command, args)
  const before = await invoke('production_list')
  const beforeIds = new Set(before.map((job) => job.id))
  const button = document.querySelector('.prod-quick-start')
  if (!button) return { error: 'quick start button missing' }
  if (button.disabled) return { error: 'quick start button disabled' }
  const url = document.querySelector('.prod-quick-field input')?.value || ''
  const started = performance.now()
  button.click()
  let job = null
  for (let attempt = 0; attempt < 240; attempt += 1) {
    await new Promise((resolve) => setTimeout(resolve, 25))
    const jobs = await invoke('production_list')
    job = jobs.find((item) => !beforeIds.has(item.id))
    if (job) break
  }
  const elapsedMs = Math.round(performance.now() - started)
  if (!job) return { error: 'no new job within 6s', url }
  let detail = null
  let cancelError = ''
  try {
    await invoke('production_cancel', { id: job.id })
  } catch (error) {
    cancelError = String(error)
  }
  try {
    detail = await invoke('production_get', { id: job.id })
  } catch {}
  let deleteError = ''
  try {
    await invoke('production_delete', { id: job.id })
  } catch (error) {
    deleteError = String(error)
  }
  return {
    elapsedMs,
    effectiveUrl: url,
    cancelError,
    deleteError,
    job: {
      id: job.id,
      title: job.title,
      templateId: job.templateId,
      status: job.status,
      progress: job.progress,
      sourceKind: job.source.kind,
      sourceUrl: job.source.url,
      pages: job.source.pages,
      destination: job.options.destination,
      folder: job.options.folder,
      prompt: job.options.prompt,
      noteTaskChars: job.options.noteTask.length,
      tags: job.options.tags,
      fields: job.options.fields,
      traceProvider: job.options.traceProvider,
      customConflictAction: job.options.conflictAction || '',
    },
    usage: job.usage,
    steps: detail?.steps?.map((step) => ({ key: step.key, status: step.status, detail: step.detail })) || [],
  }
})()`

const result = await send('Runtime.evaluate', {
  expression,
  awaitPromise: true,
  returnByValue: true,
})
const value = result.result?.value ?? result
fs.mkdirSync('docs', { recursive: true })
fs.writeFileSync('docs/p247-quick-start-probe.json', `${JSON.stringify(value, null, 2)}\n`)
console.log(JSON.stringify(value, null, 2))
ws.close()
