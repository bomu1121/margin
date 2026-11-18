<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { ArrowLeft, Check, Copy, ExternalLink, Pause, Play, RefreshCw, RotateCcw, Square } from 'lucide-vue-next'

import * as api from '../lib/api'
import { productionAgent } from '../lib/agent/worker'
import type { AgentRun } from '../lib/agent/types'
import { buildRunNodes, type RunNode } from '../lib/runModel'
import { cancelProduction, production, productionUi, retryProduction } from '../stores/production'
import type { ProductionJob, ProductionLog, ProductionRunGraph } from '../types'
import RunGraph from '../components/production/run/RunGraph.vue'
import RunInspector from '../components/production/run/RunInspector.vue'
import RunTimeline from '../components/production/run/RunTimeline.vue'

const route = useRoute()
const router = useRouter()
const id = computed(() => String(route.params.id ?? ''))

const job = ref<ProductionJob | null>(null)
const graph = ref<ProductionRunGraph | null>(null)
const logs = ref<ProductionLog[]>([])
const agentRun = ref<AgentRun | null>(null)
const agentRunLoading = ref(false)
const selectedKey = ref('')
const followCurrent = ref(true)
const paused = ref(false)
const loading = ref(true)
const busy = ref(false)
const error = ref('')
const copied = ref(false)
const inspectorOpen = ref(false)
const narrow = ref(false)
const now = ref(Date.now())

let cursor = 0
let timer: number | undefined
let clock: number | undefined
let media: MediaQueryList | null = null
let mediaHandler: ((event: MediaQueryListEvent) => void) | null = null
let disposed = false

const running = computed(() => job.value?.status === 'running' || job.value?.status === 'queued')
const nodes = computed<RunNode[]>(() => (job.value ? buildRunNodes(job.value, graph.value, logs.value) : []))
const selectedNode = computed<RunNode | null>(() => nodes.value.find((node) => node.key === selectedKey.value) ?? null)
const agentNode = computed(() => nodes.value.find((node) => node.key === 'agent' || node.key === 'author') ?? null)
const liveTurns = computed(() => (productionAgent.jobId === job.value?.id ? productionAgent.turns : []))
const agentTurns = computed(() => (liveTurns.value.length ? liveTurns.value : agentRun.value?.turns ?? []))
const agentToolTurns = computed(() => agentTurns.value.filter((turn) => turn.tool))
const agentTool = computed(() => {
  const turn = agentToolTurns.value[agentToolTurns.value.length - 1]
  return turn?.tool ?? ''
})
const agentRunning = computed(() => agentNode.value?.status === 'running')
const templateName = computed(() => {
  const template = production.templates.find((item) => item.id === job.value?.templateId)
  return template?.name ?? job.value?.templateId ?? ''
})
const status = computed(() => job.value?.status ?? '')
const statusLabel = computed(() => statusText(status.value))
const tokens = computed(() => (job.value?.usage.promptTokens ?? 0) + (job.value?.usage.completionTokens ?? 0))
const cost = computed(() => job.value?.usage.estimatedCost ?? 0)
const elapsed = computed(() => {
  if (!job.value?.startedAt) return 0
  const end = job.value.finishedAt || now.value
  return Math.max(0, Math.round((end - job.value.startedAt) / 1000))
})

function statusText(value: string): string {
  return (
    {
      running: '进行中',
      queued: '等待',
      success: '已完成',
      error: '失败',
      cancelled: '已取消',
      interrupted: '已中断',
      done: '完成',
      skipped: '跳过',
    } as Record<string, string>
  )[value] ?? value
}

function formatDuration(seconds: number): string {
  const value = Math.max(0, Math.round(seconds))
  if (value >= 3600) {
    return `${Math.floor(value / 3600)}h${String(Math.floor((value % 3600) / 60)).padStart(2, '0')}m`
  }
  if (value >= 60) return `${Math.floor(value / 60)}m${String(value % 60).padStart(2, '0')}s`
  return `${value}s`
}

function formatTime(value: number): string {
  if (!value) return '—'
  return new Date(value).toLocaleString('zh-CN', {
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
    hour12: false,
  })
}

function apply(snapshot: { job: ProductionJob; graph: ProductionRunGraph; logs: ProductionLog[]; lastLogId: number }, initial: boolean): void {
  job.value = snapshot.job
  graph.value = snapshot.graph
  if (initial) {
    logs.value = snapshot.logs
  } else if (snapshot.logs.length) {
    logs.value = [...logs.value, ...snapshot.logs].slice(-3000)
  }
  cursor = snapshot.lastLogId
  if (initial) loading.value = false
  pickSelection()
  void loadAgentRun()
}

function pickSelection(): void {
  if (!nodes.value.length) return
  if (!followCurrent.value && nodes.value.some((node) => node.key === selectedKey.value)) return
  const active = nodes.value.find((node) => node.status === 'running')
  const broken = nodes.value.find((node) => ['error', 'interrupted', 'cancelled'].includes(node.status))
  const agent = nodes.value.find((node) => node.key === 'agent' || node.key === 'author')
  const last = [...nodes.value].reverse().find((node) => node.status !== 'queued' && node.status !== 'pending' && node.status !== 'skipped')
  const next = active?.key ?? broken?.key ?? (running.value ? last?.key : agent?.key ?? last?.key) ?? nodes.value[0]?.key ?? ''
  if (next && next !== selectedKey.value) selectedKey.value = next
}

function schedule(): void {
  if (timer) window.clearTimeout(timer)
  timer = undefined
  if (disposed || paused.value || document.visibilityState === 'hidden') return
  if (!running.value) return
  timer = window.setTimeout(() => {
    void refresh()
  }, 800)
}

async function refresh(initial = false): Promise<void> {
  if (busy.value || !id.value) return
  busy.value = true
  try {
    const snapshot = await api.productionRunSnapshot(id.value, initial ? 0 : cursor)
    error.value = ''
    apply(snapshot, initial)
  } catch (cause) {
    error.value = api.errorText(cause)
    if (initial) loading.value = false
  } finally {
    busy.value = false
    schedule()
  }
}

async function refreshNow(): Promise<void> {
  await refresh(false)
}

function togglePause(): void {
  paused.value = !paused.value
  if (!paused.value) void refresh()
  else if (timer) {
    window.clearTimeout(timer)
    timer = undefined
  }
}

function selectNode(key: string): void {
  selectedKey.value = key
  followCurrent.value = false
  if (narrow.value) inspectorOpen.value = true
}

function follow(): void {
  followCurrent.value = true
  pickSelection()
  if (narrow.value) inspectorOpen.value = false
}

function back(): void {
  productionUi.taskId = id.value
  void router.push('/studio')
}

function openNote(): void {
  if (!job.value?.noteId) return
  void router.push(`/note/${job.value.noteId}`)
}

async function copyId(): Promise<void> {
  if (!job.value) return
  try {
    await navigator.clipboard.writeText(job.value.id)
    copied.value = true
    window.setTimeout(() => {
      copied.value = false
    }, 1200)
  } catch {
    /* clipboard may be unavailable */
  }
}

async function cancel(): Promise<void> {
  if (!job.value) return
  try {
    await cancelProduction(job.value.id)
    resetForRun()
    await refresh(true)
  } catch (cause) {
    error.value = api.errorText(cause)
  }
}

async function retry(): Promise<void> {
  if (!job.value) return
  try {
    await retryProduction(job.value.id)
    resetForRun()
    await refresh(true)
  } catch (cause) {
    error.value = api.errorText(cause)
  }
}

function resetForRun(): void {
  cursor = 0
  logs.value = []
  graph.value = null
  agentRun.value = null
  selectedKey.value = ''
  followCurrent.value = true
  paused.value = false
}

async function loadAgentRun(): Promise<void> {
  const artifact = graph.value?.artifacts.find((item) => (item.nodeKey === 'agent' || item.nodeKey === 'author') && item.name === 'agent-run.json')
  if (!artifact || agentRun.value || agentRunLoading.value) return
  agentRunLoading.value = true
  try {
    const text = await api.productionArtifactText(artifact.id)
    agentRun.value = JSON.parse(text) as AgentRun
  } catch {
    agentRun.value = null
  } finally {
    agentRunLoading.value = false
  }
}

function onVisibility(): void {
  if (document.visibilityState === 'hidden') {
    if (timer) window.clearTimeout(timer)
    timer = undefined
  } else if (!paused.value && running.value) {
    void refresh()
  }
}

function onKeydown(event: KeyboardEvent): void {
  if (event.key === 'Escape' && narrow.value && inspectorOpen.value) inspectorOpen.value = false
}

onMounted(() => {
  media = window.matchMedia('(max-width: 1180px)')
  narrow.value = media.matches
  inspectorOpen.value = !media.matches
  mediaHandler = (event: MediaQueryListEvent) => {
    narrow.value = event.matches
    inspectorOpen.value = !event.matches
  }
  media.addEventListener('change', mediaHandler)
  clock = window.setInterval(() => {
    if (!paused.value) now.value = Date.now()
  }, 1000)
  document.addEventListener('visibilitychange', onVisibility)
  window.addEventListener('keydown', onKeydown)
  void refresh(true)
})

onBeforeUnmount(() => {
  disposed = true
  if (timer) window.clearTimeout(timer)
  if (clock) window.clearInterval(clock)
  document.removeEventListener('visibilitychange', onVisibility)
  window.removeEventListener('keydown', onKeydown)
  if (media && mediaHandler) media.removeEventListener('change', mediaHandler)
})

watch(
  () => route.params.id,
  () => {
    resetForRun()
    loading.value = true
    void refresh(true)
  },
)
</script>

<template>
  <div class="run-page">
    <header class="run-head">
      <button type="button" class="run-back" title="返回任务" @click="back">
        <ArrowLeft :size="13" /> 任务
      </button>
      <div class="run-heading">
        <span class="run-kicker">{{ templateName || '制作任务' }} · 运行</span>
        <h1>{{ job?.title || '运行详情' }}</h1>
      </div>
      <span v-if="job" class="run-status" :class="`is-${status}`">
        <i />{{ statusLabel }}
        <template v-if="status === 'running'"> · {{ job.progress }}%</template>
      </span>
      <div class="run-actions">
        <button v-if="running" type="button" class="run-btn" :class="{ 'is-on': !paused }" @click="togglePause">
          <Pause v-if="!paused" :size="12" />
          <Play v-else :size="12" />
          {{ paused ? '继续实时' : '暂停' }}
        </button>
        <button type="button" class="run-btn" :disabled="busy" @click="refreshNow">
          <RefreshCw :size="12" />刷新
        </button>
        <button v-if="running" type="button" class="run-btn is-danger" @click="cancel">
          <Square :size="12" />取消
        </button>
        <button v-else type="button" class="run-btn" @click="retry">
          <RotateCcw :size="12" />重跑
        </button>
        <button v-if="job?.noteId" type="button" class="run-btn" @click="openNote">
          <ExternalLink :size="12" />打开笔记
        </button>
      </div>
    </header>

    <div v-if="job" class="run-facts">
      <button type="button" class="run-id" title="复制运行 ID" @click="copyId">
        <span>{{ job.id }}</span>
        <Check v-if="copied" :size="10" />
        <Copy v-else :size="10" />
      </button>
      <span>开始 {{ formatTime(job.startedAt) }}</span>
      <span>耗时 {{ formatDuration(elapsed) }}</span>
      <span>{{ tokens.toLocaleString() }} tokens</span>
      <span>{{ cost ? `¥${cost.toFixed(4)}` : '未计价' }}</span>
      <span v-if="job.status === 'running' && !paused" class="run-live"><i />实时</span>
      <span v-else-if="running && paused" class="run-paused">已暂停</span>
      <button type="button" class="run-follow-btn" :class="{ 'is-on': followCurrent }" :title="followCurrent ? '检查器跟随运行中节点' : '重新跟随运行中节点'" @click="follow">
        {{ followCurrent ? '跟随中' : '跟随' }}
      </button>
      <button v-if="selectedNode && !inspectorOpen" type="button" class="run-follow-btn" @click="inspectorOpen = true">
        节点详情
      </button>
    </div>

    <p v-if="error" class="run-error">{{ error }}</p>
    <p v-else-if="job?.error && ['error', 'interrupted'].includes(job.status)" class="run-error">{{ job.error }}</p>

    <div class="run-main" :class="{ 'is-narrow': narrow, 'is-inspector-open': inspectorOpen }">
      <div class="run-pane">
        <template v-if="loading && !nodes.length">
          <div class="run-skeleton">正在读取运行图…</div>
        </template>
        <template v-else-if="!nodes.length">
          <div class="run-skeleton">任务还没有节点记录。开始后会在这里出现。</div>
        </template>
        <template v-else>
          <RunGraph
            :nodes="nodes"
            :selected="selectedKey"
            :agent-rounds="agentToolTurns.length"
            :agent-tool="agentTool"
            :agent-running="agentRunning"
            :paused="paused"
            :reserved-right="narrow && inspectorOpen ? 390 : 0"
            @select="selectNode"
          />
          <RunTimeline
            :nodes="nodes"
            :selected="selectedKey"
            :started-at="job?.startedAt ?? 0"
            :running="running"
            :agent-turns="agentTurns"
            :paused="paused"
            @select="selectNode"
          />
        </template>
      </div>

      <RunInspector
        v-if="inspectorOpen"
        :node="selectedNode"
        :logs="logs"
        :job="job"
        :agent-turns="agentTurns"
        :agent-run="agentRun"
        :running="running && !paused"
        :class="{ 'is-overlay': narrow }"
        @close="inspectorOpen = false"
      />
    </div>

  </div>
</template>

<style scoped>
.run-page {
  position: relative;
  display: flex;
  flex-direction: column;
  height: 100%;
  min-height: 0;
  padding: 0 18px 16px;
  background: var(--paper);
  font-family: var(--sans);
  color: var(--ink);
}

.run-head {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px 12px;
  padding: 12px 0 8px;
  border-bottom: 1px solid var(--line);
}

.run-back {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  height: 26px;
  padding: 0 8px;
  border: 1px solid var(--line);
  border-radius: 4px;
  background: var(--surface);
  color: var(--ink-2);
  font-size: 11px;
  cursor: pointer;
}

.run-back:hover {
  border-color: var(--line-2);
  color: var(--ink);
}

.run-heading {
  min-width: 0;
  flex: 1 1 320px;
}

.run-kicker {
  display: block;
  color: var(--ink-3);
  font-size: 11px;
  letter-spacing: 0.05em;
}

.run-heading h1 {
  margin: 1px 0 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--ink);
  font-family: var(--serif);
  font-size: 20px;
  font-weight: 400;
  line-height: 1.3;
}

.run-status {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  flex: none;
  padding: 3px 8px;
  border: 1px solid var(--line);
  border-radius: 999px;
  color: var(--ink-3);
  font-size: 11px;
}

.run-status i {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: currentColor;
}

.run-status.is-running {
  color: var(--blue);
  border-color: var(--blue-line);
  background: var(--blue-soft);
}

.run-status.is-success { color: var(--green); border-color: color-mix(in srgb, var(--green) 40%, var(--line)); }
.run-status.is-error { color: var(--red); border-color: color-mix(in srgb, var(--red) 40%, var(--line)); }

.run-actions {
  display: flex;
  flex-wrap: wrap;
  gap: 5px;
  margin-left: auto;
}

.run-btn {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  height: 27px;
  padding: 0 8px;
  border: 1px solid var(--line);
  border-radius: 4px;
  background: var(--surface);
  color: var(--ink-2);
  font-size: 11px;
  cursor: pointer;
}

.run-btn:hover {
  border-color: var(--line-2);
  color: var(--ink);
}

.run-btn.is-on {
  color: var(--blue);
  border-color: var(--blue-line);
  background: var(--blue-soft);
}

.run-btn.is-danger {
  color: var(--red);
  border-color: color-mix(in srgb, var(--red) 35%, var(--line));
}

.run-btn:disabled {
  opacity: 0.5;
  cursor: default;
}

.run-facts {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 4px 14px;
  padding: 7px 0 10px;
  color: var(--ink-3);
  font-family: var(--mono);
  font-size: 11px;
}

.run-id {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  max-width: 320px;
  padding: 0;
  border: 0;
  background: transparent;
  color: var(--ink-3);
  font-family: var(--mono);
  font-size: 11px;
  cursor: pointer;
}

.run-id span {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.run-id:hover {
  color: var(--blue);
}

.run-live {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  color: var(--blue);
}

.run-live i {
  width: 5px;
  height: 5px;
  border-radius: 50%;
  background: var(--blue);
  animation: run-live-pulse 1.2s ease-in-out infinite;
}

.run-paused {
  color: var(--gold);
}

.run-error {
  margin: 0 0 8px;
  padding: 8px 10px;
  border-left: 2px solid var(--red);
  background: color-mix(in srgb, var(--red) 4%, transparent);
  color: var(--red);
  font-size: 11.5px;
  line-height: 1.55;
}

.run-main {
  display: flex;
  flex: 1 1 auto;
  min-height: 0;
  gap: 12px;
  padding-bottom: 8px;
}

.run-pane {
  display: flex;
  flex-direction: column;
  flex: 1 1 auto;
  min-width: 0;
  min-height: 0;
  gap: 10px;
}

.run-skeleton {
  flex: 1 1 auto;
  display: grid;
  place-items: center;
  border: 1px dashed var(--line);
  border-radius: 6px;
  color: var(--ink-3);
  font-size: 12px;
}

.run-follow-btn {
  height: 22px;
  padding: 0 8px;
  border: 1px solid var(--line);
  border-radius: 999px;
  background: transparent;
  color: var(--ink-3);
  font-family: var(--sans);
  font-size: 11px;
  cursor: pointer;
}

.run-follow-btn.is-on {
  color: var(--blue);
  border-color: var(--blue-line);
  background: var(--blue-soft);
}

.run-main :deep(.run-inspector) {
  flex: none;
  width: 390px;
  border: 1px solid var(--line);
  border-radius: 6px;
  background: var(--surface);
}

.run-main :deep(.run-inspector.is-overlay) {
  position: absolute;
  top: 0;
  right: 0;
  bottom: 0;
  z-index: 12;
  width: min(390px, 92vw);
  box-shadow: -12px 0 34px color-mix(in srgb, var(--ink) 12%, transparent);
}

.run-main {
  position: relative;
}

@keyframes run-live-pulse {
  0%, 100% { box-shadow: 0 0 0 0 color-mix(in srgb, var(--blue) 40%, transparent); }
  50% { box-shadow: 0 0 0 4px color-mix(in srgb, var(--blue) 0%, transparent); }
}

@media (max-width: 1180px) {
  .run-page { padding: 0 12px 12px; }
  .run-heading h1 { font-size: 18px; }
  .run-facts { gap: 4px 10px; }
}

@media (max-width: 920px) {
  .run-head { align-items: flex-start; }
  .run-heading { flex-basis: 100%; order: 2; }
  .run-actions { order: 3; margin-left: 0; }
  .run-status { order: 1; margin-left: auto; }
  .run-back { order: 0; }
}

@media (prefers-reduced-motion: reduce) {
  .run-live i { animation: none; }
}
</style>
