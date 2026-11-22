// Verifies that the UI shows DeepSeek Search cost inside the fill notice.
// usage: node tools/probe-cite-search-cost.mjs "<note title substring>"
//
// Creates one temporary no-URL candidate, clicks AI 找来源, waits for the rail
// notice, asserts "检索 ¥0.0050", then deletes the temporary citation. Costs
// one search and possibly one model call. Requires CDP on 9222.

const titleQuery = (process.argv[2] || '').trim()
if (!titleQuery) {
  console.error('usage: node tools/probe-cite-search-cost.mjs "<note title substring>"')
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
const wait = (ms) => new Promise((resolve) => setTimeout(resolve, ms))

const notes = await invoke('list_notes')
const note = notes.find((item) => item.title === titleQuery) || notes.find((item) => item.title.includes(titleQuery))
if (!note) throw new Error(`note not found: ${titleQuery}`)

const workTitle = `检索成本测试-${Date.now()}`
let tempId = ''
try {
  const created = await invoke('production_note_citation_add', {
    input: {
      noteId: note.id,
      noteQuote: '建安二十二年，大疫。',
      workTitle,
      status: 'candidate',
    },
  })
  tempId = created.id
  await evaluate(`location.hash='#/graph'; 'ok'`)
  await wait(700)
  await evaluate(`location.hash='#/note/${note.id}'; 'ok'`)
  await wait(900)

  const clicked = await evaluate(`(() => {
    const rows = [...document.querySelectorAll('.rail-cite-row')]
    const row = rows.find((item) => item.innerText.includes(${JSON.stringify(workTitle)}))
    if (!row) return 'row missing'
    const button = [...row.querySelectorAll('.rail-cite-actions button')].find((item) => item.innerText.trim().includes('AI 找来源'))
    if (!button) return 'button missing'
    button.click()
    return 'clicked'
  })()`)
  if (clicked !== 'clicked') throw new Error(clicked)

  let notice = ''
  for (let attempt = 0; attempt < 60; attempt += 1) {
    await wait(500)
    const state = await evaluate(`(() => ({
      notice: document.querySelector('.rail-cite-notice')?.innerText.trim() || '',
      error: document.querySelector('.rail-fact-error')?.innerText.trim() || '',
    }))()`)
    if (state.notice || state.error) {
      notice = state.notice || state.error
      break
    }
  }
  if (!notice.includes('检索 ¥0.0050')) throw new Error(`search cost missing from notice: ${notice}`)
  console.log(JSON.stringify({ note: note.title, notice }, null, 2))
} finally {
  if (tempId) {
    await invoke('production_note_citation_delete', { id: tempId }).catch(() => {})
    console.log('cleaned temp citation', tempId)
  }
  await evaluate(`location.hash='#/note/${note.id}'; 'ok'`).catch(() => {})
  await wait(500)
  ws.close()
}
