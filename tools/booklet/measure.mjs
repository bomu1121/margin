// Headless metrics for a built booklet: overflow, out-of-bounds, sub-12px text,
// figure/section counts. Uses Edge/Chrome over CDP; no extra dependencies.
//
// usage: node tools/booklet/measure.mjs <file.html> [width] [height]
import { spawn } from 'node:child_process'
import { existsSync, mkdtempSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { pathToFileURL } from 'node:url'

const args = process.argv.slice(2)
const printMode = args.includes('--print')
const rest = args.filter((a) => a !== '--print')
const [file, widthArg = printMode ? '794' : '1440', heightArg = printMode ? '1123' : '1100'] = rest
if (!file) {
  console.error('usage: node tools/booklet/measure.mjs <file.html> [width] [height]')
  process.exit(1)
}
const width = Number(widthArg)
const height = Number(heightArg)

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

const port = 9500 + Math.floor(Math.random() * 300)
const profile = mkdtempSync(join(tmpdir(), 'yebian-book-'))
const child = spawn(
  browser,
  [
    '--headless=new',
    '--disable-gpu',
    '--hide-scrollbars',
    '--allow-file-access-from-files',
    `--remote-debugging-port=${port}`,
    `--user-data-dir=${profile}`,
    `--window-size=${width},${height}`,
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
    /* not up yet */
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
await send('Emulation.setDeviceMetricsOverride', { width, height, deviceScaleFactor: 1, mobile: false })
if (printMode) await send('Emulation.setEmulatedMedia', { media: 'print' })
await send('Page.navigate', { url: pathToFileURL(file).href })
await sleep(1800)

const expression = `(() => {
  const doc = document.querySelector('.doc')
  const d = doc.getBoundingClientRect()
  const html = [...doc.querySelectorAll('*')].filter((el) => el.namespaceURI === 'http://www.w3.org/1999/xhtml')
  const label = (el) => (el.className && String(el.className).slice(0, 28)) || el.tagName
  const overflow = html
    .filter((el) => el.scrollWidth > el.clientWidth + 2)
    .filter((el) => !['auto', 'scroll'].includes(getComputedStyle(el).overflowX))
    .map((el) => ({ el: label(el), by: el.scrollWidth - el.clientWidth }))
  const outOfBounds = html
    .map((el) => ({ el, r: el.getBoundingClientRect() }))
    .filter(({ r }) => r.width > 0 && (r.right > d.right + 1 || r.left < d.left - 1))
    .map(({ el, r }) => ({ el: label(el), left: Math.round(r.left), right: Math.round(r.right) }))
  const under12 = html
    .filter((el) => [...el.childNodes].some((n) => n.nodeType === 3 && n.textContent.trim()))
    .filter((el) => el.getBoundingClientRect().width > 0 && parseFloat(getComputedStyle(el).fontSize) < 12)
    .map((el) => ({ el: label(el), size: getComputedStyle(el).fontSize, text: el.textContent.trim().slice(0, 16) }))
  const figs = [...doc.querySelectorAll('.fig')].map((f) => {
    const r = f.getBoundingClientRect()
    return { kind: f.dataset.kind, w: Math.round(r.width), h: Math.round(r.height), caption: (f.querySelector('.fig-title')?.textContent || '').trim().slice(0, 24) }
  })
  const svgClip = [...doc.querySelectorAll(".fig svg")].map((svg) => {
    const vb = svg.viewBox.baseVal
    let clipped = 0
    for (const t of svg.querySelectorAll("text")) {
      const b = t.getBBox()
      if (b.x < -2 || b.y < -2 || b.x + b.width > vb.width + 1 || b.y + b.height > vb.height + 1) clipped += 1
    }
    return { clipped, viewBox: [Math.round(vb.width), Math.round(vb.height)] }
  })
  return {
    viewport: [innerWidth, innerHeight],
    svgClip,
    doc: { x: Math.round(d.x), w: Math.round(d.width), h: Math.round(doc.scrollHeight) },
    sections: doc.querySelectorAll('.sec').length,
    figures: figs,
    overflow,
    outOfBounds,
    under12,
    title: (document.querySelector('.book-cover h1')?.textContent || '').trim(),
    pageFit: (() => {
      const pageH = Math.round((297 - 32) * 96 / 25.4)
      const pageW = Math.round((210 - 30) * 96 / 25.4)
      const secs = [...doc.querySelectorAll('.sec')].map((s, i) => ({
        n: i + 1,
        h: Math.round(s.getBoundingClientRect().height),
        wide: Math.round(s.scrollWidth) > pageW + 2,
        heading: (s.querySelector('h2')?.textContent || '').slice(0, 18),
      }))
      return { pageH, pageW, estimatedPages: secs.reduce((sum, s) => sum + Math.max(1, Math.ceil(s.h / pageH)), 0) + 2, overOne: secs.filter((s) => s.h > pageH), wide: secs.filter((s) => s.wide) }
    })(),
  }
})()`

const result = await send('Runtime.evaluate', { expression, returnByValue: true })
const value = result.result?.value
ws.close()
child.kill()
if (!value) {
  console.error('measure failed', JSON.stringify(result))
  process.exit(1)
}
console.log(JSON.stringify({ file, width, ...value }, null, 1))
const clipped = value.svgClip.reduce((sum, item) => sum + item.clipped, 0)
const failed = value.overflow.length || value.outOfBounds.length || value.under12.length || clipped
process.exit(failed ? 2 : 0)
