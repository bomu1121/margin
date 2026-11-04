// Measures GraphView build cost in the running dev webview via CDP.
// usage: node tools/measure-force.mjs <vault-absolute-path> [label]
const target = process.argv[2]
const label = process.argv[3] || 'run'
if (!target) {
  console.error('usage: node tools/measure-force.mjs <vault-absolute-path> [label]')
  process.exit(2)
}

const list = await (await fetch('http://127.0.0.1:9222/json/list')).json()
const page = list.find((item) => item.type === 'page')
if (!page) throw new Error('no CDP page target')

const ws = new WebSocket(page.webSocketDebuggerUrl)
await new Promise((resolve, reject) => {
  if (ws.readyState === 1) return resolve()
  ws.addEventListener('open', resolve, { once: true })
  ws.addEventListener('error', reject, { once: true })
})

let nextId = 0
const pending = new Map()
const consoleLines = []
const waiters = []

ws.addEventListener('message', (event) => {
  const message = JSON.parse(String(event.data))
  if (message.id) {
    const entry = pending.get(message.id)
    if (entry) {
      pending.delete(message.id)
      entry(message)
    }
    return
  }
  if (message.method === 'Runtime.consoleAPICalled') {
    const text = message.params.args
      .map((a) => a.value ?? a.description ?? '')
      .join(' ')
    consoleLines.push({ type: message.params.type, text })
    for (const waiter of [...waiters]) {
      if (waiter.match(text)) {
        waiters.splice(waiters.indexOf(waiter), 1)
        waiter.resolve(text)
      }
    }
  }
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
  if (result.exceptionDetails) {
    throw new Error(result.exceptionDetails.text + ' ' + (result.result?.description || ''))
  }
  return result.result?.value
}

function waitFor(match, timeoutMs) {
  return new Promise((resolve, reject) => {
    const found = consoleLines.find((line) => match(line.text))
    if (found) return resolve(found.text)
    const waiter = { match, resolve }
    waiters.push(waiter)
    setTimeout(() => {
      const index = waiters.indexOf(waiter)
      if (index >= 0) waiters.splice(index, 1)
      reject(new Error('timeout waiting for console log'))
    }, timeoutMs)
  })
}

await send('Runtime.enable')
await send('Page.enable')

console.log(JSON.stringify({ label, url: page.url, target }))

const invokeResult = await evaluate(
  `window.__TAURI_INTERNALS__.invoke('set_vault', { path: ${JSON.stringify(target)} })`,
)
console.log(JSON.stringify({ label, setVault: invokeResult }))

consoleLines.length = 0
await send('Page.reload', { ignoreCache: true })
const line = await waitFor((text) => text.includes('[force] build'), 30000)
await new Promise((resolve) => setTimeout(resolve, 500))
console.log(JSON.stringify({ label, build: line, console: consoleLines.map((l) => l.text) }))

ws.close()
