<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'

import type { AgentTurn } from '../../../lib/agent/types'
import { isAgentNode, nodeDuration, type RunNode } from '../../../lib/runModel'

const props = defineProps<{
  nodes: RunNode[]
  selected: string
  startedAt: number
  running: boolean
  agentTurns: AgentTurn[]
  paused?: boolean
}>()
const emit = defineEmits<{ select: [key: string] }>()

const now = ref(Date.now())
// Agent rounds are the part worth seeing; collapse them only on request.
const expanded = ref(true)
let clock: number | undefined

const start = computed(() => {
  const first = props.nodes
    .filter((node) => node.startedAt > 0)
    .map((node) => node.startedAt)
    .sort((a, b) => a - b)[0]
  return props.startedAt || first || 0
})
const end = computed(() => {
  const last = props.nodes
    .filter((node) => node.endedAt > 0)
    .map((node) => node.endedAt)
    .sort((a, b) => b - a)[0]
  return props.running ? Math.max(now.value, last || 0) : last || now.value
})
const total = computed(() => Math.max(1, end.value - start.value))
const totalLabel = computed(() => formatDuration(total.value / 1000))
const ticks = computed(() =>
  [0, 0.25, 0.5, 0.75, 1].map((ratio) => ({
    ratio,
    label: formatDuration((total.value * ratio) / 1000),
  })),
)

function formatDuration(seconds: number): string {
  const value = Math.max(0, Math.round(seconds))
  if (value >= 60) return `${Math.floor(value / 60)}:${String(value % 60).padStart(2, '0')}`
  return `${value}s`
}

function percent(value: number): number {
  return Math.min(100, Math.max(0, (value / total.value) * 100))
}

function barStyle(node: RunNode): Record<string, string> {
  if (!node.startedAt) return { display: 'none' }
  const finish = node.endedAt || (props.running ? now.value : node.startedAt)
  return {
    left: `${percent(node.startedAt - start.value)}%`,
    width: `${Math.max(0.6, percent(finish - node.startedAt))}%`,
  }
}

function agentBarStyle(turn: AgentTurn): Record<string, string> {
  const base = props.nodes.find((node) => isAgentNode(node.key))
  const from = turn.startedAt || base?.startedAt || start.value
  const to = turn.endedAt || (props.running ? now.value : from)
  return {
    left: `${percent(from - start.value)}%`,
    width: `${Math.max(0.6, percent(to - from))}%`,
  }
}

/** The model segment sits before the tool segment, so the waterfall shows
 * where the round's time actually went instead of only the tool call. */
function agentModelStyle(turn: AgentTurn): Record<string, string> {
  if (!turn.modelMs) return { display: 'none' }
  const base = turn.startedAt || start.value
  const from = base - turn.modelMs
  return {
    left: `${percent(from - start.value)}%`,
    width: `${Math.max(0.6, percent(turn.modelMs))}%`,
  }
}

function turnSplit(turn: AgentTurn): string {
  const tool = Math.max(0, (turn.endedAt || (props.running ? now.value : turn.startedAt)) - turn.startedAt)
  const total = (turn.modelMs ?? 0) + tool
  return `${(total / 1000).toFixed(1)}s`
}

function turnLabel(turn: AgentTurn): string {
  return turn.tool ? `${turn.tool}` : turn.phase
}

function toggle(event: Event): void {
  event.stopPropagation()
  expanded.value = !expanded.value
}

onMounted(() => {
  clock = window.setInterval(() => {
    if (!props.paused) now.value = Date.now()
  }, 1000)
})
onBeforeUnmount(() => {
  if (clock) window.clearInterval(clock)
})
</script>

<template>
  <section class="run-timeline">
    <div class="run-timeline-head">
      <div>
        <b>时间线</b>
        <span>总耗时 {{ totalLabel }}</span>
      </div>
      <span class="run-timeline-hint">
        <template v-if="agentTurns.length">
          <i class="run-timeline-key is-model" />模型
          <i class="run-timeline-key is-tool" />工具 ·
        </template>
        点击条带定位节点
      </span>
    </div>

    <div class="run-timeline-axis">
      <span class="run-timeline-axis-label" />
      <div class="run-timeline-axis-track">
        <span v-for="tick in ticks" :key="tick.ratio" :style="{ left: `${tick.ratio * 100}%` }">{{ tick.label }}</span>
      </div>
      <span class="run-timeline-axis-dur" />
    </div>

    <div class="run-timeline-rows">
      <div
        v-for="node in nodes"
        :key="node.key"
        class="run-timeline-row"
        :class="[`is-${node.status}`, { 'is-selected': selected === node.key }]"
        @click="emit('select', node.key)"
      >
        <button class="run-timeline-label" type="button" :title="node.label" @click="emit('select', node.key)">
          <span v-if="isAgentNode(node.key) && agentTurns.length" class="run-timeline-toggle" @click="toggle">
            {{ expanded ? '−' : '+' }}
          </span>
          <span class="run-timeline-name">{{ node.label }}</span>
        </button>
        <div class="run-timeline-track">
          <span class="run-timeline-grid" />
          <span
            v-if="node.status !== 'queued' && node.status !== 'pending'"
            class="run-timeline-bar"
            :style="barStyle(node)"
            :title="`${node.label} · ${nodeDuration(node)}s`"
          />
          <span v-else class="run-timeline-wait">等待</span>
        </div>
        <span class="run-timeline-dur">{{ node.startedAt ? `${nodeDuration(node)}s` : '—' }}</span>
      </div>

      <template v-if="expanded && agentTurns.length">
        <div v-for="turn in agentTurns" :key="`turn-${turn.index}`" class="run-timeline-row is-turn">
          <span class="run-timeline-label is-sub">#{{ turn.index + 1 }} {{ turnLabel(turn) }}</span>
          <div class="run-timeline-track">
            <span class="run-timeline-grid" />
            <span class="run-timeline-bar is-turn is-model" :style="agentModelStyle(turn)" />
            <span class="run-timeline-bar is-turn" :style="agentBarStyle(turn)" />
          </div>
          <span class="run-timeline-dur">{{ turn.startedAt ? turnSplit(turn) : '—' }}</span>
        </div>
      </template>
    </div>
  </section>
</template>

<style scoped>
.run-timeline {
  flex: none;
  padding: 10px 12px 12px;
  border: 1px solid var(--line);
  border-radius: 6px;
  background: var(--surface);
  font-family: var(--sans);
}

.run-timeline-head {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: 10px;
  margin-bottom: 6px;
  color: var(--ink-3);
  font-size: 11px;
}

.run-timeline-head b {
  margin-right: 8px;
  color: var(--ink);
  font-size: 12px;
  font-weight: 600;
}

.run-timeline-hint {
  font-size: 11px;
}

.run-timeline-axis,
.run-timeline-row {
  display: grid;
  grid-template-columns: 112px minmax(0, 1fr) 44px;
  align-items: center;
  gap: 8px;
}

.run-timeline-axis {
  height: 16px;
  margin-bottom: 2px;
}

.run-timeline-axis-track {
  position: relative;
  height: 16px;
}

.run-timeline-axis-track span {
  position: absolute;
  top: 0;
  transform: translateX(-50%);
  font-family: var(--mono);
  font-size: 11px;
  color: var(--ink-3);
  white-space: nowrap;
}

.run-timeline-axis-track span:first-child {
  transform: none;
}

.run-timeline-axis-track span:last-child {
  transform: translateX(-100%);
}

.run-timeline-rows {
  display: flex;
  flex-direction: column;
  gap: 1px;
  max-height: 220px;
  overflow: auto;
}

.run-timeline-row {
  min-height: 22px;
  border: 1px solid transparent;
  border-radius: 3px;
  cursor: pointer;
}

.run-timeline-row:hover {
  background: color-mix(in srgb, var(--blue-soft) 35%, transparent);
}

.run-timeline-row.is-selected {
  border-color: var(--blue-line);
  background: var(--blue-soft);
}

.run-timeline-label {
  display: flex;
  align-items: center;
  gap: 4px;
  min-width: 0;
  padding: 0 4px;
  border: 0;
  background: transparent;
  color: var(--ink-2);
  font-family: var(--sans);
  font-size: 11px;
  text-align: left;
  cursor: pointer;
}

.run-timeline-label.is-sub {
  padding-left: 18px;
  font-family: var(--mono);
  font-size: 11px;
  color: var(--ink-3);
}

.run-timeline-name {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.run-timeline-toggle {
  flex: none;
  width: 12px;
  color: var(--ink-3);
  font-size: 11px;
}

.run-timeline-track {
  position: relative;
  height: 16px;
  border-radius: 3px;
  background: color-mix(in srgb, var(--muted) 70%, transparent);
  overflow: hidden;
}

.run-timeline-grid {
  position: absolute;
  inset: 0;
  background-image: linear-gradient(
    90deg,
    color-mix(in srgb, var(--line) 65%, transparent) 1px,
    transparent 1px
  );
  background-size: 25% 100%;
  pointer-events: none;
}

.run-timeline-bar {
  position: absolute;
  top: 3px;
  bottom: 3px;
  min-width: 3px;
  border-radius: 2px;
  background: var(--line-2);
}

.run-timeline-row.is-running .run-timeline-bar { background: var(--blue); }
.run-timeline-row.is-done .run-timeline-bar { background: var(--green); }
.run-timeline-row.is-error .run-timeline-bar { background: var(--red); }
.run-timeline-row.is-interrupted .run-timeline-bar { background: var(--gold); }
.run-timeline-row.is-cancelled .run-timeline-bar { background: var(--line-2); opacity: 0.6; }
.run-timeline-row.is-skipped .run-timeline-bar { background: var(--gold); opacity: 0.6; }
.run-timeline-bar.is-turn { background: color-mix(in srgb, var(--blue) 55%, var(--surface)); }
.run-timeline-bar.is-turn.is-model { background: color-mix(in srgb, var(--ink-3) 35%, var(--surface)); }
.run-timeline-key {
  display: inline-block;
  width: 8px;
  height: 8px;
  margin: 0 4px 0 8px;
  border-radius: 2px;
  vertical-align: -1px;
}
.run-timeline-key.is-model { background: color-mix(in srgb, var(--ink-3) 45%, var(--surface)); }
.run-timeline-key.is-tool { background: color-mix(in srgb, var(--blue) 55%, var(--surface)); }

.run-timeline-wait {
  position: absolute;
  left: 4px;
  top: 0;
  font-size: 11px;
  line-height: 16px;
  color: var(--ink-3);
}

.run-timeline-dur {
  font-family: var(--mono);
  font-size: 11px;
  color: var(--ink-3);
  text-align: right;
}

@media (prefers-reduced-motion: reduce) {
  .run-timeline-bar {
    transition: none;
  }
}
</style>
