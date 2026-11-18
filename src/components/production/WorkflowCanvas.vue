<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import type { ProductionRunArtifact } from '../../types'

export interface FlowNode {
  key: string
  label: string
  status: string
  startedAt: number
  endedAt: number
  tokens: number
  cost: number
  summary: string
  error: string
  artifacts: ProductionRunArtifact[]
}

const props = defineProps<{ nodes: FlowNode[]; selected: string }>()
const emit = defineEmits<{ select: [key: string] }>()

const CARD_W = 160
const CARD_H = 72
const GAP_X = 34
const GAP_Y = 48
const PAD = 16
const PER_ROW = 4

const scroller = ref<HTMLElement | null>(null)
const scale = ref(1)
let observer: ResizeObserver | null = null

const rows = computed(() => Math.max(1, Math.ceil(props.nodes.length / PER_ROW)))
const width = computed(
  () => Math.min(props.nodes.length, PER_ROW) * CARD_W + Math.max(0, Math.min(props.nodes.length, PER_ROW) - 1) * GAP_X + PAD * 2,
)
const height = computed(() => rows.value * CARD_H + Math.max(0, rows.value - 1) * GAP_Y + PAD * 2)
const stageWidth = computed(() => width.value * scale.value)
const stageHeight = computed(() => height.value * scale.value)

// Snake layout: even rows run left-to-right, odd rows right-to-left, so the
// wrap between rows is a short vertical link instead of a long return line.
const positions = computed(() =>
  props.nodes.map((_, index) => {
    const row = Math.floor(index / PER_ROW)
    const col = row % 2 === 0 ? index % PER_ROW : PER_ROW - 1 - (index % PER_ROW)
    return { x: PAD + col * (CARD_W + GAP_X), y: PAD + row * (CARD_H + GAP_Y), row, col }
  }),
)

const edges = computed(() => {
  const list: Array<{ d: string; from: FlowNode; to: FlowNode; active: boolean }> = []
  for (let index = 0; index < props.nodes.length - 1; index += 1) {
    const from = positions.value[index]
    const to = positions.value[index + 1]
    let d = ''
    if (from.row === to.row) {
      const leftToRight = to.x > from.x
      const x1 = leftToRight ? from.x + CARD_W : from.x
      const x2 = leftToRight ? to.x - 2 : to.x + CARD_W + 2
      d = `M${x1} ${from.y + CARD_H / 2} H${x2}`
    } else {
      d = `M${from.x + CARD_W / 2} ${from.y + CARD_H} V${to.y - 2}`
    }
    const target = props.nodes[index + 1]
    list.push({
      d,
      from: props.nodes[index],
      to: target,
      active: target.status === 'running' && props.nodes[index].status !== 'queued',
    })
  }
  return list
})

function fit(): void {
  const el = scroller.value
  if (!el) return
  const available = el.clientWidth - 2
  scale.value = Math.min(1, Math.max(0.6, available / width.value))
}

onMounted(() => {
  fit()
  observer = new ResizeObserver(fit)
  if (scroller.value) observer.observe(scroller.value)
})
onBeforeUnmount(() => observer?.disconnect())

function seconds(node: FlowNode): string {
  if (!node.startedAt) return ''
  const end = node.endedAt || Date.now()
  const value = Math.max(0, Math.round((end - node.startedAt) / 1000))
  return value >= 60 ? `${Math.floor(value / 60)}:${String(value % 60).padStart(2, '0')}` : `${value}s`
}

function meta(node: FlowNode): string {
  if (node.status === 'queued') return '等待'
  const parts = [seconds(node)]
  if (node.tokens) parts.push(`${node.tokens} tok`)
  if (node.cost) parts.push(`¥${node.cost.toFixed(3)}`)
  return parts.filter(Boolean).join(' · ')
}
</script>

<template>
  <div ref="scroller" class="wf-canvas" :style="{ height: `${stageHeight + 12}px` }">
    <div class="wf-stage" :style="{ width: `${stageWidth}px`, height: `${stageHeight}px` }">
      <div class="wf-inner" :style="{ width: `${width}px`, height: `${height}px`, transform: `scale(${scale})` }">
        <svg class="wf-edges" :width="width" :height="height" aria-hidden="true">
          <defs>
            <marker id="wf-arrow" viewBox="0 0 8 8" refX="7" refY="4" markerWidth="6" markerHeight="6" orient="auto">
              <path d="M0 0 L8 4 L0 8 z" fill="currentColor" />
            </marker>
          </defs>
          <path
            v-for="(edge, index) in edges"
            :key="`edge-${index}`"
            :d="edge.d"
            :class="{
              'is-skipped': edge.to.status === 'skipped',
              'is-error': edge.to.status === 'error',
              'is-active': edge.active,
            }"
            marker-end="url(#wf-arrow)"
          />
          <circle v-for="(edge, index) in edges.filter((item) => item.active)" :key="`spark-${index}`" class="wf-spark" r="3">
            <animateMotion :path="edge.d" dur="1.3s" repeatCount="indefinite" />
          </circle>
        </svg>

        <button
          v-for="(node, index) in nodes"
          :key="node.key"
          type="button"
          class="wf-node"
          :class="[`is-${node.status}`, { 'is-selected': selected === node.key }]"
          :style="{ left: `${positions[index].x}px`, top: `${positions[index].y}px` }"
          :aria-label="`${node.label} · ${node.status}`"
          @click="emit('select', node.key)"
        >
          <span class="wf-dot" />
          <span class="wf-n">{{ index + 1 }}</span>
          <b class="wf-label">{{ node.label }}</b>
          <small class="wf-meta">{{ meta(node) }}</small>
          <span v-if="node.artifacts.length" class="wf-art">{{ node.artifacts.length }} 产物</span>
          <span v-if="node.status === 'skipped'" class="wf-skip">已跳过</span>
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.wf-canvas {
  flex: none;
  overflow: hidden;
  padding: 2px 0 6px;
}

.wf-stage {
  position: relative;
}

.wf-inner {
  position: relative;
  transform-origin: top left;
}

.wf-edges {
  position: absolute;
  inset: 0;
  color: var(--line-2);
  pointer-events: none;
}

.wf-edges path {
  fill: none;
  stroke: currentColor;
  stroke-width: 1.2;
}

.wf-edges path.is-skipped {
  stroke-dasharray: 4 3;
  opacity: 0.55;
}

.wf-edges path.is-error {
  color: var(--red);
}

.wf-edges path.is-active {
  color: var(--blue);
  stroke-width: 1.8;
  stroke-dasharray: 5 4;
  animation: wf-dash 0.8s linear infinite;
}

.wf-spark {
  fill: var(--blue);
}

.wf-node.is-running .wf-dot {
  animation: wf-pulse 1.2s ease-in-out infinite;
}

@keyframes wf-dash {
  to { stroke-dashoffset: -18; }
}

@keyframes wf-pulse {
  0%, 100% { box-shadow: 0 0 0 0 color-mix(in srgb, var(--blue) 40%, transparent); }
  50% { box-shadow: 0 0 0 4px color-mix(in srgb, var(--blue) 0%, transparent); }
}

@media (prefers-reduced-motion: reduce) {
  .wf-edges path.is-active,
  .wf-node.is-running .wf-dot {
    animation: none;
  }
  .wf-spark {
    display: none;
  }
}

.wf-node {
  position: absolute;
  width: 160px;
  height: 72px;
  text-align: left;
  border: 1px solid var(--line);
  border-radius: 6px;
  background: var(--paper);
  padding: 8px 10px 7px;
  display: grid;
  grid-template-rows: auto auto 1fr;
  gap: 2px;
  font-family: var(--sans);
  color: var(--ink-2);
}

.wf-node:hover {
  border-color: var(--line-2);
}

.wf-node.is-selected {
  border-color: var(--blue-line);
  box-shadow: 0 0 0 2px var(--blue-soft);
}

.wf-node.is-running {
  border-color: var(--blue-line);
  background: var(--blue-soft);
}

.wf-node.is-error {
  border-color: color-mix(in srgb, var(--red) 45%, transparent);
  background: color-mix(in srgb, var(--red) 6%, var(--paper));
}

.wf-node.is-skipped {
  border-style: dashed;
  color: var(--ink-3);
}

.wf-dot {
  position: absolute;
  top: 10px;
  right: 10px;
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--line-2);
}

.wf-n {
  position: absolute;
  right: 10px;
  bottom: 7px;
  font-family: var(--mono);
  font-size: 9.5px;
  color: var(--ink-3);
}

.wf-node.is-running .wf-n {
  color: var(--blue);
}

.wf-node.is-running .wf-dot { background: var(--blue); }
.wf-node.is-done .wf-dot { background: var(--green); }
.wf-node.is-error .wf-dot { background: var(--red); }
.wf-node.is-skipped .wf-dot { background: transparent; border: 1px solid var(--line-2); }

.wf-label {
  font-size: 12.5px;
  line-height: 1.4;
  font-weight: 600;
  color: var(--ink);
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
  padding-right: 12px;
}

.wf-node.is-skipped .wf-label,
.wf-node.is-queued .wf-label {
  color: var(--ink-2);
}

.wf-meta {
  font-family: var(--mono);
  font-size: 10.5px;
  color: var(--ink-3);
}

.wf-art,
.wf-skip {
  align-self: end;
  font-size: 10.5px;
  color: var(--ink-3);
}

.wf-skip {
  color: var(--gold);
}
</style>
