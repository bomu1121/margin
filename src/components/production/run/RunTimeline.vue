<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'

import type { AgentTurn } from '../../../lib/agent/types'
import { isAgentNode, type RunNode } from '../../../lib/runModel'

const props = defineProps<{
  nodes: RunNode[]
  selected: string
  startedAt: number
  running: boolean
  agentTurns: AgentTurn[]
  paused?: boolean
  finishedAt?: number
  hideToggle?: boolean
}>()
const emit = defineEmits<{ select: [key: string] }>()

const now = ref(Date.now())
const expanded = ref(true)
let clock: number | undefined

const agentNode = computed(() => props.nodes.find((node) => isAgentNode(node.key)) ?? null)
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
  return props.running ? Math.max(now.value, last || 0) : props.finishedAt || last || start.value
})
const total = computed(() => Math.max(1, end.value - start.value))
const ticks = computed(() =>
  [0, 0.25, 0.5, 0.75, 1].map((ratio) => ({
    ratio,
    label: durationSeconds((total.value * ratio) / 1000),
  })),
)

function percent(value: number): number {
  return Math.min(100, Math.max(0, (value / total.value) * 100))
}

function durationSeconds(seconds: number): string {
  const value = Math.max(0, seconds)
  if (value >= 60) {
    const whole = Math.round(value)
    return `${Math.floor(whole / 60)}:${String(whole % 60).padStart(2, '0')}`
  }
  return `${value.toFixed(1)}s`
}

function formatMs(ms: number): string {
  if (ms <= 0) return '—'
  if (ms < 1000) return `${Math.round(ms)}ms`
  return `${(ms / 1000).toFixed(2)}s`
}

function nodeDuration(node: RunNode): string {
  if (!node.startedAt) return '—'
  const endAt = node.endedAt || (props.running ? now.value : props.finishedAt || node.startedAt)
  return formatMs(endAt - node.startedAt)
}

function barStyle(node: RunNode): Record<string, string> {
  if (!node.startedAt) return { display: 'none' }
  const finish = node.endedAt || (props.running ? now.value : node.startedAt)
  return {
    left: `${percent(node.startedAt - start.value)}%`,
    width: `${Math.max(0.4, percent(finish - node.startedAt))}%`,
  }
}

function turnStart(turn: AgentTurn): number {
  return turn.startedAt || agentNode.value?.startedAt || start.value
}

function turnEnd(turn: AgentTurn): number {
  return turn.endedAt || (props.running ? now.value : props.finishedAt || turnStart(turn))
}

function turnStyle(turn: AgentTurn): Record<string, string> {
  const from = turnStart(turn)
  const to = turnEnd(turn)
  return {
    left: `${percent(from - start.value)}%`,
    width: `${Math.max(0.25, percent(to - from))}%`,
  }
}

function turnMs(turn: AgentTurn): number {
  if (turn.tool) return turn.toolMs ?? Math.max(0, turnEnd(turn) - turnStart(turn))
  return turn.modelMs ?? Math.max(0, turnEnd(turn) - turnStart(turn))
}

function toolTimeLabel(turn: AgentTurn): string {
  const ms = turnMs(turn)
  return ms > 0 ? formatMs(ms) : '<1ms'
}

interface AgentRound {
  key: number
  index: number
  model: AgentTurn | null
  tools: AgentTurn[]
}

const rounds = computed<AgentRound[]>(() => {
  const list: AgentRound[] = []
  for (const turn of [...props.agentTurns].sort((a, b) => a.index - b.index)) {
    if (!turn.tool) {
      list.push({ key: turn.index, index: list.length + 1, model: turn, tools: [] })
      continue
    }
    const last = list[list.length - 1]
    if (last) last.tools.push(turn)
    else list.push({ key: turn.index, index: list.length + 1, model: null, tools: [turn] })
  }
  return list
})

const summary = computed(() => {
  let modelMs = 0
  let stepsMs = 0
  let agentMs = 0
  let peak = 0
  let peakRound = 0
  for (const round of rounds.value) {
    const ms = round.model ? turnMs(round.model) : 0
    modelMs += ms
    if (ms > peak) {
      peak = ms
      peakRound = round.index
    }
  }
  for (const node of props.nodes) {
    if (!node.startedAt) continue
    const ms = Math.max(0, (node.endedAt || (props.running ? now.value : props.finishedAt || node.startedAt)) - node.startedAt)
    if (isAgentNode(node.key)) agentMs += ms
    else stepsMs += ms
  }
  return {
    modelMs,
    stepsMs,
    agentMs,
    peak,
    peakRound,
    pct: modelMs > 0 ? Math.round((peak / modelMs) * 100) : 0,
    gapMs: Math.max(0, total.value - agentMs - stepsMs),
  }
})

function roundModelMs(round: AgentRound): number {
  return round.model ? turnMs(round.model) : 0
}

function roundTotalMs(round: AgentRound): number {
  return roundModelMs(round) + round.tools.reduce((sum, tool) => sum + turnMs(tool), 0)
}

function roundStyle(round: AgentRound): Record<string, string> {
  return round.model ? turnStyle(round.model) : { display: 'none' }
}

function toolArg(turn: AgentTurn): string {
  const args = turn.args ?? {}
  const value = args.path ?? args.file ?? args.query ?? args.pattern ?? args.name
  if (typeof value !== 'string') return ''
  return value.length > 34 ? `…${value.slice(-33)}` : value
}

function selectNode(key: string): void {
  emit('select', key)
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
    <header class="run-timeline-head">
      <div class="run-timeline-title">
        <b>时间线</b>
        <span v-if="agentTurns.length" title="从第一个节点开始到最后一个节点结束">跨度 {{ formatMs(total) }}</span>
      </div>
      <div class="run-timeline-legend">
        <span><i class="is-step" />步骤</span>
        <template v-if="agentTurns.length">
          <span><i class="is-model" />模型</span>
          <span><i class="is-tool" />工具</span>
        </template>
        <button v-if="agentTurns.length && !hideToggle" type="button" @click="expanded = !expanded">
          {{ expanded ? '收起回合' : '展开回合' }}
        </button>
      </div>
    </header>

    <div class="run-timeline-axis">
      <span class="run-timeline-axis-name" />
      <div class="run-timeline-track">
        <span v-for="tick in ticks" :key="tick.ratio" class="run-timeline-tick" :style="{ left: `${tick.ratio * 100}%` }">
          {{ tick.label }}
        </span>
        <span v-if="running" class="run-timeline-now" :style="{ left: `${percent(now - start)}%` }" />
      </div>
      <span class="run-timeline-axis-duration" />
    </div>

    <div class="run-timeline-rows">
      <button
        v-for="node in nodes"
        :key="node.key"
        type="button"
        class="run-timeline-row is-step"
        :class="{ 'is-selected': selected === node.key }"
        @click="selectNode(node.key)"
      >
        <span class="run-timeline-name">
          <i class="run-timeline-dot" :class="`is-${node.status}`" />
          <b>{{ node.label }}</b>
        </span>
        <div class="run-timeline-track">
          <span class="run-timeline-bar is-step" :class="`is-${node.status}`" :style="barStyle(node)" />
        </div>
        <span class="run-timeline-duration">{{ nodeDuration(node) }}</span>
      </button>

      <template v-if="expanded && agentNode && rounds.length">
        <div v-for="round in rounds" :key="`round-${round.key}`" class="run-timeline-round">
          <button
            type="button"
            class="run-timeline-row is-round"
            :class="{ 'is-selected': selected === agentNode.key }"
            @click="selectNode(agentNode.key)"
          >
            <span class="run-timeline-name">
              <span class="run-timeline-round-tag">R{{ round.index }}</span>
              <span class="run-timeline-tool">模型</span>
              <span v-if="round.tools.length" class="run-timeline-tool-count">{{ round.tools.length }} 次工具</span>
            </span>
            <div class="run-timeline-track">
              <span class="run-timeline-bar is-turn is-model" :style="roundStyle(round)" />
              <span
                v-for="tool in round.tools"
                :key="`round-tool-${tool.index}`"
                class="run-timeline-bar is-turn is-tool"
                :style="turnStyle(tool)"
              />
            </div>
            <span class="run-timeline-duration">{{ formatMs(roundTotalMs(round)) }}</span>
          </button>
          <button
            v-for="tool in round.tools"
            :key="`turn-${tool.index}`"
            type="button"
            class="run-timeline-row is-turn"
            :class="{ 'is-selected': selected === agentNode.key }"
            @click="selectNode(agentNode.key)"
          >
            <span class="run-timeline-name">
              <i class="run-timeline-kind is-tool" />
              <span class="run-timeline-tool">{{ tool.tool }}</span>
              <span class="run-timeline-arg">{{ toolArg(tool) }}</span>
            </span>
            <div class="run-timeline-track">
              <span class="run-timeline-bar is-turn is-tool" :style="turnStyle(tool)" />
            </div>
            <span class="run-timeline-duration">{{ toolTimeLabel(tool) }}</span>
          </button>
        </div>
      </template>
    </div>

    <footer v-if="agentTurns.length" class="run-timeline-foot" title="节点 + 步骤 + 节点间等待 = 跨度">
      <span class="run-timeline-foot-label">构成</span>
      <span>节点 {{ formatMs(summary.agentMs) }}</span>
      <span>步骤 {{ formatMs(summary.stepsMs) }}</span>
      <span v-if="summary.gapMs > 0">衔接 {{ formatMs(summary.gapMs) }}</span>
      <span v-if="summary.peakRound">最慢回合 R{{ summary.peakRound }} 占 {{ summary.pct }}%</span>
    </footer>
  </section>
</template>

<style scoped>
.run-timeline {
  display: flex;
  min-height: 0;
  flex-direction: column;
  border: 1px solid var(--line);
  border-radius: 8px;
  background: var(--surface);
  overflow: hidden;
}

.run-timeline-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  min-height: 38px;
  padding: 0 14px;
  border-bottom: 1px solid var(--line);
}

.run-timeline-title {
  display: flex;
  align-items: baseline;
  gap: 8px;
}

.run-timeline-title b {
  font-family: var(--sans);
  font-size: 12.5px;
  color: var(--ink);
}

.run-timeline-title span {
  font-family: var(--mono);
  font-size: 12px;
  color: var(--ink-3);
}

.run-timeline-legend {
  display: flex;
  align-items: center;
  gap: 10px;
  font-size: 12px;
  color: var(--ink-3);
}

.run-timeline-legend span {
  display: inline-flex;
  align-items: center;
  gap: 4px;
}

.run-timeline-legend i,
.run-timeline-kind {
  display: inline-block;
  width: 7px;
  height: 7px;
  border-radius: 2px;
}

.run-timeline-legend i.is-step {
  background: color-mix(in srgb, var(--green) 62%, transparent);
}

.run-timeline-legend i.is-model,
.run-timeline-kind.is-model {
  background: var(--ink-3);
}

.run-timeline-legend i.is-tool,
.run-timeline-kind.is-tool {
  background: var(--blue);
}

.run-timeline-legend button {
  height: 22px;
  padding: 0 7px;
  border: 1px solid var(--line);
  border-radius: 4px;
  background: transparent;
  font-size: 12px;
  color: var(--ink-2);
  cursor: pointer;
}

.run-timeline-legend button:hover {
  border-color: var(--line-2);
  color: var(--ink);
}

.run-timeline-axis,
.run-timeline-row {
  display: grid;
  grid-template-columns: 150px minmax(0, 1fr) 58px;
  align-items: center;
  gap: 10px;
}

.run-timeline-axis {
  height: 22px;
  padding: 0 14px;
  border-bottom: 1px solid color-mix(in srgb, var(--line) 60%, transparent);
}

.run-timeline-track {
  position: relative;
  height: 100%;
}

.run-timeline-axis-name,
.run-timeline-axis-duration {
  height: 1px;
}

.run-timeline-tick {
  position: absolute;
  top: 4px;
  transform: translateX(-50%);
  font-family: var(--mono);
  font-size: 12px;
  color: var(--ink-3);
  white-space: nowrap;
}

.run-timeline-tick:first-child {
  transform: none;
}

.run-timeline-tick:last-child {
  transform: translateX(-100%);
}

.run-timeline-now {
  position: absolute;
  top: 0;
  bottom: 0;
  width: 1px;
  background: var(--blue);
  opacity: 0.55;
}

.run-timeline-rows {
  min-height: 0;
  flex: 1 1 auto;
  overflow: auto;
  padding: 4px 0 6px;
}

.run-timeline-row {
  width: 100%;
  min-height: 27px;
  padding: 0 14px;
  border: 0;
  background: transparent;
  text-align: left;
  cursor: pointer;
}

.run-timeline-row:hover {
  background: color-mix(in srgb, var(--paper) 70%, transparent);
}

.run-timeline-row.is-selected {
  background: color-mix(in srgb, var(--blue-soft) 48%, transparent);
}

.run-timeline-row.is-turn {
  min-height: 24px;
  padding-left: 34px;
}

.run-timeline-round + .run-timeline-round {
  border-top: 1px dashed color-mix(in srgb, var(--line) 55%, transparent);
}

.run-timeline-row.is-round {
  min-height: 25px;
  padding-left: 20px;
}

.run-timeline-round-tag {
  flex: none;
  min-width: 26px;
  padding: 1px 4px;
  border: 1px solid var(--line);
  border-radius: 3px;
  background: var(--paper);
  font-family: var(--mono);
  font-size: 11px;
  color: var(--ink-3);
  text-align: center;
}

.run-timeline-tool-count {
  margin-left: auto;
  flex: none;
  font-family: var(--mono);
  font-size: 11px;
  color: var(--ink-3);
}

.run-timeline-name {
  display: flex;
  min-width: 0;
  align-items: center;
  gap: 7px;
  overflow: hidden;
}

.run-timeline-name b {
  overflow: hidden;
  font-size: 12px;
  font-weight: 500;
  color: var(--ink-2);
  text-overflow: ellipsis;
  white-space: nowrap;
}

.run-timeline-row.is-step .run-timeline-name b {
  color: var(--ink);
}

.run-timeline-dot {
  width: 6px;
  height: 6px;
  flex: none;
  border-radius: 50%;
  background: var(--line-2);
}

.run-timeline-dot.is-done { background: var(--green); }
.run-timeline-dot.is-running { background: var(--blue); animation: run-timeline-pulse 1.2s ease-in-out infinite; }
.run-timeline-dot.is-error { background: var(--red); }
.run-timeline-dot.is-interrupted { background: var(--gold); }
.run-timeline-dot.is-skipped,
.run-timeline-dot.is-queued { background: transparent; border: 1px solid var(--line-2); }

.run-timeline-foot {
  display: flex;
  flex: none;
  flex-wrap: wrap;
  align-items: center;
  gap: 4px 14px;
  min-height: 30px;
  padding: 0 14px;
  border-top: 1px solid var(--line);
  background: color-mix(in srgb, var(--paper) 46%, transparent);
  font-family: var(--mono);
  font-size: 11.5px;
  color: var(--ink-3);
}

.run-timeline-foot-label {
  color: var(--ink-2);
  font-weight: 600;
}

.run-timeline-tool {
  overflow: hidden;
  font-family: var(--mono);
  font-size: 12px;
  color: var(--ink-3);
  text-overflow: ellipsis;
  white-space: nowrap;
}

.run-timeline-arg {
  min-width: 0;
  overflow: hidden;
  font-family: var(--mono);
  font-size: 11px;
  color: var(--ink-3);
  text-overflow: ellipsis;
  white-space: nowrap;
}

.run-timeline-bar {
  position: absolute;
  top: 50%;
  height: 7px;
  transform: translateY(-50%);
  border-radius: 3px;
  background: var(--line-2);
}

.run-timeline-bar.is-step {
  height: 8px;
  background: color-mix(in srgb, var(--ink-3) 42%, transparent);
}

.run-timeline-bar.is-step.is-done { background: color-mix(in srgb, var(--green) 55%, transparent); }
.run-timeline-bar.is-step.is-running { background: var(--blue); }
.run-timeline-bar.is-step.is-error { background: color-mix(in srgb, var(--red) 65%, transparent); }
.run-timeline-bar.is-step.is-skipped,
.run-timeline-bar.is-step.is-queued { background: var(--line); }

.run-timeline-bar.is-turn.is-model {
  height: 6px;
  background: color-mix(in srgb, var(--ink-3) 58%, transparent);
}

.run-timeline-bar.is-turn.is-tool {
  height: 6px;
  background: color-mix(in srgb, var(--blue) 78%, transparent);
}

.run-timeline-duration {
  font-family: var(--mono);
  font-size: 12px;
  color: var(--ink-3);
  text-align: right;
  white-space: nowrap;
}

@keyframes run-timeline-pulse {
  0%, 100% { box-shadow: 0 0 0 0 color-mix(in srgb, var(--blue) 40%, transparent); }
  50% { box-shadow: 0 0 0 3px color-mix(in srgb, var(--blue) 0%, transparent); }
}

@media (max-width: 1180px) {
  .run-timeline-axis,
  .run-timeline-row {
    grid-template-columns: 124px minmax(0, 1fr) 52px;
  }

  .run-timeline-arg {
    display: none;
  }
}

@media (prefers-reduced-motion: reduce) {
  .run-timeline-dot.is-running {
    animation: none;
  }
}
</style>
