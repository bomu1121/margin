// Repeated GraphView build measurements in the running dev webview via CDP.
// usage: node tools/measure-force-many.mjs <vault> <runs> [recent|all] [label]
const target = process.argv[2]
const runs = Number(process.argv[3] || 3)
const mode = process.argv[4] || 'recent'
const label = process.argv[5] || mode
if (!target) {
  console.error('usage: node tools/measure-force-many.mjs <vault> <runs> [recent|all] [label]')
  process.exit(2)
}
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
let consoleLines = []
let waiter = null
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
  if (message.method !== 'Runtime.consoleAPICalled') return
  const text = message.params.args.map((a) => a.value ?? a.description ?? '').join(' ')
  consoleLines.push(text)
  if (waiter && text.includes('[force] build')) {
    const resolve = waiter
    waiter = null
    resolve(text)
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
  const result = await send('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true })
  if (result.exceptionDetails) throw new Error(result.exceptionDetails.text)
  return result.result?.value
}
function nextBuild(timeoutMs = 30000) {
  const found = [...consoleLines].reverse().find((line) => line.includes('[force] build'))
  if (found) return Promise.resolve(found)
  return new Promise((resolve, reject) => {
    waiter = resolve
    setTimeout(() => {
      if (waiter) {
        waiter = null
        reject(new Error('timeout waiting for [force] build'))
      }
    }, timeoutMs)
  })
}
function wait(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms))
}
await send('Runtime.enable')
await send('Page.enable')
console.log(JSON.stringify({ label, target, mode, setVault: await evaluate(
  `window.__TAURI_INTERNALS__.invoke('set_vault', { path: ${JSON.stringify(target)} })`,
) }))
await evaluate(`location.hash = '#/graph'`)
await wait(800)
const results = []
for (let run = 1; run <= runs; run += 1) {
  consoleLines = []
  await send('Page.reload', { ignoreCache: true })
  const recent = await nextBuild()
  if (mode === 'all') {
    await wait(800)
    consoleLines = []
    await evaluate(`document.querySelector('.force-scope').click()`)
    const all = await nextBuild()
    results.push({ run, recent, all })
  } else {
    results.push({ run, recent })
  }
  console.log(JSON.stringify(results[results.length - 1]))
}
console.log(JSON.stringify({ label, results, console: consoleLines }))
ws.close()
