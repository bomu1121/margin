import fs from 'node:fs'
import path from 'node:path'
const shotsDir = path.resolve('shots/panels')
fs.mkdirSync(shotsDir, { recursive: true })
const sleep = (ms) => new Promise((r) => setTimeout(r, ms))
const targets = await (await fetch('http://127.0.0.1:9222/json/list')).json()
const page = targets.find((x) => x.type === 'page')
if (!page) throw new Error('no CDP page')
const ws = new WebSocket(page.webSocketDebuggerUrl)
await new Promise((r, j) => { ws.addEventListener('open', r, { once: true }); ws.addEventListener('error', j, { once: true }) })
let id = 0
const pending = new Map()
ws.addEventListener('message', (e) => { const m = JSON.parse(String(e.data)); const p = pending.get(m.id); if (!p) return; pending.delete(m.id); p(m) })
function send(method, params = {}) { const i = ++id; ws.send(JSON.stringify({ id: i, method, params })); return new Promise((r, j) => pending.set(i, (m) => m.error ? j(new Error(JSON.stringify(m.error))) : r(m.result))) }
async function ev(expression) { const r = await send('Runtime.evaluate', { expression, returnByValue: true, awaitPromise: true }); if (r.exceptionDetails) throw new Error(JSON.stringify(r.exceptionDetails)); return r.result?.value }
async function mouse(type, x, y, button = 'none', buttons = 0) { await send('Input.dispatchMouseEvent', { type, x: Math.round(x), y: Math.round(y), button, buttons, clickCount: button === 'left' ? 1 : 0 }) }
async function drag(x, y, dx, dy) { await mouse('mousePressed', x, y, 'left', 1); await mouse('mouseMoved', x + dx, y + dy, 'left', 1); await mouse('mouseReleased', x + dx, y + dy, 'left', 0); await sleep(300) }
async function shot(name) { const r = await send('Page.captureScreenshot', { format: 'png' }); const target = path.join(shotsDir, name); fs.writeFileSync(target, Buffer.from(r.data, 'base64')); console.log('wrote', target) }

await ev(`localStorage.setItem('yebian.shell.sidebar.width','240');localStorage.setItem('yebian.shell.sidebar.collapsed','0');localStorage.setItem('yebian.rail.width','380');localStorage.setItem('yebian.rail.collapsed','0');location.reload();'ok'`)
await sleep(1800)
const tree = await ev(`(()=>{const e=document.querySelector('.tree-scroll');e.scrollTop=150;const b=e.getBoundingClientRect();return {right:b.right,top:b.top,h:b.height}})()`)
await mouse('mouseMoved', tree.right - 8, tree.top + 240)
await sleep(180)
await shot('01-scrollbar-hover.png')

const doc = await ev(`(()=>{const e=document.querySelector('.doc-scroll');e.scrollTop=250;const b=e.getBoundingClientRect();return {right:b.right,top:b.top,h:b.height}})()`)
await mouse('mouseMoved', doc.right - 8, doc.top + 240)
await sleep(180)
await shot('02-doc-scrollbar-hover.png')

await ev(`document.querySelector('.doc-scroll').scrollTop=0;'ok'`)
const sr = await ev(`(()=>{const b=document.querySelector('.sidebar-resizer').getBoundingClientRect();return {x:b.x,y:b.y}})()`)
await drag(sr.x + 3, 420, 60, 0)
const rr = await ev(`(()=>{const b=document.querySelector('.rail-resizer').getBoundingClientRect();return {x:b.x,y:b.y}})()`)
await drag(rr.x + 3, 420, -50, 0)
await shot('03-resized-panels.png')

await ev(`document.querySelector('.sidebar-toggle').click();'ok'`)
await sleep(350)
await shot('04-sidebar-collapsed.png')
await ev(`document.querySelector('.sidebar-toggle').click();'ok'`)
await sleep(350)

await ev(`document.querySelector('.rail-collapse').click();'ok'`)
await sleep(350)
await shot('05-rail-collapsed.png')
await ev(`document.querySelector('.rail-expand-edge').click();'ok'`)
await sleep(350)

await ev(`localStorage.setItem('yebian.shell.sidebar.width','240');localStorage.setItem('yebian.shell.sidebar.collapsed','0');localStorage.setItem('yebian.rail.width','380');localStorage.setItem('yebian.rail.collapsed','0');location.reload();'ok'`)
await sleep(1200)
ws.close()
