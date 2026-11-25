// End-to-end check that a sticky-card link opens the browser with its full URL (including `?query`).
// Creates a temporary sticky with https://example.com/?a=1&b=2, clicks the rendered link in the live
// whiteboard window, reports the foreground window title, then removes the card.
// This opens a real browser tab (Example Domain) — run it manually.
// usage: CDP_PORT=9333 node --no-warnings tools/test-whiteboard-link-open.mjs

import { execFileSync } from 'node:child_process'

const port = process.env.CDP_PORT || '9222'
const BOARD_KEY = 'yebian.whiteboard.board'
const VIEWS_KEY = 'yebian.whiteboard.views'
const URL = 'https://example.com/?a=1&b=2'

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
async function waitFor(expression, timeout = 8000) {
  const started = Date.now()
  for (;;) {
    if (await ev(expression)) return
    if (Date.now() - started > timeout) throw new Error(`timeout: ${expression}`)
    await sleep(120)
  }
}
function foregroundTitle() {
  const script = `
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using System.Text;
public class FgTitle {
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
}
'@
$sb = New-Object System.Text.StringBuilder 256
[FgTitle]::GetWindowText([FgTitle]::GetForegroundWindow(), $sb, 256) | Out-Null
$sb.ToString()
`
  return execFileSync('powershell', ['-NoProfile', '-Command', script], { encoding: 'utf8' }).trim()
}

const saved = await ev(`({ views: localStorage.getItem('${VIEWS_KEY}') })`)
const created = await ev(`(async () => {
  const boardId = localStorage.getItem('${BOARD_KEY}') || 'board_default'
  return window.__TAURI_INTERNALS__.invoke('whiteboard_item_save', {
    input: {
      boardId,
      kind: 'sticky',
      x: 310,
      y: 40,
      w: 270,
      h: 150,
      z: 999,
      text: 'E2E 测试链接 ${URL}',
      color: 'plain',
    },
  })
})()`)
console.log('created', created.id)

try {
  await send('Page.reload', { ignoreCache: true })
  await waitFor(`!!document.querySelector('.wb-card')`)
  await waitFor(`[...document.querySelectorAll('.wb-sticky-url')].some((a) => a.dataset.url === '${URL}')`)
  await ev(`document.querySelector('.wb-zoom button[title="回到默认视图"]')?.click()`)
  await sleep(400)

  const point = await ev(`(() => {
    const anchor = [...document.querySelectorAll('.wb-sticky-url')].find((a) => a.dataset.url === '${URL}')
    const rect = anchor.getClientRects()[0]
    return { x: rect.x + 12, y: rect.y + rect.height / 2, color: getComputedStyle(anchor).color, width: rect.width }
  })()`)
  // Sweep the pointer along the wrapped link: with no hover colour rule the sampled colour must not change.
  const colors = []
  for (let step = 0; step <= 8; step += 1) {
    await mouse('mouseMoved', point.x - 12 + 2 + ((point.width - 4) * step) / 8, point.y)
    await sleep(40)
    colors.push(await ev(`getComputedStyle([...document.querySelectorAll('.wb-sticky-url')].find((a) => a.dataset.url === '${URL}')).color`))
  }
  console.log('hover colours', [...new Set(colors)])
  console.log('click point', point)
  await mouse('mouseMoved', point.x, point.y)
  await mouse('mousePressed', point.x, point.y, 'left', 1)
  await mouse('mouseReleased', point.x, point.y, 'left', 0)
  await sleep(2500)
  console.log('foreground after click:', foregroundTitle())
  console.log('webview location:', await ev('location.href'))
} finally {
  await ev(`(async () => {
    const boardId = localStorage.getItem('${BOARD_KEY}') || 'board_default'
    const items = await window.__TAURI_INTERNALS__.invoke('whiteboard_items', { boardId })
    for (const item of items) {
      if (item.x === 310 && item.y === 40 && item.z === 999) {
        await window.__TAURI_INTERNALS__.invoke('whiteboard_item_delete', { id: item.id })
      }
    }
    const views = ${JSON.stringify(saved.views === null ? '__NULL__' : saved.views)}
    if (views === '__NULL__') localStorage.removeItem('${VIEWS_KEY}')
    else localStorage.setItem('${VIEWS_KEY}', views)
  })()`)
  await send('Page.reload', { ignoreCache: true })
  await sleep(800)
  ws.close()
}
