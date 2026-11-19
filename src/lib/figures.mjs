// Figure renderer shared by the note reader (Vue) and the booklet build tool.
// Spec lives in a fenced ```figure block:
//
// ```figure
// kind: flow
// title: 图 2 所有渠道先汇到 Router，由它派活
// 渠道 -> Router
// Router -> Worker A
// note: Worker 之间一般不直接沟通。
// ```
//
// Kinds: flow | layers | bar | quote | compare | svg
// Colours come from CSS custom properties so light/dark themes work unchanged.

const CJK = /[\u3000-\u9fff\uff00-\uffef]/

function textWidth(text, size) {
  let width = 0
  for (const ch of String(text)) width += (CJK.test(ch) ? 1 : 0.56) * size
  return width
}

export function parseFigure(raw) {
  const spec = {
    kind: 'flow',
    title: '',
    note: '',
    lines: [],
    left: '',
    right: '',
    leftHeading: '',
    rightHeading: '',
    leftFoot: '',
    rightFoot: '',
    leftItems: [],
    rightItems: [],
  }
  const pair = (value) => {
    const [title, sub] = String(value).split('|').map((item) => item.trim())
    return { title, sub: sub || '' }
  }
  for (const line of String(raw).split('\n')) {
    const trimmed = line.trim()
    if (!trimmed) continue
    const meta = trimmed.match(/^(kind|title|note|left|right|left-foot|right-foot|left-item|right-item)\s*[:：]\s*(.+)$/)
    if (meta) {
      const key = meta[1]
      if (key === 'left-item') spec.leftItems.push(pair(meta[2]))
      else if (key === 'right-item') spec.rightItems.push(pair(meta[2]))
      else if (key === 'left-foot') spec.leftFoot = meta[2].trim()
      else if (key === 'right-foot') spec.rightFoot = meta[2].trim()
      else if (key === 'left' || key === 'right') {
        const [label, heading] = String(meta[2]).split('|').map((item) => item.trim())
        spec[key] = label
        spec[key === 'left' ? 'leftHeading' : 'rightHeading'] = heading || ''
      } else spec[key] = meta[2].trim()
      continue
    }
    spec.lines.push(trimmed.replace(/^[-*]\s+/, ''))
  }
  // Infer the kind from the fields the author actually wrote: a 'flow' block
  // carrying left/right items is a comparison, and would otherwise render empty.
  if (spec.leftItems.length || spec.rightItems.length) spec.kind = 'compare'
  else if (!['flow', 'layers', 'bar', 'quote', 'svg', 'compare'].includes(spec.kind)) spec.kind = 'flow'
  return spec
}

const esc = (value) =>
  String(value)
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')

const FONT = 12.5
const LINE = 1.45

function text(x, y, value, cls = '', anchor = 'middle') {
  return `<text x="${x}" y="${y}" class="${cls}" text-anchor="${anchor}">${esc(value)}</text>`
}

function wrapLabel(label, maxChars) {
  if (label.length <= maxChars) return [label]
  const out = []
  let rest = label
  while (rest.length > maxChars) {
    out.push(rest.slice(0, maxChars))
    rest = rest.slice(maxChars)
  }
  if (rest) out.push(rest)
  return out
}

function renderFlow(spec, width) {
  const edges = []
  for (const line of spec.lines) {
    const parts = line.split(/\s*->\s*/).map((s) => s.trim()).filter(Boolean)
    if (parts.length < 2) continue
    // A -> B -> C is a chain; emit each consecutive pair.
    for (let i = 0; i < parts.length - 1; i += 1) edges.push([parts[i], parts[i + 1]])
  }
  const nodes = []
  const seen = new Set()
  for (const [a, b] of edges) {
    for (const name of [a, b]) {
      if (!seen.has(name)) {
        seen.add(name)
        nodes.push(name)
      }
    }
  }
  if (!nodes.length) return { svg: '', height: 0 }

  // Longest-path levels.
  const level = new Map(nodes.map((n) => [n, 0]))
  for (let round = 0; round < nodes.length; round += 1) {
    let changed = false
    for (const [a, b] of edges) {
      if (level.get(b) <= level.get(a)) {
        level.set(b, level.get(a) + 1)
        changed = true
      }
    }
    if (!changed) break
  }
  const byLevel = new Map()
  for (const node of nodes) {
    const l = level.get(node)
    if (!byLevel.has(l)) byLevel.set(l, [])
    byLevel.get(l).push(node)
  }
  const levels = [...byLevel.keys()].sort((a, b) => a - b)

  const padX = 10
  const nodeH = 48
  const gapY = 22
  const widths = new Map(nodes.map((n) => [n, Math.max(108, Math.round(textWidth(n, FONT) + 58))]))
  const colWidths = levels.map((l) => Math.max(...byLevel.get(l).map((n) => widths.get(n))))
  const totalCols = colWidths.reduce((sum, w) => sum + w, 0)
  const gapX = Math.max(16, Math.min(56, Math.floor((width - padX * 2 - totalCols) / Math.max(1, levels.length - 1))))
  const contentW = totalCols + gapX * (levels.length - 1)
  const offsetX = Math.max(padX, (width - contentW) / 2)

  const pos = new Map()
  let x = offsetX
  const maxColH = Math.max(...levels.map((l) => byLevel.get(l).length * nodeH + (byLevel.get(l).length - 1) * gapY))
  levels.forEach((l, li) => {
    const col = byLevel.get(l)
    const colW = colWidths[li]
    // Barycentre: align a node with the average centre of its predecessors so
    // single-node columns don't need a vertical jog.
    const desired = new Map()
    for (const node of col) {
      const parents = edges.filter(([, b]) => b === node).map(([a]) => a)
      const centres = parents
        .map((p) => pos.get(p))
        .filter(Boolean)
        .map((p) => p.y + p.h / 2)
      desired.set(node, centres.length ? centres.reduce((sum, v) => sum + v, 0) / centres.length : null)
    }
    const ordered = [...col].sort((a, b) => {
      const da = desired.get(a)
      const db = desired.get(b)
      if (da === null && db === null) return col.indexOf(a) - col.indexOf(b)
      if (da === null) return 1
      if (db === null) return -1
      return da - db
    })
    let y = 10
    if (ordered.length === 1 && desired.get(ordered[0]) !== null) {
      y = Math.max(10, desired.get(ordered[0]) - nodeH / 2)
    } else {
      const colH = ordered.length * nodeH + (ordered.length - 1) * gapY
      y = 10 + Math.max(0, (maxColH - colH) / 2)
    }
    for (const node of ordered) {
      if (y + nodeH > 10 + maxColH) y = 10 + maxColH - nodeH
      pos.set(node, { x: x + (colW - widths.get(node)) / 2, y, w: widths.get(node), h: nodeH })
      y += nodeH + gapY
    }
    x += colW + gapX
  })

  const height = maxColH + 28
  const parts = []
  for (const [a, b] of edges) {
    const s = pos.get(a)
    const t = pos.get(b)
    if (!s || !t) continue
    const sx = s.x + s.w
    const sy = s.y + s.h / 2
    const tx = t.x
    const ty = t.y + t.h / 2
    if (Math.abs(ty - sy) < 6) {
      parts.push(`<path class="flow-edge" d="M${sx} ${sy} L${tx - 2} ${ty}" marker-end="url(#arrow)"/>`)
      continue
    }
    // Orthogonal routing through the gap: right stub, vertical bus, arrow into
    // the target. Edges sharing a level gap overlap on the vertical segment,
    // which reads as one bus instead of crossing over other boxes. When the
    // edge skips a level, keep the vertical in the gap just before the target.
    const skips = level.get(b) - level.get(a) > 1
    const midX = skips ? tx - gapX / 2 : sx + Math.max(16, (tx - sx) / 2)
    parts.push(
      `<path class="flow-edge" d="M${sx} ${sy} H${midX} V${ty} H${tx - 4}" marker-end="url(#arrow)"/>`,
    )
  }
  const sources = new Set(edges.map(([a]) => a))
  for (const [index, node] of nodes.entries()) {
    const p = pos.get(node)
    const terminal = !sources.has(node)
    parts.push(`<rect class="flow-node${terminal ? ' is-terminal' : ''}" x="${p.x}" y="${p.y}" width="${p.w}" height="${p.h}" rx="6"/>`)
    parts.push(`<rect class="flow-accent${terminal ? ' is-terminal' : ''}" x="${p.x}" y="${p.y + 12}" width="2.5" height="${p.h - 24}" rx="1.25"/>`)
    parts.push(text(p.x + 15, p.y + 30, String(index + 1).padStart(2, '0'), 'flow-index', 'start'))
    parts.push(text(p.x + 38, p.y + 30, node, 'flow-label', 'start'))
  }
  return { svg: parts.join(''), height }
}

function renderLayers(spec, width) {
  const rows = spec.lines.map((line) => {
    const [label, rest] = line.split(/\s*[:：]\s*/)
    const items = (rest || '').split('|').map((s) => s.trim()).filter(Boolean)
    return { label: label.trim(), items }
  })
  if (!rows.length) return { svg: '', height: 0 }
  const rowH = 58
  const gap = 14
  const labelW = Math.min(132, Math.max(76, Math.round(Math.max(...rows.map((row) => textWidth(row.label, FONT))) + 34)))
  const height = rows.length * rowH + (rows.length - 1) * gap + 12
  const parts = []
  const lastY = 6 + (rows.length - 1) * (rowH + gap) + rowH
  if (rows.length > 1) {
    parts.push(`<line class="layer-spine" x1="1.5" y1="6" x2="1.5" y2="${lastY - 6}"/>`)
  }
  rows.forEach((row, i) => {
    const y = 6 + i * (rowH + gap)
    parts.push(`<rect class="layer-band${i % 2 ? ' is-alt' : ''}" x="0" y="${y}" width="${width}" height="${rowH}" rx="6"/>`)
    parts.push(`<rect class="layer-accent" x="0" y="${y + 6}" width="3" height="${rowH - 12}" rx="1.5"/>`)
    parts.push(text(16, y + rowH / 2 + 4, row.label, 'layer-label', 'start'))
    const innerX = labelW + 20
    const innerW = width - innerX - 12
    const value = row.items.length ? row.items.join(' · ') : '（无）'
    const maxChars = Math.max(8, Math.floor(innerW / (FONT * 1.05)))
    const lines = wrapLabel(value, maxChars).slice(0, 2)
    lines.forEach((line, li) => {
      const offset = (lines.length - 1) * 9
      parts.push(text(innerX, y + rowH / 2 + 4 - offset + li * 18, line, 'layer-value', 'start'))
    })
  })
  return { svg: parts.join(''), height }
}

function renderBar(spec, width) {
  const rows = spec.lines.map((line) => {
    const [label, value, note] = line.split('|').map((s) => s.trim())
    return { label, value: Number(value), display: value, note: note || '' }
  })
  const max = Math.max(1, ...rows.map((r) => (Number.isFinite(r.value) ? r.value : 0)))
  const rowH = 48
  const labelW = Math.min(
    Math.round(width * 0.34),
    Math.max(80, Math.round(Math.max(...rows.map((r) => textWidth(r.label, FONT))) + 20)),
  )
  // Reserve enough for the longest value label: ranges like "2 到近 4" are
  // wider than a single number and used to overflow the viewBox.
  const valueW = Math.max(56, Math.max(...rows.map((row) => textWidth(row.display, FONT))) + 10)
  const height = rows.length * rowH + 10
  const parts = []
  rows.forEach((row, i) => {
    const y = i * rowH + 4
    parts.push(text(2, y + 24, row.label, 'label', 'start'))
    const barX = labelW
    const barMax = Math.max(40, width - labelW - valueW - 8)
    const w = Math.max(6, (barMax * (Number.isFinite(row.value) ? row.value : 0)) / max)
    parts.push(`<rect class="bar-bg" x="${barX}" y="${y + 10}" width="${barMax}" height="16" rx="3"/>`)
    parts.push(`<rect class="bar" x="${barX}" y="${y + 10}" width="${w}" height="16" rx="3"/>`)
    // Value sits in a reserved column so a full-width bar can't push it out.
    parts.push(text(width - valueW + 4, y + 23, row.display, 'value', 'start'))
    if (row.note) parts.push(text(barX, y + 40, row.note, 'muted', 'start'))
  })
  return { svg: parts.join(''), height }
}

function renderQuote(spec, width) {
  const lines = spec.lines.length ? spec.lines : [spec.title]
  const maxChars = Math.max(10, Math.floor((width - 72) / (FONT * 1.6)))
  const wrapped = lines.flatMap((line) => wrapLabel(line, maxChars))
  const height = 40 + wrapped.length * Math.round(FONT * LINE * 1.5)
  const parts = []
  parts.push(`<rect class="band" x="0" y="0" width="${width}" height="${height}" rx="6"/>`)
  wrapped.forEach((line, i) => {
    parts.push(text(24, 30 + i * FONT * LINE * 1.5, line, 'quote', 'start'))
  })
  return { svg: parts.join(''), height }
}

/// Two-panel comparison: kicker + heading, nested cards, side accents and an
/// optional foot line. The panels use the theme's warm/blue accents instead of
/// a dashed "unfinished" frame.
function renderCompare(spec, width) {
  const gap = 48
  const panelW = Math.round((width - gap) / 2)
  const cardH = 58
  const cardGap = 10
  const headPad = 58
  const frameTop = 34
  const panelH = (items) => 18 + headPad + items.length * (cardH + cardGap) - cardGap + 18
  const frameH = Math.max(panelH(spec.leftItems), panelH(spec.rightItems))
  const footY = frameTop + frameH + 28
  const hasFoot = Boolean(spec.leftFoot || spec.rightFoot)
  const height = footY + (hasFoot ? 22 : 0) + 10
  const parts = []
  const drawPanel = (x, label, heading, items, side, foot) => {
    parts.push(`<rect class="compare-panel is-${side}" x="${x}" y="${frameTop}" width="${panelW}" height="${frameH}" rx="6"/>`)
    if (label) parts.push(text(x + 18, 18, label, 'compare-kicker', 'start'))
    if (heading) parts.push(text(x + 18, frameTop + 30, heading, 'compare-heading', 'start'))
    let y = frameTop + headPad
    for (const item of items) {
      const cardX = x + 18
      const cardW = panelW - 36
      parts.push(`<rect class="compare-card" x="${cardX}" y="${y}" width="${cardW}" height="${cardH}" rx="5"/>`)
      parts.push(`<rect class="compare-accent is-${side}" x="${cardX}" y="${y + 10}" width="2.5" height="${cardH - 20}" rx="1.25"/>`)
      parts.push(text(cardX + 15, y + 24, item.title, 'compare-item-title', 'start'))
      if (item.sub) parts.push(text(cardX + 15, y + 44, item.sub, 'compare-item-sub', 'start'))
      y += cardH + cardGap
    }
    if (foot) parts.push(text(x + panelW / 2, footY, foot, `compare-foot is-${side}`, 'middle'))
  }
  drawPanel(0, spec.left, spec.leftHeading, spec.leftItems, 'left', spec.leftFoot)
  drawPanel(panelW + gap, spec.right, spec.rightHeading, spec.rightItems, 'right', spec.rightFoot)
  const cx = panelW + gap / 2
  const cy = frameTop + frameH / 2
  parts.push(`<circle class="compare-vs-bg" cx="${cx}" cy="${cy}" r="20"/>`)
  parts.push(text(cx, cy + 4, 'vs', 'compare-vs'))
  return { svg: parts.join(''), height }
}

export function renderFigure(spec, width = 724) {
  const w = Math.max(320, Math.round(width))
  const caption =
    `<figcaption class="fig-caption">` +
    `<span class="fig-title">${esc(spec.title)}</span>` +
    (spec.note ? `<span class="fig-note">${esc(spec.note)}</span>` : '') +
    `</figcaption>`
  // `kind: svg` is the escape hatch for figures the four templates cannot
  // express (nested cards, dashed containers, annotated arrows). The author
  // writes the SVG markup directly; the reader and the PDF share it.
  if (spec.kind === 'svg') {
    const raw = spec.lines.join('\n').trim()
    if (!raw) return { html: '', caption: spec.title }
    const html =
      `<figure class="fig" data-kind="svg">` +
      `<div class="fig-frame">${raw}</div>` +
      caption +
      `</figure>`
    return { html, caption: spec.title }
  }
  const render = { flow: renderFlow, layers: renderLayers, bar: renderBar, quote: renderQuote, compare: renderCompare }[spec.kind]
  const { svg, height } = render(spec, w)
  if (!svg) return { html: '', caption: spec.title }
  const header = `<defs><marker id="arrow" viewBox="0 0 8 8" refX="7" refY="4" markerWidth="7" markerHeight="7" orient="auto"><path d="M0 0 L8 4 L0 8 z" class="arrow-head"/></marker></defs>`
  const html =
    `<figure class="fig" data-kind="${spec.kind}">` +
    `<div class="fig-frame"><svg viewBox="0 0 ${w} ${Math.round(height)}" width="100%" role="img" aria-label="${esc(spec.title)}">${header}${svg}</svg></div>` +
    caption +
    `</figure>`
  return { html, caption: spec.title }
}
