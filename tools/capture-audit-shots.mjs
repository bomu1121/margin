// Captures the new fact/source/split pages for the UI audit.
// usage: node --no-warnings tools/capture-audit-shots.mjs

import fs from 'node:fs'
import path from 'node:path'

const shotsDir = path.resolve('shots/audit')
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
      // context can disappear during navigation
    }
    await sleep(120)
  }
  throw new Error(`timeout waiting for ${label}`)
}
async function viewport(width, height) {
  await send('Emulation.setDeviceMetricsOverride', { width, height, deviceScaleFactor: 1, mobile: false })
  await sleep(250)
}
async function shot(name) {
  const result = await send('Page.captureScreenshot', { format: 'png' })
  const target = path.join(shotsDir, name)
  fs.mkdirSync(path.dirname(target), { recursive: true })
  fs.writeFileSync(target, Buffer.from(result.data, 'base64'))
  console.log('wrote', target)
}
async function top() {
  await evaluate(`(() => {
    window.scrollTo(0, 0)
    for (const el of document.querySelectorAll('*')) if (el.scrollTop > 0) el.scrollTop = 0
  })()`)
  await sleep(250)
}
async function goto(hash, waitText, label) {
  await evaluate(`location.hash = ${JSON.stringify(hash)}`)
  await waitFor(`document.body.innerText.includes(${JSON.stringify(waitText)})`, label)
  await sleep(700)
  await top()
}

await send('Page.enable')
await send('Runtime.enable')
await viewport(1440, 900)

await goto('#/facts', '事实库', 'facts')
await shot('01-facts-1440.png')
await viewport(1040, 760)
await sleep(500)
await top()
await shot('02-facts-1040.png')
await viewport(1440, 900)

await goto('#/facts/curate', '事实整理', 'curation')
await shot('03-curation-1440.png')
await evaluate(`(() => { const h = [...document.querySelectorAll('.curation-section-head h2')].find((x) => x.textContent.includes('复合事实')); h?.scrollIntoView({ block: 'start' }); return !!h })()`)
await sleep(400)
await shot('04-curation-composite-1440.png')
await viewport(1040, 760)
await sleep(400)
await top()
await shot('05-curation-1040.png')
await viewport(1440, 900)

await goto('#/sources', '史料库', 'sources')
await shot('06-sources-1440.png')

await goto('#/sources/src_b66505a8fdcb4d788819fcb475f2626d', '明史', 'source detail')
await shot('07-source-detail-1440.png')

await goto('#/sources/pending', '待补录候选', 'queue')
await shot('08-queue-1440.png')
await evaluate(`(() => { const cards = document.querySelectorAll('.source-queue-fact, .source-queue-group, details, article'); return cards.length })()`)
await viewport(1040, 760)
await sleep(400)
await top()
await shot('09-queue-1040.png')
await viewport(1440, 900)

await goto('#/facts/fact_cc798cf03ac6efb10e3e7ef2', '严嵩', 'history fact')
await shot('10-history-fact-1440.png')
await evaluate(`(() => { const h = [...document.querySelectorAll(':scope h2, h2')].find((x) => x.textContent.trim() === '史料'); h?.scrollIntoView({ block: 'start' }); return !!h })()`)
await sleep(500)
await shot('11-history-sources-1440.png')

await goto('#/facts/fact_45555c8f1c2c0dfa82e4c062', '小布什', 'general fact')
await shot('12-general-fact-1440.png')
await viewport(1040, 760)
await sleep(400)
await top()
await shot('13-general-fact-1040.png')
await viewport(1440, 900)

await send('Emulation.clearDeviceMetricsOverride')
ws.close()
