<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { ChevronRight } from 'lucide-vue-next'

import type { AgentTurn } from '../../../lib/agent/types'
import { isAgentNode, type RunNode } from '../../../lib/runModel'

const props = defineProps<{
  nodes: RunNode[]
  selected: string
  agentTurns: AgentTurn[]
  paused?: boolean
  finishedAt?: number
}>()
const emit = defineEmits<{ select: [key: string] }>()

const now = ref(Date.now())
const scroller = ref<HTMLElement | null>(null)
const nodeEls = new Map<string, HTMLElement>()
let clock: number | undefined

function bindNode(key: string, el: Element | null): void {
  if (el) nodeEls.set(key, el as HTMLElement)
  else nodeEls.delete(key)
}

function setNodeRef(key: string) {
  return (el: unknown): void => bindNode(key, el as Element | null)
}

async function revealSelected(): Promise<void> {
  await nextTick()
  const el = nodeEls.get(props.selected)
  const sc = scroller.value
  if (!el || !sc) return
  const left = el.offsetLeft - 20
  const right = el.offsetLeft + el.offsetWidth + 20
  if (left < sc.scrollLeft) sc.scrollTo({ left, behavior: 'smooth' })
  else if (right > sc.scrollLeft + sc.clientWidth) sc.scrollTo({ left: right - sc.clientWidth, behavior: 'smooth' })
}

watch(() => [props.selected, props.nodes.length], () => {
  void revealSelected()
})

const agentTurnsCount = computed(() => props.agentTurns.filter((turn) => !turn.tool).length)
const lastTool = computed(() => {
  const tools = props.agentTurns.filter((turn) => turn.tool)
  return tools[tools.length - 1]?.tool ?? ''
})

function statusText(status: string): string {
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

function formatMs(ms: number): string {
  if (ms <= 0) return ''
  if (ms < 1000) return `${Math.round(ms)}ms`
  if (ms < 60_000) return `${(ms / 1000).toFixed(2)}s`
  const seconds = ms / 1000
  return `${Math.floor(seconds / 60)}m${String(Math.round(seconds % 60)).padStart(2, '0')}s`
}

function duration(node: RunNode): string {
  if (!node.startedAt) return ''
  const end = node.endedAt || (node.status === 'running' ? now.value : props.finishedAt || node.startedAt)
  return formatMs(end - node.startedAt)
}

function tokenLabel(node: RunNode): string {
  if (!node.tokens) return ''
  return node.tokens >= 1000 ? `${(node.tokens / 1000).toFixed(1)}k` : String(node.tokens)
}

function stateLine(node: RunNode): string {
  if (node.status === 'queued' || node.status === 'pending') return '等待'
  if (node.status === 'skipped') return '跳过'
  if (node.status === 'error') return '失败'
  if (node.status === 'interrupted') return '已中断'
  if (node.status === 'cancelled') return '已取消'
  if (node.status === 'running') return '进行中'
  if (isAgentNode(node.key) && agentTurnsCount.value) return `${agentTurnsCount.value} 回合`
  return ''
}

function metaLine(node: RunNode): string {
  const parts = [duration(node), tokenLabel(node) ? `${tokenLabel(node)} tok` : ''].filter(Boolean)
  if (!isAgentNode(node.key)) return parts.join(' · ')
  if (node.status === 'running') {
    if (parts.length) return `${parts.join(' · ')}${lastTool.value ? ` · ${lastTool.value}` : ''}`
    return lastTool.value ? `调用 ${lastTool.value}` : '等待模型'
  }
  return parts.join(' · ')
}

function linkActive(next: RunNode): boolean {
  return next.status === 'running'
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
  <section class="run-flow" aria-label="运行流程">
    <div ref="scroller" class="run-flow-scroll">
      <div class="run-flow-track">
        <template v-for="(node, index) in nodes" :key="node.key">
          <button
            :ref="setNodeRef(node.key)"
            type="button"
            class="run-flow-node"
            :class="[
              `is-${node.status}`,
              {
                'is-selected': selected === node.key,
                'is-agent': isAgentNode(node.key),
                'is-live': node.status === 'running',
              },
            ]"
            :aria-label="`${node.label} · ${statusText(node.status)}`"
            :aria-current="selected === node.key ? 'step' : undefined"
            @click="emit('select', node.key)"
          >
            <span class="run-flow-top">
              <i class="run-flow-dot" />
              <span class="run-flow-phase">{{ node.phase.label }}</span>
              <span class="run-flow-index">{{ String(index + 1).padStart(2, '0') }}</span>
            </span>
            <b class="run-flow-label" :title="node.label">{{ node.label }}</b>
            <span class="run-flow-meta">
              <span v-if="stateLine(node)" class="run-flow-state">{{ stateLine(node) }}</span>
              <span v-if="metaLine(node)" class="run-flow-detail" :title="metaLine(node)">{{ metaLine(node) }}</span>
            </span>
            <span v-if="node.status === 'running'" class="run-flow-progress" aria-hidden="true" />
          </button>
          <span
            v-if="index < nodes.length - 1"
            class="run-flow-link"
            :class="{
              'is-active': linkActive(nodes[index + 1]),
              'is-error': nodes[index + 1].status === 'error',
              'is-skipped': nodes[index + 1].status === 'skipped',
            }"
            aria-hidden="true"
          >
            <i />
            <ChevronRight :size="12" />
          </span>
        </template>
      </div>
    </div>
  </section>
</template>

<style scoped>
.run-flow {
  position: relative;
  flex: 0 0 auto;
  padding: 18px 20px;
  border: 1px solid var(--line);
  border-radius: 8px;
  background:
    radial-gradient(circle at 1px 1px, color-mix(in srgb, var(--line) 42%, transparent) 1px, transparent 0) 0 0 / 18px 18px,
    linear-gradient(180deg, color-mix(in srgb, var(--surface) 92%, var(--paper)), var(--surface));
  overflow: hidden;
}

.run-flow-scroll {
  display: flex;
  width: 100%;
  justify-content: safe center;
  overflow-x: auto;
  scrollbar-width: thin;
}

.run-flow-track {
  display: flex;
  width: 100%;
  min-width: fit-content;
  align-items: center;
  justify-content: center;
  gap: 0;
  padding: 2px 2px 4px;
}

.run-flow-node {
  position: relative;
  display: flex;
  min-width: 128px;
  max-width: 208px;
  flex: 1 1 176px;
  flex-direction: column;
  gap: 3px;
  padding: 10px 11px 9px;
  border: 1px solid var(--line);
  border-radius: 8px;
  background: var(--surface);
  color: var(--ink-2);
  font-family: var(--sans);
  text-align: left;
  cursor: pointer;
  box-shadow: 0 1px 0 color-mix(in srgb, var(--ink) 4%, transparent);
  transition: border-color .14s ease, box-shadow .14s ease, transform .14s ease;
}

.run-flow-node:hover {
  border-color: var(--ink-3);
  transform: translateY(-1px);
}

.run-flow-node.is-selected {
  border-color: var(--blue);
  box-shadow: 0 0 0 2px var(--blue-soft), 0 6px 18px color-mix(in srgb, var(--ink) 7%, transparent);
}

.run-flow-node.is-running {
  border-color: var(--blue);
  background: linear-gradient(180deg, var(--blue-soft), var(--surface) 88%);
}

.run-flow-node.is-error {
  border-color: color-mix(in srgb, var(--red) 58%, var(--line));
  background: color-mix(in srgb, var(--red) 4%, var(--surface));
}

.run-flow-node.is-interrupted {
  border-color: color-mix(in srgb, var(--gold) 55%, var(--line));
  background: color-mix(in srgb, var(--gold) 5%, var(--surface));
}

.run-flow-node.is-cancelled,
.run-flow-node.is-skipped {
  border-style: dashed;
  opacity: .72;
}

.run-flow-node.is-queued {
  border-style: dashed;
}

.run-flow-top {
  display: flex;
  align-items: center;
  gap: 5px;
  min-height: 12px;
}

.run-flow-dot {
  width: 6px;
  height: 6px;
  flex: none;
  border-radius: 50%;
  background: var(--line-2);
}

.run-flow-node.is-running .run-flow-dot {
  background: var(--blue);
  animation: run-flow-pulse 1.3s ease-in-out infinite;
}

.run-flow-node.is-done .run-flow-dot { background: var(--green); }
.run-flow-node.is-error .run-flow-dot { background: var(--red); }
.run-flow-node.is-interrupted .run-flow-dot { background: var(--gold); }
.run-flow-node.is-cancelled .run-flow-dot,
.run-flow-node.is-skipped .run-flow-dot { background: transparent; border: 1px solid var(--line-2); }

.run-flow-phase {
  color: var(--ink-3);
  font-family: var(--mono);
  font-size: 10px;
  letter-spacing: .08em;
  text-transform: uppercase;
}

.run-flow-index {
  margin-left: auto;
  color: var(--ink-3);
  font-family: var(--mono);
  font-size: 10px;
  font-variant-numeric: tabular-nums;
}

.run-flow-label {
  display: -webkit-box;
  overflow: hidden;
  color: var(--ink);
  font-size: 13px;
  font-weight: 600;
  line-height: 1.35;
  -webkit-box-orient: vertical;
  -webkit-line-clamp: 2;
}

.run-flow-node.is-agent .run-flow-label {
  color: var(--blue);
}

.run-flow-meta {
  display: flex;
  flex-wrap: wrap;
  align-items: baseline;
  gap: 0 6px;
  min-height: 14px;
  margin-top: auto;
  color: var(--ink-3);
  font-family: var(--mono);
  font-size: 11px;
  line-height: 1.35;
}

.run-flow-state {
  color: var(--ink-2);
}

.run-flow-node.is-running .run-flow-state { color: var(--blue); }
.run-flow-node.is-error .run-flow-state { color: var(--red); }
.run-flow-node.is-queued .run-flow-state { color: var(--ink-3); }

.run-flow-detail {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.run-flow-node.is-agent .run-flow-detail {
  color: color-mix(in srgb, var(--blue) 68%, var(--ink-3));
}

.run-flow-progress {
  position: absolute;
  right: 9px;
  bottom: 0;
  left: 9px;
  height: 2px;
  border-radius: 2px;
  background: linear-gradient(90deg, transparent, var(--blue), transparent);
  background-size: 48% 100%;
  animation: run-flow-sweep 1.5s linear infinite;
}

.run-flow-link {
  position: relative;
  display: flex;
  width: clamp(22px, 3vw, 44px);
  flex: none;
  align-items: center;
  justify-content: center;
  align-self: center;
  color: var(--line-2);
}

.run-flow-link::before {
  content: '';
  position: absolute;
  right: 12px;
  left: 12px;
  height: 1px;
  background: currentColor;
}

.run-flow-link i {
  position: absolute;
  left: 10px;
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--blue);
  opacity: 0;
}

.run-flow-link.is-active {
  color: color-mix(in srgb, var(--blue) 55%, var(--line-2));
}

.run-flow-link.is-active i {
  opacity: 1;
  animation: run-flow-travel 1.4s linear infinite;
}

.run-flow-link.is-error {
  color: color-mix(in srgb, var(--red) 55%, var(--line-2));
}

.run-flow-link.is-skipped {
  color: var(--line);
  border-top: 1px dashed var(--line-2);
}

.run-flow-link svg {
  position: relative;
  z-index: 1;
  color: var(--ink-3);
  background: var(--surface);
}

.run-flow-link.is-active svg { color: var(--blue); }
.run-flow-link.is-error svg { color: var(--red); }

@keyframes run-flow-pulse {
  0%, 100% { box-shadow: 0 0 0 0 color-mix(in srgb, var(--blue) 45%, transparent); }
  50% { box-shadow: 0 0 0 4px color-mix(in srgb, var(--blue) 0%, transparent); }
}

@keyframes run-flow-sweep {
  from { background-position: -48% 0; }
  to { background-position: 148% 0; }
}

@keyframes run-flow-travel {
  0% { left: 10px; opacity: 0; }
  18% { opacity: 1; }
  82% { opacity: 1; }
  100% { left: calc(100% - 16px); opacity: 0; }
}

@media (max-width: 1180px) {
  .run-flow { padding: 14px 16px; }
  .run-flow-node { width: 148px; padding: 9px 10px 8px; }
  .run-flow-label { font-size: 12.5px; }
}

@media (max-height: 800px) {
  .run-flow { padding: 12px 16px; }
  .run-flow-node { padding: 8px 10px; }
  .run-flow-label { -webkit-line-clamp: 1; }
  .run-flow-meta { min-height: 12px; font-size: 10.5px; }
}

@media (prefers-reduced-motion: reduce) {
  .run-flow-node,
  .run-flow-dot,
  .run-flow-progress,
  .run-flow-link i {
    animation: none !important;
    transition: none !important;
  }
  .run-flow-link i { display: none; }
}
</style>
