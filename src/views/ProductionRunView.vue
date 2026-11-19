<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { ArrowLeft, Check, Copy, ExternalLink, PanelRightOpen, Pause, Play, RefreshCw, RotateCcw, Square } from 'lucide-vue-next'

import * as api from '../lib/api'
import type { AgentRun, AgentTurn } from '../lib/agent/types'
import { buildRunNodes, isAgentNode, type RunNode } from '../lib/runModel'
import { cancelProduction, production, productionUi, retryProduction } from '../stores/production'
import type { ProductionJob, ProductionLog, ProductionRunGraph } from '../types'
import RunFlow from '../components/production/run/RunFlow.vue'
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
const nodes = computed<RunNode[]>(() => (job.value ? buildRunNodes(job.value, graph.value) : []))
const selectedNode = computed<RunNode | null>(() => nodes.value.find((node) => node.key === selectedKey.value) ?? null)
const agentNode = computed(() => nodes.value.find((node) => isAgentNode(node.key)) ?? null)
// The Rust engine writes one "#n tool · observation" log per turn. Use those
// as the live trajectory until agent-run.json is available.
const liveAgentTurns = computed<AgentTurn[]>(() => {
  const node = agentNode.value
  if (!node) return []
  const byIndex = new Map<number, AgentTurn>()
  const requestStarts = new Map<number, number>()
  let modelLogs = 0
  for (const log of logs.value) {
    if (log.stepIndex != null && log.stepIndex !== node.stepIndex) continue
    const request = /^引擎第 (\d+) 回合：请求模型/.exec(log.message)
    if (request) {
      requestStarts.set(Number(request[1]), log.createdAt)
      continue
    }
    const match = /^#(\d+)\s+(\S+)(?:\s+·\s+([\s\S]*))?$/.exec(log.message)
    if (!match) continue
    const index = Number(match[1]) - 1
    const tool = match[2]
    const existing = byIndex.get(index)
    if (tool === 'model') {
      modelLogs += 1
      const start = requestStarts.get(modelLogs) ?? existing?.startedAt ?? log.createdAt
      let observation = match[3] ?? ''
      let usage: AgentTurn['usage'] | undefined
      const token = /^(\d+)\/(\d+) tok(?:\s+·\s+([\s\S]*))?$/.exec(observation)
      if (token) {
        usage = {
          promptTokens: Number(token[1]),
          completionTokens: Number(token[2]),
          reasoningTokens: 0,
        }
        observation = token[3] ?? ''
      }
      byIndex.set(index, {
        index,
        phase: 'model',
        note: '模型完成',
        observation,
        usage,
        startedAt: start,
        endedAt: log.createdAt,
        modelMs: Math.max(0, log.createdAt - start),
      })
      continue
    }
    byIndex.set(index, {
      index,
      phase: 'observe',
      tool,
      observation: match[3] ?? '',
      startedAt: existing?.startedAt ?? log.createdAt,
      endedAt: log.createdAt,
      toolMs: existing ? Math.max(0, log.createdAt - existing.startedAt) : 0,
    })
  }
  return [...byIndex.values()].sort((a, b) => a.index - b.index)
})
const agentTurns = computed(() => (agentRun.value?.turns.length ? agentRun.value.turns : liveAgentTurns.value))
const agentModelTurns = computed(() => agentTurns.value.filter((turn) => !turn.tool))
const liveRound = computed(() => {
  const node = agentNode.value
  if (!node) return 0
  for (let index = logs.value.length - 1; index >= 0; index -= 1) {
    const log = logs.value[index]
    if (log.stepIndex != null && log.stepIndex !== node.stepIndex) continue
    const match = /^引擎第 (\d+) 回合/.exec(log.message)
    if (match) return Number(match[1])
  }
  return 0
})
const agentRoundDisplay = computed(() => Math.max(agentModelTurns.value.length, liveRound.value))
const gatePassed = computed<boolean | null>(() => {
  for (let index = agentTurns.value.length - 1; index >= 0; index -= 1) {
    const match = /门禁：(通过|未通过)/.exec(agentTurns.value[index].observation ?? '')
    if (match) return match[1] === '通过'
  }
  return null
})
const shortId = computed(() => (job.value ? `${job.value.id.slice(0, 12)}…` : ''))
const templateName = computed(() => {
  const template = production.templates.find((item) => item.id === job.value?.templateId)
  return template?.name ?? '历史任务'
})
const status = computed(() => job.value?.status ?? '')
const statusLabel = computed(() => statusText(status.value))
const liveTokens = computed(() =>
  agentTurns.value.reduce(
    (sum, turn) => sum + (turn.usage?.promptTokens ?? 0) + (turn.usage?.completionTokens ?? 0),
    0,
  ),
)
const tokens = computed(() => {
  const jobTokens = (job.value?.usage.promptTokens ?? 0) + (job.value?.usage.completionTokens ?? 0)
  return jobTokens || liveTokens.value
})
const cost = computed(() => job.value?.usage.estimatedCost ?? 0)
const noteBody = computed(() => job.value?.result?.body ?? '')
const noteTitle = computed(() => {
  const match = /^#\s+(.+)$/m.exec(noteBody.value)
  return match?.[1]?.trim() || job.value?.title || '未命名笔记'
})
const noteCjk = computed(() => [...noteBody.value].filter((ch) => /[\u4e00-\u9fff]/.test(ch)).length)
const noteH2 = computed(() => noteBody.value.split('\n').filter((line) => line.startsWith('## ')).length)
const noteFigures = computed(() => (noteBody.value.match(/```figure/g) ?? []).length)
const sourceLabel = computed(() => {
  const kind = job.value?.sourceKind ?? ''
  if (kind === 'text') return '校对稿'
  if (kind === 'bili' || kind === 'favorites') return '视频素材'
  if (kind === 'file') return '本地素材'
  return kind || '素材'
})
const noteSections = computed(() =>
  noteBody.value
    .split('\n')
    .filter((line) => line.startsWith('## '))
    .map((line) => line.slice(3).trim())
    .slice(0, 6),
)
const preNodes = computed(() => nodes.value.filter((node) => node.key === 'transcribe' || node.key === 'refine'))
const elapsed = computed(() => {
  if (!job.value?.startedAt) return 0
  const end = job.value.finishedAt || now.value
  return Math.max(0, Math.round((end - job.value.startedAt) / 10) / 100)
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
  const value = Math.max(0, seconds)
  if (value >= 3600) {
    return `${Math.floor(value / 3600)}h${String(Math.floor((value % 3600) / 60)).padStart(2, '0')}m`
  }
  if (value >= 60) {
    const minutes = Math.floor(value / 60)
    const rest = Math.round(value - minutes * 60)
    return `${minutes}m${String(rest).padStart(2, '0')}s`
  }
  return `${value.toFixed(2)}s`
}


function formatMs(ms: number): string {
  if (ms <= 0) return '—'
  if (ms < 1000) return `${Math.round(ms)}ms`
  return `${(ms / 1000).toFixed(2)}s`
}

function nodeElapsed(node: RunNode): number {
  if (!node.startedAt) return 0
  return Math.max(0, (node.endedAt || now.value) - node.startedAt)
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
  const agent = nodes.value.find((node) => isAgentNode(node.key))
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
  const artifact = graph.value?.artifacts.find((item) => isAgentNode(item.nodeKey) && item.name === 'agent-run.json')
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
  media = window.matchMedia('(max-width: 980px)')
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
      </span>
      <div class="run-actions">
        <button v-if="running" type="button" class="run-btn" :class="{ 'is-on': !paused }" @click="togglePause">
          <Pause v-if="!paused" :size="12" />
          <Play v-else :size="12" />
          {{ paused ? '继续刷新' : '暂停刷新' }}
        </button>
        <button type="button" class="run-btn is-icon" title="刷新" aria-label="刷新" :disabled="busy" @click="refreshNow">
          <RefreshCw :size="12" />
        </button>
        <button v-if="running" type="button" class="run-btn is-danger" @click="cancel">
          <Square :size="12" />取消
        </button>
        <button v-else type="button" class="run-btn" @click="retry">
          <RotateCcw :size="12" />重跑
        </button>
        <button v-if="selectedNode && !inspectorOpen && !agentNode" type="button" class="run-btn" @click="inspectorOpen = true">
          <PanelRightOpen :size="12" />节点详情
        </button>
        <button v-if="job?.noteId && agentNode" type="button" class="run-btn run-open-compact" @click="openNote">
          <ExternalLink :size="12" />打开笔记
        </button>
        <button v-if="job?.noteId && !agentNode" type="button" class="run-btn" @click="openNote">
          <ExternalLink :size="12" />打开笔记
        </button>
      </div>
    </header>

    <div v-if="job" class="run-facts">
      <button type="button" class="run-id" title="复制运行 ID" @click="copyId">
        <span>{{ shortId }}</span>
        <Check v-if="copied" :size="10" />
        <Copy v-else :size="10" />
      </button>
      <span>开始 {{ formatTime(job.startedAt) }}</span>
      <span>总耗时 {{ formatDuration(elapsed) }}</span>
      <span v-if="agentNode">{{ agentRoundDisplay }} 回合</span>
      <span>{{ tokens ? `${tokens.toLocaleString()} tok` : running ? 'tok 统计中' : '0 tok' }}</span>
      <span>{{ cost ? `¥${cost.toFixed(4)}` : running ? '成本统计中' : '未计价' }}</span>
      <span v-if="job.status === 'running' && !paused" class="run-live"><i />实时</span>
      <span v-else-if="running && paused" class="run-paused">已暂停</span>
      <button v-if="!followCurrent" type="button" class="run-follow-btn" @click="follow">回到当前节点</button>
    </div>

    <p v-if="error" class="run-error">{{ error }}</p>
    <p v-else-if="job?.error && ['error', 'interrupted'].includes(job.status)" class="run-error">{{ job.error }}</p>

    <RunFlow
      v-if="nodes.length && !agentNode"
      :nodes="nodes"
      :selected="selectedKey"
      :agent-turns="agentTurns"
      :paused="paused"
      :finished-at="job?.finishedAt ?? 0"
      @select="selectNode"
    />

    <div v-if="agentNode" class="run-agent">
      <div class="run-path">
        <span class="run-path-item is-input">
          <FileText :size="12" />
          <b>{{ sourceLabel }}</b>
          <span>{{ job?.sourceKind === 'text' ? '文本输入' : '已读取' }}</span>
        </span>
        <template v-for="pre in preNodes" :key="pre.key">
          <ChevronRight :size="12" class="run-path-arrow" />
          <span class="run-path-item">
            <b>{{ pre.label }}</b>
            <span>{{ formatMs(nodeElapsed(pre)) }}</span>
          </span>
        </template>
        <ChevronRight :size="12" class="run-path-arrow" />
        <span class="run-path-item is-agent">
          <Bot :size="12" />
          <b>AI 成稿</b>
        </span>
        <ChevronRight :size="12" class="run-path-arrow" />
        <span class="run-path-item is-saved">
          <Check :size="12" />
          <b>已保存</b>
        </span>
      </div>
      <section class="run-agent-main">
        <RunInspector
          workspace
          :node="agentNode"
          :logs="logs"
          :job="job"
          :agent-turns="agentTurns"
          :agent-run="agentRun"
          :running="running && !paused"
          :now="now"
        />
      </section>

      <aside class="run-output">
        <header class="run-output-head">
          <span>产出</span>
          <span v-if="job?.status === 'success' && noteBody" class="run-output-saved"><Check :size="11" />已保存</span>
        </header>
        <template v-if="noteBody">
          <h2>{{ noteTitle }}</h2>
          <div class="run-output-stats">
            <div><b>{{ noteCjk }}</b><span>字</span></div>
            <div><b>{{ noteH2 }}</b><span>H2</span></div>
            <div><b>{{ noteFigures }}</b><span>图</span></div>
          </div>
          <p v-if="gatePassed !== null" class="run-output-gate" :class="gatePassed ? 'is-pass' : 'is-fail'">
            {{ gatePassed ? '门禁通过' : '门禁未通过' }}
          </p>
          <button type="button" class="run-output-open" :disabled="!job?.noteId" @click="openNote">
            打开笔记 <ExternalLink :size="12" />
          </button>
          <div v-if="noteSections.length" class="run-output-sections">
            <span class="run-output-label">目录</span>
            <ol>
              <li v-for="section in noteSections" :key="section">
                {{ section }}
              </li>
            </ol>
          </div>
          <div class="run-output-note">
            <span>来源：{{ sourceLabel }}</span>
            <span class="run-output-path" :title="job?.noteRelPath">{{ job?.noteRelPath || '已写入笔记库' }}</span>
          </div>
        </template>
        <div v-else class="run-output-empty">
          <p>{{ running ? '正在生成笔记…' : '这次运行没有产出笔记。' }}</p>
          <button v-if="job?.noteId" type="button" class="run-output-link" @click="openNote">
            打开已有笔记 <ExternalLink :size="12" />
          </button>
        </div>
      </aside>
    </div>

    <div v-else class="run-main" :class="{ 'is-narrow': narrow, 'is-inspector-open': inspectorOpen }">
      <div class="run-pane">
        <template v-if="loading && !nodes.length">
          <div class="run-skeleton">正在读取运行图…</div>
        </template>
        <template v-else-if="!nodes.length">
          <div class="run-skeleton">任务还没有节点记录。开始后会在这里出现。</div>
        </template>
        <RunTimeline
          v-else
          :nodes="nodes"
          :selected="selectedKey"
          :started-at="job?.startedAt ?? 0"
          :running="running"
          :agent-turns="agentTurns"
          :paused="paused"
          :finished-at="job?.finishedAt ?? 0"
          :hide-toggle="narrow && inspectorOpen"
          @select="selectNode"
        />
      </div>

      <RunInspector
        v-if="inspectorOpen"
        :node="selectedNode"
        :logs="logs"
        :job="job"
        :agent-turns="agentTurns"
        :agent-run="agentRun"
        :running="running && !paused"
        :now="now"
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
  font-size: 12px;
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
  font-size: 12px;
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
  font-size: 12px;
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
  font-size: 12px;
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

.run-btn.is-icon {
  width: 27px;
  padding: 0;
  justify-content: center;
}

.run-btn:disabled {
  opacity: 0.5;
  cursor: default;
}

.run-facts {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 3px 14px;
  padding: 5px 0 8px;
  color: var(--ink-3);
  font-family: var(--mono);
  font-size: 12px;
}

.run-id {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  max-width: 180px;
  padding: 0;
  border: 0;
  background: transparent;
  color: var(--ink-3);
  font-family: var(--mono);
  font-size: 12px;
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
  font-size: 12px;
  line-height: 1.55;
}

.run-main {
  display: flex;
  flex: 1 1 auto;
  min-height: 0;
  gap: 12px;
  padding-bottom: 8px;
}

.run-page > :deep(.run-flow) {
  margin-bottom: 10px;
}

.run-agent {
  display: grid;
  flex: 1 1 auto;
  min-height: 0;
  grid-template-columns: minmax(0, 1fr) 340px;
  grid-template-rows: auto minmax(0, 1fr);
  gap: 10px 16px;
  padding-bottom: 8px;
}

.run-path {
  display: flex;
  grid-column: 1;
  grid-row: 1;
  flex-wrap: wrap;
  align-items: center;
  gap: 5px 8px;
  padding: 8px 11px;
  border: 1px solid var(--line);
  border-radius: 6px;
  background: var(--surface);
  font-family: var(--sans);
  font-size: 12px;
  color: var(--ink-3);
}

.run-path-item {
  display: inline-flex;
  align-items: center;
  gap: 5px;
}

.run-path-item b {
  font-weight: 600;
  color: var(--ink-2);
}

.run-path-item span {
  font-family: var(--mono);
  font-size: 11.5px;
}

.run-path-item.is-input svg {
  color: var(--gold);
}

.run-path-item.is-agent svg {
  color: var(--blue);
}

.run-path-item.is-saved svg {
  color: var(--green);
}

.run-path-arrow {
  flex: none;
  color: var(--line-2);
}

.run-agent-main {
  display: flex;
  grid-column: 1;
  grid-row: 2;
  flex: 1 1 auto;
  min-width: 0;
  min-height: 0;
  flex-direction: column;
  gap: 10px;
}

.run-output {
  grid-column: 2;
  grid-row: 1 / span 2;
  width: 340px;
  min-width: 0;
  align-self: start;
  padding: 14px;
  border: 1px solid var(--line);
  border-radius: 8px;
  background: var(--surface);
}

.run-output-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  font-family: var(--sans);
  font-size: 12px;
  font-weight: 600;
  color: var(--ink-2);
}

.run-output-saved {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  font-size: 11px;
  font-weight: 500;
  color: var(--green);
}

.run-output h2 {
  margin: 14px 0 12px;
  text-wrap: balance;
  font-family: var(--serif);
  font-size: 20px;
  font-weight: 400;
  line-height: 1.4;
  color: var(--ink);
}

.run-output-stats {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 6px;
}

.run-output-stats > div {
  display: flex;
  flex-direction: column;
  gap: 1px;
  padding: 8px;
  border-radius: 5px;
  background: var(--paper);
}

.run-output-stats b {
  font-family: var(--serif);
  font-size: 19px;
  font-weight: 400;
  color: var(--ink);
}

.run-output-stats span {
  font-family: var(--sans);
  font-size: 11px;
  color: var(--ink-3);
}

.run-output-gate {
  display: inline-flex;
  margin: 12px 0 0;
  padding: 3px 8px;
  border-radius: 99px;
  font-family: var(--sans);
  font-size: 11px;
  font-weight: 600;
}

.run-output-gate.is-pass {
  background: color-mix(in srgb, var(--green) 8%, var(--surface));
  color: color-mix(in srgb, var(--green) 82%, var(--ink));
}

.run-output-gate.is-fail {
  background: color-mix(in srgb, var(--red) 8%, var(--surface));
  color: var(--red);
}

.run-output-open {
  display: inline-flex;
  width: 100%;
  height: 32px;
  align-items: center;
  justify-content: center;
  gap: 5px;
  margin-top: 12px;
  border: 1px solid var(--ink);
  border-radius: 4px;
  background: var(--ink);
  font-family: var(--sans);
  font-size: 12px;
  color: var(--paper);
  cursor: pointer;
}

.run-open-compact {
  display: none;
}

.run-output-open:disabled {
  cursor: default;
  opacity: 0.45;
}

.run-output-metrics {
  display: flex;
  flex-wrap: wrap;
  gap: 5px 10px;
  margin-top: 12px;
  font-family: var(--mono);
  font-size: 11.5px;
  color: var(--ink-3);
}

.run-output-sections {
  margin-top: 12px;
  padding-top: 10px;
  border-top: 1px solid var(--line);
}

.run-output-label {
  display: block;
  margin-bottom: 5px;
  font-family: var(--sans);
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.04em;
  color: var(--ink-3);
}

.run-output-sections ol {
  display: flex;
  flex-direction: column;
  gap: 4px;
  margin: 0;
  padding: 0;
  list-style: none;
}

.run-output-sections li {
  display: flex;
  gap: 6px;
  font-family: var(--sans);
  font-size: 11.5px;
  line-height: 1.45;
  color: var(--ink-2);
}

.run-output-sections li span {
  flex: none;
  font-family: var(--mono);
  font-size: 10.5px;
  color: var(--ink-3);
}

.run-output-note {
  display: grid;
  gap: 2px;
  margin: 12px 0 0;
  padding-top: 10px;
  border-top: 1px solid var(--line);
  font-family: var(--sans);
  font-size: 11.5px;
  line-height: 1.55;
  color: var(--ink-3);
}

.run-output-path {
  overflow: hidden;
  font-family: var(--mono);
  text-overflow: ellipsis;
  white-space: nowrap;
}

.run-output-empty {
  display: grid;
  gap: 10px;
  padding: 18px 0 4px;
}

.run-output-empty p {
  margin: 0;
  color: var(--ink-3);
  font-family: var(--sans);
  font-size: 12px;
  line-height: 1.6;
}

.run-output-link {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 5px;
  min-height: 30px;
  border: 1px solid var(--line-2);
  border-radius: 4px;
  background: var(--surface);
  color: var(--ink-2);
  font-family: var(--sans);
  font-size: 12px;
}

.run-output-link:hover {
  border-color: var(--ink-3);
  color: var(--ink);
}

.run-pane {
  display: flex;
  flex-direction: column;
  flex: 1 1 auto;
  min-width: 0;
  min-height: 0;
  gap: 12px;
}

.run-pane :deep(.run-timeline) {
  flex: 1 1 auto;
  max-height: none;
  min-height: 160px;
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
  height: auto;
  padding: 0;
  border: 0;
  background: transparent;
  color: var(--ink-3);
  font-family: var(--sans);
  font-size: 12px;
  cursor: pointer;
}

.run-follow-btn:hover {
  color: var(--ink);
}

.run-follow-btn:not(.is-on) {
  text-decoration: underline;
  text-underline-offset: 2px;
}

.run-follow-btn.is-on {
  color: var(--ink-2);
}

.run-main :deep(.run-inspector) {
  flex: none;
  width: 420px;
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
  border-radius: 8px 0 0 8px;
  box-shadow: -12px 0 34px color-mix(in srgb, var(--ink) 12%, transparent);
}

.run-main.is-narrow.is-inspector-open::after {
  content: '';
  position: absolute;
  inset: 0;
  z-index: 11;
  border-radius: 8px;
  background: color-mix(in srgb, var(--ink) 7%, transparent);
  pointer-events: none;
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
  .run-facts { gap: 3px 10px; }
  .run-main :deep(.run-inspector) {
    width: 360px;
  }
  .run-agent {
    grid-template-columns: minmax(0, 1fr) 280px;
    gap: 10px 12px;
  }
  .run-output {
    width: 280px;
  }
}

@media (max-width: 1024px) {
  .run-open-compact {
    display: inline-flex;
  }
}

@media (max-width: 980px) {
  .run-page {
    overflow: auto;
  }
  .run-agent {
    display: flex;
    flex: none;
    flex-direction: column;
    overflow: visible;
  }
  .run-path {
    order: 0;
  }
  .run-agent-main {
    min-height: 0;
  }
  .run-output {
    width: 100%;
    max-width: 720px;
    align-self: flex-start;
  }
  .run-inspector.is-workspace,
  .run-inspector.is-workspace .run-inspector-body {
    overflow: visible;
  }
  .run-inspector.is-workspace .run-inspector-body {
    min-height: 0;
  }
  .run-main :deep(.run-inspector) {
    width: 360px;
  }
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
