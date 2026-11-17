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
// Kinds: flow | layers | bar | quote
// Colours come from CSS custom properties so light/dark themes work unchanged.

const CJK = /[\u3000-\u9fff\uff00-\uffef]/

function textWidth(text, size) {
  let width = 0
  for (const ch of String(text)) width += (CJK.test(ch) ? 1 : 0.56) * size
  return width
}

export function parseFigure(raw) {
  const spec = { kind: 'flow', title: '', note: '', lines: [] }
  for (const line of String(raw).split('\n')) {
    const trimmed = line.trim()
    if (!trimmed) continue
    const meta = trimmed.match(/^(kind|title|note)\s*[:：]\s*(.+)$/)
    if (meta) {
      spec[meta[1]] = meta[2].trim()
      continue
    }
    spec.lines.push(trimmed.replace(/^[-*]\s+/, ''))
  }
  if (!['flow', 'layers', 'bar', 'quote'].includes(spec.kind)) spec.kind = 'flow'
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
  const nodeH = 42
  const gapY = 16
  const widths = new Map(nodes.map((n) => [n, Math.max(96, Math.round(textWidth(n, FONT) + 28))]))
  const colWidths = levels.map((l) => Math.max(...byLevel.get(l).map((n) => widths.get(n))))
  const totalCols = colWidths.reduce((sum, w) => sum + w, 0)
  const gapX = Math.max(28, Math.min(72, Math.floor((width - padX * 2 - totalCols) / Math.max(1, levels.length - 1))))
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

  const height = maxColH + 20
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
      parts.push(`<path class="edge" d="M${sx} ${sy} L${tx - 2} ${ty}" marker-end="url(#arrow)"/>`)
      continue
    }
    // Orthogonal routing through the gap: right stub, vertical bus, arrow into
    // the target. Edges sharing a level gap overlap on the vertical segment,
    // which reads as one bus instead of crossing over other boxes. When the
    // edge skips a level, keep the vertical in the gap just before the target.
    const skips = level.get(b) - level.get(a) > 1
    const midX = skips ? tx - gapX / 2 : sx + Math.max(16, (tx - sx) / 2)
    parts.push(
      `<path class="edge" d="M${sx} ${sy} H${midX} V${ty} H${tx - 2}" marker-end="url(#arrow)"/>`,
    )
  }
  for (const node of nodes) {
    const p = pos.get(node)
    parts.push(`<rect class="box" x="${p.x}" y="${p.y}" width="${p.w}" height="${p.h}" rx="4"/>`)
    parts.push(text(p.x + p.w / 2, p.y + p.h / 2 + 4, node, 'label'))
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
  const labelW = Math.min(120, Math.max(64, Math.round(textWidth(rows[0].label, FONT) + 32)))
  const height = rows.length * rowH + (rows.length - 1) * gap + 12
  const parts = []
  rows.forEach((row, i) => {
    const y = 6 + i * (rowH + gap)
    parts.push(`<rect class="band" x="0" y="${y}" width="${width}" height="${rowH}" rx="6"/>`)
    parts.push(text(14, y + rowH / 2 + 4, row.label, 'band-label', 'start'))
    const innerX = labelW + 10
    const innerW = width - innerX - 10
    const count = Math.max(1, row.items.length)
    const chipGap = 10
    const natural = row.items.map((item) => Math.max(64, Math.round(textWidth(item, FONT) + 26)))
    const totalNatural = natural.reduce((sum, w) => sum + w, 0) + chipGap * (count - 1)
    const scale = totalNatural > innerW ? innerW / totalNatural : 1
    let cx = innerX
    row.items.forEach((item, j) => {
      const chipW = Math.round(natural[j] * scale)
      parts.push(`<rect class="chip" x="${cx}" y="${y + 12}" width="${chipW}" height="${rowH - 24}" rx="4"/>`)
      parts.push(text(cx + chipW / 2, y + rowH / 2 + 4, item, 'chip-label'))
      cx += chipW + chipGap
    })
    if (!row.items.length) {
      parts.push(text(innerX + 4, y + rowH / 2 + 4, '（无）', 'muted', 'start'))
    }
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
  const valueW = 56
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

export function renderFigure(spec, width = 724) {
  const w = Math.max(320, Math.round(width))
  const render = { flow: renderFlow, layers: renderLayers, bar: renderBar, quote: renderQuote }[spec.kind]
  const { svg, height } = render(spec, w)
  if (!svg) return { html: '', caption: spec.title }
  const header = `<defs><marker id="arrow" viewBox="0 0 8 8" refX="7" refY="4" markerWidth="7" markerHeight="7" orient="auto"><path d="M0 0 L8 4 L0 8 z" class="arrow-head"/></marker></defs>`
  const caption = spec.note
    ? `<span class="fig-note">${esc(spec.note)}</span>`
    : ''
  const html =
    `<figure class="fig" data-kind="${spec.kind}">` +
    `<div class="fig-title">${esc(spec.title)}</div>` +
    `<div class="fig-frame"><svg viewBox="0 0 ${w} ${Math.round(height)}" width="100%" role="img" aria-label="${esc(spec.title)}">${header}${svg}</svg></div>` +
    (caption ? `<figcaption class="fig-cap">${caption}</figcaption>` : '') +
    `</figure>`
  return { html, caption: spec.title }
}
