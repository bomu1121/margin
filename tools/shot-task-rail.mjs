// Captures the production task dialog's right rail for visual review.
// usage: node --no-warnings tools/shot-task-rail.mjs <prefix> [width] [height] [theme] [mode]
// Requires the app on CDP 9222 with a production task dialog open.
// Writes shots/audit/<prefix>-<width>-<theme>-<mode>.png (full viewport + rail clip).

import fs from 'node:fs'
import path from 'node:path'

const prefix = process.argv[2] || 'rail'
const width = Number(process.argv[3] || 1440)
const height = Number(process.argv[4] || 900)
const theme = process.argv[5] || ''
const mode = process.argv[6] || ''
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
async function shot(file, clip) {
  const params = { format: 'png' }
  if (clip) params.clip = { ...clip, scale: 1 }
  const result = await send('Page.captureScreenshot', params)
  const target = path.resolve('shots/audit', file)
  fs.mkdirSync(path.dirname(target), { recursive: true })
  fs.writeFileSync(target, Buffer.from(result.data, 'base64'))
  console.log('wrote', target)
}

await send('Page.enable')
await send('Runtime.enable')
const restore = await evaluate(`(() => {
  const root = document.documentElement
  return JSON.stringify({ theme: root.dataset.theme, mode: root.dataset.mode })
})()`)
const before = JSON.parse(restore)
if (theme) await evaluate(`document.documentElement.dataset.theme = ${JSON.stringify(theme)}`)
if (mode) await evaluate(`document.documentElement.dataset.mode = ${JSON.stringify(mode)}`)
if (theme || mode) await sleep(350)
await send('Emulation.setDeviceMetricsOverride', { width, height, deviceScaleFactor: 1, mobile: false })
await sleep(400)

const rail = await evaluate(`(() => {
  const el = document.querySelector('.prod-task-rail')
  if (!el) return null
  el.scrollTop = 0
  const r = el.getBoundingClientRect()
  return JSON.stringify({ x: Math.floor(r.x) - 6, y: Math.floor(r.y) - 6, width: Math.ceil(r.width) + 12, height: Math.ceil(r.height) + 12 })
})()`)
if (!rail) throw new Error('no .prod-task-rail; open the task dialog first')
const clip = JSON.parse(rail)
const name = [prefix, width, theme || before.theme, mode || before.mode].join('-')
await shot(`${name}-full.png`)
await shot(`${name}-rail.png`, clip)

await send('Emulation.clearDeviceMetricsOverride')
await evaluate(`document.documentElement.dataset.theme = ${JSON.stringify(before.theme)}; document.documentElement.dataset.mode = ${JSON.stringify(before.mode)}`)
ws.close()
