// Regression: the fixed overlay scrollbar must not float over the relation
// graph dialog. Repro: scroll the note/sidebar so the custom `.yb-scrollbar`
// is visible, then open the full-screen relation graph.
//
// Prereq: app running with --remote-debugging-port=9222 and a note that has a
// relation graph (the current note is fine).
import { connectCdp } from './lib/cdp.mjs'

const cdp = await connectCdp()
const ev = cdp.evaluate
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms))
const assert = (condition, message) => { if (!condition) throw new Error(message) }

try {
  const links = await ev(`(() => { const b=[...document.querySelectorAll('button')].find(x=>x.textContent.trim()==='关联'); if(!b) return false; b.click(); return true })()`)
  assert(links, '关联 tab not found')
  await sleep(500)
  assert(await ev(`Boolean(document.querySelector('.graph-expand'))`), 'graph expand button not found')
  await ev(`document.querySelector('.graph-expand').click()`)
  await sleep(500)
  assert(await ev(`Boolean(document.querySelector('.graph-overlay'))`), 'graph overlay did not open')

  // Force the stale bar into the state it would have after a sidebar/doc scroll.
  const forced = await ev(`(() => {
    const b = document.querySelector('.yb-scrollbar');
    b.classList.add('is-visible');
    return { opacity: getComputedStyle(b).opacity, cls: b.className };
  })()`)
  assert(forced.opacity === '0', `custom scrollbar is visible over graph: ${JSON.stringify(forced)}`)

  // Moving near the old sidebar edge must not resurrect it.
  await ev(`window.dispatchEvent(new PointerEvent('pointermove', { clientX: 315, clientY: 400, bubbles: true }))`)
  await sleep(220)
  const moved = await ev(`(() => { const b=document.querySelector('.yb-scrollbar'); return { opacity:getComputedStyle(b).opacity, cls:b.className }; })()`)
  assert(moved.opacity === '0' && !moved.cls.includes('is-visible'), `scrollbar reappeared over graph: ${JSON.stringify(moved)}`)

  await ev(`document.querySelector('.graph-close').click()`)
  await sleep(300)
  const closed = await ev(`(() => { const b=document.querySelector('.yb-scrollbar'); return { opacity:getComputedStyle(b).opacity, cls:b.className }; })()`)
  assert(!closed.cls.includes('is-visible'), `scrollbar state leaked after close: ${JSON.stringify(closed)}`)

  console.log(JSON.stringify({ ok: true, forced, moved, closed }, null, 2))
} finally {
  cdp.close()
}
