// Audits the production wizard's rendered geometry:
// - fixed circular/grid boxes must have their text optically centered
// - compact bordered labels must not inherit the body's 1.95 line-height
// - no clipped text in non-scrollable boxes
// - the footer must stay inside the viewport with a long favorites list
//
// usage: node tools/audit-wizard.mjs
// Requires the app running with CDP on 127.0.0.1:9222.
// The script opens 制作 → 新建制作, walks 来源/模板/选项/确认, then
// switches to B站收藏 and re-checks at 1280x607, 1040x680 and 820x680.
// It never clicks 开始制作, so it does not create a task.
//
// Writes docs/wizard-audit.json and exits non-zero on findings.

import fs from 'node:fs'
import path from 'node:path'

const list = await (await fetch('http://127.0.0.1:9222/json/list')).json()
const page = list.find((item) => item.type === 'page')
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
  const callId = ++nextId
  ws.send(JSON.stringify({ id: callId, method, params }))
  return new Promise((resolve, reject) => {
    pending.set(callId, (message) => {
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
  if (result.exceptionDetails) {
    throw new Error(result.exceptionDetails.exception?.description ?? 'evaluate failed')
  }
  return result.result.value
}

const wait = (ms) => new Promise((resolve) => setTimeout(resolve, ms))

async function click(selector, text, startsWith = false) {
  return evaluate(`(() => {
    const text = ${JSON.stringify(text)}
    const el = [...document.querySelectorAll(${JSON.stringify(selector)})].find((item) => {
      const value = item.innerText.trim()
      return ${startsWith ? 'value.startsWith(text)' : 'value === text'}
    })
    if (!el) return false
    el.click()
    return true
  })()`)
}

async function setViewport(width, height) {
  await send('Emulation.setDeviceMetricsOverride', {
    width,
    height,
    deviceScaleFactor: 1,
    mobile: false,
  })
  await wait(350)
}

async function clearViewport() {
  await send('Emulation.clearDeviceMetricsOverride')
}

const AUDIT = String.raw`(() => {
  const TOL = 0.75
  const wizard = document.querySelector('.prod-wizard')
  if (!wizard) return { error: 'wizard not open' }
  const failures = []
  const centerChecks = []
  const badges = []
  const overflow = []
  const box = (el) => {
    const r = el.getBoundingClientRect()
    return {
      left: +r.left.toFixed(1), top: +r.top.toFixed(1),
      right: +r.right.toFixed(1), bottom: +r.bottom.toFixed(1),
      width: +r.width.toFixed(1), height: +r.height.toFixed(1),
    }
  }
  const textDelta = (el) => {
    const range = document.createRange()
    range.selectNodeContents(el)
    const rects = [...range.getClientRects()].filter((r) => r.width > 0 || r.height > 0)
    if (rects.length !== 1) return null
    const glyph = rects[0]
    const r = el.getBoundingClientRect()
    const s = getComputedStyle(el)
    const top = r.top + parseFloat(s.borderTopWidth) + parseFloat(s.paddingTop)
    const bottom = r.bottom - parseFloat(s.borderBottomWidth) - parseFloat(s.paddingBottom)
    return +((glyph.top + glyph.height / 2) - (top + bottom) / 2).toFixed(2)
  }
  const addCenter = (kind, el, delta, tolerance = TOL) => {
    const entry = { kind, text: el.textContent.trim().slice(0, 24), delta }
    centerChecks.push(entry)
    if (delta !== null && Math.abs(delta) > tolerance) {
      failures.push({ kind: 'center', ...entry, tolerance })
    }
  }

  const dialog = box(wizard)
  const footerEl = wizard.querySelector('.prod-wizard-foot')
  const footer = footerEl ? box(footerEl) : null
  const bodyEl = wizard.querySelector('.prod-wizard-body')
  const body = bodyEl ? {
    clientWidth: bodyEl.clientWidth, scrollWidth: bodyEl.scrollWidth,
    clientHeight: bodyEl.clientHeight, scrollHeight: bodyEl.scrollHeight,
    scrollTop: bodyEl.scrollTop,
  } : null
  if (footer && (footer.top < -0.5 || footer.bottom > innerHeight + 0.5)) {
    failures.push({ kind: 'footer-outside', footer, viewportHeight: innerHeight })
  }
  if (dialog.left < -0.5 || dialog.right > innerWidth + 0.5) {
    failures.push({ kind: 'dialog-outside', dialog, viewportWidth: innerWidth })
  }

  // Every fixed-height, grid-centered text box must be optically centered.
  // This covers the stepper digits, the parsed-video placeholder and the
  // task-detail step marks when they happen to be in the DOM.
  for (const el of wizard.querySelectorAll('*')) {
    if (!el.offsetParent) continue
    const s = getComputedStyle(el)
    if (s.display !== 'grid' || s.alignItems !== 'center' || s.justifyItems !== 'center') continue
    if (el.children.length || !el.textContent.trim()) continue
    const r = el.getBoundingClientRect()
    if (r.width > 400 || r.height > 60) continue
    addCenter(el.classList.contains('prod-wizard-step') ? 'step-number' : 'grid-text', el, textDelta(el))
  }

  // Icon boxes and icon+label rows.
  for (const el of wizard.querySelectorAll('.prod-icon-btn, .prod-template-icon')) {
    const icon = el.querySelector('svg')
    if (!icon) continue
    const r = el.getBoundingClientRect()
    const ir = icon.getBoundingClientRect()
    const delta = +(((ir.top + ir.height / 2) - (r.top + r.height / 2))).toFixed(2)
    centerChecks.push({ kind: 'icon', text: '', delta })
    if (Math.abs(delta) > TOL) failures.push({ kind: 'center', kindDetail: 'icon', delta })
  }

  // Small bordered labels must not inherit the 1.95 body line-height.
  for (const el of wizard.querySelectorAll('.prod-badge')) {
    const r = el.getBoundingClientRect()
    const s = getComputedStyle(el)
    const height = +r.height.toFixed(1)
    const lineHeight = parseFloat(s.lineHeight)
    const fontSize = parseFloat(s.fontSize)
    const delta = textDelta(el)
    badges.push({ text: el.textContent.trim(), height, lineHeight, fontSize, delta })
    if (height < 18 || height > 24) failures.push({ kind: 'badge-height', text: el.textContent.trim(), height })
    if (lineHeight > fontSize * 1.55) failures.push({ kind: 'badge-line-height', text: el.textContent.trim(), lineHeight })
    if (delta !== null && Math.abs(delta) > 1) failures.push({ kind: 'badge-center', text: el.textContent.trim(), delta })
  }

  // No clipped text in boxes that are not explicitly scrollable.
  for (const el of wizard.querySelectorAll('*')) {
    if (!el.offsetParent) continue
    if (['INPUT', 'TEXTAREA', 'SELECT'].includes(el.tagName)) continue
    const s = getComputedStyle(el)
    const r = el.getBoundingClientRect()
    if (r.width <= 0 || r.height <= 0) continue
    if (el.scrollWidth > el.clientWidth + 1 && !['auto', 'scroll'].includes(s.overflowX) && s.textOverflow !== 'ellipsis') {
      overflow.push({ axis: 'x', tag: el.tagName, cls: el.className.toString().slice(0, 50), text: el.textContent.trim().slice(0, 30), client: el.clientWidth, scroll: el.scrollWidth })
    }
    if (el.scrollHeight > el.clientHeight + 1 && !['auto', 'scroll'].includes(s.overflowY) && s.webkitLineClamp === 'none') {
      overflow.push({ axis: 'y', tag: el.tagName, cls: el.className.toString().slice(0, 50), text: el.textContent.trim().slice(0, 30), client: el.clientHeight, scroll: el.scrollHeight })
    }
  }
  if (overflow.length) failures.push({ kind: 'overflow', items: overflow })

  // Synthetic checks cover fixed boxes that are not always on screen
  // (task step marks, parsed-video placeholder, compact badges).
  const syntheticChecks = []
  const probe = document.createElement('div')
  probe.style.cssText = 'position:fixed;left:-10000px;top:0;width:860px;display:flex;flex-direction:column;gap:8px;pointer-events:none'
  probe.innerHTML =
    '<span class="prod-wizard-step" style="width:200px"><i>1</i>来源</span>' +
    '<span class="prod-step-mark">5</span>' +
    '<span class="prod-badge">基础</span>' +
    '<span class="prod-auto-badge">自动</span>' +
    '<span class="prod-engine-pill">本地</span>' +
    '<span class="prod-parsed-thumb">B站</span>' +
    '<span class="prod-account-avatar">薄</span>'
  document.body.appendChild(probe)
  try {
    const targets = [
      ['wizard-step-digit', probe.querySelector('.prod-wizard-step i'), 0.75],
      ['task-step-mark', probe.querySelector('.prod-step-mark'), 0.75],
      ['badge', probe.querySelector('.prod-badge'), 1],
      ['auto-badge', probe.querySelector('.prod-auto-badge'), 1],
      ['engine-pill', probe.querySelector('.prod-engine-pill'), 1],
      ['parsed-thumb', probe.querySelector('.prod-parsed-thumb'), 0.75],
      ['account-avatar', probe.querySelector('.prod-account-avatar'), 0.75],
    ]
    for (const [kind, el, tolerance] of targets) {
      if (!el) {
        failures.push({ kind: 'synthetic-missing', target: kind })
        continue
      }
      const delta = textDelta(el)
      const r = el.getBoundingClientRect()
      const s = getComputedStyle(el)
      syntheticChecks.push({ kind, height: +r.height.toFixed(1), lineHeight: parseFloat(s.lineHeight), delta })
      if (delta !== null && Math.abs(delta) > tolerance) {
        failures.push({ kind: 'synthetic-center', target: kind, delta, tolerance })
      }
      if (['badge', 'auto-badge', 'engine-pill'].includes(kind)) {
        if (r.height < 18 || r.height > 24) failures.push({ kind: 'synthetic-height', target: kind, height: +r.height.toFixed(1) })
        if (parseFloat(s.lineHeight) > parseFloat(s.fontSize) * 1.55) failures.push({ kind: 'synthetic-line-height', target: kind, lineHeight: s.lineHeight })
      }
    }
  } finally {
    probe.remove()
  }

  return {
    viewport: { width: innerWidth, height: innerHeight },
    dialog,
    footer,
    body,
    centerChecks,
    badges,
    syntheticChecks,
    overflow,
    failures,
  }
})()`

const reports = {}

async function audit(label) {
  const report = await evaluate(AUDIT)
  reports[label] = report
  if (report.error) throw new Error(`${label}: ${report.error}`)
  return report
}

function assert(ok, message) {
  if (!ok) throw new Error(message)
}

// Close any dialog that was left open by a previous run.
await evaluate(`(() => {
  for (const el of document.querySelectorAll('.prod-dialog .prod-icon-btn[aria-label="关闭"]')) el.click()
  return true
})()`)
await wait(250)

// Make sure we are on the production page.
await click('button, a, [role="button"]', '制作')
await wait(500)
assert(await click('button', '新建制作'), 'could not open the production wizard')
await wait(500)

try {
  await setViewport(1280, 607)
  await audit('step1-bili-empty')

  assert(await click('.prod-source-option', '粘贴文稿'), 'could not choose the text source')
  await wait(250)
  await evaluate(`(() => {
    const el = document.querySelector('.prod-wizard textarea.prod-textarea')
    if (!el) return false
    const setter = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value').set
    setter.call(el, '向导布局审计用文稿。')
    el.dispatchEvent(new Event('input', { bubbles: true }))
    return true
  })()`)
  await wait(200)
  assert(await click('.prod-wizard button', '下一步', true), 'could not leave source step')
  await wait(400)
  await audit('step2-templates')

  assert(await click('.prod-wizard button', '下一步', true), 'could not leave template step')
  await wait(350)
  await audit('step3-options')

  assert(await click('.prod-wizard button', '下一步', true), 'could not leave options step')
  await wait(350)
  await audit('step4-confirm')

  for (let i = 0; i < 3; i += 1) {
    assert(await click('.prod-wizard button', '上一步'), 'could not go back to source step')
    await wait(200)
  }
  assert(await click('.prod-source-option', 'B站收藏'), 'could not choose the favorites source')
  await wait(2200)
  await audit('favorites-1280x607')

  await setViewport(1040, 680)
  await audit('favorites-1040x680')

  await setViewport(820, 680)
  await audit('favorites-820x680')
} finally {
  try {
    await clearViewport()
  } catch {}
  try {
    await evaluate(`(() => {
      window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
      return true
    })()`)
    await wait(250)
  } catch {}
}

ws.close()

const all = Object.entries(reports)
const failureCount = all.reduce((sum, [, report]) => sum + report.failures.length, 0)
const summary = Object.fromEntries(all.map(([label, report]) => [label, {
  viewport: report.viewport,
  footer: report.footer,
  body: report.body,
  centers: report.centerChecks,
  badges: report.badges,
  synthetic: report.syntheticChecks,
  failures: report.failures,
}]))
const output = {
  generatedAt: new Date().toISOString(),
  tolerancePx: 0.75,
  failureCount,
  reports: summary,
}
const target = path.resolve('docs/wizard-audit.json')
fs.mkdirSync(path.dirname(target), { recursive: true })
fs.writeFileSync(target, `${JSON.stringify(output, null, 2)}\n`)
console.log(JSON.stringify({ failureCount, target, labels: all.map(([label]) => label) }, null, 2))
if (failureCount > 0) process.exit(1)
