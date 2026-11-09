// Capture the current app viewport to a PNG.
// usage: node --no-warnings tools/shot-page.mjs shots/current.png [width] [height]

import fs from 'node:fs'
import path from 'node:path'

const target = path.resolve(process.argv[2] || 'shots/current.png')
const width = Number(process.argv[3] || 0)
const height = Number(process.argv[4] || 0)
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
if (width && height) {
  await send('Emulation.setDeviceMetricsOverride', { width, height, deviceScaleFactor: 1, mobile: false })
  // Let container queries and flex wrapping settle before capturing.
  await new Promise((resolve) => setTimeout(resolve, 300))
}
const result = await send('Page.captureScreenshot', { format: 'png' })
fs.mkdirSync(path.dirname(target), { recursive: true })
fs.writeFileSync(target, Buffer.from(result.data, 'base64'))
console.log('wrote', target)
if (width && height) await send('Emulation.clearDeviceMetricsOverride')
ws.close()
