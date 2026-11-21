// Exercises the citation rail actions against the running app.
// usage: node tools/probe-cite-ui.mjs "<note title substring>" [quote]
//
// Creates one temporary citation (work title 测试出处), clicks 改原文 and 删除
// through the real Vue UI, then removes the temporary row. It never touches
// the note body or existing citations. Requires CDP on 9222.

const titleQuery = (process.argv[2] || '').trim()
const quoteArg = (process.argv[3] || '').trim()
if (!titleQuery) {
  console.error('usage: node tools/probe-cite-ui.mjs "<note title substring>" [quote]')
  process.exit(2)
}

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
const note = notes.find((item) => item.title === titleQuery) || notes.find((item) => item.title.includes(titleQuery))
if (!note) throw new Error(`note not found: ${titleQuery}`)
const detail = await invoke('get_note', { id: note.id })
if (!detail?.body?.trim()) throw new Error(`note body is empty: ${note.title}`)
const quote =
  quoteArg ||
  detail.body
    .split(/\r?\n/)
    .map((line) => line.trim())
    .find((line) => line.length >= 8 && !line.startsWith('#'))
if (!quote) throw new Error('no quote found in note; pass one as the second argument')
if (!detail.body.split(/\s+/).join('').includes(quote.split(/\s+/).join(''))) {
  throw new Error(`quote is not verbatim in note: ${quote}`)
}

const workTitle = `测试出处-${Date.now()}`
let tempId = ''
try {
  const created = await invoke('production_note_citation_add', {
    input: {
      noteId: note.id,
      noteQuote: quote,
      workTitle,
      url: `https://example.com/cite-ui-${Date.now()}`,
      originalText: '测试原文',
      status: 'found',
    },
  })
  tempId = created.id

  // Leaving and re-entering the note remounts NoteView and reloads citations.
  await evaluate(`location.hash='#/graph'; 'ok'`)
  await new Promise((resolve) => setTimeout(resolve, 700))
  await evaluate(`location.hash='#/note/${note.id}'; 'ok'`)
  await new Promise((resolve) => setTimeout(resolve, 900))

  const before = await invoke('production_note_citations', { noteId: note.id })
  const rowInfo = await evaluate(`(() => {
    const rows = [...document.querySelectorAll('.rail-cite-row')]
    const row = rows.find((item) => item.innerText.includes(${JSON.stringify(workTitle)}))
    if (!row) return { error: 'temp row not found', rows: rows.length }
    return {
      rows: rows.length,
      actions: [...row.querySelectorAll('.rail-cite-actions button')].map((button) => button.innerText.trim()),
    }
  })()`)
  if (rowInfo.error) throw new Error(rowInfo.error)

  await evaluate(`(() => {
    const rows = [...document.querySelectorAll('.rail-cite-row')]
    const row = rows.find((item) => item.innerText.includes(${JSON.stringify(workTitle)}))
    const button = [...row.querySelectorAll('.rail-cite-actions button')].find((item) => item.innerText.trim() === '改原文')
    if (!button) throw new Error('改原文 button not found')
    button.click()
    return true
  })()`)
  await new Promise((resolve) => setTimeout(resolve, 250))
  await evaluate(`(() => {
    const rows = [...document.querySelectorAll('.rail-cite-row')]
    const row = rows.find((item) => item.innerText.includes(${JSON.stringify(workTitle)}))
    const textarea = row.querySelector('.rail-cite-edit textarea')
    if (!textarea) throw new Error('edit textarea not found')
    const setter = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value').set
    setter.call(textarea, '测试原文二')
    textarea.dispatchEvent(new Event('input', { bubbles: true }))
    const save = [...row.querySelectorAll('.rail-cite-save')].find((item) => item.innerText.trim() === '保存原文')
    if (!save) throw new Error('保存原文 button not found')
    save.click()
    return true
  })()`)
  await new Promise((resolve) => setTimeout(resolve, 700))
  const afterEdit = await invoke('production_note_citations', { noteId: note.id })
  const edited = afterEdit.find((item) => item.id === tempId)
  if (edited?.originalText !== '测试原文二') throw new Error(`edit did not persist: ${edited?.originalText}`)

  await evaluate(`window.__citeProbeConfirm = window.confirm; window.confirm = () => true; true`)
  await evaluate(`(() => {
    const rows = [...document.querySelectorAll('.rail-cite-row')]
    const row = rows.find((item) => item.innerText.includes(${JSON.stringify(workTitle)}))
    const button = [...row.querySelectorAll('.rail-cite-actions button')].find((item) => item.innerText.trim() === '删除')
    if (!button) throw new Error('删除 button not found')
    button.click()
    return true
  })()`)
  await new Promise((resolve) => setTimeout(resolve, 700))
  const afterDelete = await invoke('production_note_citations', { noteId: note.id })
  const deleted = !afterDelete.some((item) => item.id === tempId)
  if (!deleted) throw new Error('temp citation still present after 删除')

  console.log(
    JSON.stringify(
      {
        note: note.title,
        noteId: note.id,
        quote,
        actions: rowInfo.actions,
        countBefore: before.length,
        countAfter: afterDelete.length,
        editedText: edited.originalText,
        deleted,
      },
      null,
      2,
    ),
  )
  tempId = ''
} finally {
  await evaluate(`if (window.__citeProbeConfirm) { window.confirm = window.__citeProbeConfirm; delete window.__citeProbeConfirm; } true`).catch(() => {})
  if (tempId) {
    await invoke('production_note_citation_delete', { id: tempId }).catch(() => {})
    console.log('cleaned temp citation', tempId)
  }
  await evaluate(`location.hash='#/note/${note.id}'; 'ok'`).catch(() => {})
  await new Promise((resolve) => setTimeout(resolve, 500))
  ws.close()
}
