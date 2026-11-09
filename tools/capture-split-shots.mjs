// Captures split-dialog / split-result / undo-result on a real composite fact.
// usage: node --no-warnings tools/capture-split-shots.mjs
// The script ends with the split undone; the app data is left as it was.

import fs from 'node:fs'
import path from 'node:path'

const factId = process.argv[2] || 'fact_45555c8f1c2c0dfa82e4c062'
const shotsDir = path.resolve('shots')
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
async function waitFor(expression, label, timeout = 15000) {
  const start = Date.now()
  while (Date.now() - start < timeout) {
    try {
      if (await evaluate(expression)) return
    } catch {
      // page context can disappear during navigation
    }
    await sleep(120)
  }
  throw new Error(`timeout waiting for ${label}`)
}
async function shot(name) {
  const result = await send('Page.captureScreenshot', { format: 'png' })
  const target = path.join(shotsDir, 'split', name)
  fs.mkdirSync(path.dirname(target), { recursive: true })
  fs.writeFileSync(target, Buffer.from(result.data, 'base64'))
  console.log('wrote', target)
}
async function scrollTop() {
  await evaluate(`(() => {
    window.scrollTo(0, 0)
    for (const el of document.querySelectorAll('*')) if (el.scrollTop > 0) el.scrollTop = 0
  })()`)
  await sleep(250)
}

await send('Page.enable')
await send('Runtime.enable')
await evaluate(`location.hash = '#/facts/${factId}'`)
await waitFor(`document.body.innerText.includes('小布什的父亲')`, 'fact detail')
await sleep(500)
await scrollTop()
const opened = await evaluate(`(() => {
  const button = [...document.querySelectorAll('.fact-detail-actions button')].find((item) => item.textContent.includes('拆分'))
  if (!button) return false
  button.click()
  return true
})()`)
if (!opened) throw new Error('split button not found')
await waitFor(`!!document.querySelector('.fact-split-dialog')`, 'split dialog')
await sleep(400)
console.log('dialog rows:', await evaluate(`[...document.querySelectorAll('.fact-split-input')].map((item) => item.value)`))
await shot('01-dialog.png')

const submitted = await evaluate(`(() => {
  const button = [...document.querySelectorAll('.fact-split-dialog .prod-dialog-foot button')].find((item) => item.textContent.includes('确认拆分'))
  if (!button) return false
  button.click()
  return true
})()`)
if (!submitted) throw new Error('split submit button not found')
await waitFor(`!!document.querySelector('.fact-split-children')`, 'split result')
await sleep(500)
await scrollTop()
console.log('children:', await evaluate(`[...document.querySelectorAll('.fact-split-child b')].map((item) => item.textContent)`))
await shot('02-result.png')

await evaluate(`window.confirm = () => true`)
const undone = await evaluate(`(() => {
  const button = [...document.querySelectorAll('button')].find((item) => item.textContent.includes('撤销拆分'))
  if (!button) return false
  button.click()
  return true
})()`)
if (!undone) throw new Error('undo button not found')
await waitFor(`!document.querySelector('.fact-split-children')`, 'split undone')
await sleep(500)
await scrollTop()
await shot('03-undone.png')
console.log('status:', await evaluate(`document.querySelector('.fact-detail-title-row .fact-status-pill')?.textContent`))
ws.close()
