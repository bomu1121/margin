// Regression: the sidebar context menu must escape the tree scroll container
// and stay inside the viewport when opened near the right/bottom edge.
// Prereq: app running with --remote-debugging-port=9222.
// usage: node --no-warnings tools/verify-context-menu.mjs
import { connectCdp } from './lib/cdp.mjs'

const cdp = await connectCdp()
const expression = `(async () => {
  const row = [...document.querySelectorAll('.note-row')].find((element) => {
    const rect = element.getBoundingClientRect()
    return rect.width > 0 && rect.height > 0
  })
  if (!row) return { error: 'no visible note row' }
  const rect = row.getBoundingClientRect()
  row.dispatchEvent(new MouseEvent('contextmenu', {
    bubbles: true,
    cancelable: true,
    clientX: window.innerWidth - 8,
    clientY: Math.min(window.innerHeight - 8, rect.top + 8),
  }))
  await new Promise((resolve) => setTimeout(resolve, 80))
  const menu = document.querySelector('.ctx')
  if (!menu) return { error: 'context menu did not open' }
  const bounds = menu.getBoundingClientRect()
  const result = {
    teleported: menu.parentElement === document.body,
    withinViewport:
      bounds.left >= 0 &&
      bounds.top >= 0 &&
      bounds.right <= window.innerWidth &&
      bounds.bottom <= window.innerHeight,
    left: Math.round(bounds.left),
    top: Math.round(bounds.top),
    right: Math.round(bounds.right),
    bottom: Math.round(bounds.bottom),
    viewport: [window.innerWidth, window.innerHeight],
    items: [...menu.querySelectorAll('button')].map((button) => button.textContent.trim()),
  }
  document.dispatchEvent(new MouseEvent('click', { bubbles: true }))
  return result
})()`

const result = await cdp.evaluate(expression)
cdp.close()

if (result?.error) {
  console.error('FAIL', result.error)
  process.exit(1)
}
const hasItems = result.items.some((item) => item.startsWith('重命名')) && result.items.some((item) => item.startsWith('删除'))
const pass = result.teleported && result.withinViewport && hasItems
console.log(pass ? 'PASS' : 'FAIL', JSON.stringify(result))
if (!pass) process.exit(1)
