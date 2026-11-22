// Exercises the citation batch controls against the running app.
// usage: node tools/probe-cite-batch-ui.mjs "<note title substring>"
//
// Creates temporary candidate citations that point at a known source page,
// then drives the real batch button. It verifies:
//   1. 停止 prevents the next target from starting.
//   2. a tiny budget cap prevents the next target after one paid call.
//   3. failure reasons are rendered when a fetch/model call fails.
// It never touches the note body or existing citations. Costs one or two
// model calls. Requires CDP on 9222.

const titleQuery = (process.argv[2] || '').trim()
if (!titleQuery) {
  console.error('usage: node tools/probe-cite-batch-ui.mjs "<note title substring>"')
  process.exit(2)
}

const SOURCE_URL = 'https://www.shidianguji.com/zh/book/7514547545549307939/chapter/1levotpolodn3'
const QUOTES = [
  '华佗让病人饮下麻沸散后剖开腹部',
  '华佗给病人断肠湔洗后缝合腹部',
  '华佗说这个病难以根治但可以延长岁月',
]

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

async function addTemp(noteId, index, tag, url = SOURCE_URL) {
  const row = await invoke('production_note_citation_add', {
    input: {
      noteId,
      noteQuote: QUOTES[index % QUOTES.length],
      workTitle: `批量测试出处-${tag}-${Date.now()}-${index}`,
      url,
      status: 'candidate',
    },
  })
  return row.id
}

async function waitBatchEnd(maxMs) {
  const started = Date.now()
  while (Date.now() - started < maxMs) {
    await wait(300)
    const running = await evaluate(`Boolean(document.querySelector('[data-cite-batch-stop]'))`)
    if (!running) return true
  }
  return false
}

const notes = await invoke('list_notes')
const note = notes.find((item) => item.title === titleQuery) || notes.find((item) => item.title.includes(titleQuery))
if (!note) throw new Error(`note not found: ${titleQuery}`)

const tempIds = []
try {
  for (let index = 0; index < 3; index += 1) tempIds.push(await addTemp(note.id, index, 'stop'))
  await evaluate(`location.hash='#/graph'; 'ok'`)
  await wait(700)
  await evaluate(`location.hash='#/note/${note.id}'; 'ok'`)
  await wait(1000)

  const before = await evaluate(`(() => ({
    button: document.querySelector('[data-cite-batch-button]')?.innerText.trim() || '',
    budget: document.querySelector('[data-cite-batch-budget]')?.value || '',
    rows: document.querySelectorAll('.rail-cite-row').length,
  }))()`)
  if (!before.button.includes('AI 补齐候选')) throw new Error(`batch button missing: ${before.button}`)

  await evaluate(`document.querySelector('[data-cite-batch-button]').click(); true`)
  await wait(50)
  const running = await evaluate(`(() => ({
    button: document.querySelector('[data-cite-batch-button]')?.innerText.trim() || '',
    stop: document.querySelector('[data-cite-batch-stop]')?.innerText.trim() || '',
    cap: document.querySelector('.rail-cite-batch-cap')?.innerText.trim() || '',
  }))()`)
  if (!running.button.includes('AI 补齐 0/')) throw new Error(`batch did not start: ${JSON.stringify(running)}`)
  if (!running.stop) throw new Error('stop button missing while running')
  if (!running.cap) throw new Error('budget cap label missing while running')

  await evaluate(`document.querySelector('[data-cite-batch-stop]').click(); true`)
  if (!(await waitBatchEnd(90_000))) throw new Error('batch did not end after stop')
  const afterStop = await invoke('production_note_citations', { noteId: note.id })
  const stopRows = afterStop.filter((item) => tempIds.slice(0, 3).includes(item.id))
  const stopFound = stopRows.filter((item) => item.status === 'found').length
  const stopped = await evaluate(`(() => ({
    error: document.querySelector('.rail-fact-error')?.innerText.trim() || '',
    notice: document.querySelector('.rail-cite-notice')?.innerText.trim() || '',
    failures: [...document.querySelectorAll('.rail-cite-batch-failures li')].map((item) => item.innerText.trim()),
    stop: Boolean(document.querySelector('[data-cite-batch-stop]')),
  }))()`)
  const stopText = `${stopped.error} ${stopped.notice}`.trim()
  if (!stopText.includes('已停止')) throw new Error(`stop notice missing: ${stopText}`)
  if (!stopText.includes('2 条未开始')) throw new Error(`stop should leave two targets: ${stopText}`)
  if (stopFound > 1) throw new Error(`stop started too many targets: found=${stopFound}`)
  if (stopped.stop) throw new Error('stop button still present after batch ended')

  // Budget run: fresh candidates, cap below one paid call.
  for (const tempId of tempIds) {
    await invoke('production_note_citation_delete', { id: tempId }).catch(() => {})
  }
  tempIds.length = 0
  const budgetIds = []
  for (let index = 0; index < 2; index += 1) budgetIds.push(await addTemp(note.id, index, 'budget'))
  tempIds.push(...budgetIds)
  await evaluate(`location.hash='#/graph'; 'ok'`)
  await wait(700)
  await evaluate(`location.hash='#/note/${note.id}'; 'ok'`)
  await wait(1000)

  await evaluate(`(() => {
    const input = document.querySelector('[data-cite-batch-budget]')
    const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set
    setter.call(input, '0.0001')
    input.dispatchEvent(new Event('input', { bubbles: true }))
    document.querySelector('[data-cite-batch-button]').click()
    return true
  })()`)
  if (!(await waitBatchEnd(90_000))) throw new Error('budget batch did not end')
  const afterBudget = await invoke('production_note_citations', { noteId: note.id })
  const budgetRows = afterBudget.filter((item) => budgetIds.includes(item.id))
  const budgetFound = budgetRows.filter((item) => item.status === 'found').length
  const budgeted = await evaluate(`(() => ({
    error: document.querySelector('.rail-fact-error')?.innerText.trim() || '',
    notice: document.querySelector('.rail-cite-notice')?.innerText.trim() || '',
    calls: [...document.querySelectorAll('.rail-cite-batch-failures li')].map((item) => item.innerText.trim()),
  }))()`)
  const budgetText = `${budgeted.error} ${budgeted.notice}`.trim()
  if (!budgetText.includes('预算上限')) throw new Error(`budget notice missing: ${budgetText}`)
  if (!budgetText.includes('1 条未开始')) throw new Error(`budget should leave one target: ${budgetText}`)
  if (budgetFound > 1) throw new Error(`budget started too many targets: found=${budgetFound}`)

  // Failure run: a 404 URL must surface its reason in the list.
  for (const tempId of tempIds) {
    await invoke('production_note_citation_delete', { id: tempId }).catch(() => {})
  }
  tempIds.length = 0
  const failId = await addTemp(note.id, 0, 'fail', `https://example.com/cite-batch-missing-${Date.now()}`)
  tempIds.push(failId)
  await evaluate(`location.hash='#/graph'; 'ok'`)
  await wait(700)
  await evaluate(`location.hash='#/note/${note.id}'; 'ok'`)
  await wait(1000)
  await evaluate(`(() => {
    const input = document.querySelector('[data-cite-batch-budget]')
    const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set
    setter.call(input, '0.05')
    input.dispatchEvent(new Event('input', { bubbles: true }))
    document.querySelector('[data-cite-batch-button]').click()
    return true
  })()`)
  if (!(await waitBatchEnd(60_000))) throw new Error('failure batch did not end')
  const failed = await evaluate(`(() => ({
    error: document.querySelector('.rail-fact-error')?.innerText.trim() || '',
    failures: [...document.querySelectorAll('.rail-cite-batch-failures li')].map((item) => item.innerText.trim()),
  }))()`)
  if (!failed.error.includes('AI 补齐失败')) throw new Error(`failure notice missing: ${failed.error}`)
  if (!failed.failures.some((item) => item.includes('404'))) {
    throw new Error(`HTTP failure reason missing: ${JSON.stringify(failed.failures)}`)
  }

  console.log(
    JSON.stringify(
      {
        note: note.title,
        noteId: note.id,
        before,
        running,
        afterStop: { found: stopFound, ...stopped },
        afterBudget: { found: budgetFound, ...budgeted },
        afterFailure: failed,
      },
      null,
      2,
    ),
  )
} finally {
  for (const tempId of tempIds) {
    await invoke('production_note_citation_delete', { id: tempId }).catch(() => {})
  }
  if (tempIds.length) console.log('cleaned temp citations', tempIds.length)
  await evaluate(`location.hash='#/note/${note.id}'; 'ok'`).catch(() => {})
  await wait(500)
  ws.close()
}
