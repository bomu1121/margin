// CDP acceptance for the ScribeFlow studio + note provenance.
// usage: node --no-warnings tools/verify-studio.mjs [fixture] [restoreVault]
// Prereq: app running with CDP on 9222.
import { execFileSync } from 'node:child_process'
import fs from 'node:fs'
import path from 'node:path'

const fixture = process.argv[2] || 'D:/Develop/yebian-studio-fixture'
const restore = process.argv[3] || 'D:/Develop/yebian/sample-vault'
const resolvedFixture = path.resolve(fixture)
if (!path.basename(resolvedFixture).includes('fixture')) {
  console.error('refusing to delete a non-fixture directory; name it *fixture*')
  process.exit(2)
}
fs.rmSync(resolvedFixture, { recursive: true, force: true })
execFileSync(process.execPath, [path.resolve('tools/make-studio-fixture.mjs'), resolvedFixture], { stdio: 'inherit' })

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
  const result = await send('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true })
  if (result.exceptionDetails) throw new Error(`page error: ${JSON.stringify(result.exceptionDetails)}`)
  return result.result?.value
}
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms))
const checks = []
function check(name, ok, detail = '') {
  checks.push({ name, ok, detail })
  console.log(`${ok ? 'PASS' : 'FAIL'}  ${name}${detail ? `  ${detail}` : ''}`)
}

const previous = await evaluate(`window.__TAURI_INTERNALS__.invoke('get_vault')`)
await evaluate(`window.__TAURI_INTERNALS__.invoke('set_vault', { path: ${JSON.stringify(resolvedFixture)} })`)
await evaluate(`location.hash = '#/studio'`)
await sleep(1600)

const side = await evaluate(`(() => ({
  side: !!document.querySelector('.studio-side'),
  recent: [...document.querySelectorAll('.studio-recent-item')].map((el) => el.textContent.trim()),
  title: document.querySelector('.studio-title')?.textContent?.trim() || '',
}))()`)
check('studio-sidebar', side.side, JSON.stringify(side.recent))
check('studio-artifacts', side.recent.length >= 3, String(side.recent.length))
check('studio-title', side.title.includes('中国社保'), side.title)

await evaluate(`(() => {
  const buttons = [...document.querySelectorAll('.studio-recent-item')];
  buttons.find((el) => el.textContent.includes('中国社保'))?.click();
  return true;
})()`)
await sleep(700)
const detail = await evaluate(`(() => ({
  title: document.querySelector('.studio-title')?.textContent?.trim() || '',
  run: document.body.textContent.includes('run_128'),
  prompt: document.body.textContent.includes('观点提炼 · 成文版'),
  model: document.body.textContent.includes('DeepSeek V3.2'),
  cost: document.body.textContent.includes('0.42'),
  preview: document.body.textContent.includes('人社部“用数据找人”'),
}))()`)
check('artifact-detail', detail.title.includes('中国社保') && detail.run && detail.prompt && detail.model && detail.cost, JSON.stringify(detail))
check('artifact-preview', detail.preview)

await evaluate(`document.querySelector('.studio-title-actions .studio-btn-primary')?.click()`)
await sleep(800)
const note = await evaluate(`(() => ({
  hash: location.hash,
  prov: document.querySelector('.prov-strip')?.textContent?.trim() || '',
  rail: document.querySelector('.prov-rail')?.textContent?.trim() || '',
}))()`)
check('open-note', note.hash.startsWith('#/note/'), note.hash)
check('note-provenance', note.prov.includes('ScribeFlow 生成') && note.prov.includes('run_128'), note.prov)
check('note-source-rail', note.rail.includes('来源与制作'), note.rail.slice(0, 80))

const pendingItems = await evaluate(`(async () => {
  location.hash = '#/studio';
  await new Promise((r) => setTimeout(r, 350));
  return [...document.querySelectorAll('.studio-recent-item')].map((el) => el.textContent.trim());
})()`)
check('pending-artifact', pendingItems.some((item) => item.includes('等待入库')), JSON.stringify(pendingItems))

await evaluate(`window.__TAURI_INTERNALS__.invoke('set_vault', { path: ${JSON.stringify(previous)} })`)
console.log(`vault restored to ${previous}`)
console.log(`\n${checks.filter((item) => item.ok).length}/${checks.length} checks passed`)
console.log(JSON.stringify({ fixture, restore, results: checks }, null, 2))
ws.close()
if (checks.some((item) => !item.ok)) process.exit(1)
