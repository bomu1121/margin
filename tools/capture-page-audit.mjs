// Full functional-page audit: screenshots + geometry/count metrics.
// usage: node --no-warnings tools/capture-page-audit.mjs
//
// Prereq: the app is running with CDP on 9222. It navigates read-only routes,
// opens the facts/sources detail pages, and switches the production sub-views;
// it does not start a production, save settings or edit notes.
//
// Writes shots/audit/150-*.png ... and docs/page-audit-2026-11-16.json.

import fs from 'node:fs'
import path from 'node:path'

const shotsDir = path.resolve('shots/audit')
const idOffset = Number(process.argv[2] || 0)
const metricsPath = path.resolve(process.argv[3] || 'docs/page-audit-2026-11-16.json')
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms))
const pageId = (id) => String(Number(id) + idOffset).padStart(3, '0')

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
async function waitFor(expression, label, timeout = 15000) {
  const start = Date.now()
  while (Date.now() - start < timeout) {
    try {
      if (await evaluate(expression)) return true
    } catch {
      // context can briefly disappear during route changes
    }
    await sleep(120)
  }
  throw new Error(`timeout waiting for ${label}`)
}
async function top() {
  await evaluate(`(() => {
    for (const el of document.querySelectorAll('*')) if (el.scrollTop > 0) el.scrollTop = 0
  })()`)
  await sleep(180)
}
async function goto(hash, waitExpr, label, settle = 800) {
  await evaluate(`location.hash = ${JSON.stringify(hash)}`)
  await waitFor(waitExpr, label)
  await sleep(settle)
  await top()
}
async function shot(name) {
  const result = await send('Page.captureScreenshot', { format: 'png' })
  fs.mkdirSync(shotsDir, { recursive: true })
  const target = path.join(shotsDir, name)
  fs.writeFileSync(target, Buffer.from(result.data, 'base64'))
  console.log('wrote', name)
}

const commonMetrics = `(() => {
  const q = (s) => document.querySelector(s)
  const rect = (s) => { const e = q(s); if (!e) return null; const r = e.getBoundingClientRect(); return { x: Math.round(r.x), y: Math.round(r.y), w: Math.round(r.width), h: Math.round(r.height) } }
  const style = (s, props) => { const e = q(s); if (!e) return null; const c = getComputedStyle(e); return Object.fromEntries(props.map((p) => [p, c.getPropertyValue(p)])) }
  const body = document.body
  const tb = q('.topbar')
  return {
    hash: location.hash,
    bodyOverflowX: document.documentElement.scrollWidth > document.documentElement.clientWidth,
    topbarOverflow: tb ? tb.scrollWidth > tb.clientWidth : null,
    h1: q('h1') ? { text: q('h1').innerText, fs: getComputedStyle(q('h1')).fontSize, rect: rect('h1') } : null,
    main: rect('.main'),
    sidebar: rect('.sidebar'),
    rail: rect('.rail'),
    topbar: rect('.topbar'),
    question: null,
  }
})()`

const pageMetrics = {
  note: `(() => {
    const q = (s) => document.querySelector(s)
    const rect = (s) => { const e = q(s); if (!e) return null; const r = e.getBoundingClientRect(); return { w: Math.round(r.width), h: Math.round(r.height) } }
    const c = q('.note-title'); const p = q('.sec > .unit .prose')
    return {
      title: c?.innerText.replace(/\\n/g, ' '),
      doc: rect('.doc'), prose: p ? { rect: rect('.sec > .unit .prose'), fs: getComputedStyle(p).fontSize, lh: getComputedStyle(p).lineHeight } : null,
      titleFs: c ? getComputedStyle(c).fontSize : null,
      byline: q('.note-byline')?.innerText.replace(/\\n/g, ' · '),
      railTab: q('.rail-tabs .is-active')?.innerText.trim(),
      facts: document.querySelectorAll('.rail-fact-row').length,
      annotations: document.querySelectorAll('.rail-anno-card').length,
      takes: document.querySelectorAll('.take').length,
      sections: document.querySelectorAll('.sec').length,
      newDots: document.querySelectorAll('.tree-new').length,
    }
  })()`,
  graph: `(() => {
    const q = (s) => document.querySelector(s)
    const rect = (s) => { const e = q(s); if (!e) return null; const r = e.getBoundingClientRect(); return { w: Math.round(r.width), h: Math.round(r.height) } }
    return {
      view: rect('.force-view'), stage: rect('.force-stage'),
      labels: document.querySelectorAll('.fv-label').length,
      notes: document.querySelectorAll('.fv-note').length,
      legend: q('.force-legend')?.innerText.replace(/\\n/g, ' | '),
      hud: q('.force-toolbar')?.innerText.replace(/\\n/g, ' | '),
      smallestLabel: [...document.querySelectorAll('.fv-label')].map((e) => parseFloat(getComputedStyle(e).fontSize)).sort((a, b) => a - b)[0] ?? null,
    }
  })()`,
  tags: `(() => {
    const q = (s) => document.querySelector(s)
    const box = (s) => { const e = q(s); if (!e) return null; const r = e.getBoundingClientRect(); return { w: Math.round(r.width), h: Math.round(r.height) } }
    return {
      board: box('.tag-board'), main: box('.tag-main'),
      boardTitle: q('.tag-board-title')?.innerText.replace(/\\n/g, ' | '),
      groups: document.querySelectorAll('.tag-group').length,
      tagRows: document.querySelectorAll('.tag-row').length,
      personCollapsed: [...document.querySelectorAll('.tag-group-head')].find((e) => e.innerText.startsWith('人物'))?.getAttribute('aria-expanded'),
      resultRows: document.querySelectorAll('.result-row').length,
      resultHeight: box('.result-row'),
      selected: q('.querybar')?.innerText.replace(/\\n/g, ' | '),
      tagChips: document.querySelectorAll('.result-tag').length,
      summary: q('.tag-summary')?.innerText.replace(/\\n/g, ' '),
    }
  })()`,
  facts: `(() => {
    const q = (s) => document.querySelector(s)
    const box = (s) => { const e = q(s); if (!e) return null; const r = e.getBoundingClientRect(); return { w: Math.round(r.width), h: Math.round(r.height) } }
    return {
      page: box('.facts-page'), head: q('.facts-head')?.innerText.replace(/\\n/g, ' | '),
      scope: q('.facts-scope')?.innerText.replace(/\\n/g, ' | '),
      rows: document.querySelectorAll('.fact-index-row').length,
      sections: [...document.querySelectorAll('.facts-group, .facts-section, section')].map((e) => e.querySelector('h2')?.innerText).filter(Boolean),
      merges: document.querySelectorAll('.fact-merge-row').length,
    }
  })()`,
  curation: `(() => {
    const q = (s) => document.querySelector(s)
    const box = (s) => { const e = q(s); if (!e) return null; const r = e.getBoundingClientRect(); return { w: Math.round(r.width), h: Math.round(r.height) } }
    return {
      page: box('.curation-page'), head: q('.curation-head, .prod-page-head')?.innerText.replace(/\\n/g, ' | '),
      sections: [...document.querySelectorAll('.curation-section')].map((e) => ({ title: e.querySelector('h2')?.innerText, count: e.querySelectorAll('.curation-card, article, .curation-row').length })),
      cards: document.querySelectorAll('.curation-card').length,
      pairs: document.querySelectorAll('.curation-pair').length,
    }
  })()`,
  factDetail: `(() => {
    const q = (s) => document.querySelector(s)
    const box = (s) => { const e = q(s); if (!e) return null; const r = e.getBoundingClientRect(); return { w: Math.round(r.width), h: Math.round(r.height) } }
    return {
      page: box('.fact-detail-page'), main: box('.fact-detail-main'),
      kicker: q('.fact-detail-kicker')?.innerText,
      title: q('.fact-detail-title-row')?.innerText.slice(0, 120).replace(/\\n/g, ' '),
      status: q('.fact-detail-statusline')?.innerText.replace(/\\n/g, ' | '),
      sections: [...document.querySelectorAll('.fact-detail-section h2')].map((e) => e.innerText),
      sources: document.querySelectorAll('.fact-detail-citation, .fact-detail-source').length,
      evidence: document.querySelectorAll('.fact-detail-evidence-row').length,
      candidates: document.querySelectorAll('.fact-detail-candidate, .fact-candidate-row').length,
    }
  })()`,
  sources: `(() => {
    const q = (s) => document.querySelector(s)
    const box = (s) => { const e = q(s); if (!e) return null; const r = e.getBoundingClientRect(); return { w: Math.round(r.width), h: Math.round(r.height) } }
    return {
      page: box('.sources-page'), head: q('.sources-head')?.innerText.replace(/\\n/g, ' | '),
      rows: document.querySelectorAll('.source-index-row').length,
      filters: q('.sources-filters')?.innerText.replace(/\\n/g, ' | '),
      guide: q('.sources-guide')?.innerText.slice(0, 160).replace(/\\n/g, ' | '),
    }
  })()`,
  sourceDetail: `(() => {
    const q = (s) => document.querySelector(s)
    const box = (s) => { const e = q(s); if (!e) return null; const r = e.getBoundingClientRect(); return { w: Math.round(r.width), h: Math.round(r.height) } }
    return {
      page: box('.source-detail-page'), title: q('.source-detail-head h1, .source-detail-head h2')?.innerText,
      byline: q('.source-detail-byline')?.innerText.replace(/\\n/g, ' | '),
      sections: [...document.querySelectorAll('.source-detail-section h2, .source-detail-section h3')].map((e) => e.innerText),
      passages: document.querySelectorAll('.source-passage, .source-detail-passage').length,
      citations: document.querySelectorAll('.source-citation, .source-detail-citation').length,
    }
  })()`,
  pending: `(() => {
    const q = (s) => document.querySelector(s)
    const box = (s) => { const e = q(s); if (!e) return null; const r = e.getBoundingClientRect(); return { w: Math.round(r.width), h: Math.round(r.height) } }
    return {
      page: box('.source-queue-page'), head: q('.source-queue-head, .prod-page-head')?.innerText.replace(/\\n/g, ' | '),
      groups: document.querySelectorAll('.source-queue-group, .source-queue-fact').length,
      candidates: document.querySelectorAll('.source-queue-candidate, .source-queue-row').length,
      filters: q('.source-queue-filters, .prod-filterbar')?.innerText.replace(/\\n/g, ' | '),
    }
  })()`,
  production: `(() => {
    const q = (s) => document.querySelector(s)
    const box = (s) => { const e = q(s); if (!e) return null; const r = e.getBoundingClientRect(); return { w: Math.round(r.width), h: Math.round(r.height) } }
    return {
      layout: box('.prod-layout'), main: box('.prod-main'), rail: box('.prod-rail'),
      view: q('.prod-layout')?.__vueParentComponent?.setupState?.productionUi?.view,
      head: q('.prod-main h1')?.innerText,
      nav: q('.prod-nav, .studio-nav, .side-nav')?.innerText.replace(/\\n/g, ' | '),
      templates: document.querySelectorAll('.prod-template').length,
      tasks: document.querySelectorAll('.prod-task-row, .prod-table-row').length,
      prompts: document.querySelectorAll('.prod-prompt-row').length,
      settings: document.querySelectorAll('.prod-settings-group, .prod-setting-row, .prod-field').length,
    }
  })()`,
}

const pages = []
async function record({ id, label, hash, waitExpr, settle = 800, metrics = 'common' }) {
  await goto(hash, waitExpr, label, settle)
  const common = await evaluate(commonMetrics)
  const specific = pageMetrics[metrics] ? await evaluate(pageMetrics[metrics]) : null
  const name = `${pageId(id)}-${label}.png`
  await shot(name)
  pages.push({ id: pageId(id), label, hash: common.hash, shot: name, common, specific })
}

await send('Page.enable')
await send('Runtime.enable')
await send('Emulation.setDeviceMetricsOverride', { width: 1440, height: 900, deviceScaleFactor: 1, mobile: false })
await sleep(500)

// 1. Note page with facts in the rail.
await goto('#/note/yn-3306fa8a6a82ae9abc406f72', `!!document.querySelector('.note-title')`, 'note')
let noteTitle = await evaluate(`document.querySelector('.note-title')?.innerText || ''`)
if (!noteTitle.includes('假如华佗')) {
  await goto('#/note', `!!document.querySelector('.tree-body .note-row')`, 'note list')
  const clicked = await evaluate(`(() => {
    const row = [...document.querySelectorAll('.tree-body .note-row')].find((el) => el.innerText.includes('假如华佗'))
    row?.click(); return !!row
  })()`)
  if (clicked) await waitFor(`!!document.querySelector('.note-title')`, 'note by row')
  await sleep(700)
}
await evaluate(`(() => { const tab = [...document.querySelectorAll('.rail-tabs button')].find((b) => b.innerText.startsWith('事实')); tab?.click(); return true })()`)
await sleep(400)
await top()
const noteHash = await evaluate('location.hash')
await record({ id: '150', label: 'note', hash: noteHash, waitExpr: `!!document.querySelector('.note-title')`, metrics: 'note' })

// 2. Graph.
await record({ id: '151', label: 'graph', hash: '#/graph', waitExpr: `!!document.querySelector('.force-view')`, settle: 1800, metrics: 'graph' })

// 3. Tags.
await record({ id: '152', label: 'tags', hash: '#/tags', waitExpr: `!!document.querySelector('.tags-view')`, metrics: 'tags' })

// 4. Facts library + curation.
await record({ id: '153', label: 'facts', hash: '#/facts', waitExpr: `!!document.querySelector('.facts-page')`, metrics: 'facts' })
await record({ id: '154', label: 'facts-curate', hash: '#/facts/curate', waitExpr: `!!document.querySelector('.curation-page')`, metrics: 'curation' })

// 5. Fact detail (history variant; fall back to the first row).
await goto('#/facts', `!!document.querySelector('.facts-page')`, 'facts for detail')
const firstFactClicked = await evaluate(`(() => {
  const first = document.querySelector('.fact-index-row')
  first?.click(); return !!first
})()`)
if (firstFactClicked) {
  await waitFor(`!!document.querySelector('.fact-detail-page')`, 'fact detail by row')
  await sleep(700)
  await top()
  await record({ id: '155', label: 'fact-detail-1', hash: await evaluate('location.hash'), waitExpr: `!!document.querySelector('.fact-detail-page')`, metrics: 'factDetail' })
} else {
  await record({ id: '155', label: 'fact-detail-1', hash: '#/facts/fact_cc798cf03ac6efb10e3e7ef2', waitExpr: `!!document.querySelector('.fact-detail-page')`, metrics: 'factDetail' })
}
// General fact variant if the known id still exists.
await record({ id: '156', label: 'fact-detail-general', hash: '#/facts/fact_45555c8f1c2c0dfa82e4c062', waitExpr: `!!document.querySelector('.fact-detail-page')`, metrics: 'factDetail' })

// 6. Sources + detail + queue.
await record({ id: '157', label: 'sources', hash: '#/sources', waitExpr: `!!document.querySelector('.sources-page')`, metrics: 'sources' })
await goto('#/sources', `!!document.querySelector('.sources-page')`, 'sources for detail')
const sourceClicked = await evaluate(`(() => { const first = document.querySelector('.source-index-row'); first?.click(); return !!first })()`)
if (sourceClicked) {
  await waitFor(`location.hash.startsWith('#/sources/') && !!document.querySelector('.source-detail-page')`, 'source detail by row')
  await sleep(700)
  await top()
  await record({ id: '158', label: 'source-detail', hash: await evaluate('location.hash'), waitExpr: `!!document.querySelector('.source-detail-page')`, metrics: 'sourceDetail' })
} else {
  await record({ id: '158', label: 'source-detail', hash: '#/sources/src_b66505a8fdcb4d788819fcb475f2626d', waitExpr: `!!document.querySelector('.source-detail-page')`, metrics: 'sourceDetail' })
}
await record({ id: '159', label: 'sources-pending', hash: '#/sources/pending', waitExpr: `!!document.querySelector('.source-queue-page')`, metrics: 'pending' })

// 7. Production sub-views (the /studio route keeps its view in the store).
await goto('#/studio', `!!document.querySelector('.prod-layout')`, 'production', 1000)
const productionViews = [
  ['160', 'production-home', 'home'],
  ['161', 'production-tasks', 'tasks'],
  ['162', 'production-templates', 'templates'],
  ['163', 'production-prompts', 'prompts'],
  ['164', 'production-subscriptions', 'subscriptions'],
  ['165', 'production-settings', 'settings'],
]
for (const [id, label, view] of productionViews) {
  await evaluate(`(() => {
    const root = document.querySelector('.prod-layout')
    const state = root?.__vueParentComponent?.setupState
    if (state?.productionUi) state.productionUi.view = ${JSON.stringify(view)}
    return true
  })()`)
  await sleep(view === 'subscriptions' || view === 'settings' ? 900 : 500)
  await top()
  const common = await evaluate(commonMetrics)
  const specific = await evaluate(pageMetrics.production)
  const name = `${pageId(id)}-${label}.png`
  await shot(name)
  pages.push({ id: pageId(id), label, hash: common.hash, shot: name, common, specific })
}
// Leave the app on the note page the user was working on, and restore the window metrics.
await evaluate(`location.hash = '#/note/yn-550bccb2ec126c964672319e'`)
await sleep(600)
await send('Emulation.clearDeviceMetricsOverride')

fs.mkdirSync(path.dirname(metricsPath), { recursive: true })
fs.writeFileSync(metricsPath, JSON.stringify({ generatedAt: new Date().toISOString(), viewport: [1440, 900, 1], pages }, null, 2))
console.log('wrote', metricsPath)
ws.close()
