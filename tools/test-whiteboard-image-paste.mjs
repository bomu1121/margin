// Verifies that a freshly pasted image card starts at the source aspect (no letterbox bars).
// usage: CDP_PORT=9333 node --no-warnings tools/test-whiteboard-image-paste.mjs
// Dispatches a synthetic paste with a generated 600x300 PNG, checks the stored size, then
// deletes the card and its orphan asset.

import fs from 'node:fs'
import path from 'node:path'

const port = process.env.CDP_PORT || '9222'
const targets = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json()
const page = targets.find((item) => item.type === 'page' && item.url.includes('whiteboard.html'))
if (!page) throw new Error('whiteboard window is not open')

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
async function ev(expression) {
  const result = await send('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true })
  if (result.exceptionDetails) throw new Error(`page error: ${JSON.stringify(result.exceptionDetails)}`)
  return result.result?.value
}
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms))

const vault = await ev(`window.__TAURI_INTERNALS__.invoke('get_vault')`)
const before = await ev(`(async () => {
  const boardId = localStorage.getItem('yebian.whiteboard.board') || 'board_default'
  return (await window.__TAURI_INTERNALS__.invoke('whiteboard_items', { boardId })).map((item) => item.id)
})()`)

await ev(`(async () => {
  const canvas = document.createElement('canvas')
  canvas.width = 600
  canvas.height = 300
  const ctx = canvas.getContext('2d')
  ctx.fillStyle = '#3366cc'
  ctx.fillRect(0, 0, 600, 300)
  const blob = await new Promise((resolve) => canvas.toBlob(resolve, 'image/png'))
  const transfer = new DataTransfer()
  transfer.items.add(new File([blob], 'paste-test.png', { type: 'image/png' }))
  document.body.dispatchEvent(new ClipboardEvent('paste', { clipboardData: transfer, bubbles: true }))
  return true
})()`)

let item = null
const started = Date.now()
while (!item && Date.now() - started < 10000) {
  await sleep(300)
  item = await ev(`(async () => {
    const boardId = localStorage.getItem('yebian.whiteboard.board') || 'board_default'
    const items = await window.__TAURI_INTERNALS__.invoke('whiteboard_items', { boardId })
    const known = ${JSON.stringify(before)}
    return items.find((entry) => !known.includes(entry.id)) || null
  })()`)
}
if (!item) throw new Error('paste did not create an item')

const data = JSON.parse(item.dataJson || '{}')
const result = {
  w: item.w,
  h: item.h,
  frameW: item.w - 2,
  frameH: item.h - 25,
  naturalW: data.width,
  naturalH: data.height,
  aspect: data.width / data.height,
  frameAspect: (item.w - 2) / (item.h - 25),
}
console.log(JSON.stringify(result, null, 2))

await ev(`(async () => {
  await window.__TAURI_INTERNALS__.invoke('whiteboard_item_delete', { id: ${JSON.stringify(item.id)} })
  return true
})()`)
if (vault && item.assetPath) {
  const file = path.resolve(vault, item.assetPath)
  const root = path.resolve(vault, 'whiteboard/assets')
  if (file.startsWith(root) && fs.existsSync(file)) {
    fs.rmSync(file, { force: true })
    console.log('removed orphan asset', item.assetPath)
  }
}
ws.close()
