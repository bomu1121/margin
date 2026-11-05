// Captures sidebar states and measured geometry through the running dev app.
// usage: node --no-warnings tools/ui-shots.mjs <label>
//
// Prereq: `npm run tauri dev` with
//   WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222
// and the vault pointed at the fixture from tools/make-ui-fixture.mjs.
//
// Writes shots/ui-<label>-*.png and docs/ui-measure-<label>.json.

import fs from 'node:fs'
import path from 'node:path'

const rawLabel = process.argv[2] || 'current'
const theme = process.argv[3] === 'dark' ? 'dark' : 'light'
const label = theme === 'dark' ? `${rawLabel}-dark` : rawLabel
const shotsDir = path.resolve('shots')
const docsDir = path.resolve('docs')
fs.mkdirSync(shotsDir, { recursive: true })

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

async function waitFor(expression, labelText, timeout = 15000) {
  const start = Date.now()
  while (Date.now() - start < timeout) {
    try {
      if (await evaluate(expression)) return true
    } catch {
      // context is briefly gone during reload
    }
    await sleep(120)
  }
  throw new Error(`timeout waiting for ${labelText}`)
}

async function shot(name) {
  const result = await send('Page.captureScreenshot', { format: 'png' })
  fs.writeFileSync(path.join(shotsDir, `ui-${label}-${name}.png`), Buffer.from(result.data, 'base64'))
}

async function mouse(x, y, type, button = 'none') {
  await send('Input.dispatchMouseEvent', {
    type,
    x: Math.round(x),
    y: Math.round(y),
    button,
    clickCount: button === 'none' ? 0 : 1,
  })
}

async function press(key, code, keyCode) {
  await send('Input.dispatchKeyEvent', { type: 'keyDown', key, code, windowsVirtualKeyCode: keyCode })
  await send('Input.dispatchKeyEvent', { type: 'keyUp', key, code, windowsVirtualKeyCode: keyCode })
}

await send('Page.enable')
await send('Runtime.enable')
if (theme === 'dark') {
  await send('Emulation.setEmulatedMedia', { features: [{ name: 'prefers-color-scheme', value: 'dark' }] })
}
await send('Page.reload', { ignoreCache: true })
await waitFor(`!!document.querySelector('.side-nav .tree-title')`, 'sidebar')
await sleep(500)

// ui-shots is about the sidebar; keep the reading pane on a real note so the
// screenshots do not catch the load/redirect gap.
const onNote = await evaluate(`location.hash.startsWith('#/note/')`)
if (!onNote) {
  await evaluate(`(() => {
    const row = document.querySelector('.tree-body .note-row')
    const id = row?.dataset.key?.slice(2)
    if (id) location.hash = '#/note/' + id
  })()`)
}
await waitFor(`!!document.querySelector('.doc h1')`, 'note content', 12000).catch(() => {})

const vault = await evaluate(`window.__TAURI_INTERNALS__.invoke('get_vault')`)
const base = await evaluate(`(() => {
  const rect = (el) => { if (!el) return null; const r = el.getBoundingClientRect(); return [Math.round(r.x), Math.round(r.y), Math.round(r.width), Math.round(r.height)]; };
  const style = (el, props) => { if (!el) return null; const c = getComputedStyle(el); return Object.fromEntries(props.map((p) => [p, c.getPropertyValue(p)])); };
  return {
    vault: ${JSON.stringify('')},
    treeRole: document.querySelector('.tree-body')?.getAttribute('role'),
    noteRows: [...document.querySelectorAll('.tree-body .note-row')].slice(0, 6).map((el) => ({ h: Math.round(el.getBoundingClientRect().height), title: el.querySelector('.tree-title')?.textContent.trim() })),
    folderRows: [...document.querySelectorAll('[data-folder]')].slice(0, 6).map((el) => ({ h: Math.round(el.getBoundingClientRect().height), name: el.querySelector('.tree-name')?.textContent.trim() })),
    fileTitle: { rect: rect(document.querySelector('.tree-title')), ...style(document.querySelector('.tree-title'), ['font-size','line-height','font-family','color']) },
    fileMeta: { rect: rect(document.querySelector('.tree-meta')), ...style(document.querySelector('.tree-meta'), ['font-size','color']) },
    folder: { rect: rect(document.querySelector('[data-folder]')), ...style(document.querySelector('[data-folder]'), ['font-size','padding','color','background-color']) },
    chev: rect(document.querySelector('.chev')),
    pin: rect(document.querySelector('.pin')),
    sideTool: rect(document.querySelector('.side-tool')),
    filter: rect(document.querySelector('.side-filter')),
    treeFocusables: [...document.querySelectorAll('.side-nav a, .side-nav button, .side-nav input')].length,
  };
})()`)

await shot('01-default')

// Hover the second note row so the selected first row stays visible alongside it.
const hoverPoint = await evaluate(`(() => {
  const row = document.querySelectorAll('.tree-body .note-row')[1] || document.querySelector('.tree-body .note-row');
  const r = row.getBoundingClientRect();
  return { x: r.x + r.width / 2, y: r.y + r.height / 2 };
})()`)
await mouse(hoverPoint.x, hoverPoint.y, 'mouseMoved')
await sleep(250)
const hoverStyle = await evaluate(`(() => {
  const row = document.querySelectorAll('.tree-body .note-row')[1] || document.querySelector('.tree-body .note-row');
  const c = getComputedStyle(row);
  return { background: c.backgroundColor, title: getComputedStyle(row.querySelector('.tree-title')).color, pinOpacity: getComputedStyle(row.querySelector('.pin')).opacity };
})()`)
await shot('02-hover')

// Right-click a note row for the context menu.
await mouse(hoverPoint.x, hoverPoint.y, 'mousePressed', 'right')
await mouse(hoverPoint.x, hoverPoint.y, 'mouseReleased', 'right')
await sleep(250)
let menuOpen = await evaluate(`!!document.querySelector('.ctx')`)
if (!menuOpen) {
  await evaluate(`(() => {
    const row = document.querySelectorAll('.tree-body .note-row')[1] || document.querySelector('.tree-body .note-row');
    const r = row.getBoundingClientRect();
    row.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX: r.x + 20, clientY: r.y + 20 }));
  })()`)
  await sleep(200)
  menuOpen = await evaluate(`!!document.querySelector('.ctx')`)
}
await shot('03-menu')
const menuStyle = await evaluate(`(() => {
  const menu = document.querySelector('.ctx');
  if (!menu) return null;
  const item = menu.querySelector('button');
  const c = item ? getComputedStyle(item) : null;
  return { rect: (() => { const r = menu.getBoundingClientRect(); return [Math.round(r.width), Math.round(r.height)]; })(), itemHeight: item ? Math.round(item.getBoundingClientRect().height) : null, itemFont: c?.fontSize };
})()`)
await press('Escape', 'Escape', 27)
await evaluate(`document.body.click()`)
await sleep(150)

// Inline rename.
await evaluate(`(() => {
  const row = document.querySelectorAll('.tree-body .note-row')[1] || document.querySelector('.tree-body .note-row');
  const r = row.getBoundingClientRect();
  row.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX: r.x + 20, clientY: r.y + 20 }));
})()`)
await sleep(150)
await evaluate(`[...document.querySelectorAll('.ctx button')].find((b) => b.textContent.includes('重命名'))?.click()`)
await sleep(250)
const renameShown = await evaluate(`!!document.querySelector('.rename-input')`)
await shot('04-rename')
const renameStyle = await evaluate(`(() => {
  const el = document.querySelector('.rename-input');
  if (!el) return null;
  const c = getComputedStyle(el);
  return { height: Math.round(el.getBoundingClientRect().height), font: c.fontSize, border: c.borderColor, outline: c.outlineStyle + ' ' + c.outlineWidth };
})()`)
await evaluate(`document.querySelector('.rename-input')?.focus()`)
await press('Escape', 'Escape', 27)
await sleep(200)

// Keyboard focus on the filter (a reliable focus-visible check).
await evaluate(`document.querySelector('.side-filter')?.focus()`)
await sleep(150)
const filterFocus = await evaluate(`(() => {
  const el = document.querySelector('.side-filter');
  const c = getComputedStyle(el);
  return { outlineStyle: c.outlineStyle, outlineWidth: c.outlineWidth, outlineColor: c.outlineColor, outlineOffset: c.outlineOffset, boxShadow: c.boxShadow };
})()`)
await shot('05-filter-focus')
await evaluate(`document.activeElement?.blur()`)

// Drag state: drag a note over a folder, then over the root drop zone.
const dragInfo = await evaluate(`(() => {
  const row = document.querySelectorAll('.tree-body .note-row')[0];
  const folder = document.querySelector('[data-folder]');
  if (!row || !folder) return null;
  const dt = new DataTransfer();
  const start = (el, type) => el.dispatchEvent(new DragEvent(type, { bubbles: true, cancelable: true, dataTransfer: dt }));
  start(row, 'dragstart');
  folder.dispatchEvent(new DragEvent('dragover', { bubbles: true, cancelable: true, dataTransfer: dt }));
  const fr = folder.getBoundingClientRect();
  return { folder: [Math.round(fr.x), Math.round(fr.y), Math.round(fr.width), Math.round(fr.height)], row: (() => { const r = row.getBoundingClientRect(); return [Math.round(r.x), Math.round(r.y), Math.round(r.width), Math.round(r.height)]; })() };
})()`)
await sleep(250)
await shot('06-drag-folder')
const dragStyle = await evaluate(`(() => {
  const folder = document.querySelector('[data-folder]');
  const row = document.querySelectorAll('.tree-body .note-row')[0];
  const fc = folder ? getComputedStyle(folder) : null;
  return {
    folderBackground: fc?.backgroundColor,
    folderShadow: fc?.boxShadow,
    sourceOpacity: row ? getComputedStyle(row).opacity : null,
    rootZone: !!document.querySelector('.drop-root'),
  };
})()`)
// Move the same drag over the root zone, then cancel.
await evaluate(`(() => {
  const root = document.querySelector('.drop-root');
  const row = document.querySelectorAll('.tree-body .note-row')[0];
  if (!root || !row) return false;
  const dt = new DataTransfer();
  row.dispatchEvent(new DragEvent('dragstart', { bubbles: true, cancelable: true, dataTransfer: dt }));
  root.dispatchEvent(new DragEvent('dragover', { bubbles: true, cancelable: true, dataTransfer: dt }));
  return true;
})()`)
await sleep(200)
await shot('07-drag-root')
await evaluate(`(() => {
  const row = document.querySelectorAll('.tree-body .note-row')[0];
  row?.dispatchEvent(new DragEvent('dragend', { bubbles: true }));
})()`)
await sleep(150)

// Collapse a folder and open the empty folder.
const collapsed = await evaluate(`(() => {
  const folder = document.querySelector('[data-folder]');
  if (!folder) return false;
  folder.click();
  return true;
})()`)
await sleep(250)
await shot('08-collapsed')
await evaluate(`(() => {
  const folder = document.querySelector('[data-folder]');
  if (folder && folder.getAttribute('aria-expanded') === 'false') folder.click();
})()`)
await sleep(250)

const emptyFolder = await evaluate(`(() => {
  const buttons = [...document.querySelectorAll('[data-folder]')];
  const empty = buttons.find((b) => b.querySelector('.tree-count')?.textContent.trim() === '空');
  if (!empty) return false;
  if (empty.getAttribute('aria-expanded') === 'false') empty.click();
  return true;
})()`)
await sleep(250)
await shot('09-empty-folder')

// Pin state: pin the first note, capture the pinned section, then unpin.
const pinned = await evaluate(`(() => {
  const row = document.querySelectorAll('.tree-body .note-row')[0];
  const pin = row?.querySelector('.pin');
  if (!pin) return false;
  const was = pin.getAttribute('aria-pressed');
  if (was !== 'true') pin.click();
  return { was, now: pin.getAttribute('aria-pressed') };
})()`)
await sleep(250)
await shot('10-pinned')
await evaluate(`(() => {
  const pinnedSection = document.querySelector('.pinned .pin');
  if (pinnedSection && pinnedSection.getAttribute('aria-pressed') === 'true') pinnedSection.click();
})()`)

const report = {
  label,
  vault,
  measuredAt: new Date().toISOString(),
  geometry: base,
  hover: hoverStyle,
  menu: menuStyle,
  rename: { shown: renameShown, ...renameStyle },
  filterFocus,
  drag: { ...dragInfo, ...dragStyle },
  collapsedWorked: collapsed,
  emptyFolderFound: emptyFolder,
  pinnedWorked: pinned,
}
fs.writeFileSync(path.join(docsDir, `ui-measure-${label}.json`), `${JSON.stringify(report, null, 2)}\n`)
if (theme === 'dark') await send('Emulation.setEmulatedMedia', { features: [] })
console.log(JSON.stringify(report, null, 2))
ws.close()
