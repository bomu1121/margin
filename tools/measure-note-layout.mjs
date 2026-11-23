// Measures note layout at 1440 and 1024 for the concise-template notes.
// usage: node tools/measure-note-layout.mjs
// The app must be running with CDP on YEBian_LAYOUT_PORT (default 9223).

import fs from 'node:fs'
import path from 'node:path'
import { connectCdp } from './lib/cdp.mjs'

const port = Number(process.env.YEBian_LAYOUT_PORT || 9223)
const keys = ['liubei', 'gongdou', 'ide', 'byd', 'apec', 'mulao', 'datacenter', 'gaokao']
const cdp = await connectCdp(port)

async function measure(width, height) {
  await cdp.send('Emulation.setDeviceMetricsOverride', {
    width,
    height,
    deviceScaleFactor: 1,
    mobile: false,
  })
  await new Promise((resolve) => setTimeout(resolve, 250))
  const rows = []
  for (const key of keys) {
    await cdp.evaluate(`location.hash = '#/note/lab-c-${key}'`)
    await new Promise((resolve) => setTimeout(resolve, 1000))
    const row = await cdp.evaluate(`(() => {
      const doc = document.querySelector('.doc')
      if (!doc) return { error: 'no doc' }
      const h1 = doc.querySelector('h1')
      const h2s = [...doc.querySelectorAll('h2')]
      const prose = [...doc.querySelectorAll('.prose')].filter((node) => node.innerText.trim())
      const frames = [...doc.querySelectorAll('iframe')]
      const lines = (node) => {
        const style = getComputedStyle(node)
        const lineHeight = parseFloat(style.lineHeight) || parseFloat(style.fontSize) * 1.5
        return Math.max(1, Math.round(node.getBoundingClientRect().height / lineHeight))
      }
      const stats = prose.map((node) => ({
        chars: node.innerText.trim().length,
        lines: lines(node),
        height: Math.round(node.getBoundingClientRect().height),
      }))
      const conclusion = stats[0] || { chars: 0, lines: 0, height: 0 }
      const h2Lines = h2s.map(lines)
      const h2Chars = h2s.map((node) => node.innerText.trim().length)
      const sum = (values) => values.reduce((total, value) => total + value, 0)
      return {
        viewport: innerWidth,
        docWidth: doc.clientWidth,
        contentWidth: Math.round(prose[0]?.getBoundingClientRect().width || 0),
        docHeight: doc.scrollHeight,
        bodyOverflow: document.documentElement.scrollWidth > innerWidth,
        h1: h1 ? { chars: h1.innerText.trim().length, lines: lines(h1) } : null,
        h2: {
          count: h2s.length,
          maxChars: h2Chars.length ? Math.max(...h2Chars) : 0,
          lines: h2Lines,
          multiLine: h2Lines.filter((value) => value > 1).length,
        },
        prose: {
          count: stats.length,
          totalChars: sum(stats.map((item) => item.chars)),
          avgChars: Math.round(sum(stats.map((item) => item.chars)) / Math.max(1, stats.length)),
          maxChars: Math.max(0, ...stats.map((item) => item.chars)),
          avgLines: Number((sum(stats.map((item) => item.lines)) / Math.max(1, stats.length)).toFixed(1)),
          maxLines: Math.max(0, ...stats.map((item) => item.lines)),
          over6Lines: stats.filter((item) => item.lines > 6).length,
        },
        conclusion,
        frames: {
          count: frames.length,
          maxWidth: Math.max(0, ...frames.map((node) => Math.round(node.getBoundingClientRect().width))),
          maxHeight: Math.max(0, ...frames.map((node) => Math.round(node.getBoundingClientRect().height))),
          overflow: frames.some((node) => node.getBoundingClientRect().right > doc.getBoundingClientRect().right + 1),
        },
        lists: doc.querySelectorAll('ul,ol').length,
        tables: doc.querySelectorAll('table').length,
      }
    })()`)
    rows.push({ key, ...row })
  }
  return rows
}

const wide = await measure(1440, 1000)
const narrow = await measure(1024, 900)
await cdp.send('Emulation.clearDeviceMetricsOverride')
const out = path.resolve('docs/experiments/template-lab/layout-metrics.json')
fs.writeFileSync(out, JSON.stringify({ measuredAt: new Date().toISOString(), wide, narrow }, null, 2))
for (const row of wide) console.log(JSON.stringify({ width: '1440', ...row }))
for (const row of narrow) console.log(JSON.stringify({ width: '1024', ...row }))
console.log('wrote', out)
cdp.close()
