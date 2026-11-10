// Regression check: a programmatic scroll (route change, auto-reveal, layout
// reset) must not flash the overlay scrollbar. Only wheel/touch/key intent may
// reveal it, and a scroll-revealed bar must survive pointer movement.
//
// Prereq: app running with --remote-debugging-port=9222.
const targets = await (await fetch('http://127.0.0.1:9222/json/list')).json()
const page = targets.find((x) => x.type === 'page')
if (!page) throw new Error('no CDP page target')
const ws = new WebSocket(page.webSocketDebuggerUrl)
await new Promise((resolve, reject) => {
  ws.addEventListener('open', resolve, { once: true })
  ws.addEventListener('error', reject, { once: true })
})
let id = 0
const pending = new Map()
ws.addEventListener('message', (event) => {
  const message = JSON.parse(String(event.data))
  const entry = pending.get(message.id)
  if (!entry) return
  pending.delete(message.id)
  entry(message)
})
function send(method, params = {}) {
  const callId = ++id
  ws.send(JSON.stringify({ id: callId, method, params }))
  return new Promise((resolve, reject) => {
    pending.set(callId, (message) => (message.error ? reject(new Error(JSON.stringify(message.error))) : resolve(message.result)))
  })
}
async function ev(expression) {
  const result = await send('Runtime.evaluate', { expression, returnByValue: true, awaitPromise: true })
  if (result.exceptionDetails) throw new Error(JSON.stringify(result.exceptionDetails))
  return result.result?.value
}
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
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms))

function assert(condition, message) {
  if (!condition) throw new Error(message)
}

// The tree must keep its yb-scroll class while collapsed and after expanding
// all folders; otherwise the native bar paints before the debounced scan.
const treeBefore = await ev(`(()=>{const b=document.querySelector('.side-head button[title="收起所有文件夹"]');if(b)b.click();return true})()`)
await sleep(300)
const collapsed = await ev(`(()=>{const t=document.querySelector('.tree-scroll');return {cls:t.className,scrollHeight:t.scrollHeight,clientHeight:t.clientHeight}})()`)
assert(collapsed.cls.includes('yb-scroll'), `collapsed tree lost yb-scroll: ${JSON.stringify(collapsed)}`)
await ev(`(()=>{const b=document.querySelector('.side-head button[title="展开所有文件夹"]');if(b)b.click();return true})()`)
await sleep(50)
const expanded = await ev(`(()=>{const t=document.querySelector('.tree-scroll');return {cls:t.className,scrollHeight:t.scrollHeight,clientHeight:t.clientHeight}})()`)
assert(expanded.cls.includes('yb-scroll'), `expanded tree lost yb-scroll: ${JSON.stringify(expanded)}`)

// Open a long note through the sidebar so the doc-scroll container exists.
await ev(`(()=>{const row=[...document.querySelectorAll('.note-row')].find(x=>x.textContent.includes('李世民当汉武帝的太子'));if(row)row.click();return !!row})()`)
await sleep(1600)
await ev(`(()=>{
  window.__scrollFlashLog = [];
  const root = document.querySelector('.yb-scrollbar');
  if (!root) return false;
  new MutationObserver(() => window.__scrollFlashLog.push({ t: Math.round(performance.now()), cls: root.className }))
    .observe(root, { attributes: true, attributeFilter: ['class'] });
  return true;
})()`)

const doc = await ev(`(()=>{const e=document.querySelector('.doc-scroll');const b=e.getBoundingClientRect();return {left:b.left,right:b.right,top:b.top,bottom:b.bottom,scrollHeight:e.scrollHeight,clientHeight:e.clientHeight}})()`)
assert(doc.scrollHeight > doc.clientHeight + 10, 'doc-scroll is not scrollable; pick a longer note')

// 1. Programmatic scroll must not reveal the overlay.
await ev(`document.querySelector('.doc-scroll').scrollTop = 600`)
await sleep(250)
let state = await ev(`({cls:document.querySelector('.yb-scrollbar').className, log:window.__scrollFlashLog.slice()})`)
assert(!state.cls.includes('is-visible'), `programmatic scroll flashed: ${JSON.stringify(state)}`)

// 2. Switching notes resets scrollTop; that reset must not reveal it either.
await ev(`(()=>{const row=[...document.querySelectorAll('.note-row')].find(x=>x.textContent.includes('中期选举零分交卷'));if(row)row.click();return !!row})()`)
await sleep(350)
state = await ev(`({cls:document.querySelector('.yb-scrollbar').className, log:window.__scrollFlashLog.slice()})`)
assert(!state.cls.includes('is-visible'), `route scroll reset flashed: ${JSON.stringify(state)}`)

// 3. Wheel intent must reveal it.
const wheelX = doc.right - 40
const wheelY = Math.min(doc.bottom - 80, doc.top + 300)
await send('Input.dispatchMouseEvent', { type: 'mouseWheel', x: Math.round(wheelX), y: Math.round(wheelY), deltaX: 0, deltaY: 160 })
await sleep(100)
state = await ev(`({cls:document.querySelector('.yb-scrollbar').className})`)
assert(state.cls.includes('is-visible'), `wheel did not reveal: ${JSON.stringify(state)}`)

// 4. Moving the pointer away must not cut the scroll-revealed bar to 180ms.
await mouse('mouseMoved', 10, 300)
await sleep(500)
state = await ev(`({cls:document.querySelector('.yb-scrollbar').className})`)
assert(state.cls.includes('is-visible'), `scroll-revealed bar vanished too early: ${JSON.stringify(state)}`)
await sleep(600)
state = await ev(`({cls:document.querySelector('.yb-scrollbar').className, log:window.__scrollFlashLog.slice()})`)
assert(!state.cls.includes('is-visible'), `bar did not auto-hide: ${JSON.stringify(state)}`)

console.log(JSON.stringify({ ok: true, tree: { collapsed, expanded }, doc: { scrollHeight: doc.scrollHeight, clientHeight: doc.clientHeight }, final: state }, null, 2))
ws.close()
