<script setup lang="ts">
import { onBeforeUnmount, onMounted, reactive, ref, watch } from 'vue'
import { useRouter } from 'vue-router'

import * as api from '../lib/api'
import {
  buildGraph,
  kindLabel,
  noteShort,
  tagValue,
  type Graph,
  type GraphNode,
} from '../lib/graph'
import { parseNote } from '../lib/markdown'
import { library, loadLibrary } from '../stores/library'
import type { NoteMeta } from '../types'

const router = useRouter()

const MAX_NOTES = 40
const MAX_ALL_NOTES = 200
const MAX_TICKS = 320
// Note cards have a fixed footprint; laying them out as circles wastes more
// than half the canvas and still lets corners overlap. These are the real
// box dimensions used by the collision pass (padding included), so they must
// track .fv-note in readability.css.
const NOTE_W = 184
const NOTE_H = 56
const NOTE_GAP = 10

const canvasRef = ref<HTMLElement | null>(null)
const stageRef = ref<HTMLElement | null>(null)
const edgesRef = ref<SVGSVGElement | null>(null)
const nodesRef = ref<HTMLElement | null>(null)
const crumbsRef = ref<HTMLElement | null>(null)
const pathbarRef = ref<HTMLElement | null>(null)
const panelRef = ref<HTMLElement | null>(null)
const panelBodyRef = ref<HTMLElement | null>(null)
const panelKindRef = ref<HTMLElement | null>(null)
const panelTitleRef = ref<HTMLElement | null>(null)
const tipRef = ref<HTMLElement | null>(null)
const zoomValueRef = ref<HTMLElement | null>(null)
const resultsRef = ref<HTMLElement | null>(null)
const statsRef = ref<HTMLElement | null>(null)

const filters = [
  { key: 'note', label: '笔记' },
  { key: 'tag', label: '标签' },
  { key: 'person', label: '人物' },
  { key: 'period', label: '时期' },
]
const hidden = reactive<Record<string, boolean>>({
  note: false,
  tag: false,
  person: true,
  period: true,
})
const pathMode = ref(false)
const scopeAll = ref(false)

interface SimItem {
  id: string
  node: GraphNode
  x: number
  y: number
  vx: number
  vy: number
  fx: number
  fy: number
  hw: number
  hh: number
  r: number
  placed?: boolean
}

let graph: Graph | null = null
let noteById = new Map<string, NoteMeta>()
let items: SimItem[] = []
let itemById = new Map<string, SimItem>()
let nodeEls = new Map<string, HTMLElement>()
let edgeEls = new Map<string, SVGLineElement>()
let totalNotes = 0
let truncated = false

const state = {
  focus: null as string | null,
  hover: null as string | null,
  pathStart: null as string | null,
  pathEnd: null as string | null,
  path: null as string[] | null,
  pathMissing: false,
}
const view = { x: 0, y: 0, z: 1 }
let drag = { on: false, pointerId: 0, sx: 0, sy: 0, ox: 0, oy: 0, moved: false }
let animToken = 0
let panelToken = 0
let cleanup: Array<() => void> = []

function esc(value: string): string {
  return value
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;')
}

function truncate(value: string, max: number): string {
  return value.length > max ? `${value.slice(0, max)}…` : value
}

function clamp(value: number, min: number, max: number): number {
  return Math.max(min, Math.min(max, value))
}

function groupOf(kind: string): string {
  if (kind === 'note') return 'note'
  if (kind === 'person') return 'person'
  if (kind === 'period') return 'period'
  return 'tag'
}

function isTagKind(kind: string): boolean {
  return kind === 'tag' || kind === 'domain' || kind === 'type' || kind === 'topic' || kind === 'up'
}

function nodeLabel(node: GraphNode): string {
  if (node.kind === 'note') {
    const note = noteById.get(node.noteId ?? '')
    return note?.title ?? node.label
  }
  if (node.kind === 'person' || node.kind === 'period' || node.kind === 'source') {
    return node.label
  }
  return tagValue(node.label)
}

function radiusOf(node: GraphNode): number {
  if (node.kind === 'note') return 64
  if (node.kind === 'person' || node.kind === 'period') return 12
  return 30
}

function seedRandom(seed: number): () => number {
  return () => {
    seed |= 0
    seed = (seed + 0x6d2b79f5) | 0
    let t = Math.imul(seed ^ (seed >>> 15), 1 | seed)
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296
  }
}
const random = seedRandom(42)

function build(): void {
  const buildStarted = performance.now()
  const ordered = [...library.notes].sort((a, b) =>
    (b.date || '').localeCompare(a.date || ''),
  )
  totalNotes = ordered.length
  const limit = scopeAll.value ? MAX_ALL_NOTES : MAX_NOTES
  const used = ordered.slice(0, limit)
  truncated = done(ordered, used)
  noteById = new Map(used.map((note) => [note.id, note]))
  graph = used.length ? buildGraph(used, true) : null

  state.focus = null
  state.hover = null
  state.pathStart = null
  state.pathEnd = null
  state.path = null
  state.pathMissing = false
  pathMode.value = false
  panelRef.value && (panelRef.value.hidden = true)
  pathbarRef.value && (pathbarRef.value.hidden = true)

  if (!graph) {
    nodesRef.value && (nodesRef.value.innerHTML = '')
    edgesRef.value && (edgesRef.value.innerHTML = '')
    if (statsRef.value) statsRef.value.textContent = '0 节点 · 0 条边'
    console.info(`[force] build ${Math.round(performance.now() - buildStarted)}ms · 0 nodes`)
    return
  }

  initPositions()
  simulate()
  compact()
  normalize()
  renderGraph()
  positionNodes()
  applyFocus()
  fit(false)

  const target = { x: view.x, y: view.y, z: view.z }
  view.x = 0
  view.y = 0
  view.z = 0.55
  applyView()
  requestAnimationFrame(() => animateView(target, 720))
  console.info(
    `[force] build ${Math.round(performance.now() - buildStarted)}ms · ${graph.nodes.length} nodes · ${graph.edges.length} edges · ${graph.notes.length} notes`,
  )
}

function done(ordered: NoteMeta[], used: NoteMeta[]): boolean {
  return ordered.length > used.length
}

function initPositions(): void {
  if (!graph) return
  items = graph.nodes.map((node) => {
    const isNote = node.kind === 'note'
    const circle = node.kind === 'person' || node.kind === 'period'
    return {
      id: node.id,
      node,
      x: 0,
      y: 0,
      vx: 0,
      vy: 0,
      fx: 0,
      fy: 0,
      hw: isNote ? (NOTE_W + NOTE_GAP) / 2 : 0,
      hh: isNote ? (NOTE_H + NOTE_GAP) / 2 : 0,
      r: isNote ? 0 : circle ? 10 : 30,
    }
  })
  itemById = new Map(items.map((item) => [item.id, item]))
  const notes = items.filter((item) => item.node.kind === 'note')
  // Golden-angle spiral: even area coverage with no initial pile-up. The
  // radius scales with note count instead of squeezing every vault into a
  // 180px ring.
  notes.forEach((item, index) => {
    const angle = index * 2.399963229728653
    const radius = 100 * Math.sqrt(index + 0.5)
    item.x = Math.cos(angle) * radius
    item.y = Math.sin(angle) * radius
  })
  for (const edge of graph.edges) {
    const source = itemById.get(edge.source)
    const target = itemById.get(edge.target)
    if (!source || !target || target.placed) continue
    const angle = random() * Math.PI * 2
    const distance = 80 + random() * 60
    target.x = source.x + Math.cos(angle) * distance
    target.y = source.y + Math.sin(angle) * distance
    target.placed = true
  }
}

function simulate(): void {
  if (!graph) return
  const K = 150
  let alpha = 1
  for (let tick = 0; tick < MAX_TICKS; tick += 1) {
    for (const item of items) {
      item.fx = 0
      item.fy = 0
    }
    for (let i = 0; i < items.length; i += 1) {
      const a = items[i]
      for (let j = i + 1; j < items.length; j += 1) {
        const b = items[j]
        const dx = b.x - a.x
        const dy = b.y - a.y
        const distance = Math.sqrt(dx * dx + dy * dy) || 0.1
        const force = ((K * K) / distance) * 0.014
        const ux = dx / distance
        const uy = dy / distance
        a.fx -= ux * force
        a.fy -= uy * force
        b.fx += ux * force
        b.fy += uy * force
      }
    }
    for (const edge of graph.edges) {
      const a = itemById.get(edge.source)
      const b = itemById.get(edge.target)
      if (!a || !b) continue
      const dx = b.x - a.x
      const dy = b.y - a.y
      const distance = Math.sqrt(dx * dx + dy * dy) || 0.1
      const force = ((distance * distance) / K) * 0.009
      const ux = dx / distance
      const uy = dy / distance
      a.fx += ux * force
      a.fy += uy * force
      b.fx -= ux * force
      b.fy -= uy * force
    }
    for (const item of items) {
      // Hubs with many notes all want the same centre. Give tags a
      // degree-scaled orbit so high-degree labels separate instead of
      // stacking into an unreadable pile.
      if (isTagKind(item.node.kind)) {
        const distance = Math.sqrt(item.x * item.x + item.y * item.y) || 1
        const target = 90 + Math.min(item.node.weight, 36) * 9
        const pull = (target - distance) * 0.004
        item.fx += (item.x / distance) * pull
        item.fy += (item.y / distance) * pull
      }
      item.fx -= item.x * 0.0016
      item.fy -= item.y * 0.0016
      item.vx = clamp((item.vx + item.fx * alpha) * 0.82, -9, 9)
      item.vy = clamp((item.vy + item.fy * alpha) * 0.82, -9, 9)
      item.x += item.vx
      item.y += item.vy
    }
    resolveCollisions()
    alpha *= 0.985
  }
  // Springs keep pulling cards back into each other after the velocity step;
  // finish with a dedicated relaxation so no card is left overlapping.
  for (let pass = 0; pass < 24; pass += 1) resolveCollisions()
}

// Pack the settled graph into a camera-sized box. Positions shrink, cards do
// not, so each round ends with a collision relaxation; the loop stops when
// the cards themselves are the only thing holding the bounds open.
function compact(): void {
  const targetW = 1700
  const targetH = 820
  for (let round = 0; round < 36; round += 1) {
    let minX = Infinity
    let maxX = -Infinity
    let minY = Infinity
    let maxY = -Infinity
    for (const item of items) {
      minX = Math.min(minX, item.x - item.hw - item.r)
      maxX = Math.max(maxX, item.x + item.hw + item.r)
      minY = Math.min(minY, item.y - item.hh - item.r)
      maxY = Math.max(maxY, item.y + item.hh + item.r)
    }
    const width = Math.max(1, maxX - minX)
    const height = Math.max(1, maxY - minY)
    const scale = Math.min(0.96, targetW / width, targetH / height)
    if (scale > 0.995) break
    for (const item of items) {
      item.x *= scale
      item.y *= scale
    }
    for (let pass = 0; pass < 4; pass += 1) resolveCollisions()
  }
  for (let pass = 0; pass < 24; pass += 1) resolveCollisions()
}

function resolveCollisions(): void {
  for (let pass = 0; pass < 2; pass += 1) {
    for (let i = 0; i < items.length; i += 1) {
      const a = items[i]
      for (let j = i + 1; j < items.length; j += 1) {
        const b = items[j]
        if (a.hw && b.hw) resolveRectRect(a, b)
        else if (a.hw) resolveRectCircle(a, b)
        else if (b.hw) resolveRectCircle(b, a)
        else resolveCircleCircle(a, b)
      }
    }
  }
}

function resolveRectRect(a: SimItem, b: SimItem): void {
  const dx = b.x - a.x
  const dy = b.y - a.y
  const overlapX = a.hw + b.hw - Math.abs(dx)
  const overlapY = a.hh + b.hh - Math.abs(dy)
  if (overlapX <= 0 || overlapY <= 0) return
  if (overlapX < overlapY) {
    const push = (overlapX / 2) * (dx >= 0 ? 1 : -1)
    a.x -= push
    b.x += push
  } else {
    const push = (overlapY / 2) * (dy >= 0 ? 1 : -1)
    a.y -= push
    b.y += push
  }
}

function resolveRectCircle(rect: SimItem, circle: SimItem): void {
  const x = clamp(circle.x, rect.x - rect.hw, rect.x + rect.hw)
  const y = clamp(circle.y, rect.y - rect.hh, rect.y + rect.hh)
  let dx = circle.x - x
  let dy = circle.y - y
  let distance = Math.sqrt(dx * dx + dy * dy)
  if (distance >= circle.r) return
  if (distance < 0.01) {
    // Centre is inside the card: leave through the nearest edge.
    const left = circle.x - (rect.x - rect.hw)
    const right = rect.x + rect.hw - circle.x
    const top = circle.y - (rect.y - rect.hh)
    const bottom = rect.y + rect.hh - circle.y
    const edge = Math.min(left, right, top, bottom)
    if (edge === left) {
      dx = -1
      dy = 0
    } else if (edge === right) {
      dx = 1
      dy = 0
    } else if (edge === top) {
      dx = 0
      dy = -1
    } else {
      dx = 0
      dy = 1
    }
    distance = 1
  }
  const push = circle.r - distance
  const ux = dx / distance
  const uy = dy / distance
  rect.x -= ux * push * 0.2
  rect.y -= uy * push * 0.2
  circle.x += ux * push * 0.8
  circle.y += uy * push * 0.8
}

function resolveCircleCircle(a: SimItem, b: SimItem): void {
  const dx = b.x - a.x
  const dy = b.y - a.y
  const distance = Math.sqrt(dx * dx + dy * dy) || 0.01
  const min = a.r + b.r
  if (distance >= min) return
  const push = (min - distance) / 2
  const ux = dx / distance
  const uy = dy / distance
  a.x -= ux * push
  a.y -= uy * push
  b.x += ux * push
  b.y += uy * push
}

function normalize(): void {
  let minX = Infinity
  let maxX = -Infinity
  let minY = Infinity
  let maxY = -Infinity
  for (const item of items) {
    minX = Math.min(minX, item.x)
    maxX = Math.max(maxX, item.x)
    minY = Math.min(minY, item.y)
    maxY = Math.max(maxY, item.y)
  }
  const width = Math.max(1, maxX - minX)
  const height = Math.max(1, maxY - minY)
  // Never scale positions down here: card sizes are in screen pixels, so a
  // downscale would squeeze the spacing while leaving the cards full size and
  // reintroduce overlaps. The fit step zooms the whole stage instead.
  const scale = Math.max(1, Math.min(1440 / width, 900 / height, 1.45))
  // A 40-note force layout settles taller than the 16:9 canvas; widen it so
  // the graph uses the horizontal space instead of leaving two dead margins.
  const aspect = width / height
  const stretch = aspect < 1.2 ? Math.min(2.2, 1.2 / aspect) : 1
  const cx = (minX + maxX) / 2
  const cy = (minY + maxY) / 2
  for (const item of items) {
    item.x = (item.x - cx) * scale * stretch
    item.y = (item.y - cy) * scale
  }
}

function renderGraph(): void {
  if (!graph || !nodesRef.value || !edgesRef.value) return
  const nodesHost = nodesRef.value
  const edgesHost = edgesRef.value
  nodesHost.innerHTML = graph.nodes
    .map((node) => {
      const group = groupOf(node.kind)
      const label = nodeLabel(node)
      if (node.kind === 'note') {
        const note = noteById.get(node.noteId ?? '')
        const sub = note ? `${note.up} · ${note.date}` : node.meta ?? ''
        return `<button type="button" class="fv-node fv-note" data-id="${esc(node.id)}" data-kind="note"><span class="fv-label">${esc(label)}</span><span class="fv-sub">${esc(sub)}</span></button>`
      }
      if (group === 'tag') {
        return `<button type="button" class="fv-node fv-tag" data-id="${esc(node.id)}" data-kind="${esc(node.kind)}"><i class="fv-dot"></i><span class="fv-label">${esc(truncate(label, 10))}</span></button>`
      }
      return `<button type="button" class="fv-node fv-attr" data-id="${esc(node.id)}" data-kind="${esc(node.kind)}"><i class="fv-dot"></i><span class="fv-label">${esc(truncate(label, 10))}</span></button>`
    })
    .join('')
  edgesHost.innerHTML = graph.edges
    .map(
      (edge) =>
        `<line class="fv-edge" data-id="${esc(edge.source + '|' + edge.target)}" data-kind="${esc(edge.kind)}" data-source="${esc(edge.source)}" data-target="${esc(edge.target)}"></line>`,
    )
    .join('')
  // At map zoom only the strongest hubs keep a label; everything else is a
  // dot. Ranking lives here because CSS cannot sort by weight.
  const majorIds = new Set(
    graph.nodes
      .filter((node) => isTagKind(node.kind) && node.weight >= 2)
      .sort((a, b) => b.weight - a.weight)
      .slice(0, 7)
      .map((node) => node.id),
  )
  nodeEls = new Map(
    [...nodesHost.querySelectorAll<HTMLElement>('.fv-node')].map((element) => {
      element.classList.toggle('is-major', majorIds.has(element.dataset.id ?? ''))
      return [element.dataset.id ?? '', element]
    }),
  )
  edgeEls = new Map(
    [...edgesHost.querySelectorAll<SVGLineElement>('.fv-edge')].map((element) => [
      element.dataset.id ?? '',
      element,
    ]),
  )
  const noteCount = graph.notes.length
  const suffix = truncated
    ? ` · 仅画最近 ${noteCount} 篇 / 共 ${totalNotes} 篇`
    : scopeAll.value && totalNotes > MAX_ALL_NOTES
      ? ` · 显示前 ${MAX_ALL_NOTES} 篇 / 共 ${totalNotes} 篇`
      : ''
  if (statsRef.value) {
    statsRef.value.textContent = `${graph.nodes.length} 节点 · ${graph.edges.length} 条边 · ${noteCount} 篇笔记${suffix}`
  }
}

function positionNodes(): void {
  for (const item of items) {
    const element = nodeEls.get(item.id)
    if (element) {
      element.style.transform = `translate(${item.x}px,${item.y}px) translate(-50%,-50%)`
    }
  }
  if (!graph) return
  for (const edge of graph.edges) {
    const line = edgeEls.get(`${edge.source}|${edge.target}`)
    const a = itemById.get(edge.source)
    const b = itemById.get(edge.target)
    if (!line || !a || !b) continue
    line.setAttribute('x1', String(a.x))
    line.setAttribute('y1', String(a.y))
    line.setAttribute('x2', String(b.x))
    line.setAttribute('y2', String(b.y))
  }
}

function applyView(): void {
  if (stageRef.value) {
    stageRef.value.style.transform = `translate(${view.x}px, ${view.y}px) scale(${view.z})`
    stageRef.value.dataset.zoom = view.z < 0.78 ? 'far' : view.z >= 1.02 ? 'near' : 'mid'
    stageRef.value.style.setProperty('--fv-inv', String(1 / view.z))
  }
  if (zoomValueRef.value) zoomValueRef.value.textContent = `${Math.round(view.z * 100)}%`
}

function animateView(target: { x: number; y: number; z: number }, duration: number): void {
  animToken += 1
  const token = animToken
  const from = { ...view }
  const started = performance.now()
  const step = (now: number): void => {
    if (token !== animToken) return
    const progress = clamp((now - started) / duration, 0, 1)
    const eased = 1 - Math.pow(1 - progress, 3)
    view.x = from.x + (target.x - from.x) * eased
    view.y = from.y + (target.y - from.y) * eased
    view.z = from.z + (target.z - from.z) * eased
    applyView()
    if (progress < 1) requestAnimationFrame(step)
  }
  requestAnimationFrame(step)
}

function visibleItems(): SimItem[] {
  return items.filter((item) => !hidden[groupOf(item.node.kind)])
}

function fit(animate: boolean): void {
  const canvas = canvasRef.value
  const visible = visibleItems()
  if (!canvas || !visible.length) return
  let minX = Infinity
  let maxX = -Infinity
  let minY = Infinity
  let maxY = -Infinity
  for (const item of visible) {
    const radius = radiusOf(item.node) + 24
    minX = Math.min(minX, item.x - radius)
    maxX = Math.max(maxX, item.x + radius)
    minY = Math.min(minY, item.y - radius)
    maxY = Math.max(maxY, item.y + radius)
  }
  const rect = canvas.getBoundingClientRect()
  const padding = 72
  const z = clamp(
    Math.min((rect.width - padding * 2) / (maxX - minX), (rect.height - padding * 2) / (maxY - minY), 1.35),
    0.42,
    1.35,
  )
  const cx = (minX + maxX) / 2
  const cy = (minY + maxY) / 2
  const target = { x: -cx * z, y: -cy * z, z }
  if (animate) animateView(target, 520)
  else {
    view.x = target.x
    view.y = target.y
    view.z = target.z
    applyView()
  }
}

function flyTo(id: string): void {
  const item = itemById.get(id)
  if (!item) return
  const z = Math.max(view.z, 1.08)
  animateView({ x: -item.x * z, y: -item.y * z, z }, 480)
}

function neighborSet(id: string): Set<string> {
  const set = new Set<string>([id])
  if (!graph) return set
  const node = graph.nodeMap[id]
  for (const edge of graph.edges) {
    if (edge.source === id) set.add(edge.target)
    if (edge.target === id) set.add(edge.source)
  }
  if (node?.kind === 'note') {
    for (const edge of graph.edges) {
      let other: string | null = null
      if (edge.source === id) other = edge.target
      if (edge.target === id) other = edge.source
      if (!other) continue
      const attr = graph.nodeMap[other]
      if (!attr || !isTagKind(attr.kind)) continue
      for (const inner of graph.edges) {
        if (inner.source === other && inner.target !== id) set.add(inner.target)
        if (inner.target === other && inner.source !== id) set.add(inner.source)
      }
    }
  }
  return set
}

function isPathEdge(source: string, target: string): boolean {
  if (!state.path) return false
  for (let index = 0; index < state.path.length - 1; index += 1) {
    const a = state.path[index]
    const b = state.path[index + 1]
    if ((source === a && target === b) || (source === b && target === a)) return true
  }
  return false
}

function applyFocus(): void {
  if (!graph) return
  const set = state.focus
    ? neighborSet(state.focus)
    : !state.path && state.hover
      ? neighborSet(state.hover)
      : null
  for (const node of graph.nodes) {
    const element = nodeEls.get(node.id)
    if (!element) continue
    const group = groupOf(node.kind)
    const isHidden = Boolean(hidden[group])
    let dim = false
    if (state.path) dim = !state.path.includes(node.id)
    else if (set) dim = !set.has(node.id)
    element.classList.toggle('is-hidden', isHidden)
    element.classList.toggle(
      'is-focus',
      node.id === state.focus || (!state.focus && node.id === state.hover),
    )
    element.classList.toggle('is-dim', !isHidden && dim)
    element.classList.toggle('is-path', Boolean(state.path?.includes(node.id)))
  }
  for (const edge of graph.edges) {
    const line = edgeEls.get(`${edge.source}|${edge.target}`)
    if (!line) continue
    const source = graph.nodeMap[edge.source]
    const target = graph.nodeMap[edge.target]
    if (!source || !target) continue
    const isHidden = hidden[groupOf(source.kind)] || hidden[groupOf(target.kind)]
    const inSet = !set || (set.has(edge.source) && set.has(edge.target))
    const pathEdge = isPathEdge(edge.source, edge.target)
    line.classList.toggle('is-hidden', isHidden)
    line.classList.toggle(
      'is-dim',
      !isHidden && ((Boolean(set) && !inSet) || (Boolean(state.path) && !pathEdge)),
    )
    line.classList.toggle(
      'is-hot',
      !state.focus &&
        !state.path &&
        Boolean(state.hover) &&
        (edge.source === state.hover || edge.target === state.hover),
    )
    line.classList.toggle('is-path', pathEdge && !isHidden)
  }
  renderCrumbs()
}

function renderCrumbs(): void {
  const host = crumbsRef.value
  if (!host) return
  const node = state.focus && graph ? graph.nodeMap[state.focus] : null
  if (!node) {
    const scope = truncated
      ? `最近 ${graph?.notes.length ?? 0} 篇 · 共 ${totalNotes} 篇`
      : `${graph?.notes.length ?? 0} 篇笔记`
    host.innerHTML =
      `<button type="button" data-act="clear">全部</button>` +
      `<span class="now">${scope}</span>`
    return
  }
  host.innerHTML =
    `<button type="button" data-act="clear">全部</button><span class="sep">/</span>` +
    `<span class="now">${esc(node.kind === 'note' ? nodeLabel(node) : node.label)}</span>`
}

function focusNode(id: string): void {
  if (!graph) return
  const node = graph.nodeMap[id]
  if (!node) return
  const group = groupOf(node.kind)
  if (hidden[group]) {
    hidden[group] = false
  }
  state.focus = id
  state.hover = null
  state.path = null
  state.pathMissing = false
  state.pathStart = null
  state.pathEnd = null
  pathMode.value = false
  if (pathbarRef.value) pathbarRef.value.hidden = true
  applyFocus()
  renderPanel(id)
  flyTo(id)
}

function clearFocus(): void {
  state.focus = null
  state.hover = null
  state.path = null
  state.pathMissing = false
  state.pathStart = null
  state.pathEnd = null
  pathMode.value = false
  if (panelRef.value) panelRef.value.hidden = true
  if (pathbarRef.value) pathbarRef.value.hidden = true
  hideTip()
  applyFocus()
  fit(true)
}

function renderPanel(id: string): void {
  if (!graph || !panelRef.value || !panelBodyRef.value) return
  const node = graph.nodeMap[id]
  if (!node) return
  panelRef.value.hidden = false
  if (panelKindRef.value) panelKindRef.value.textContent = kindLabel(node.kind).toUpperCase()
  if (panelTitleRef.value) {
    panelTitleRef.value.textContent = node.kind === 'note' ? nodeLabel(node) : node.label
  }
  if (node.kind === 'note') {
    renderNotePanel(node)
    return
  }
  const notes = graph.edges
    .filter((edge) => edge.source === node.id || edge.target === node.id)
    .map((edge) => graph?.nodeMap[edge.source === node.id ? edge.target : edge.source])
    .filter((entry): entry is GraphNode => Boolean(entry) && entry?.kind === 'note')
  let html = `<div class="fv-pmeta"><span>${esc(kindLabel(node.kind))}</span><span>${node.weight} 个连接</span></div>`
  html += `<div class="fv-psection">${notes.length} 篇笔记</div>`
  html += notes
    .map((noteNode) => {
      const note = noteById.get(noteNode.noteId ?? '')
      return `<button type="button" class="fv-plink" data-act="focus" data-id="${esc(noteNode.id)}">${esc(note?.title ?? noteNode.label)}<em>${esc(note?.up ?? '')} · ${esc(note?.date ?? '')}</em></button>`
    })
    .join('')
  html += `<button type="button" class="fv-plink" data-act="tags" data-label="${esc(node.label)}">在标签台查看 →</button>`
  panelBodyRef.value.innerHTML = html
}

function renderNotePanel(node: GraphNode): void {
  const note = noteById.get(node.noteId ?? '')
  if (!note || !graph || !panelBodyRef.value) return
  panelToken += 1
  const token = panelToken
  const connections = graph.edges.filter((edge) => edge.source === node.id).length
  const tags = note.tags
    .map(
      (tag) =>
        `<button type="button" class="fv-ptag" data-act="focus" data-id="${esc(`t:${tag}`)}">${esc(tag)}</button>`,
    )
    .join('')
  const related: Array<{ node: GraphNode; note: NoteMeta; common: string[] }> = []
  for (const link of graph.noteLinks) {
    if (link.a !== note.id && link.b !== note.id) continue
    const common = link.common.filter((item) => item.kind !== 'source')
    if (!common.length) continue
    const otherId = link.a === note.id ? link.b : link.a
    const other = noteById.get(otherId)
    if (!other) continue
    related.push({
      node: graph.nodeMap[`n:${otherId}`],
      note: other,
      common: common.map((item) => item.label),
    })
  }
  related.sort((a, b) => b.common.length - a.common.length)
  let html = `<div class="fv-pmeta"><span>${esc(note.up)}</span><span>${esc(note.date)}</span><span>${connections} 个连接</span></div>`
  if (tags) html += `<div class="fv-ptags">${tags}</div>`
  html += `<ol class="fv-psum" data-summary><li class="fv-pempty">正在读取这篇笔记…</li></ol>`
  html += `<div class="fv-psection">经标签 / 人物连到</div>`
  if (related.length) {
    html += related
      .slice(0, 6)
      .map(
        (entry) =>
          `<button type="button" class="fv-plink" data-act="focus" data-id="${esc(entry.node?.id ?? '')}">${esc(entry.note.title)}<em>${esc(entry.common.join(' · '))}</em></button>`,
      )
      .join('')
  } else {
    html += `<div class="fv-pempty">没有共享实质标签的笔记。</div>`
  }
  html += `<button type="button" class="fv-open" data-act="open" data-id="${esc(note.id)}">打开笔记 →</button>`
  panelBodyRef.value.innerHTML = html

  void api
    .getNote(note.id)
    .then((detail) => {
      if (token !== panelToken || !detail) return
      const parsed = parseNote(detail.body)
      const takes = parsed.summary.slice(0, 3)
      const host = panelBodyRef.value?.querySelector<HTMLElement>('[data-summary]')
      if (!host) return
      host.innerHTML = takes.length
        ? takes
            .map(
              (block, index) =>
                `<li><b>${String(index + 1).padStart(2, '0')}</b><span>${esc(block.text)}</span></li>`,
            )
            .join('')
        : `<li class="fv-pempty">这篇没有速览段。</li>`
    })
    .catch(() => {
      if (token !== panelToken) return
      const host = panelBodyRef.value?.querySelector<HTMLElement>('[data-summary]')
      if (host) host.innerHTML = `<li class="fv-pempty">读取正文失败。</li>`
    })
}

function toggleKind(key: string): void {
  hidden[key] = !hidden[key]
  if (state.focus) {
    const node = graph?.nodeMap[state.focus]
    if (node && hidden[groupOf(node.kind)]) clearFocus()
  }
  applyFocus()
}

function showTip(event: PointerEvent, id: string): void {
  if (!graph || !tipRef.value) return
  const node = graph.nodeMap[id]
  if (!node) return
  const label = node.kind === 'note' ? nodeLabel(node) : node.label
  tipRef.value.innerHTML = `<b>${esc(kindLabel(node.kind))}</b>${esc(label)}`
  tipRef.value.hidden = false
  tipRef.value.style.left = `${event.clientX + 14}px`
  tipRef.value.style.top = `${event.clientY + 14}px`
}

function hideTip(): void {
  if (tipRef.value) tipRef.value.hidden = true
}

function searchNodes(query: string): GraphNode[] {
  if (!graph) return []
  const value = query.trim().toLowerCase()
  if (!value) return []
  return graph.nodes
    .filter((node) => {
      const note = node.kind === 'note' ? noteById.get(node.noteId ?? '') : null
      const text = `${node.label} ${note?.title ?? ''} ${node.kind}`.toLowerCase()
      return text.includes(value)
    })
    .slice(0, 8)
}

function renderResults(query: string): void {
  const host = resultsRef.value
  if (!host) return
  const list = searchNodes(query)
  if (!query.trim() || !list.length) {
    host.hidden = true
    host.innerHTML = ''
    return
  }
  host.innerHTML = list
    .map((node) => {
      const label = node.kind === 'note' ? nodeLabel(node) : node.label
      return `<button type="button" class="fv-result" data-act="result" data-id="${esc(node.id)}"><em>${esc(kindLabel(node.kind))}</em>${esc(label)}</button>`
    })
    .join('')
  host.hidden = false
}

function onSearchInput(event: Event): void {
  renderResults((event.target as HTMLInputElement).value)
}

function onSearchKey(event: KeyboardEvent): void {
  const input = event.target as HTMLInputElement
  if (event.key === 'Enter') {
    const list = searchNodes(input.value)
    if (list[0]) {
      input.value = ''
      if (resultsRef.value) resultsRef.value.hidden = true
      focusNode(list[0].id)
    }
  }
  if (event.key === 'Escape' && resultsRef.value) resultsRef.value.hidden = true
}

function selectResult(id: string): void {
  const input = document.querySelector<HTMLInputElement>('.fv-search input')
  if (input) input.value = ''
  if (resultsRef.value) resultsRef.value.hidden = true
  focusNode(id)
}

function findPath(from: string, to: string): string[] | null {
  if (!graph) return null
  if (from === to) return [from]
  const adjacency = new Map<string, string[]>()
  for (const edge of graph.edges) {
    const a = adjacency.get(edge.source)
    if (a) a.push(edge.target)
    else adjacency.set(edge.source, [edge.target])
    const b = adjacency.get(edge.target)
    if (b) b.push(edge.source)
    else adjacency.set(edge.target, [edge.source])
  }
  const queue = [from]
  const parent = new Map<string, string | null>([[from, null]])
  let cursor = 0
  while (cursor < queue.length) {
    const current = queue[cursor]
    cursor += 1
    if (current === to) {
      const path: string[] = []
      let step: string | null = current
      while (step) {
        path.push(step)
        step = parent.get(step) ?? null
      }
      return path.reverse()
    }
    for (const next of adjacency.get(current) ?? []) {
      if (parent.has(next)) continue
      const nextNode = graph.nodeMap[next]
      if (!nextNode) continue
      // A public source hub is not a meaningful relation. Hidden kinds are
      // allowed: a path through a person/period unhides that layer below.
      if (nextNode.kind === 'source') continue
      parent.set(next, current)
      queue.push(next)
    }
  }
  return null
}

function toggleScope(): void {
  scopeAll.value = !scopeAll.value
  build()
}

function togglePath(): void {
  pathMode.value = !pathMode.value
  state.path = null
  state.pathMissing = false
  state.pathStart = null
  state.pathEnd = null
  state.focus = null
  if (panelRef.value) panelRef.value.hidden = true
  renderPathbar()
  applyFocus()
  hideTip()
}

function clickPathNode(id: string): void {
  if (!graph) return
  if (!state.pathStart) {
    state.pathStart = id
    state.pathEnd = null
    state.path = null
    renderPathbar()
    applyFocus()
    return
  }
  state.pathEnd = id
  const result = findPath(state.pathStart, id)
  state.path = result
  state.pathMissing = !result
  state.pathStart = null
  if (result) {
    for (const nodeId of result) {
      const node = graph.nodeMap[nodeId]
      if (node) hidden[groupOf(node.kind)] = false
    }
  }
  renderPathbar()
  applyFocus()
}

function renderPathbar(): void {
  const host = pathbarRef.value
  if (!host) return
  if (!pathMode.value) {
    host.hidden = true
    return
  }
  host.hidden = false
  if (!state.path) {
    host.innerHTML =
      `<span class="fv-path-label">PATH</span>` +
      `<span class="fv-path-hint">${
        state.pathMissing
          ? '两篇之间没有经标签/人物/时期的路径'
          : state.pathStart
            ? '再点一篇笔记'
            : '点两篇笔记找最短关系'
      }</span>` +
      `<button type="button" class="fv-path-exit" data-act="path-exit">退出路径</button>`
    return
  }
  const steps = state.path.map((id) => {
    const node = graph?.nodeMap[id]
    return {
      id,
      note: node?.kind === 'note',
      label:
        node?.kind === 'note'
          ? noteShort(nodeLabel(node), 14)
          : truncate(node?.label ?? id, 12),
    }
  })
  host.innerHTML =
    `<span class="fv-path-label">PATH</span>` +
    steps
      .map(
        (step, index) =>
          `<span class="fv-path-step"><i class="${step.note ? 'note' : ''}"></i>${esc(step.label)}${index < steps.length - 1 ? '<em>→</em>' : ''}</span>`,
      )
      .join('') +
    `<button type="button" class="fv-path-clear" data-act="path-clear">清除</button>` +
    `<button type="button" class="fv-path-exit" data-act="path-exit">退出路径</button>`
}

function zoomBy(delta: number): void {
  view.z = clamp(view.z + delta, 0.42, 2.4)
  applyView()
}

function onWheel(event: WheelEvent): void {
  event.preventDefault()
  const canvas = canvasRef.value
  if (!canvas) return
  const rect = canvas.getBoundingClientRect()
  const cx = rect.width / 2
  const cy = rect.height / 2
  const sx = event.clientX - rect.left
  const sy = event.clientY - rect.top
  const wx = (sx - cx - view.x) / view.z
  const wy = (sy - cy - view.y) / view.z
  const next = clamp(view.z * Math.exp(-event.deltaY * 0.0015), 0.42, 2.4)
  view.x = sx - cx - wx * next
  view.y = sy - cy - wy * next
  view.z = next
  applyView()
}

function onPointerDown(event: PointerEvent): void {
  const target = event.target as HTMLElement
  if (target.closest('.fv-node')) return
  // Overlay controls sit inside the canvas container; let their clicks
  // through instead of starting a pan and capturing the pointer.
  if (target.closest('.force-filters, .force-pathbar, .force-zoom, .force-crumbs, .force-legend')) return
  const canvas = canvasRef.value
  if (!canvas) return
  drag = {
    on: true,
    pointerId: event.pointerId,
    sx: event.clientX,
    sy: event.clientY,
    ox: view.x,
    oy: view.y,
    moved: false,
  }
  canvas.classList.add('is-dragging')
  canvas.setPointerCapture(event.pointerId)
}

function onPointerMove(event: PointerEvent): void {
  if (!drag.on) return
  const dx = event.clientX - drag.sx
  const dy = event.clientY - drag.sy
  if (Math.hypot(dx, dy) > 3) drag.moved = true
  view.x = drag.ox + dx
  view.y = drag.oy + dy
  applyView()
}

function onPointerUp(event: PointerEvent): void {
  if (!drag.on) return
  drag.on = false
  const canvas = canvasRef.value
  canvas?.classList.remove('is-dragging')
  try {
    canvas?.releasePointerCapture(event.pointerId)
  } catch {
    // pointer capture may already be released
  }
}

function onNodesClick(event: MouseEvent): void {
  const node = (event.target as HTMLElement).closest<HTMLElement>('.fv-node')
  if (!node) return
  const id = node.dataset.id
  if (!id) return
  if (pathMode.value) {
    clickPathNode(id)
    return
  }
  focusNode(id)
}

function onNodesOver(event: PointerEvent): void {
  const node = (event.target as HTMLElement).closest<HTMLElement>('.fv-node')
  if (!node || !node.dataset.id) return
  const id = node.dataset.id
  if (state.hover === id) return
  state.hover = id
  showTip(event, id)
  if (!state.focus && !state.path) applyFocus()
}

function onNodesOut(event: PointerEvent): void {
  const related = event.relatedTarget as HTMLElement | null
  if (related?.closest('.fv-node')) return
  state.hover = null
  hideTip()
  if (!state.focus && !state.path) applyFocus()
}

function onPanelClick(event: MouseEvent): void {
  const action = (event.target as HTMLElement).closest<HTMLElement>('[data-act]')
  if (!action) return
  const act = action.dataset.act
  if (act === 'focus' && action.dataset.id) focusNode(action.dataset.id)
  if (act === 'open' && action.dataset.id) {
    void router.push(`/note/${action.dataset.id}`)
  }
  if (act === 'tags' && action.dataset.label) {
    void router.push({ path: '/tags', query: { tag: action.dataset.label } })
  }
}

function onResultsClick(event: MouseEvent): void {
  const result = (event.target as HTMLElement).closest<HTMLElement>('[data-act="result"]')
  if (!result?.dataset.id) return
  selectResult(result.dataset.id)
}

function onDocumentClick(event: MouseEvent): void {
  const target = event.target as HTMLElement
  if (!target.closest('.fv-search')) {
    if (resultsRef.value) resultsRef.value.hidden = true
  }
  const action = target.closest<HTMLElement>('[data-act]')
  if (!action) return
  const act = action.dataset.act
  if (act === 'clear') clearFocus()
  if (act === 'path-exit') togglePath()
  if (act === 'path-clear') {
    state.path = null
    state.pathMissing = false
    state.pathStart = null
    state.pathEnd = null
    renderPathbar()
    applyFocus()
  }
}

function onResize(): void {
  if (!state.focus) fit(false)
}

function goBack(): void {
  const first = library.notes[0]?.id
  void router.push(first ? `/note/${first}` : '/note')
}

function goTags(): void {
  void router.push('/tags')
}

function bind(target: EventTarget | null, event: string, handler: EventListener, options?: AddEventListenerOptions): void {
  if (!target) return
  target.addEventListener(event, handler, options)
  cleanup.push(() => target.removeEventListener(event, handler, options))
}

onMounted(async () => {
  if (!library.notes.length) await loadLibrary()
  build()
  const canvas = canvasRef.value
  bind(canvas, 'pointerdown', onPointerDown as EventListener)
  bind(canvas, 'pointermove', onPointerMove as EventListener)
  bind(canvas, 'pointerup', onPointerUp as EventListener)
  bind(canvas, 'pointercancel', onPointerUp as EventListener)
  bind(canvas, 'wheel', onWheel as EventListener, { passive: false })
  bind(nodesRef.value, 'click', onNodesClick as EventListener)
  bind(nodesRef.value, 'pointerover', onNodesOver as EventListener)
  bind(nodesRef.value, 'pointerout', onNodesOut as EventListener)
  bind(panelBodyRef.value, 'click', onPanelClick as EventListener)
  bind(resultsRef.value, 'click', onResultsClick as EventListener)
  bind(document, 'click', onDocumentClick as EventListener)
  bind(window, 'resize', onResize as EventListener)
})

watch(
  () => library.notes.length,
  () => {
    if (graph) build()
  },
)

onBeforeUnmount(() => {
  for (const off of cleanup) off()
  cleanup = []
  animToken += 1
})
</script>

<template>
  <div class="force-view">
    <header class="force-hud">
      <button type="button" class="force-brand" @click="goBack">
        <span class="force-mark">✦</span>
        <b>页边</b><span>图谱</span>
      </button>
      <span ref="statsRef" class="force-stats">正在读取…</span>
      <button
        type="button"
        class="force-tool force-scope"
        :aria-pressed="scopeAll"
        :title="scopeAll ? '只看最近 40 篇' : '显示全部笔记'"
        @click="toggleScope"
      >
        {{ scopeAll ? '最近' : '全部' }}
      </button>
      <span class="force-spacer"></span>
      <div class="fv-search">
        <input
          type="text"
          placeholder="搜笔记 / 标签 / 人物…"
          autocomplete="off"
          aria-label="搜索节点"
          @input="onSearchInput"
          @keydown="onSearchKey"
        />
        <div ref="resultsRef" class="fv-results" hidden></div>
      </div>
      <button type="button" class="force-tool" @click="goTags">标签</button>
      <button type="button" class="force-tool" :aria-pressed="pathMode" @click="togglePath">
        找路径
      </button>
      <button type="button" class="force-tool" @click="fit(true)">适配</button>
      <button type="button" class="force-back" @click="goBack">← 笔记</button>
    </header>

    <div ref="canvasRef" class="force-canvas">
      <div ref="stageRef" class="force-stage">
        <svg ref="edgesRef" class="force-edges" viewBox="-2000 -1500 4000 3000" aria-hidden="true"></svg>
        <div ref="nodesRef" class="force-nodes"></div>
      </div>

      <div ref="crumbsRef" class="force-crumbs"></div>

      <div class="force-filters">
        <button
          v-for="filter in filters"
          :key="filter.key"
          type="button"
          :data-kind="filter.key"
          :aria-pressed="!hidden[filter.key]"
          @click="toggleKind(filter.key)"
        >
          {{ filter.label }}
        </button>
      </div>

      <div class="force-legend">
        <span><i class="note"></i>笔记</span>
        <span><i class="tag"></i>标签</span>
        <span><i class="source"></i>来源</span>
        <span><i class="person"></i>人物</span>
        <span><i class="period"></i>时期</span>
        <span class="hint">滚轮缩放 · 拖动画布 · 点节点聚焦</span>
      </div>

      <div class="force-zoom">
        <button type="button" aria-label="缩小" @click="zoomBy(-0.12)">−</button>
        <span ref="zoomValueRef" class="force-zoom-value">100%</span>
        <button type="button" aria-label="放大" @click="zoomBy(0.12)">+</button>
        <button type="button" class="force-zoom-fit" @click="fit(true)">适配</button>
      </div>

      <div ref="pathbarRef" class="force-pathbar" hidden></div>
    </div>

    <aside ref="panelRef" class="force-panel" hidden>
      <header class="force-panel-head">
        <div>
          <div ref="panelKindRef" class="force-panel-kind"></div>
          <div ref="panelTitleRef" class="force-panel-title"></div>
        </div>
        <button type="button" class="force-panel-close" aria-label="关闭" @click="clearFocus">×</button>
      </header>
      <div ref="panelBodyRef" class="force-panel-body"></div>
    </aside>

    <div ref="tipRef" class="force-tip" hidden></div>
  </div>
</template>
