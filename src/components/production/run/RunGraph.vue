<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'

import {
  NODE_H,
  NODE_W,
  isAgentNode,
  layoutRun,
  type RunNode,
} from '../../../lib/runModel'

const props = defineProps<{
  nodes: RunNode[]
  selected: string
  agentRounds: number
  agentTool: string
  agentRunning: boolean
  paused?: boolean
  reservedRight?: number
}>()
const emit = defineEmits<{ select: [key: string] }>()

const scroller = ref<HTMLElement | null>(null)
const zoom = ref(1)
const pan = ref({ x: 0, y: 0 })
const autoFit = ref(true)
const now = ref(Date.now())
let resizeObserver: ResizeObserver | null = null
let clock: number | undefined

const layout = computed(() => layoutRun(props.nodes))
const stageW = computed(() => layout.value.width)
const stageH = computed(() => layout.value.height)
const zoomLabel = computed(() => `${Math.round(zoom.value * 100)}%`)

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value))
}

function fit(readable = true): void {
  const el = scroller.value
  if (!el) return
  const availableW = Math.max(200, el.clientWidth - 32)
  const availableH = Math.max(180, el.clientHeight - 32)
  const min = readable ? 0.85 : 0.5
  const next = clamp(Math.min(availableW / stageW.value, availableH / stageH.value, 1.15), min, 1.15)
  zoom.value = next
  pan.value = {
    x: Math.max(16, (el.clientWidth - stageW.value * next) / 2),
    y: Math.max(12, (el.clientHeight - stageH.value * next) / 2),
  }
}

function zoomTo(next: number): void {
  const el = scroller.value
  if (!el) return
  const value = clamp(next, 0.5, 1.6)
  const px = el.clientWidth / 2
  const py = el.clientHeight / 2
  const wx = (px - pan.value.x) / zoom.value
  const wy = (py - pan.value.y) / zoom.value
  autoFit.value = false
  zoom.value = value
  pan.value = { x: px - wx * value, y: py - wy * value }
}

function onWheel(event: WheelEvent): void {
  const el = scroller.value
  if (!el) return
  event.preventDefault()
  const rect = el.getBoundingClientRect()
  const px = event.clientX - rect.left
  const py = event.clientY - rect.top
  const factor = event.deltaY < 0 ? 1.08 : 0.92
  const next = clamp(zoom.value * factor, 0.5, 1.6)
  const wx = (px - pan.value.x) / zoom.value
  const wy = (py - pan.value.y) / zoom.value
  autoFit.value = false
  zoom.value = next
  pan.value = { x: px - wx * next, y: py - wy * next }
}

let dragging = false
let dragStart = { x: 0, y: 0, panX: 0, panY: 0 }

function onPointerDown(event: PointerEvent): void {
  if (event.button !== 0) return
  const target = event.target as HTMLElement
  if (target.closest('.run-node') || target.closest('.run-zoom')) return
  const el = scroller.value
  if (!el) return
  dragging = true
  dragStart = { x: event.clientX, y: event.clientY, panX: pan.value.x, panY: pan.value.y }
  el.setPointerCapture(event.pointerId)
  el.classList.add('is-panning')
}

function onPointerMove(event: PointerEvent): void {
  if (!dragging) return
  autoFit.value = false
  pan.value = {
    x: dragStart.panX + (event.clientX - dragStart.x),
    y: dragStart.panY + (event.clientY - dragStart.y),
  }
}

function onPointerUp(event: PointerEvent): void {
  if (!dragging) return
  dragging = false
  scroller.value?.classList.remove('is-panning')
  if (scroller.value?.hasPointerCapture(event.pointerId)) scroller.value.releasePointerCapture(event.pointerId)
}

function resetZoom(): void {
  const el = scroller.value
  if (!el) return
  autoFit.value = false
  zoom.value = 1
  pan.value = {
    x: Math.max(16, (el.clientWidth - stageW.value) / 2),
    y: Math.max(12, (el.clientHeight - stageH.value) / 2),
  }
}

function fitNow(): void {
  autoFit.value = true
  fit(false)
}

/**
 * Keep the selected node out from under the narrow-screen inspector drawer.
 * The drawer overlays the right side, so shift the stage left when needed.
 */
function ensureVisible(): void {
  const el = scroller.value
  const key = props.selected
  if (!el || !key) return
  const node = layout.value.nodes.find((item) => item.key === key)
  if (!node) return
  const reserved = props.reservedRight ?? 0
  const visibleW = el.clientWidth - reserved
  const left = pan.value.x + node.x * zoom.value
  const right = pan.value.x + (node.x + NODE_W) * zoom.value
  const top = pan.value.y + node.y * zoom.value
  const bottom = pan.value.y + (node.y + NODE_H) * zoom.value
  let dx = 0
  let dy = 0
  if (right > visibleW - 20) dx = visibleW - 20 - right
  else if (left < 20) dx = 20 - left
  if (bottom > el.clientHeight - 20) dy = el.clientHeight - 20 - bottom
  else if (top < 20) dy = 20 - top
  if (dx || dy) {
    autoFit.value = false
    pan.value = { x: pan.value.x + dx, y: pan.value.y + dy }
  }
}

function statusLabel(status: string): string {
  return (
    {
      running: '进行中',
      done: '完成',
      error: '失败',
      queued: '等待',
      pending: '等待',
      skipped: '跳过',
      interrupted: '已中断',
      cancelled: '已取消',
    } as Record<string, string>
  )[status] ?? status
}

function duration(node: RunNode): string {
  if (!node.startedAt) return ''
  const end = node.endedAt || now.value
  const seconds = Math.max(0, Math.round((end - node.startedAt) / 1000))
  return seconds >= 60 ? `${Math.floor(seconds / 60)}m${String(seconds % 60).padStart(2, '0')}s` : `${seconds}s`
}

function nodeMeta(node: RunNode): string {
  if (isAgentNode(node.key)) {
    if (props.agentRunning) {
      return `第 ${props.agentRounds} 回合${props.agentTool ? ` · ${props.agentTool}` : ''}`
    }
    if (props.agentRounds) return `↻ ${props.agentRounds} 回合`
  }
  if (node.status === 'queued' || node.status === 'pending') return '等待'
  const parts = [duration(node)]
  if (node.tokens) parts.push(`${Math.round(node.tokens / 100) / 10}k tok`)
  return parts.filter(Boolean).join(' · ')
}

watch(
  () => [stageW.value, stageH.value],
  async () => {
    if (!autoFit.value) return
    await nextTick()
    fit()
  },
)

watch(
  [() => props.selected, () => props.reservedRight],
  async () => {
    await nextTick()
    ensureVisible()
  },
)

onMounted(() => {
  fit()
  clock = window.setInterval(() => {
    if (!props.paused) now.value = Date.now()
  }, 1000)
  resizeObserver = new ResizeObserver(() => {
    if (autoFit.value) fit()
  })
  if (scroller.value) resizeObserver.observe(scroller.value)
})

onBeforeUnmount(() => {
  if (clock) window.clearInterval(clock)
  resizeObserver?.disconnect()
})
</script>

<template>
  <section class="run-canvas-shell">
    <div
      ref="scroller"
      class="run-canvas"
      @wheel="onWheel"
      @pointerdown="onPointerDown"
      @pointermove="onPointerMove"
      @pointerup="onPointerUp"
      @pointercancel="onPointerUp"
      @dblclick="fitNow"
    >
      <div
        class="run-stage"
        :style="{
          width: `${stageW}px`,
          height: `${stageH}px`,
          transform: `translate(${pan.x}px, ${pan.y}px) scale(${zoom})`,
        }"
      >
        <div
          v-for="band in layout.bands"
          :key="`band-${band.key}`"
          class="run-band"
          :class="`is-${band.key}`"
          :style="{ left: `${band.x}px`, top: `${band.y}px`, width: `${band.width}px`, height: `${band.height}px` }"
        >
          <span>{{ band.label }}</span>
        </div>

        <svg class="run-edges" :width="stageW" :height="stageH" aria-hidden="true">
          <defs>
            <marker id="run-arrow" viewBox="0 0 8 8" refX="7" refY="4" markerWidth="6" markerHeight="6" orient="auto">
              <path d="M0 0 L8 4 L0 8 z" fill="currentColor" />
            </marker>
          </defs>
          <path
            v-for="(edge, index) in layout.edges"
            :key="`edge-${edge.from.key}-${edge.to.key}-${index}`"
            :d="edge.d"
            :class="{
              'is-active': edge.active,
              'is-error': edge.to.status === 'error',
              'is-skipped': edge.to.status === 'skipped',
            }"
            marker-end="url(#run-arrow)"
          />
          <circle
            v-for="(edge, index) in layout.edges.filter((item) => item.active)"
            :key="`spark-${index}`"
            class="run-spark"
            r="3"
          >
            <animateMotion :path="edge.d" dur="1.35s" repeatCount="indefinite" />
          </circle>
        </svg>

        <button
          v-for="(node, index) in layout.nodes"
          :key="node.key"
          type="button"
          class="run-node"
          :class="[`is-${node.status}`, { 'is-selected': selected === node.key, 'is-agent': isAgentNode(node.key) }]"
          :style="{ left: `${node.x}px`, top: `${node.y}px` }"
          :aria-label="`${node.label} · ${statusLabel(node.status)}`"
          @click="emit('select', node.key)"
        >
          <span class="run-node-index">{{ String(index + 1).padStart(2, '0') }}</span>
          <span class="run-node-dot" />
          <b class="run-node-label">{{ node.label }}</b>
          <span class="run-node-status">{{ statusLabel(node.status) }}</span>
          <span class="run-node-meta">{{ nodeMeta(node) }}</span>
          <span v-if="node.status === 'running'" class="run-node-progress" aria-hidden="true" />
        </button>
      </div>
    </div>

    <div class="run-zoom">
      <button type="button" title="适配画布" @click="fitNow">适配</button>
      <button type="button" title="缩小" @click="zoomTo(zoom - 0.1)">−</button>
      <button type="button" title="实际大小" @click="resetZoom">{{ zoomLabel }}</button>
      <button type="button" title="放大" @click="zoomTo(zoom + 0.1)">＋</button>
    </div>
  </section>
</template>

<style scoped>
.run-canvas-shell {
  position: relative;
  flex: 1 1 auto;
  min-height: 0;
  border: 1px solid var(--line);
  border-radius: 6px;
  background:
    radial-gradient(circle at 1px 1px, color-mix(in srgb, var(--line) 55%, transparent) 1px, transparent 0) 0 0 / 18px 18px,
    var(--paper);
  overflow: hidden;
}

.run-canvas {
  position: absolute;
  inset: 0;
  overflow: hidden;
  cursor: grab;
  touch-action: none;
}

.run-canvas.is-panning {
  cursor: grabbing;
}

.run-stage {
  position: relative;
  transform-origin: 0 0;
  will-change: transform;
}

.run-band {
  position: absolute;
  display: flex;
  align-items: flex-start;
  padding: 7px 10px;
  border: 1px dashed color-mix(in srgb, var(--line) 85%, transparent);
  border-radius: 7px;
  background: color-mix(in srgb, var(--surface) 55%, transparent);
  font-family: var(--sans);
  font-size: 11px;
  color: var(--ink-3);
  pointer-events: none;
}

.run-band.is-quality {
  background: color-mix(in srgb, var(--blue-soft) 42%, transparent);
  border-color: color-mix(in srgb, var(--blue-line) 55%, transparent);
}

.run-band.is-publish {
  background: color-mix(in srgb, var(--green) 5%, transparent);
}

.run-band.is-export {
  background: color-mix(in srgb, var(--gold) 6%, transparent);
}

.run-edges {
  position: absolute;
  inset: 0;
  color: var(--line-2);
  pointer-events: none;
}

.run-edges path {
  fill: none;
  stroke: currentColor;
  stroke-width: 1.4;
}

.run-edges path.is-error {
  color: var(--red);
  stroke-dasharray: 4 3;
}

.run-edges path.is-skipped {
  stroke-dasharray: 3 4;
  opacity: 0.55;
}

.run-edges path.is-active {
  color: var(--blue);
  stroke-width: 2;
  stroke-dasharray: 6 5;
  animation: run-dash 0.9s linear infinite;
}

.run-spark {
  fill: var(--blue);
}

@keyframes run-dash {
  to { stroke-dashoffset: -22; }
}

.run-node {
  position: absolute;
  width: 156px;
  height: 84px;
  padding: 9px 10px 8px;
  border: 1px solid var(--line);
  border-radius: 6px;
  background: var(--surface);
  color: var(--ink-2);
  text-align: left;
  font-family: var(--sans);
  display: grid;
  grid-template-rows: auto auto 1fr;
  gap: 2px;
  cursor: pointer;
  box-shadow: 0 1px 0 color-mix(in srgb, var(--ink) 3%, transparent);
}

.run-node:hover {
  border-color: var(--line-2);
}

.run-node.is-selected {
  border-color: var(--blue);
  box-shadow: 0 0 0 2px var(--blue-soft);
}

.run-node.is-running {
  border-color: var(--blue-line);
  background: color-mix(in srgb, var(--blue-soft) 55%, var(--surface));
}

.run-node.is-error {
  border-color: color-mix(in srgb, var(--red) 55%, var(--line));
  background: color-mix(in srgb, var(--red) 5%, var(--surface));
}

.run-node.is-interrupted {
  border-color: color-mix(in srgb, var(--gold) 55%, var(--line));
  background: color-mix(in srgb, var(--gold) 6%, var(--surface));
}

.run-node.is-cancelled {
  border-style: dashed;
  opacity: 0.78;
}

.run-node.is-skipped {
  border-style: dashed;
  color: var(--ink-3);
}

.run-node-index {
  position: absolute;
  top: 7px;
  right: 9px;
  font-family: var(--mono);
  font-size: 10.5px;
  color: var(--ink-3);
}

.run-node-dot {
  position: absolute;
  top: 9px;
  left: 10px;
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--line-2);
}

.run-node.is-running .run-node-dot {
  background: var(--blue);
  animation: run-pulse 1.2s ease-in-out infinite;
}

.run-node.is-done .run-node-dot { background: var(--green); }
.run-node.is-error .run-node-dot { background: var(--red); }
.run-node.is-interrupted .run-node-dot { background: var(--gold); }
.run-node.is-cancelled .run-node-dot { background: transparent; border: 1px solid var(--line-2); }
.run-node.is-skipped .run-node-dot { background: transparent; border: 1px solid var(--line-2); }

.run-node-label {
  padding-left: 14px;
  padding-right: 22px;
  font-size: 13px;
  font-weight: 600;
  line-height: 1.35;
  color: var(--ink);
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
}

.run-node.is-skipped .run-node-label,
.run-node.is-queued .run-node-label {
  color: var(--ink-2);
}

.run-node-status {
  font-size: 11px;
  color: var(--ink-3);
}

.run-node.is-running .run-node-status { color: var(--blue); }
.run-node.is-error .run-node-status { color: var(--red); }
.run-node.is-done .run-node-status { color: var(--green); }

.run-node-meta {
  align-self: end;
  font-family: var(--mono);
  font-size: 11px;
  line-height: 1.3;
  color: var(--ink-3);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.run-node.is-agent .run-node-meta {
  color: var(--blue);
}

.run-node-progress {
  position: absolute;
  left: 0;
  right: 0;
  bottom: 0;
  height: 2px;
  border-radius: 0 0 5px 5px;
  background: linear-gradient(90deg, transparent, var(--blue), transparent);
  background-size: 50% 100%;
  animation: run-progress 1.4s linear infinite;
}

@keyframes run-progress {
  from { background-position: -50% 0; }
  to { background-position: 150% 0; }
}

@keyframes run-pulse {
  0%, 100% { box-shadow: 0 0 0 0 color-mix(in srgb, var(--blue) 45%, transparent); }
  50% { box-shadow: 0 0 0 4px color-mix(in srgb, var(--blue) 0%, transparent); }
}

.run-zoom {
  position: absolute;
  right: 10px;
  bottom: 10px;
  display: flex;
  gap: 3px;
  padding: 3px;
  border: 1px solid var(--line);
  border-radius: 5px;
  background: color-mix(in srgb, var(--surface) 92%, transparent);
  box-shadow: 0 2px 8px color-mix(in srgb, var(--ink) 6%, transparent);
}

.run-zoom button {
  min-width: 26px;
  height: 24px;
  padding: 0 6px;
  border: 0;
  border-radius: 3px;
  background: transparent;
  font-family: var(--mono);
  font-size: 11px;
  color: var(--ink-2);
  cursor: pointer;
}

.run-zoom button:hover {
  background: var(--blue-soft);
  color: var(--blue);
}

@media (prefers-reduced-motion: reduce) {
  .run-edges path.is-active,
  .run-node.is-running .run-node-dot,
  .run-node-progress {
    animation: none;
  }
  .run-spark {
    display: none;
  }
}
</style>
