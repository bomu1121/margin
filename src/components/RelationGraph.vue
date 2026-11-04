<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useRouter } from 'vue-router'

import { kindLabel, localGraph, noteShort, tagValue, type GraphNode } from '../lib/graph'
import { layoutLocalGraph, type LocalGraphLayout } from '../lib/graphLayout'
import type { NoteMeta } from '../types'

const props = defineProps<{
  notes: NoteMeta[]
  noteId: string
}>()

const router = useRouter()

const cardRef = ref<HTMLElement | null>(null)
const canvasRef = ref<HTMLCanvasElement | null>(null)
const overlayCardRef = ref<HTMLElement | null>(null)
const overlayCanvasRef = ref<HTMLCanvasElement | null>(null)

const graph = computed(() => localGraph(props.notes, props.noteId))
const currentTitle = computed(
  () =>
    props.notes.find((note) => note.id === props.noteId)?.title ??
    graph.value?.current.label ??
    '',
)
const layout = ref<LocalGraphLayout | null>(null)
const expanded = ref(false)
const hoverId = ref<string | null>(null)
const tooltip = ref<{
  title: string
  kind: string
  x: number
  y: number
  below: boolean
} | null>(null)

const COLORS = {
  note: '#e3ad4f',
  tag: '#6fa8ff',
  person: '#6fbf9a',
  period: '#8b97a8',
  text: '#e6e9ef',
  text2: '#c2ccd9',
  text3: '#7e8b9b',
} as const

const SUMMARY = computed(() => {
  const value = layout.value
  if (!value) return '0 个连接'
  const notes = value.related.length ? ` · ${value.related.length} 篇邻居` : ''
  return `${value.attrs.length} 个连接${notes}`
})

let resizeObserver: ResizeObserver | null = null
let overlayObserver: ResizeObserver | null = null
let frame = 0
let revealStart = performance.now()
let dragging:
  | {
      id: string
      pointerId: number
      moved: boolean
      startX: number
      startY: number
      overlay: boolean
    }
  | null = null
let panning: { pointerId: number; startX: number; startY: number; tx: number; ty: number } | null =
  null
const viewState = { scale: 1, tx: 0, ty: 0 }

interface Scene {
  width: number
  height: number
  cx: number
  cy: number
  scale: number
}

interface DrawNode {
  key: string
  kind: string
  label: string
  full: string
  x: number
  y: number
  r: number
  color: string
  noteId?: string
  attrId?: string
  center?: boolean
}

function kindColor(kind: string): string {
  if (kind === 'note') return COLORS.note
  if (kind === 'person') return COLORS.person
  if (kind === 'period' || kind === 'source') return COLORS.period
  return COLORS.tag
}

function rgba(hex: string, alpha: number): string {
  const value = hex.replace('#', '')
  const red = parseInt(value.slice(0, 2), 16)
  const green = parseInt(value.slice(2, 4), 16)
  const blue = parseInt(value.slice(4, 6), 16)
  return `rgba(${red}, ${green}, ${blue}, ${alpha})`
}

function labelOf(node: GraphNode): string {
  if (node.kind === 'person' || node.kind === 'period' || node.kind === 'source') {
    return node.label
  }
  return tagValue(node.label)
}

function easeOut(value: number): number {
  return 1 - Math.pow(1 - value, 3)
}

function titleLines(title: string, maxChars: number): string[] {
  const cleaned = title.replace(/^【[^】]*】/, '').replace(/\s+/g, ' ').trim()
  const lines: string[] = []
  let rest = cleaned
  while (rest && lines.length < 2) {
    lines.push(rest.slice(0, maxChars))
    rest = rest.slice(maxChars)
  }
  if (rest && lines.length) lines[lines.length - 1] += '…'
  return lines
}

function setupCanvas(canvas: HTMLCanvasElement): CanvasRenderingContext2D | null {
  const width = canvas.clientWidth
  const height = canvas.clientHeight
  if (!width || !height) return null
  const ratio = Math.min(2, window.devicePixelRatio || 1)
  const pixelWidth = Math.round(width * ratio)
  const pixelHeight = Math.round(height * ratio)
  if (canvas.width !== pixelWidth || canvas.height !== pixelHeight) {
    canvas.width = pixelWidth
    canvas.height = pixelHeight
  }
  const context = canvas.getContext('2d')
  if (!context) return null
  context.setTransform(ratio, 0, 0, ratio, 0, 0)
  context.clearRect(0, 0, width, height)
  return context
}

function sceneFor(canvas: HTMLCanvasElement, overlay: boolean): Scene | null {
  const width = canvas.clientWidth
  const height = canvas.clientHeight
  if (!width || !height) return null
  const padding = overlay ? 76 : 30
  const base = Math.min(width - padding * 2, height - padding * 2) / 2
  if (base <= 2) return null
  return {
    width,
    height,
    cx: width / 2 + (overlay ? viewState.tx : 0),
    cy: height / 2 + (overlay ? viewState.ty : 0),
    scale: base * (overlay ? viewState.scale : 1),
  }
}

function project(
  node: { x: number; y: number },
  scene: Scene,
  progress: number,
): { x: number; y: number } {
  return {
    x: scene.cx + node.x * scene.scale * progress,
    y: scene.cy + node.y * scene.scale * progress,
  }
}

function attrRadius(weight: number, overlay: boolean): number {
  return (overlay ? 5.2 : 4.2) + Math.min(3, weight) * (overlay ? 1.25 : 0.9)
}

function relatedRadius(weight: number, overlay: boolean): number {
  return (overlay ? 7 : 5.4) + Math.min(3, weight) * (overlay ? 1.35 : 1.05)
}

function drawNodes(
  value: LocalGraphLayout,
  scene: Scene,
  progress: number,
  overlay: boolean,
): DrawNode[] {
  const nodes: DrawNode[] = [
    {
      key: value.current.id,
      kind: 'note',
      label: currentTitle.value,
      full: currentTitle.value,
      x: 0,
      y: 0,
      r: overlay ? 64 : 30,
      color: COLORS.note,
      center: true,
    },
  ]
  for (const attr of value.attrs) {
    nodes.push({
      key: attr.node.id,
      kind: attr.node.kind,
      label: labelOf(attr.node),
      full: attr.node.label,
      x: attr.x,
      y: attr.y,
      r: attrRadius(attr.weight, overlay),
      color: kindColor(attr.node.kind),
      attrId: attr.node.label,
    })
  }
  for (const related of value.related) {
    nodes.push({
      key: related.node.id,
      kind: 'note',
      label: noteShort(related.note.title, overlay ? 15 : 9),
      full: related.note.title,
      x: related.x,
      y: related.y,
      r: relatedRadius(related.weight, overlay),
      color: COLORS.note,
      noteId: related.note.id,
    })
  }
  for (const node of nodes) {
    const point = project(node, scene, progress)
    node.x = point.x
    node.y = point.y
  }
  return nodes
}

function roundRect(
  context: CanvasRenderingContext2D,
  x: number,
  y: number,
  width: number,
  height: number,
  radius: number,
): void {
  const r = Math.min(radius, width / 2, height / 2)
  context.beginPath()
  context.moveTo(x + r, y)
  context.arcTo(x + width, y, x + width, y + height, r)
  context.arcTo(x + width, y + height, x, y + height, r)
  context.arcTo(x, y + height, x, y, r)
  context.arcTo(x, y, x + width, y, r)
  context.closePath()
}

function fitText(
  context: CanvasRenderingContext2D,
  text: string,
  maxWidth: number,
): string {
  if (context.measureText(text).width <= maxWidth) return text
  let value = text
  while (value.length > 1 && context.measureText(`${value}…`).width > maxWidth) {
    value = value.slice(0, -1)
  }
  return `${value}…`
}

function edgePath(
  from: { x: number; y: number },
  to: { x: number; y: number },
  bend: number,
): { x: number; y: number } {
  const dx = to.x - from.x
  const dy = to.y - from.y
  const length = Math.hypot(dx, dy) || 1
  return {
    x: (from.x + to.x) / 2 + (-dy / length) * bend,
    y: (from.y + to.y) / 2 + (dx / length) * bend,
  }
}

function pointOnQuadratic(
  from: { x: number; y: number },
  control: { x: number; y: number },
  to: { x: number; y: number },
  t: number,
): { x: number; y: number } {
  const one = 1 - t
  return {
    x: one * one * from.x + 2 * one * t * control.x + t * t * to.x,
    y: one * one * from.y + 2 * one * t * control.y + t * t * to.y,
  }
}

function drawGraph(
  canvas: HTMLCanvasElement,
  overlay: boolean,
  progress: number,
  time: number,
): void {
  const context = setupCanvas(canvas)
  const scene = sceneFor(canvas, overlay)
  const value = layout.value
  if (!context || !scene || !value) return

  const color = (hex: string, alpha: number) => rgba(hex, alpha)
  const labelFontSize = overlay ? 12.5 : 10
  const labelFont = `500 ${labelFontSize}px "Sora", "PingFang SC", "Microsoft YaHei", sans-serif`
  const centerFont = `${overlay ? 15 : 10.5}px "Newsreader", "Noto Serif SC", "Songti SC", serif`
  const localAlpha = Math.min(1, progress * 1.35)

  context.save()
  context.globalAlpha = localAlpha

  context.save()
  context.strokeStyle = 'rgba(255,255,255,0.055)'
  context.lineWidth = 1
  context.setLineDash([2, 7])
  for (const radius of [0.58, 0.94]) {
    context.beginPath()
    context.arc(scene.cx, scene.cy, Math.max(0, radius * scene.scale * progress), 0, Math.PI * 2)
    context.stroke()
  }
  context.restore()

  const nodes = drawNodes(value, scene, progress, overlay)
  const nodeById = new Map(nodes.map((node) => [node.key, node]))
  const center = nodeById.get(value.current.id)
  if (!center) {
    context.restore()
    return
  }

  value.edges.forEach((edge, index) => {
    const source = nodeById.get(edge.source)
    const target = nodeById.get(edge.target)
    if (!source || !target) return
    const connected = hoverId.value
      ? hoverId.value === edge.source || hoverId.value === edge.target
      : false
    const dimmed = Boolean(hoverId.value) && !connected
    const alpha = dimmed ? 0.13 : connected ? 0.9 : 0.42
    const bend = (index % 2 === 0 ? 1 : -1) * (overlay ? 16 : 7)
    const control = edgePath(source, target, bend)
    const gradient = context.createLinearGradient(source.x, source.y, target.x, target.y)
    gradient.addColorStop(0, color(source.color, alpha))
    gradient.addColorStop(1, color(target.color, alpha))
    context.save()
    context.strokeStyle = gradient
    context.beginPath()
    context.moveTo(source.x, source.y)
    context.quadraticCurveTo(control.x, control.y, target.x, target.y)
    if (connected) {
      context.save()
      context.strokeStyle = color(target.color, 0.16)
      context.lineWidth = overlay ? 5 : 4
      context.stroke()
      context.restore()
    }
    context.lineWidth = connected ? (overlay ? 1.8 : 1.5) : overlay ? 1.2 : 1
    context.stroke()
    context.restore()

    if (connected && time) {
      const count = overlay ? 4 : 3
      for (let particle = 0; particle < count; particle += 1) {
        const t = (time * 0.00042 + particle / count + index * 0.11) % 1
        const point = pointOnQuadratic(source, control, target, t)
        context.save()
        context.fillStyle = color(particle % 2 === 0 ? source.color : target.color, 0.95)
        context.beginPath()
        context.arc(point.x, point.y, overlay ? 2.1 : 1.6, 0, Math.PI * 2)
        context.fill()
        context.restore()
      }
    }
  })

  const drawDot = (node: DrawNode, x: number, y: number, active: boolean): void => {
    const radius = node.r * (active ? 1.25 : 1)
    context.save()
    context.fillStyle = node.color
    context.beginPath()
    context.arc(x, y, radius, 0, Math.PI * 2)
    context.fill()
    context.strokeStyle = 'rgba(8, 11, 16, 0.9)'
    context.lineWidth = overlay ? 2 : 1.5
    context.stroke()
    context.strokeStyle = 'rgba(255,255,255,0.28)'
    context.lineWidth = 1
    context.beginPath()
    context.arc(x, y, radius + (overlay ? 3.2 : 2.2), 0, Math.PI * 2)
    context.stroke()
    context.restore()
  }

  for (const node of nodes) {
    if (node.center) continue
    drawDot(node, node.x, node.y, hoverId.value === node.key)
  }

  const disc = context.createRadialGradient(
    center.x - center.r * 0.2,
    center.y - center.r * 0.3,
    1,
    center.x,
    center.y,
    center.r,
  )
  disc.addColorStop(0, '#252b33')
  disc.addColorStop(1, '#10151b')
  context.fillStyle = disc
  context.beginPath()
  context.arc(center.x, center.y, center.r, 0, Math.PI * 2)
  context.fill()
  context.strokeStyle = rgba(COLORS.note, 0.72)
  context.lineWidth = overlay ? 1.4 : 1.1
  context.stroke()
  context.strokeStyle = 'rgba(255,255,255,0.08)'
  context.lineWidth = 1
  context.beginPath()
  context.arc(center.x, center.y, center.r + 5, 0, Math.PI * 2)
  context.stroke()

  context.textAlign = 'center'
  context.textBaseline = 'middle'
  context.font = centerFont
  context.fillStyle = '#f2eee4'
  const lines = titleLines(currentTitle.value, overlay ? 8 : 4)
  const lineHeight = overlay ? 20 : 14
  const firstY = center.y - ((lines.length - 1) * lineHeight) / 2
  lines.forEach((line, index) => {
    context.fillText(line, center.x, firstY + index * lineHeight)
  })
  if (overlay) {
    context.font = '500 9px "Sora", "PingFang SC", sans-serif'
    context.fillStyle = 'rgba(227,173,79,0.72)'
    context.fillText('CURRENT NOTE', center.x, center.y - center.r - 16)
  }

  context.font = labelFont
  context.textBaseline = 'middle'
  const boxes: Array<{ left: number; top: number; right: number; bottom: number }> = [
    {
      left: center.x - center.r,
      top: center.y - center.r,
      right: center.x + center.r,
      bottom: center.y + center.r,
    },
  ]

  const ordered = nodes
    .filter((node) => !node.center)
    .sort((a, b) => a.y - b.y)
  for (const node of ordered) {
    const full = overlay ? node.full : node.label
    const text = fitText(context, full, overlay ? 150 : 82)
    const width = context.measureText(text).width + (overlay ? 18 : 12)
    const height = labelFontSize + (overlay ? 10 : 7)
    const dx = node.x - scene.cx
    const dy = node.y - scene.cy
    const length = Math.hypot(dx, dy) || 1
    const dirX = dx / length
    const dirY = dy / length
    const anchorX = node.x + dirX * (node.r + (overlay ? 12 : 8))
    const anchorY = node.y + dirY * (node.r + (overlay ? 12 : 8))
    const rightSide = dirX > 0.25
    const leftSide = dirX < -0.25
    let left = rightSide
      ? anchorX
      : leftSide
        ? anchorX - width
        : anchorX - width / 2
    let top = anchorY - height / 2
    const limit = 4
    left = Math.max(limit, Math.min(scene.width - width - limit, left))
    top = Math.max(limit, Math.min(scene.height - height - limit, top))

    for (let attempt = 0; attempt < 4; attempt += 1) {
      let overlaps = false
      for (const box of boxes) {
        if (
          left < box.right + 2 &&
          left + width > box.left - 2 &&
          top < box.bottom + 2 &&
          top + height > box.top - 2
        ) {
          overlaps = true
          break
        }
      }
      if (!overlaps) break
      left += dirX * (overlay ? 12 : 7)
      top += dirY * (overlay ? 12 : 7)
      left = Math.max(limit, Math.min(scene.width - width - limit, left))
      top = Math.max(limit, Math.min(scene.height - height - limit, top))
    }

    boxes.push({ left, top, right: left + width, bottom: top + height })
    const active = hoverId.value === node.key
    const boxAlpha = active ? 0.94 : 0.74
    context.save()
    context.strokeStyle = active ? rgba(node.color, 0.55) : 'rgba(255,255,255,0.07)'
    context.lineWidth = 1
    context.beginPath()
    const startX = node.x + dirX * (node.r + 2)
    const startY = node.y + dirY * (node.r + 2)
    const endX = Math.max(left, Math.min(left + width, anchorX))
    const endY = Math.max(top, Math.min(top + height, anchorY))
    context.moveTo(startX, startY)
    context.lineTo(endX, endY)
    context.stroke()
    context.restore()

    context.save()
    context.fillStyle = `rgba(8, 12, 17, ${boxAlpha})`
    context.strokeStyle = active ? rgba(node.color, 0.55) : 'rgba(255,255,255,0.07)'
    roundRect(context, left, top, width, height, overlay ? 5 : 3)
    context.fill()
    context.stroke()
    context.fillStyle = active ? '#ffffff' : COLORS.text2
    context.textAlign = 'center'
    context.fillText(text, left + width / 2, top + height / 2 + 0.5)
    context.restore()
  }

  context.restore()
}

function schedule(): void {
  if (frame) return
  frame = requestAnimationFrame(tick)
}

function tick(time: number): void {
  frame = 0
  const elapsed = Math.min(1, (time - revealStart) / 620)
  const progress = easeOut(elapsed)
  drawAll(progress, hoverId.value ? time : 0)
  if (elapsed < 1 || hoverId.value) schedule()
}

function drawAll(progress: number, time: number): void {
  if (canvasRef.value) drawGraph(canvasRef.value, false, progress, time)
  if (expanded.value && overlayCanvasRef.value) {
    drawGraph(overlayCanvasRef.value, true, 1, time)
  }
}

function nodesInCanvas(
  canvas: HTMLCanvasElement,
  overlay: boolean,
  progress: number,
): DrawNode[] {
  const value = layout.value
  const scene = sceneFor(canvas, overlay)
  if (!value || !scene) return []
  return drawNodes(value, scene, progress, overlay)
}

function hitTest(
  canvas: HTMLCanvasElement,
  clientX: number,
  clientY: number,
  overlay: boolean,
): DrawNode | null {
  const rect = canvas.getBoundingClientRect()
  const x = clientX - rect.left
  const y = clientY - rect.top
  const candidates = nodesInCanvas(canvas, overlay, 1)
  for (let index = candidates.length - 1; index >= 0; index -= 1) {
    const node = candidates[index]
    if (node.center) continue
    if (Math.hypot(node.x - x, node.y - y) <= node.r + 9) return node
  }
  return null
}

function nodePosition(id: string): { x: number; y: number } | null {
  const value = layout.value
  if (!value) return null
  const attr = value.attrs.find((item) => item.node.id === id)
  if (attr) return attr
  const related = value.related.find((item) => item.node.id === id)
  return related ?? null
}

function updateTooltip(
  canvas: HTMLCanvasElement,
  overlay: boolean,
  node: DrawNode | null,
): void {
  if (overlay) {
    hoverId.value = node ? node.key : null
    return
  }
  const scene = sceneFor(canvas, false)
  if (!scene || !node) {
    hoverId.value = null
    tooltip.value = null
    return
  }
  hoverId.value = node.key
  const rect = canvas.getBoundingClientRect()
  const above = node.y - node.r - 8 > 80
  tooltip.value = {
    title: node.full,
    kind: kindLabel(node.kind),
    x: Math.max(112, Math.min(rect.width - 112, node.x)),
    y: above
      ? Math.max(8, node.y - node.r - 8)
      : Math.min(rect.height - 8, node.y + node.r + 10),
    below: !above,
  }
  schedule()
}

function pointerDown(event: PointerEvent, overlay: boolean): void {
  const canvas = event.currentTarget as HTMLCanvasElement
  const node = hitTest(canvas, event.clientX, event.clientY, overlay)
  if (!node) {
    if (overlay) {
      panning = {
        pointerId: event.pointerId,
        startX: event.clientX,
        startY: event.clientY,
        tx: viewState.tx,
        ty: viewState.ty,
      }
      canvas.setPointerCapture(event.pointerId)
      canvas.style.cursor = 'grabbing'
    }
    return
  }
  if (node.center) return
  dragging = {
    id: node.key,
    pointerId: event.pointerId,
    moved: false,
    startX: event.clientX,
    startY: event.clientY,
    overlay,
  }
  canvas.setPointerCapture(event.pointerId)
  canvas.style.cursor = 'grabbing'
}

function pointerMove(event: PointerEvent, overlay: boolean): void {
  const canvas = event.currentTarget as HTMLCanvasElement
  if (panning && panning.pointerId === event.pointerId) {
    viewState.tx = panning.tx + event.clientX - panning.startX
    viewState.ty = panning.ty + event.clientY - panning.startY
    drawAll(1, hoverId.value ? performance.now() : 0)
    return
  }
  if (dragging && dragging.pointerId === event.pointerId) {
    if (Math.hypot(event.clientX - dragging.startX, event.clientY - dragging.startY) > 3) {
      dragging.moved = true
    }
    const scene = sceneFor(canvas, overlay)
    const position = nodePosition(dragging.id)
    if (scene && position) {
      const rect = canvas.getBoundingClientRect()
      const nx = (event.clientX - rect.left - scene.cx) / scene.scale
      const ny = (event.clientY - rect.top - scene.cy) / scene.scale
      const distance = Math.hypot(nx, ny)
      if (distance > 0.98) {
        position.x = (nx / distance) * 0.98
        position.y = (ny / distance) * 0.98
      } else {
        position.x = nx
        position.y = ny
      }
      drawAll(1, hoverId.value ? performance.now() : 0)
    }
    return
  }
  const node = hitTest(canvas, event.clientX, event.clientY, overlay)
  canvas.style.cursor = node ? 'pointer' : overlay ? 'grab' : 'default'
  if ((node?.key ?? null) !== hoverId.value) {
    updateTooltip(canvas, overlay, node)
    drawAll(1, hoverId.value ? performance.now() : 0)
  }
}

function pointerUp(event: PointerEvent, overlay: boolean): void {
  const canvas = event.currentTarget as HTMLCanvasElement
  if (panning && panning.pointerId === event.pointerId) {
    panning = null
    canvas.style.cursor = 'grab'
    return
  }
  if (dragging && dragging.pointerId === event.pointerId) {
    const drag = dragging
    dragging = null
    canvas.style.cursor = 'pointer'
    if (!drag.moved) {
      const node = hitTest(canvas, event.clientX, event.clientY, overlay)
      if (node) activate(node)
    }
    drawAll(1, hoverId.value ? performance.now() : 0)
  }
}

function pointerLeave(): void {
  if (dragging || panning) return
  hoverId.value = null
  tooltip.value = null
  drawAll(1, 0)
}

function activate(node: DrawNode): void {
  if (node.noteId && node.noteId !== props.noteId) {
    closeOverlay()
    void router.push(`/note/${node.noteId}`)
    return
  }
  if (node.attrId) {
    closeOverlay()
    void router.push({ path: '/tags', query: { tag: node.attrId } })
  }
}

function wheel(event: WheelEvent): void {
  if (!expanded.value) return
  event.preventDefault()
  const canvas = event.currentTarget as HTMLCanvasElement
  const rect = canvas.getBoundingClientRect()
  const sx = event.clientX - rect.left
  const sy = event.clientY - rect.top
  const next = Math.min(2.4, Math.max(0.62, viewState.scale * Math.exp(-event.deltaY * 0.0012)))
  const ratio = next / viewState.scale
  viewState.tx = sx - (sx - viewState.tx) * ratio
  viewState.ty = sy - (sy - viewState.ty) * ratio
  viewState.scale = next
  drawAll(1, hoverId.value ? performance.now() : 0)
}

function openOverlay(): void {
  if (!layout.value) return
  expanded.value = true
  viewState.scale = 1
  viewState.tx = 0
  viewState.ty = 0
  hoverId.value = null
  tooltip.value = null
  void nextTick(() => {
    if (!overlayCardRef.value) return
    overlayObserver?.disconnect()
    overlayObserver = new ResizeObserver(() => drawAll(1, 0))
    overlayObserver.observe(overlayCardRef.value)
    drawAll(1, 0)
  })
}

function closeOverlay(): void {
  if (!expanded.value) return
  expanded.value = false
  overlayObserver?.disconnect()
  overlayObserver = null
  hoverId.value = null
  tooltip.value = null
  drawAll(1, 0)
}

function resetView(): void {
  viewState.scale = 1
  viewState.tx = 0
  viewState.ty = 0
  drawAll(1, 0)
}

function onKeydown(event: KeyboardEvent): void {
  if (event.key === 'Escape' && expanded.value) closeOverlay()
}

watch(
  () => props.noteId,
  () => {
    const value = graph.value
    layout.value = value ? layoutLocalGraph(value) : null
    hoverId.value = null
    tooltip.value = null
    expanded.value = false
    viewState.scale = 1
    viewState.tx = 0
    viewState.ty = 0
    revealStart = performance.now()
    schedule()
  },
  { immediate: true },
)

watch(
  () => props.notes.length,
  () => {
    if (!layout.value && graph.value) {
      layout.value = layoutLocalGraph(graph.value)
      revealStart = performance.now()
      schedule()
    }
  },
)

onMounted(() => {
  if (cardRef.value) {
    resizeObserver = new ResizeObserver(() => drawAll(1, 0))
    resizeObserver.observe(cardRef.value)
  }
  window.addEventListener('keydown', onKeydown)
  revealStart = performance.now()
  void document.fonts.ready.then(() => {
    if (!frame) drawAll(1, 0)
  })
  schedule()
})

onBeforeUnmount(() => {
  resizeObserver?.disconnect()
  overlayObserver?.disconnect()
  if (frame) cancelAnimationFrame(frame)
  window.removeEventListener('keydown', onKeydown)
})
</script>

<template>
  <section ref="cardRef" class="graph-card" aria-label="当前笔记的局部关系图">
    <header class="graph-head">
      <div class="graph-heading">
        <span class="graph-eyebrow">RELATION MAP</span>
        <b>这篇的关系</b>
      </div>
      <div class="graph-actions">
        <span>{{ SUMMARY }}</span>
        <button class="graph-expand" type="button" aria-label="全屏查看关系图" @click="openOverlay">
          <svg viewBox="0 0 24 24" aria-hidden="true">
            <path d="M9 3H3v6M15 3h6v6M9 21H3v-6M15 21h6v-6" />
          </svg>
        </button>
      </div>
    </header>

    <div class="graph-stage">
      <canvas
        ref="canvasRef"
        class="graph-canvas"
        role="img"
        :aria-label="`${currentTitle} 的局部关系图`"
        @pointerdown="pointerDown($event, false)"
        @pointermove="pointerMove($event, false)"
        @pointerup="pointerUp($event, false)"
        @pointercancel="pointerUp($event, false)"
        @pointerleave="pointerLeave"
      />
      <div
        v-if="tooltip"
        class="graph-tip"
        :class="{ 'is-below': tooltip.below }"
        :style="{ left: `${tooltip.x}px`, top: `${tooltip.y}px` }"
      >
        <span>{{ tooltip.kind }}</span>
        <b>{{ tooltip.title }}</b>
      </div>
    </div>

    <footer class="graph-foot">
      <div class="graph-legend">
        <span><i class="legend-note" />笔记</span>
        <span><i class="legend-tag" />标签</span>
        <span><i class="legend-person" />人物</span>
        <span><i class="legend-period" />时期</span>
      </div>
      <span class="graph-hint">拖动 · 点击打开</span>
    </footer>
  </section>

  <Teleport to="body">
    <div v-if="expanded" class="graph-overlay" @click.self="closeOverlay">
      <div ref="overlayCardRef" class="graph-modal">
        <header class="graph-modal-head">
          <div>
            <span class="graph-eyebrow">RELATION MAP</span>
            <b>{{ currentTitle }}</b>
          </div>
          <div class="graph-modal-actions">
            <button type="button" class="graph-ghost" @click="resetView">重置视图</button>
            <button type="button" class="graph-close" aria-label="关闭关系图" @click="closeOverlay">
              <svg viewBox="0 0 24 24" aria-hidden="true">
                <path d="M6 6l12 12M18 6L6 18" />
              </svg>
            </button>
          </div>
        </header>
        <div class="graph-stage graph-stage-large">
          <canvas
            ref="overlayCanvasRef"
            class="graph-canvas"
            role="img"
            :aria-label="`${currentTitle} 的局部关系图，全屏视图`"
            @pointerdown="pointerDown($event, true)"
            @pointermove="pointerMove($event, true)"
            @pointerup="pointerUp($event, true)"
            @pointercancel="pointerUp($event, true)"
            @pointerleave="pointerLeave"
            @wheel="wheel"
          />
        </div>
        <footer class="graph-modal-foot">
          <span>拖动节点 · 滚轮缩放 · 拖背景平移</span>
          <span>点击相邻笔记换一篇 · 点击标签去检索</span>
        </footer>
      </div>
    </div>
  </Teleport>
</template>
