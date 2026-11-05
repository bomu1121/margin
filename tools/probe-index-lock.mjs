// Measures whether a second DB command can make progress while set_vault
// indexes a large vault. This is the experiment behind the "background first
// index is not enough" claim: all Store methods share one Mutex<Connection>.
//
// usage: node --no-warnings tools/probe-index-lock.mjs <large-vault> <restore-vault>
// Prereq: release/dev app running with CDP on 9222.

const large = process.argv[2]
const restore = process.argv[3]
if (!large || !restore) {
  console.error('usage: node tools/probe-index-lock.mjs <large-vault> <restore-vault>')
  process.exit(2)
}

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms))
const list = await (await fetch('http://127.0.0.1:9222/json/list')).json()
const page = list.find((item) => item.type === 'page')
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

await evaluate(`(() => {
  window.__probe = { start: performance.now() };
  window.__TAURI_INTERNALS__.invoke('set_vault', { path: ${JSON.stringify(large)} })
    .then((stats) => {
      window.__probe.setMs = performance.now() - window.__probe.start;
      window.__probe.setStats = stats;
    })
    .catch((error) => { window.__probe.setError = String(error); });
  window.__TAURI_INTERNALS__.invoke('list_notes')
    .then((notes) => {
      window.__probe.listMs = performance.now() - window.__probe.start;
      window.__probe.noteCount = notes.length;
    })
    .catch((error) => { window.__probe.listError = String(error); });
  return true;
})()`)

let probe = null
const start = Date.now()
while (Date.now() - start < 40000) {
  probe = await evaluate(`window.__probe`)
  if ((probe?.setMs || probe?.setError) && (probe?.listMs || probe?.listError)) break
  await sleep(250)
}

const restoreStart = Date.now()
const restored = await evaluate(
  `window.__TAURI_INTERNALS__.invoke('set_vault', { path: ${JSON.stringify(restore)} })`,
)
const restoreMs = Date.now() - restoreStart
const noteCount = await evaluate(`window.__TAURI_INTERNALS__.invoke('list_notes').then((notes) => notes.length)`)

console.log(JSON.stringify({ probe, restoreMs, restored, noteCount }, null, 2))
ws.close()
