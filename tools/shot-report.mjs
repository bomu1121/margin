// Screenshot the audit report inside the running WebView2 via CDP.
// usage: node --no-warnings tools/shot-report.mjs <file-url> <out-prefix>

import fs from 'node:fs'
import path from 'node:path'

const url = process.argv[2]
const prefix = process.argv[3] || 'shots/audit/report'
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
  const result = await send('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true })
  if (result.exceptionDetails) throw new Error(JSON.stringify(result.exceptionDetails))
  return result.result?.value
}
async function shot(name) {
  const result = await send('Page.captureScreenshot', { format: 'png' })
  const target = path.resolve(`${prefix}-${name}.png`)
  fs.mkdirSync(path.dirname(target), { recursive: true })
  fs.writeFileSync(target, Buffer.from(result.data, 'base64'))
  console.log('wrote', target)
}

await send('Page.enable')
await send('Runtime.enable')
await send('Page.navigate', { url })
await sleep(2500)
console.log(await evaluate(`({ title: document.title, text: document.body.innerText.slice(0, 200), height: document.documentElement.scrollHeight })`))
for (let index = 0; index < 7; index += 1) {
  await evaluate(`window.scrollTo(0, ${index * 900})`)
  await sleep(350)
  await shot(String(index + 1).padStart(2, '0'))
}
ws.close()
