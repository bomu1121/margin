// Interaction test for resizable shell panels + overlay scrollbars.
import fs from 'node:fs'
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
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms))
async function mouse(type, x, y, button = 'none', buttons = 0) {
  await send('Input.dispatchMouseEvent', { type, x: Math.round(x), y: Math.round(y), button, buttons, clickCount: button === 'left' ? 1 : 0 })
}
async function drag(x, y, dx, dy) {
  await mouse('mousePressed', x, y, 'left', 1)
  await mouse('mouseMoved', x + dx, y + dy, 'left', 1)
  await mouse('mouseReleased', x + dx, y + dy, 'left', 0)
  await sleep(250)
}

const initial = await evaluate(`(() => {
  const r = (el) => { if (!el) return null; const b = el.getBoundingClientRect(); return { x: b.x, y: b.y, w: b.width, h: b.height }; };
  return {
    route: location.hash,
    sidebar: r(document.querySelector('.sidebar')),
    sidebarResizer: r(document.querySelector('.sidebar-resizer')),
    rail: r(document.querySelector('.rail')),
    railResizer: r(document.querySelector('.rail-resizer')),
    local: { sidebar: localStorage.getItem('yebian.shell.sidebar.width'), rail: localStorage.getItem('yebian.rail.width') }
  };
})()`)
console.log('initial', JSON.stringify(initial))

const sr = initial.sidebarResizer
await drag(sr.x + 3, 420, 60, 0)
const afterSidebar = await evaluate(`({sidebar:document.querySelector('.sidebar').getBoundingClientRect().width, stored:localStorage.getItem('yebian.shell.sidebar.width')})`)
console.log('after sidebar drag', JSON.stringify(afterSidebar))

await evaluate(`document.querySelector('.sidebar-toggle').click()`)
await sleep(300)
const collapsed = await evaluate(`({collapsed:document.querySelector('.body').classList.contains('sidebar-collapsed'), sidebar:getComputedStyle(document.querySelector('.sidebar')).display, grid:getComputedStyle(document.querySelector('.body')).gridTemplateColumns})`)
console.log('sidebar collapsed', JSON.stringify(collapsed))
await evaluate(`document.querySelector('.sidebar-toggle').click()`)
await sleep(300)

const rr = await evaluate(`(() => { const el=document.querySelector('.rail-resizer'); const b=el.getBoundingClientRect(); return {x:b.x,y:b.y,w:b.width,h:b.height}; })()`)
await drag(rr.x + 3, 420, -50, 0)
const afterRail = await evaluate(`({rail:document.querySelector('.rail').getBoundingClientRect().width, stored:localStorage.getItem('yebian.rail.width')})`)
console.log('after rail drag', JSON.stringify(afterRail))

await evaluate(`document.querySelector('.rail-collapse').click()`)
await sleep(350)
const railCollapsed = await evaluate(`({collapsed:document.querySelector('.note-layout').classList.contains('rail-collapsed'), rail:getComputedStyle(document.querySelector('.rail')).display, edge:!!document.querySelector('.rail-expand-edge')})`)
console.log('rail collapsed', JSON.stringify(railCollapsed))
await evaluate(`document.querySelector('.rail-expand-edge').click()`)
await sleep(350)
console.log('rail expanded', JSON.stringify(await evaluate(`document.querySelector('.note-layout').classList.contains('rail-collapsed')`)))

// Overlay scrollbar: hover near the tree scrollbar edge, wait for reveal.
const tree = await evaluate(`(() => { const el=document.querySelector('.tree-scroll'); const b=el.getBoundingClientRect(); return {right:b.right, top:b.top, height:b.height, scrollable:el.scrollHeight>el.clientHeight, cls:el.className}; })()`)
await mouse('mouseMoved', tree.right - 8, tree.top + Math.min(200, tree.height / 2))
await sleep(180)
const visible = await evaluate(`({cls:document.querySelector('.yb-scrollbar').className, thumb:document.querySelector('.yb-scrollbar-thumb').getBoundingClientRect().height})`)
console.log('scrollbar hover', JSON.stringify(visible))
await sleep(1100)
const faded = await evaluate(`document.querySelector('.yb-scrollbar').className`)
console.log('scrollbar after idle', faded)

// Quick pass should not reveal.
await mouse('mouseMoved', tree.right - 8, tree.top + 100)
await sleep(20)
await mouse('mouseMoved', tree.right - 200, tree.top + 100)
await sleep(140)
const quick = await evaluate(`document.querySelector('.yb-scrollbar').className`)
console.log('scrollbar quick pass', quick)

// Restore defaults and persisted state.
await evaluate(`localStorage.setItem('yebian.shell.sidebar.width','240'); localStorage.setItem('yebian.shell.sidebar.collapsed','0'); localStorage.setItem('yebian.rail.width','380'); localStorage.setItem('yebian.rail.collapsed','0'); location.reload(); 'restored'`)
await sleep(1200)
ws.close()
