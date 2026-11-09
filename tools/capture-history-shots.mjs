// Captures the current pending-candidate queue and fact curation overview.
// usage: node --no-warnings tools/capture-history-shots.mjs
// Prereq: running app with remote debugging on 127.0.0.1:9222.

import fs from 'node:fs'
import path from 'node:path'

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
  const result = await send('Runtime.evaluate', {
    expression,
    awaitPromise: true,
    returnByValue: true,
  })
  if (result.exceptionDetails) throw new Error(`page error: ${JSON.stringify(result.exceptionDetails)}`)
  return result.result?.value
}

async function waitFor(expression, label, timeout = 15000) {
  const start = Date.now()
  while (Date.now() - start < timeout) {
    try {
      if (await evaluate(expression)) return true
    } catch {
      // context can briefly disappear during navigation
    }
    await sleep(120)
  }
  throw new Error(`timeout waiting for ${label}`)
}

async function shot(relativePath) {
  const result = await send('Page.captureScreenshot', { format: 'png' })
  const target = path.join(shotsDir, relativePath)
  fs.mkdirSync(path.dirname(target), { recursive: true })
  fs.writeFileSync(target, Buffer.from(result.data, 'base64'))
  console.log('wrote', target)
}

async function goto(hash, waitExpression, label) {
  await evaluate(`location.hash = ${JSON.stringify(hash)}`)
  await waitFor(waitExpression, label)
  await sleep(600)
}

await send('Page.enable')
await send('Runtime.enable')
await send('Page.reload', { ignoreCache: true })
await sleep(1500)
await waitFor(`document.body.innerText.includes('待补录候选')`, 'app reload', 20000)

async function scrollTop() {
  await evaluate(`(() => {
    window.scrollTo(0, 0)
    for (const el of document.querySelectorAll('*')) {
      if (el.scrollTop > 0) el.scrollTop = 0
    }
  })()`)
  await sleep(250)
}

await goto('#/sources/pending', `document.body.innerText.includes('待补录候选') && document.body.innerText.includes('360')`, 'pending queue')
await scrollTop()
console.log(await evaluate(`document.body.innerText.slice(0, 320)`))
await shot('queue/04-pending-queue-360.png')

const opened = await evaluate(`(() => {
  const button = [...document.querySelectorAll('button')].find((item) => item.textContent.trim() === '补录原文')
  if (!button) return false
  button.click()
  return true
})()`)
if (opened) {
  await waitFor(`!!document.querySelector('.dialog, [role="dialog"], .modal, .paste-dialog')`, 'paste dialog').catch(() => sleep(1000))
  await sleep(400)
  await shot('queue/05-paste-from-queue-360.png')
  await send('Input.dispatchKeyEvent', { type: 'keyDown', key: 'Escape', code: 'Escape', windowsVirtualKeyCode: 27 })
  await send('Input.dispatchKeyEvent', { type: 'keyUp', key: 'Escape', code: 'Escape', windowsVirtualKeyCode: 27 })
  await sleep(400)
}

await goto('#/sources', `document.body.innerText.includes('史料库') && document.body.innerText.includes('待补录')`, 'source library')
await scrollTop()
console.log(await evaluate(`document.body.innerText.slice(0, 320)`))
await shot('queue/06-source-library-pending-360.png')

await goto('#/facts/curate', `document.body.innerText.includes('整理事实') && document.body.innerText.includes('复合事实')`, 'fact curation')
await scrollTop()
console.log(await evaluate(`document.body.innerText.slice(0, 420)`))
await shot('curation/08-overview-59-facts.png')

await goto('#/facts/fact_60dca4e93c801adc7b160960', `document.body.innerText.includes('萧道成')`, 'ninth fact detail')
await scrollTop()
console.log(await evaluate(`document.body.innerText.slice(0, 420)`))
await shot('queue/07-ninth-fact-candidates.png')

ws.close()
