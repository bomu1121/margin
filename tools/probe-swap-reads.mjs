// Checks that reads keep working while a staging index is swapped in.
// usage: node --no-warnings tools/probe-swap-reads.mjs <large-vault> <restore-vault>
// Prereq: app running with CDP on 9222.

const large = process.argv[2]
const restore = process.argv[3]
if (!large || !restore) {
  console.error('usage: node tools/probe-swap-reads.mjs <large-vault> <restore-vault>')
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

function percentile(values, p) {
  if (!values.length) return null
  const sorted = [...values].sort((a, b) => a - b)
  return sorted[Math.floor(((sorted.length - 1) * p) / 100)]
}

async function runSwitch(target, label) {
  await evaluate(`(() => {
    window.__swap = { started: performance.now(), done: false };
    window.__TAURI_INTERNALS__.invoke('set_vault', { path: ${JSON.stringify(target)} })
      .then(() => { window.__swap.done = true; window.__swap.ms = performance.now() - window.__swap.started; })
      .catch((error) => { window.__swap.done = true; window.__swap.error = String(error); });
    return true;
  })()`)

  const readLatencies = []
  const writeLatencies = []
  const errors = []
  while (true) {
    const started = Date.now()
    try {
      await evaluate(`window.__TAURI_INTERNALS__.invoke('list_notes').then((notes) => notes.length)`)
      const listMs = Date.now() - started
      const searchStart = Date.now()
      await evaluate(`window.__TAURI_INTERNALS__.invoke('search_notes', { query: '养老金' }).then((hits) => hits.length)`)
      readLatencies.push({ listMs, searchMs: Date.now() - searchStart })
    } catch (error) {
      errors.push(String(error))
    }
    const state = await evaluate(`window.__swap`)
    if (state.done) break
    await sleep(20)
  }
  const swap = await evaluate(`window.__swap`)
  return {
    label,
    swapMs: Math.round(swap.ms ?? 0),
    error: swap.error ?? null,
    reads: readLatencies.length,
    listP50Ms: percentile(readLatencies.map((r) => r.listMs), 50),
    listP95Ms: percentile(readLatencies.map((r) => r.listMs), 95),
    listMaxMs: Math.max(...readLatencies.map((r) => r.listMs), 0),
    searchP50Ms: percentile(readLatencies.map((r) => r.searchMs), 50),
    searchP95Ms: percentile(readLatencies.map((r) => r.searchMs), 95),
    searchMaxMs: Math.max(...readLatencies.map((r) => r.searchMs), 0),
    readErrors: errors.length,
    firstError: errors[0] ?? null,
  }
}

const toLarge = await runSwitch(large, 'to-large')
const toSmall = await runSwitch(restore, 'to-small')
console.log(JSON.stringify({ toLarge, toSmall }, null, 2))
ws.close()
