// Screenshot slices of a built booklet at a fixed width, via CDP.
// usage: node tools/booklet/shot.mjs <file.html> <outPrefix> [width] [sliceHeight]
import { spawn } from 'node:child_process'
import { existsSync, mkdtempSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { pathToFileURL } from 'node:url'

const [file, outPrefix, widthArg = '1440', sliceArg = '1800'] = process.argv.slice(2)
if (!file || !outPrefix) {
  console.error('usage: node tools/booklet/shot.mjs <file.html> <outPrefix> [width] [sliceHeight]')
  process.exit(1)
}
const width = Number(widthArg)
const sliceHeight = Number(sliceArg)

const candidates = [
  process.env.BOOK_BROWSER,
  'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe',
  'C:/Program Files/Google/Chrome/Application/chrome.exe',
  'C:/Program Files (x86)/Google/Chrome/Application/chrome.exe',
].filter(Boolean)
const browser = candidates.find((path) => existsSync(path))
if (!browser) {
  console.error('no Edge/Chrome found; set BOOK_BROWSER')
  process.exit(1)
}

const port = 9800 + Math.floor(Math.random() * 150)
const profile = mkdtempSync(join(tmpdir(), 'yebian-shot-'))
const child = spawn(
  browser,
  [
    '--headless=new',
    '--disable-gpu',
    '--hide-scrollbars',
    '--allow-file-access-from-files',
    `--remote-debugging-port=${port}`,
    `--user-data-dir=${profile}`,
    `--window-size=${width},1200`,
    'about:blank',
  ],
  { stdio: 'ignore' },
)

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms))
let target
for (let i = 0; i < 40; i += 1) {
  try {
    const list = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json()
    target = list.find((item) => item.type === 'page')
    if (target) break
  } catch {
    /* not up */
  }
  await sleep(250)
}
if (!target) {
  child.kill()
  console.error('no CDP target')
  process.exit(1)
}

const ws = new WebSocket(target.webSocketDebuggerUrl)
let nextId = 0
const pending = new Map()
ws.onmessage = (event) => {
  const message = JSON.parse(String(event.data))
  const entry = pending.get(message.id)
  if (!entry) return
  pending.delete(message.id)
  entry(message)
}
await new Promise((resolve, reject) => {
  ws.onopen = resolve
  ws.onerror = reject
})
const send = (method, params = {}) =>
  new Promise((resolve, reject) => {
    const id = ++nextId
    pending.set(id, (message) => (message.error ? reject(new Error(JSON.stringify(message.error))) : resolve(message.result)))
    ws.send(JSON.stringify({ id, method, params }))
  })

await send('Page.enable')
await send('Runtime.enable')
await send('Emulation.setDeviceMetricsOverride', { width, height: 1200, deviceScaleFactor: 1, mobile: false })
await send('Page.navigate', { url: pathToFileURL(file).href })
await sleep(1800)

const height = await send('Runtime.evaluate', {
  expression: 'document.querySelector(".doc").scrollHeight',
  returnByValue: true,
})
const total = height.result?.value || 1200
const slices = Math.ceil(total / sliceHeight)
for (let i = 0; i < slices; i += 1) {
  const y = i * sliceHeight
  const shot = await send('Page.captureScreenshot', {
    format: 'png',
    captureBeyondViewport: true,
    clip: { x: 0, y, width, height: Math.min(sliceHeight, total - y), scale: 1 },
  })
  writeFileSync(`${outPrefix}-s${String(i).padStart(2, '0')}.png`, Buffer.from(shot.data, 'base64'))
}
console.log(JSON.stringify({ total, slices, width, outPrefix }))
ws.close()
child.kill()
