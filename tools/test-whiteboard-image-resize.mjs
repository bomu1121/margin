// Verifies the whiteboard image card interaction in a live dev window:
//   1. the image card no longer carries its own hover action pill;
//   2. selecting one image shows 查来源 / 搜相似 in the bottom selection bar;
//   3. dragging the resize handle keeps the bitmap frame at the source aspect.
// usage: CDP_PORT=9333 node --no-warnings tools/test-whiteboard-image-resize.mjs
// Creates a temporary 700x700 image card, measures it, then deletes it and restores the view.

import fs from 'node:fs'
import path from 'node:path'

const port = process.env.CDP_PORT || '9222'
const BOARD_KEY = 'yebian.whiteboard.board'
const VIEWS_KEY = 'yebian.whiteboard.views'
const OUT_DIR = path.resolve('shots/whiteboard')

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
async function mouse(type, x, y, button = 'none', buttons = 0) {
  await send('Input.dispatchMouseEvent', {
    type,
    x: Math.round(x),
    y: Math.round(y),
    button,
    buttons,
    clickCount: button === 'left' ? 1 : 0,
  })
}
async function shot(base) {
  const name = wide ? base.replace(/\.png$/, '-wide.png') : base
  const result = await send('Page.captureScreenshot', { format: 'png' })
  fs.mkdirSync(OUT_DIR, { recursive: true })
  const file = path.join(OUT_DIR, name)
  fs.writeFileSync(file, Buffer.from(result.data, 'base64'))
  console.log('shot', path.relative(process.cwd(), file))
}
async function waitFor(expression, timeout = 8000) {
  const started = Date.now()
  for (;;) {
    if (await ev(expression)) return
    if (Date.now() - started > timeout) throw new Error(`timeout: ${expression}`)
    await sleep(120)
  }
}
async function cardMetrics(left) {
  return ev(`(() => {
    const card = [...document.querySelectorAll('.wb-card')].find((el) => el.style.left === '${left}px')
    if (!card) return null
    const img = card.querySelector('img')
    const frame = card.querySelector('.wb-image-frame')
    const frameRect = frame?.getBoundingClientRect()
    return {
      w: card.getBoundingClientRect().width,
      h: card.getBoundingClientRect().height,
      frameW: frameRect?.width || 0,
      frameH: frameRect?.height || 0,
      naturalW: img?.naturalWidth || 0,
      naturalH: img?.naturalHeight || 0,
      selected: card.classList.contains('is-selected'),
      overlay: card.querySelectorAll('.wb-image-actions').length,
    }
  })()`)
}

const saved = await ev(`({ board: localStorage.getItem('${BOARD_KEY}'), views: localStorage.getItem('${VIEWS_KEY}') })`)
// WIDE=1 uses the real 970x527 screenshot asset instead of the square cat.
const wide = process.env.WIDE === '1'
const sourceWidth = wide ? 970 : 700
const sourceHeight = wide ? 527 : 700
const frameHeight = Math.round((320 / sourceWidth) * sourceHeight * 100) / 100
const created = await ev(`(async () => {
  const boardId = localStorage.getItem('${BOARD_KEY}') || 'board_default'
  const items = await window.__TAURI_INTERNALS__.invoke('whiteboard_items', { boardId })
  const source = items.find((item) => item.kind === 'image' && item.dataJson.includes('"width":${sourceWidth}'))
  if (!source) throw new Error('no ${sourceWidth}x${sourceHeight} image asset on this board')
  return window.__TAURI_INTERNALS__.invoke('whiteboard_item_save', {
    input: {
      boardId,
      kind: 'image',
      x: 310,
      y: 40,
      w: 322,
      h: ${frameHeight + 25},
      z: 999,
      assetPath: source.assetPath,
      dataJson: source.dataJson,
    },
  })
})()`)
console.log('created', created.id, created.w, created.h)

try {
  await send('Page.reload', { ignoreCache: true })
  await waitFor(`!!document.querySelector('.wb-card')`)
  await waitFor(`[...document.querySelectorAll('.wb-card')].some((el) => el.style.left === '310px')`)
  await ev(`document.querySelector('.wb-zoom button[title="回到默认视图"]')?.click()`)
  await sleep(400)

  const before = await cardMetrics(310)
  console.log('before', before)

  const head = await ev(`(() => {
    const card = [...document.querySelectorAll('.wb-card')].find((el) => el.style.left === '310px')
    const rect = card.querySelector('.wb-card-grip').getBoundingClientRect()
    return { x: rect.left + 40, y: rect.top + 10 }
  })()`)
  await mouse('mouseMoved', head.x, head.y)
  await mouse('mousePressed', head.x, head.y, 'left', 1)
  await mouse('mouseReleased', head.x, head.y, 'left', 0)
  await sleep(250)

  const bar = await ev(`(() => {
    const bar = document.querySelector('.wb-selection-bar')
    return { visible: !!bar, text: bar?.innerText.replace(/\\s+/g, ' ') || '', buttons: bar ? bar.querySelectorAll('button').length : 0 }
  })()`)
  console.log('selection bar', bar)
  const selected = await cardMetrics(310)
  console.log('after select', selected)
  await shot('38-image-selection-bar.png')

  const handle = await ev(`(() => {
    const card = [...document.querySelectorAll('.wb-card')].find((el) => el.style.left === '310px')
    const rect = card.querySelector('.wb-resize').getBoundingClientRect()
    return { x: rect.left + rect.width / 2, y: rect.top + rect.height / 2 }
  })()`)
  await mouse('mouseMoved', handle.x, handle.y)
  await mouse('mousePressed', handle.x, handle.y, 'left', 1)
  for (let step = 1; step <= 8; step += 1) {
    await mouse('mouseMoved', handle.x + (150 * step) / 8, handle.y + (18 * step) / 8, 'left', 1)
    await sleep(30)
  }
  await mouse('mouseReleased', handle.x + 150, handle.y + 18, 'left', 0)
  await sleep(400)
  const wide = await cardMetrics(310)
  console.log('after horizontal drag', wide)
  await shot('39-image-resize-horizontal.png')

  const handle2 = await ev(`(() => {
    const card = [...document.querySelectorAll('.wb-card')].find((el) => el.style.left === '310px')
    const rect = card.querySelector('.wb-resize').getBoundingClientRect()
    return { x: rect.left + rect.width / 2, y: rect.top + rect.height / 2 }
  })()`)
  await mouse('mouseMoved', handle2.x, handle2.y)
  await mouse('mousePressed', handle2.x, handle2.y, 'left', 1)
  for (let step = 1; step <= 6; step += 1) {
    await mouse('mouseMoved', handle2.x + (4 * step) / 6, handle2.y - (60 * step) / 6, 'left', 1)
    await sleep(30)
  }
  await mouse('mouseReleased', handle2.x + 4, handle2.y - 60, 'left', 0)
  await sleep(400)
  const tall = await cardMetrics(310)
  console.log('after vertical drag', tall)
  await shot('40-image-resize-vertical.png')

  // The bar wraps instead of overflowing at the narrow window width.
  await send('Emulation.setDeviceMetricsOverride', { width: 520, height: 760, deviceScaleFactor: 1, mobile: false })
  await sleep(400)
  const narrowBar = await ev(`(() => {
    const bar = document.querySelector('.wb-selection-bar')
    const rect = bar.getBoundingClientRect()
    return { left: Math.round(rect.left), right: Math.round(rect.right), width: Math.round(rect.width), viewport: innerWidth, scrollWidth: bar.scrollWidth, clientWidth: bar.clientWidth }
  })()`)
  console.log('narrow bar', narrowBar)
  await shot('41-image-selection-bar-520.png')
  await send('Emulation.clearDeviceMetricsOverride')
  await sleep(300)

  const report = {
    before,
    selected,
    wide,
    tall,
    bar,
    aspectError: [wide, tall].map((m) => Math.abs(m.frameW / m.frameH - m.naturalW / m.naturalH)),
    // object-fit: contain leaves bars when the frame aspect differs from the bitmap aspect.
    letterboxPx: [wide, tall].map((m) => {
      const aspect = m.naturalW / m.naturalH
      return m.frameW / m.naturalW > m.frameH / m.naturalH
        ? Math.round(m.frameW - m.frameH * aspect)
        : Math.round(m.frameH - m.frameW / aspect)
    }),
  }
  console.log(JSON.stringify(report, null, 2))

  await ev(`[...document.querySelectorAll('.wb-selection-bar button')].find((el) => el.textContent.includes('删除'))?.click()`)
  await sleep(600)
} finally {
  // Restore the user's view settings and drop the temporary card if the test bailed early.
  await ev(`(async () => {
    try {
      const boardId = localStorage.getItem('${BOARD_KEY}') || 'board_default'
      const items = await window.__TAURI_INTERNALS__.invoke('whiteboard_items', { boardId })
      for (const item of items) {
        if (item.x === 310 && item.y === 40 && item.z === 999) {
          await window.__TAURI_INTERNALS__.invoke('whiteboard_item_delete', { id: item.id })
        }
      }
    } catch {}
    const board = ${JSON.stringify(saved.board === null ? '__NULL__' : saved.board)}
    const views = ${JSON.stringify(saved.views === null ? '__NULL__' : saved.views)}
    if (board === '__NULL__') localStorage.removeItem('${BOARD_KEY}')
    else localStorage.setItem('${BOARD_KEY}', board)
    if (views === '__NULL__') localStorage.removeItem('${VIEWS_KEY}')
    else localStorage.setItem('${VIEWS_KEY}', views)
  })()`)
  await send('Page.reload', { ignoreCache: true })
  await sleep(800)
  ws.close()
}
