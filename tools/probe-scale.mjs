// Measures a vault switch at scale without flooding the app with full
// list_notes calls (each one serializes every note). Progress is polled through
// the small get_index_status command; full list/search calls happen only after
// the switch, when the UI would actually issue them.
//
// usage: node --no-warnings tools/probe-scale.mjs <large-vault> <restore-vault>
// Prereq: app running with CDP on 9222.

const large = process.argv[2]
const restore = process.argv[3]
if (!large || !restore) {
  console.error('usage: node tools/probe-scale.mjs <large-vault> <restore-vault>')
  process.exit(2)
}

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms))
const targets = await (await fetch('http://127.0.0.1:9222/json/list')).json()
const page = targets.find((item) => item.type === 'page')
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

async function evaluate(expression) {
  const result = await send('Runtime.evaluate', {
    expression,
    awaitPromise: true,
    returnByValue: true,
  })
  if (result.exceptionDetails) throw new Error(`page error: ${JSON.stringify(result.exceptionDetails)}`)
  return result.result?.value
}

const invoke = (name, args = {}) =>
  evaluate(`window.__TAURI_INTERNALS__.invoke(${JSON.stringify(name)}, ${JSON.stringify(args)})`)

const started = Date.now()
await evaluate(`(() => {
  window.__scaleDone = false;
  window.__scaleError = null;
  window.__TAURI_INTERNALS__.invoke('set_vault', { path: ${JSON.stringify(large)} })
    .then(() => { window.__scaleDone = true; })
    .catch((error) => { window.__scaleDone = true; window.__scaleError = String(error); });
  return true;
})()`)

const progress = []
while (true) {
  await sleep(400)
  const status = await invoke('get_index_status')
  const sample = { atMs: Date.now() - started, state: status.state, done: status.done, total: status.total }
  const last = progress.at(-1)
  if (!last || last.state !== sample.state || last.done !== sample.done) progress.push(sample)
  if (await evaluate('window.__scaleDone === true')) break
  if (Date.now() - started > 300000) throw new Error('set_vault did not finish in 300s')
}
const switchMs = Date.now() - started
const error = await evaluate('window.__scaleError')
if (error) throw new Error(error)

await sleep(500)
const list = await evaluate(`(async () => {
  const started = performance.now();
  const notes = await window.__TAURI_INTERNALS__.invoke('list_notes');
  return { ms: performance.now() - started, count: notes.length };
})()`)

const searches = []
for (let i = 0; i < 5; i += 1) {
  searches.push(
    await evaluate(`(async () => {
      const started = performance.now();
      const hits = await window.__TAURI_INTERNALS__.invoke('search_notes', { query: '养老金' });
      return { ms: performance.now() - started, count: hits.length };
    })()`),
  )
}

const dom = await evaluate(`(() => {
  const scroll = document.querySelector('.tree-scroll');
  const body = document.querySelector('.tree-body');
  const rows = [...document.querySelectorAll('.tree-body .tree-row')];
  return {
    renderedRows: rows.length,
    treeitems: document.querySelectorAll('.tree-body [role=treeitem]').length,
    rowHeight: rows[0]?.getBoundingClientRect().height ?? null,
    scrollHeight: scroll?.scrollHeight ?? null,
    clientHeight: scroll?.clientHeight ?? null,
    bodyHeight: body ? Math.round(body.getBoundingClientRect().height) : null,
  };
})()`)

const restoreStarted = Date.now()
await invoke('set_vault', { path: restore })
const restoreMs = Date.now() - restoreStarted

const searchTimes = searches.map((item) => Math.round(item.ms))
console.log(
  JSON.stringify(
    {
      large,
      restore,
      switchMs,
      progress,
      list: { ms: Math.round(list.ms), count: list.count },
      searches: { count: searches[0]?.count ?? null, timesMs: searchTimes },
      dom,
      restoreMs,
    },
    null,
    2,
  ),
)
ws.close()
