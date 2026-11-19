<script setup lang="ts">
import { computed, nextTick, ref, watch } from 'vue'
import { writeText } from '@tauri-apps/plugin-clipboard-manager'
import { Bot, Brain, ChevronRight, FilePen, FilePlus, FileText, FolderOpen, User, Wrench } from 'lucide-vue-next'

import * as api from '../../../lib/api'
import type { AgentRun, AgentTurn } from '../../../lib/agent/types'
import type { ProductionJob, ProductionLog, ProductionRunArtifact } from '../../../types'
import { isAgentNode, type RunNode } from '../../../lib/runModel'

const props = defineProps<{
  node: RunNode | null
  logs: ProductionLog[]
  job: ProductionJob | null
  agentTurns: AgentTurn[]
  agentRun: AgentRun | null
  running: boolean
  now: number
  workspace?: boolean
}>()
const emit = defineEmits<{ close: [] }>()

type Tab = 'overview' | 'rounds' | 'logs' | 'artifacts' | 'metrics'
const tab = ref<Tab>('overview')
const manualTab = ref(false)
const level = ref<'all' | 'info' | 'warn' | 'error'>('all')
const search = ref('')
const follow = ref(true)
const logRef = ref<HTMLElement | null>(null)
const expandedLogs = ref<Set<number>>(new Set())
const preview = ref<{ name: string; text: string } | null>(null)
const previewLoading = ref(false)
const copied = ref('')

const isAgent = computed(() => isAgentNode(props.node?.key))
const turns = computed<AgentTurn[]>(() => {
  if (props.agentTurns.length) return props.agentTurns
  return props.agentRun?.turns ?? []
})
const modelTurns = computed(() => turns.value.filter((turn) => !turn.tool))

interface Round {
  index: number
  key: number
  model: AgentTurn | null
  tools: AgentTurn[]
}

const rounds = computed<Round[]>(() => {
  const list: Round[] = []
  for (const turn of turns.value) {
    if (!turn.tool) {
      list.push({ index: list.length + 1, key: turn.index, model: turn, tools: [] })
    } else {
      if (!list.length) list.push({ index: 1, key: turn.index, model: null, tools: [] })
      list[list.length - 1].tools.push(turn)
    }
  }
  return list
})

const totals = computed(() => {
  let prompt = 0
  let completion = 0
  let reasoning = 0
  let modelMs = 0
  let toolMs = 0
  let subMs = 0
  for (const turn of turns.value) {
    if (turn.tool) {
      const value = turn.toolMs ?? Math.max(0, (turn.endedAt ?? turn.startedAt) - turn.startedAt)
      toolMs += value
      if (value <= 0) subMs += 1
      continue
    }
    modelMs += turn.modelMs ?? Math.max(0, (turn.endedAt ?? turn.startedAt) - turn.startedAt)
    prompt += turn.usage?.promptTokens ?? 0
    completion += turn.usage?.completionTokens ?? 0
    reasoning += turn.usage?.reasoningTokens ?? 0
  }
  return { prompt, completion, reasoning, modelMs, toolMs, subMs }
})

const goal = computed(() => props.agentRun?.goal || '')
const slowestRound = computed(() => {
  let best = { index: 0, ms: 0 }
  for (const round of rounds.value) {
    const ms = roundMs(round)
    if (ms > best.ms) best = { index: round.index, ms }
  }
  return {
    ...best,
    pct: totals.value.modelMs > 0 ? Math.round((best.ms / totals.value.modelMs) * 100) : 0,
  }
})
const nodeTokens = computed(() => props.node?.tokens || totals.value.prompt + totals.value.completion)
const nodeCost = computed(() => props.node?.cost || 0)
const durationMs = computed(() => {
  const node = props.node
  if (!node?.startedAt) return 0
  const live = node.status === 'running' && props.job?.status === 'running'
  const end = node.endedAt || (live ? props.now : props.job?.finishedAt || node.startedAt)
  return Math.max(0, end - node.startedAt)
})
const nodeOtherMs = computed(() =>
  Math.max(0, durationMs.value - totals.value.modelMs - totals.value.toolMs),
)
const roundCount = computed(() => modelTurns.value.length)
const writeCount = computed(
  () => turns.value.filter((turn) => turn.tool === 'write_file' || turn.tool === 'edit_file').length,
)
const templateLabel = computed(() => {
  const id = props.job?.templateId ?? ''
  if (id === 'ai-note') return 'AI 成稿'
  if (id === 'video-note') return '视频笔记'
  if (id === 'text-note') return '文稿笔记'
  if (id === 'trace') return '速览与溯源'
  return id
})

interface GateState {
  h1: number
  h2: number
  cjk: number
  tables: number
  figures: number
  passed: boolean
  issues: string
}

const gate = computed<GateState | null>(() => {
  for (let index = turns.value.length - 1; index >= 0; index -= 1) {
    const observation = turns.value[index].observation ?? ''
    const match = /结构：H1 (\d+) · H2 (\d+) · (\d+) 字 · 表格 (\d+) · 图 (\d+)；门禁：(通过|未通过（([\s\S]+)）)/.exec(
      observation,
    )
    if (!match) continue
    return {
      h1: Number(match[1]),
      h2: Number(match[2]),
      cjk: Number(match[3]),
      tables: Number(match[4]),
      figures: Number(match[5]),
      passed: match[6] === '通过',
      issues: match[7] ?? '',
    }
  }
  return null
})

const conclusion = computed(() => {
  const node = props.node
  if (!node) return ''
  const parts: string[] = []
  if (isAgent.value) {
    parts.push(gate.value ? (gate.value.passed ? '门禁通过' : '门禁未通过') : statusText(node.status))
    if (gate.value) parts.push(`${gate.value.cjk.toLocaleString('zh-CN')} 字`)
    parts.push(`${roundCount.value} 回合`)
    if (writeCount.value) parts.push(`${writeCount.value} 次写入`)
  } else {
    parts.push(statusText(node.status))
    if (node.artifacts.length) parts.push(`${node.artifacts.length} 个产物`)
  }
  if (durationMs.value) parts.push(formatMs(durationMs.value))
  return parts.join(' · ')
})

const conclusionTone = computed(() => {
  if (!props.node) return 'is-idle'
  if (props.node.status === 'error' || (gate.value && !gate.value.passed)) return 'is-fail'
  if (props.node.status === 'running' || props.node.status === 'queued') return 'is-running'
  return 'is-pass'
})

const tabs = computed<Array<{ id: Tab; label: string }>>(() => {
  const list: Array<{ id: Tab; label: string }> = [
    { id: 'overview', label: '概览' },
    { id: 'rounds', label: '对话' },
    { id: 'logs', label: '日志' },
    { id: 'artifacts', label: '产物' },
    { id: 'metrics', label: '指标' },
  ]
  return isAgent.value ? list : list.filter((item) => item.id !== 'rounds')
})

const nodeLogs = computed(() => {
  const node = props.node
  if (!node || node.stepIndex < 0) return []
  return props.logs.filter((log) => log.stepIndex === node.stepIndex)
})

const filteredLogs = computed(() => {
  const query = search.value.trim().toLowerCase()
  return nodeLogs.value.filter((log) => {
    if (isAgent.value && /^#\d+\s/.test(log.message)) return false
    if (level.value !== 'all' && log.level !== level.value) return false
    if (query && !log.message.toLowerCase().includes(query)) return false
    return true
  })
})

watch(
  () => props.node?.key,
  () => {
    manualTab.value = false
    tab.value = isAgentNode(props.node?.key) ? 'rounds' : 'overview'
  },
  { immediate: true },
)

watch(
  () => modelTurns.value.length,
  (count) => {
    if (!manualTab.value && count && isAgentNode(props.node?.key)) tab.value = 'rounds'
  },
)

function selectTab(next: Tab): void {
  tab.value = next
  manualTab.value = true
}

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
  if (ms <= 0) return '—'
  if (ms < 1000) return `${Math.round(ms)}ms`
  return `${(ms / 1000).toFixed(2)}s`
}

function formatTokens(value: number): string {
  if (!value) return '0'
  return value >= 1000 ? `${(value / 1000).toFixed(1)}k` : String(value)
}

function formatCost(value: number): string {
  if (!value) return '—'
  return `¥${value < 1 ? value.toFixed(4) : value.toFixed(2)}`
}

function formatBytes(value: number): string {
  if (value >= 1024 * 1024) return `${(value / 1024 / 1024).toFixed(1)} MB`
  if (value >= 1024) return `${(value / 1024).toFixed(1)} KB`
  return `${value} B`
}

function timeLabel(value: number): string {
  if (!value) return '—'
  return new Date(value).toLocaleTimeString('zh-CN', { hour12: false })
}

function logTime(value: number): string {
  if (!value) return '—'
  return new Date(value).toLocaleTimeString('zh-CN', { hour12: false }).slice(0, 8)
}

function tokensLabel(): string {
  if (nodeTokens.value) return `${formatTokens(nodeTokens.value)} tok`
  return props.running ? 'tok 统计中' : '0 tok'
}

function usageSummary(): string {
  if (!isAgent.value) return tokensLabel()
  if (!totals.value.completion) return props.running ? 'tok 统计中' : '0 tok'
  const reasoning = totals.value.reasoning
  return `输出 ${formatTokens(totals.value.completion)} tok${reasoning ? `（推理 ${formatTokens(reasoning)}）` : ''}`
}

function roundMs(round: Round): number {
  if (!round.model) return 0
  return round.model.modelMs ?? Math.max(0, (round.model.endedAt ?? round.model.startedAt) - round.model.startedAt)
}

function toolMs(turn: AgentTurn): number {
  return turn.toolMs ?? Math.max(0, (turn.endedAt ?? turn.startedAt) - turn.startedAt)
}

function toolTimeLabel(turn: AgentTurn): string {
  const ms = toolMs(turn)
  return ms > 0 ? formatMs(ms) : '<1ms'
}

function totalToolLabel(): string {
  if (totals.value.toolMs > 0) return formatMs(totals.value.toolMs)
  return totals.value.subMs ? '<1ms' : '—'
}

function modelText(round: Round): string {
  return (round.model?.content ?? '').trim()
}

function reasoningText(round: Round): string {
  return (round.model?.reasoning ?? '').trim()
}

function isFinalRound(round: Round): boolean {
  return round.index === rounds.value.length && !props.running
}

function toolAction(turn: AgentTurn): string {
  if (turn.tool === 'read_file' || turn.tool === 'read_draft') return '读取'
  if (turn.tool === 'list_dir') return '浏览目录'
  if (turn.tool === 'write_file') return '写入'
  if (turn.tool === 'edit_file') return '修改'
  return turn.tool || '工具'
}

function toolTarget(turn: AgentTurn): string {
  const args = turn.args ?? {}
  const value = args.path ?? args.file ?? args.query ?? args.pattern ?? args.name
  if (typeof value === 'string') {
    if (turn.tool === 'list_dir' && (value === '.' || value === './' || value === '')) return '当前目录'
    return value
  }
  if (turn.tool === 'list_dir') return '当前目录'
  return ''
}

function toolIcon(turn: AgentTurn) {
  if (turn.tool === 'read_file' || turn.tool === 'read_draft') return FileText
  if (turn.tool === 'list_dir') return FolderOpen
  if (turn.tool === 'write_file') return FilePlus
  if (turn.tool === 'edit_file') return FilePen
  return Wrench
}

function gateInObservation(turn: AgentTurn): boolean | null {
  const match = /门禁：(通过|未通过)/.exec(turn.observation ?? '')
  return match ? match[1] === '通过' : null
}

function safeArgs(turn: AgentTurn): string {
  if (!turn.args) return ''
  const args = { ...turn.args } as Record<string, unknown>
  if (turn.tool === 'write_file' || turn.tool === 'edit_file') {
    const content = typeof args.content === 'string' ? args.content : typeof args.new === 'string' ? args.new : ''
    delete args.content
    delete args.old
    delete args.new
    return JSON.stringify({ ...args, ...(content ? { contentChars: content.length } : {}) }, null, 2)
  }
  const text = JSON.stringify(args, null, 2)
  return text.length > 2400 ? `${text.slice(0, 2400)}\n…` : text
}

function toggleLog(id: number): void {
  const next = new Set(expandedLogs.value)
  if (next.has(id)) next.delete(id)
  else next.add(id)
  expandedLogs.value = next
}

function onLogScroll(): void {
  const el = logRef.value
  if (!el) return
  follow.value = el.scrollHeight - el.scrollTop - el.clientHeight < 24
}

async function scrollLogs(): Promise<void> {
  if (!follow.value) return
  await nextTick()
  const el = logRef.value
  if (el) el.scrollTop = el.scrollHeight
}

function setFollow(value: boolean): void {
  follow.value = value
  if (value) void scrollLogs()
}

async function copy(value: string, key: string): Promise<void> {
  try {
    await writeText(value)
    copied.value = key
    window.setTimeout(() => {
      if (copied.value === key) copied.value = ''
    }, 1200)
  } catch {
    /* clipboard unavailable */
  }
}

async function openArtifact(artifact: ProductionRunArtifact): Promise<void> {
  previewLoading.value = true
  try {
    const text = await api.productionArtifactText(artifact.id)
    preview.value = { name: artifact.name, text: text.slice(0, 20000) }
  } catch {
    preview.value = { name: artifact.name, text: '（无法读取这个产物）' }
  } finally {
    previewLoading.value = false
  }
}
</script>

<template>
  <aside class="run-inspector" :class="{ 'is-workspace': workspace }">
    <header v-if="!workspace" class="run-inspector-head">
      <div class="run-inspector-heading">
        <span class="run-inspector-kicker">节点</span>
        <div class="run-inspector-title">
          <b>{{ node?.label || '未选择节点' }}</b>
          <span v-if="node" class="run-inspector-status" :class="`is-${node.status}`">{{ statusText(node.status) }}</span>
        </div>
      </div>
      <button type="button" class="run-inspector-close" aria-label="关闭检查器" @click="emit('close')">×</button>
    </header>

    <div v-if="node" class="run-inspector-meta" :class="{ 'is-chat-meta': tab === 'rounds' }">
      <template v-if="tab === 'rounds'">
        <span>{{ formatMs(durationMs) }}</span>
        <span v-if="gate" class="run-ins-inline-gate" :class="gate.passed ? 'is-pass' : 'is-fail'">
          门禁{{ gate.passed ? '通过' : '未通过' }}
        </span>
      </template>
      <template v-else>
        <span>节点 {{ formatMs(durationMs) }}</span>
        <span>{{ usageSummary() }}</span>
        <span>{{ formatCost(nodeCost) }}</span>
        <span v-if="isAgent && gate">{{ gate.cjk.toLocaleString('zh-CN') }} 字</span>
        <span v-if="isAgent && gate" class="run-inspector-gate-chip" :class="gate.passed ? 'is-pass' : 'is-fail'">
          门禁 {{ gate.passed ? '通过' : '未通过' }}
        </span>
      </template>
    </div>

    <nav class="run-inspector-tabs" aria-label="节点信息">
      <button
        v-for="item in tabs"
        :key="item.id"
        type="button"
        :class="{ 'is-active': tab === item.id }"
        @click="selectTab(item.id)"
      >
        {{ item.label }}
      </button>
    </nav>

    <div class="run-inspector-body">
      <section v-if="tab === 'overview'" class="run-ins-stack">
        <div class="run-ins-conclusion" :class="conclusionTone">
          <i />
          <span>{{ conclusion }}</span>
        </div>

        <div v-if="isAgent" class="run-ins-card">
          <h4>结构门禁</h4>
          <template v-if="gate">
            <p class="run-ins-gate" :class="gate.passed ? 'is-pass' : 'is-fail'">
              {{ gate.passed ? '通过' : '未通过' }}
            </p>
            <div class="run-ins-stat-grid">
              <div><span>H1</span><b>{{ gate.h1 }}</b></div>
              <div><span>H2</span><b>{{ gate.h2 }}</b></div>
              <div><span>正文</span><b>{{ gate.cjk.toLocaleString('zh-CN') }} 字</b></div>
              <div><span>表格</span><b>{{ gate.tables }}</b></div>
              <div><span>图</span><b>{{ gate.figures }}</b></div>
            </div>
            <p v-if="gate.issues" class="run-ins-issues">{{ gate.issues }}</p>
          </template>
          <p v-else class="run-ins-muted">还没有结构门禁记录。</p>
        </div>

        <div class="run-ins-card">
          <h4>任务</h4>
          <div class="run-ins-kv">
            <span>模板</span><b>{{ templateLabel }}</b>
            <span>标题</span><b class="is-wrap">{{ job?.title || '—' }}</b>
          </div>
        </div>

        <details v-if="isAgent && goal" class="run-ins-card run-ins-brief">
          <summary>写作要求（点击展开）</summary>
          <pre>{{ goal }}</pre>
        </details>
      </section>

      <section v-else-if="tab === 'rounds'" class="run-chat">
        <div class="run-chat-turn is-user">
          <span class="run-chat-avatar is-user"><User :size="13" /></span>
          <div class="run-chat-body">
            <div class="run-chat-meta"><b>你</b></div>
            <p class="run-chat-text is-prompt">{{ goal || '把素材写成一份可直接阅读的笔记。' }}</p>
            <div class="run-chat-source"><FileText :size="11" /><span>material.md</span></div>
          </div>
        </div>

        <p v-if="!rounds.length" class="run-chat-empty">还没有对话记录。运行中的回合会从日志实时出现。</p>

        <div v-if="rounds.length" class="run-chat-turn is-assistant">
          <span class="run-chat-avatar"><Bot :size="13" /></span>
          <div class="run-chat-body">
            <div class="run-chat-meta">
              <b>AI</b>
            </div>

            <div class="run-chat-steps">
              <div v-for="round in rounds" :key="round.key" class="run-chat-step" :class="{ 'is-final': isFinalRound(round) }">
                <details v-if="reasoningText(round)" class="run-chat-think">
                  <summary>
                    <Brain :size="12" />
                    <span>思考 {{ formatMs(roundMs(round)) }}</span>
                    <ChevronRight :size="12" class="run-chat-arrow" />
                  </summary>
                  <pre>{{ reasoningText(round) }}</pre>
                </details>

                <p v-if="modelText(round)" class="run-chat-text" :class="{ 'is-final': isFinalRound(round) }">
                  {{ modelText(round) }}
                </p>

                <div v-if="round.tools.length" class="run-chat-tools">
                  <details v-for="turn in round.tools" :key="turn.index" class="run-chat-tool">
                    <summary>
                      <component :is="toolIcon(turn)" :size="12" class="run-chat-tool-icon" />
                      <b>{{ toolAction(turn) }}</b>
                      <span class="run-chat-tool-target">{{ toolTarget(turn) }}</span>
                      <span
                        v-if="gateInObservation(turn) !== null"
                        class="run-chat-gate"
                        :class="gateInObservation(turn) ? 'is-pass' : 'is-fail'"
                      >
                        门禁{{ gateInObservation(turn) ? '通过' : '未通过' }}
                      </span>
                      <span class="run-chat-tool-time">{{ toolTimeLabel(turn) }}</span>
                      <ChevronRight :size="12" class="run-chat-arrow" />
                    </summary>
                    <div class="run-chat-tool-body">
                      <pre class="run-chat-obs">{{ turn.observation || '（无输出）' }}</pre>
                      <details v-if="turn.args" class="run-chat-args">
                        <summary>参数</summary>
                        <pre>{{ safeArgs(turn) }}</pre>
                      </details>
                    </div>
                  </details>
                </div>

                <p v-if="!reasoningText(round) && !modelText(round) && !round.tools.length" class="run-chat-empty">
                  本轮没有文本输出。
                </p>
              </div>
            </div>
          </div>
        </div>
      </section>

      <section v-else-if="tab === 'logs'" class="run-ins-logs">
        <div class="run-ins-log-tools">
          <select v-model="level" class="run-ins-select" aria-label="日志级别">
            <option value="all">全部级别</option>
            <option value="info">信息</option>
            <option value="warn">警告</option>
            <option value="error">错误</option>
          </select>
          <input v-model="search" class="run-ins-search" placeholder="搜索日志" />
          <button type="button" class="run-ins-follow" :class="{ 'is-on': follow }" @click="setFollow(!follow)">
            {{ follow ? '跟随' : '已暂停' }}
          </button>
        </div>
        <div ref="logRef" class="run-ins-log-list" @scroll="onLogScroll">
          <p v-if="!filteredLogs.length" class="run-ins-muted">没有匹配的日志。</p>
          <button
            v-for="log in filteredLogs"
            :key="log.id"
            type="button"
            class="run-ins-log-row"
            :class="`is-${log.level}`"
            @click="toggleLog(log.id)"
          >
            <span class="run-ins-log-time">{{ logTime(log.createdAt) }}</span>
            <span class="run-ins-log-level">{{ log.level }}</span>
            <span class="run-ins-log-message" :class="{ 'is-open': expandedLogs.has(log.id) }">{{ log.message }}</span>
          </button>
        </div>
      </section>

      <section v-else-if="tab === 'artifacts'" class="run-ins-stack">
        <p v-if="!node?.artifacts.length" class="run-ins-muted">这个节点还没有落产物。</p>
        <button
          v-for="artifact in node?.artifacts || []"
          :key="artifact.id"
          type="button"
          class="run-ins-artifact"
          @click="openArtifact(artifact)"
        >
          <span class="run-ins-artifact-name">{{ artifact.name }}</span>
          <span class="run-ins-artifact-meta">{{ artifact.kind }} · {{ formatBytes(artifact.bytes) }}</span>
        </button>
        <div v-if="preview" class="run-ins-preview">
          <div class="run-ins-preview-head">
            <b>{{ preview.name }}</b>
            <button type="button" class="run-ins-link" @click="preview = null">关闭</button>
          </div>
          <pre>{{ preview.text }}</pre>
        </div>
        <p v-if="previewLoading" class="run-ins-muted">读取产物中…</p>
        <button
          v-if="node?.artifacts.length"
          type="button"
          class="run-ins-link"
          @click="copy(node?.artifacts[0]?.path || '', 'artifact')"
        >
          {{ copied === 'artifact' ? '已复制' : '复制产物路径' }}
        </button>
      </section>

      <section v-else class="run-ins-stack">
        <div class="run-ins-card">
          <h4>节点</h4>
          <div class="run-ins-kv is-four">
            <span>状态</span><b>{{ statusText(node?.status || '') }}</b>
            <span>耗时</span><b>{{ formatMs(durationMs) }}</b>
            <span>开始</span><b>{{ timeLabel(node?.startedAt || 0) }}</b>
            <span>结束</span><b>{{ node?.endedAt ? timeLabel(node.endedAt) : '—' }}</b>
          </div>
        </div>
        <div v-if="isAgent" class="run-ins-card">
          <h4>
            执行分解
            <span class="run-ins-h4-meta">{{ roundCount }} 回合 · {{ writeCount }} 次写入</span>
          </h4>
          <div class="run-ins-kv">
            <span>模型</span><b>{{ formatMs(totals.modelMs) }}</b>
            <span>工具</span>
            <b :title="totals.subMs ? `${totals.subMs} 次工具耗时不足 1ms` : undefined">
              {{ totalToolLabel() }}<template v-if="totals.subMs"> · {{ totals.subMs }}×&lt;1ms</template>
            </b>
            <span>其他</span><b>{{ formatMs(nodeOtherMs) }}</b>
          </div>
          <p v-if="slowestRound.index" class="run-ins-slowest">
            最慢回合 <b>R{{ slowestRound.index }} · {{ formatMs(slowestRound.ms) }}（占 {{ slowestRound.pct }}%）</b>
          </p>
        </div>
        <div v-if="isAgent" class="run-ins-card">
          <h4>Token</h4>
          <div class="run-ins-stat-grid is-three">
            <div><span>输入</span><b>{{ totals.prompt.toLocaleString('zh-CN') }} tok</b></div>
            <div><span>输出</span><b>{{ totals.completion.toLocaleString('zh-CN') }} tok</b></div>
            <div title="推理 token 已包含在输出内，不重复相加">
              <span>推理</span><b>{{ totals.reasoning.toLocaleString('zh-CN') }} tok</b>
            </div>
          </div>
          <p class="run-ins-muted is-small run-ins-token-note">推理 token 已包含在输出内，不重复相加。</p>
        </div>
      </section>
    </div>
  </aside>
</template>

<style scoped>
.run-inspector {
  display: flex;
  min-height: 0;
  flex-direction: column;
  border: 1px solid var(--line);
  border-radius: 8px;
  background: var(--surface);
  overflow: hidden;
}

.run-inspector.is-workspace {
  width: 100%;
  flex: 1 1 auto;
  border: 0;
  border-radius: 0;
  background: transparent;
}

.run-inspector.is-workspace .run-inspector-meta,
.run-inspector.is-workspace .run-inspector-tabs {
  width: 100%;
  max-width: 1030px;
  margin-right: auto;
  margin-left: 0;
}

.run-inspector.is-workspace .run-inspector-meta {
  display: none;
}

.run-inspector.is-workspace .run-inspector-tabs {
  padding: 2px 0 0;
}

.run-inspector.is-workspace .run-inspector-body {
  padding: 0 0 24px;
}

.run-inspector.is-workspace .run-chat {
  width: 100%;
  max-width: 1030px;
  margin: 0;
  padding: 14px 0;
}

.run-inspector.is-workspace .run-chat-text,
.run-inspector.is-workspace .run-chat-think {
  max-width: 760px;
}

.run-inspector.is-workspace .run-chat-tools {
  max-width: 860px;
}

.run-inspector-head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 12px;
  padding: 12px 14px 9px;
  border-bottom: 1px solid var(--line);
}

.run-inspector-heading {
  min-width: 0;
}

.run-inspector-kicker {
  display: block;
  font-family: var(--mono);
  font-size: 12px;
  letter-spacing: 0.08em;
  color: var(--ink-3);
  text-transform: uppercase;
}

.run-inspector-title {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-top: 3px;
}

.run-inspector-title b {
  overflow: hidden;
  font-size: 14px;
  font-weight: 600;
  color: var(--ink);
  text-overflow: ellipsis;
  white-space: nowrap;
}

.run-inspector-status {
  flex: none;
  padding: 2px 7px;
  border-radius: 99px;
  background: var(--paper);
  font-size: 12px;
  color: var(--ink-3);
}

.run-inspector-status.is-done { background: color-mix(in srgb, var(--green) 12%, var(--paper)); color: var(--green); }
.run-inspector-status.is-running { background: var(--blue-soft); color: var(--blue); }
.run-inspector-status.is-error { background: color-mix(in srgb, var(--red) 10%, var(--paper)); color: var(--red); }
.run-inspector-status.is-interrupted { background: color-mix(in srgb, var(--gold) 14%, var(--paper)); color: var(--ink-2); }

.run-inspector-close {
  width: 24px;
  height: 24px;
  flex: none;
  border: 0;
  border-radius: 4px;
  background: transparent;
  font-size: 18px;
  line-height: 1;
  color: var(--ink-3);
  cursor: pointer;
}

.run-inspector-close:hover {
  background: var(--paper);
  color: var(--ink);
}

.run-inspector-meta {
  display: flex;
  flex-wrap: wrap;
  gap: 5px 8px;
  padding: 8px 14px;
  border-bottom: 1px solid color-mix(in srgb, var(--line) 70%, transparent);
  font-family: var(--mono);
  font-size: 12px;
  color: var(--ink-3);
}

.run-inspector-meta > span + span::before {
  content: '·';
  margin-right: 8px;
  color: var(--line-2);
}

.run-inspector-meta > .run-inspector-gate-chip::before {
  content: none;
}

.run-inspector-gate-chip {
  padding: 1px 7px;
  border-radius: 99px;
  font-family: var(--sans);
  font-weight: 600;
}

.run-inspector-gate-chip.is-pass {
  background: color-mix(in srgb, var(--green) 8%, var(--surface));
  color: color-mix(in srgb, var(--green) 82%, var(--ink));
}

.run-inspector-gate-chip.is-fail {
  background: color-mix(in srgb, var(--red) 12%, var(--paper));
  color: var(--red);
}

.run-ins-inline-gate {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  font-family: var(--sans);
  font-size: 11.5px;
  color: var(--ink-3);
}

.run-ins-inline-gate::before {
  content: '';
  width: 5px;
  height: 5px;
  border-radius: 50%;
  background: var(--green);
}

.run-ins-inline-gate.is-fail::before {
  background: var(--red);
}

.run-inspector-tabs {
  display: flex;
  gap: 2px;
  padding: 0 8px;
  border-bottom: 1px solid var(--line);
}

.run-inspector-tabs button {
  height: 34px;
  padding: 0 10px;
  border: 0;
  border-bottom: 2px solid transparent;
  background: transparent;
  font-size: 12px;
  color: var(--ink-3);
  cursor: pointer;
}

.run-inspector-tabs button:hover {
  color: var(--ink);
}

.run-inspector-tabs button.is-active {
  border-bottom-color: var(--blue);
  color: var(--ink);
  font-weight: 600;
}

.run-inspector-body {
  min-height: 0;
  flex: 1;
  overflow: auto;
  padding: 12px 14px 18px;
}

.run-ins-stack {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.run-chat {
  display: flex;
  flex-direction: column;
  gap: 15px;
}

.run-chat-turn {
  display: grid;
  grid-template-columns: 26px minmax(0, 1fr);
  gap: 9px;
}

.run-chat-avatar {
  display: grid;
  width: 24px;
  height: 24px;
  place-items: center;
  margin-top: 1px;
  border-radius: 50%;
  background: var(--blue-soft);
  color: var(--blue);
}

.run-chat-avatar.is-user {
  background: color-mix(in srgb, var(--gold) 12%, var(--paper));
  color: var(--gold);
}

.run-chat-body {
  min-width: 0;
}

.run-chat-steps {
  position: relative;
  padding-left: 14px;
}

.run-chat-steps::before {
  content: '';
  position: absolute;
  top: 5px;
  bottom: 5px;
  left: 3px;
  width: 1px;
  background: var(--line);
}

.run-chat-step {
  position: relative;
  padding-bottom: 11px;
}

.run-chat-step:last-child {
  padding-bottom: 0;
}

.run-chat-step::before {
  content: '';
  position: absolute;
  top: 5px;
  left: -14px;
  width: 5px;
  height: 5px;
  border-radius: 50%;
  background: var(--line-2);
}

.run-chat-step.is-final::before {
  background: var(--green);
}

.run-chat-meta {
  display: flex;
  align-items: baseline;
  gap: 8px;
  margin-bottom: 5px;
  font-family: var(--mono);
  font-size: 12px;
  color: var(--ink-3);
}

.run-chat-meta b {
  font-family: var(--sans);
  font-size: 13px;
  font-weight: 600;
  color: var(--ink);
}

.run-chat-text {
  margin: 0;
  font-family: var(--sans);
  font-size: 13.5px;
  line-height: 1.7;
  color: var(--ink);
  white-space: pre-wrap;
}

.run-chat-text.is-prompt {
  max-height: 170px;
  padding: 8px 10px;
  border: 1px solid var(--blue-line);
  border-radius: 6px;
  background: var(--blue-soft);
  font-size: 12.5px;
  line-height: 1.65;
  overflow: auto;
}

.run-chat-text.is-final {
  color: var(--ink);
}

.run-chat-source {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  margin-top: 6px;
  padding: 2px 6px;
  border-radius: 4px;
  background: var(--paper);
  font-family: var(--mono);
  font-size: 12px;
  color: var(--ink-3);
}

.run-chat-think {
  margin: 2px 0 7px;
  padding-left: 8px;
  border-left: 2px solid var(--line-2);
}

.run-chat-think > summary,
.run-chat-tool > summary,
.run-chat-args > summary {
  list-style: none;
}

.run-chat-think > summary::-webkit-details-marker,
.run-chat-tool > summary::-webkit-details-marker,
.run-chat-args > summary::-webkit-details-marker {
  display: none;
}

.run-chat-think > summary {
  display: flex;
  align-items: center;
  gap: 5px;
  font-family: var(--sans);
  font-size: 12px;
  color: var(--ink-3);
  cursor: pointer;
}

.run-chat-think pre {
  max-height: 220px;
  margin: 6px 0 0;
  padding: 8px;
  overflow: auto;
  border-radius: 4px;
  background: var(--paper);
  font-family: var(--mono);
  font-size: 11.5px;
  line-height: 1.55;
  color: var(--ink-2);
  white-space: pre-wrap;
}

.run-chat-arrow {
  margin-left: auto;
  flex: none;
  color: var(--ink-3);
  transition: transform 0.15s ease;
}

details[open] > summary .run-chat-arrow {
  transform: rotate(90deg);
}

.run-chat-tools {
  display: flex;
  flex-direction: column;
  gap: 6px;
  margin-top: 8px;
}

.run-chat-tool {
  border: 0;
  border-radius: 4px;
  background: color-mix(in srgb, var(--paper) 55%, var(--surface));
  overflow: hidden;
}

.run-chat-tool > summary {
  display: flex;
  min-height: 32px;
  align-items: center;
  gap: 6px;
  padding: 0 9px;
  font-family: var(--sans);
  font-size: 13px;
  color: var(--ink-2);
  cursor: pointer;
}

.run-chat-tool-icon {
  flex: none;
  color: var(--blue);
}

.run-chat-tool b {
  flex: none;
  font-size: 13px;
  font-weight: 600;
  color: var(--ink);
}

.run-chat-tool-target {
  min-width: 0;
  overflow: hidden;
  font-family: var(--mono);
  font-size: 12px;
  color: var(--ink-3);
  text-overflow: ellipsis;
  white-space: nowrap;
}

.run-chat-tool > summary .run-chat-arrow {
  margin-left: 2px;
}

.run-chat-tool-time {
  margin-left: auto;
  flex: none;
  font-family: var(--mono);
  font-size: 12px;
  color: var(--ink-3);
}

.run-chat-gate {
  flex: none;
  padding: 1px 5px;
  border-radius: 99px;
  font-size: 11px;
  font-weight: 500;
}

.run-chat-gate.is-pass {
  background: var(--paper);
  color: var(--ink-3);
}

.run-chat-gate.is-fail {
  background: color-mix(in srgb, var(--red) 9%, var(--surface));
  color: var(--red);
}

.run-chat-tool-body {
  padding: 7px 9px 9px;
  border-top: 1px solid color-mix(in srgb, var(--line) 60%, transparent);
  background: color-mix(in srgb, var(--paper) 52%, var(--surface));
}

.run-chat-obs {
  max-height: 260px;
  margin: 0;
  overflow: auto;
  font-family: var(--mono);
  font-size: 12px;
  line-height: 1.55;
  color: var(--ink-2);
  white-space: pre-wrap;
  word-break: break-word;
}

.run-chat-args {
  margin-top: 6px;
}

.run-chat-args > summary {
  font-size: 11px;
  color: var(--ink-3);
  cursor: pointer;
}

.run-chat-args pre {
  max-height: 200px;
  margin: 5px 0 0;
  overflow: auto;
  font-family: var(--mono);
  font-size: 11px;
  line-height: 1.5;
  color: var(--ink-2);
  white-space: pre-wrap;
}

.run-chat-empty {
  margin: 0;
  font-size: 12px;
  color: var(--ink-3);
}

.run-ins-card {
  padding: 9px 10px;
  border: 1px solid color-mix(in srgb, var(--line) 80%, transparent);
  border-radius: 6px;
  background: color-mix(in srgb, var(--paper) 42%, var(--surface));
}

.run-ins-conclusion {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 9px 11px;
  border: 1px solid color-mix(in srgb, var(--green) 22%, var(--line));
  border-radius: 6px;
  background: color-mix(in srgb, var(--green) 7%, var(--surface));
  font-size: 13px;
  font-weight: 600;
  line-height: 1.45;
  color: var(--ink);
}

.run-ins-conclusion i {
  width: 7px;
  height: 7px;
  flex: none;
  border-radius: 50%;
  background: var(--green);
}

.run-ins-conclusion.is-running {
  border-color: color-mix(in srgb, var(--blue) 25%, var(--line));
  background: color-mix(in srgb, var(--blue) 7%, var(--surface));
}

.run-ins-conclusion.is-running i {
  background: var(--blue);
}

.run-ins-conclusion.is-fail {
  border-color: color-mix(in srgb, var(--red) 25%, var(--line));
  background: color-mix(in srgb, var(--red) 6%, var(--surface));
}

.run-ins-conclusion.is-fail i {
  background: var(--red);
}

.run-ins-conclusion.is-idle {
  border-color: var(--line);
  background: var(--paper);
}

.run-ins-conclusion.is-idle i {
  background: var(--line-2);
}

.run-ins-card h4 {
  display: flex;
  flex-wrap: wrap;
  align-items: baseline;
  gap: 3px 8px;
  margin: 0 0 7px;
  font-size: 12px;
  font-weight: 600;
  letter-spacing: 0.03em;
  color: var(--ink-2);
}

.run-ins-h4-meta {
  margin-left: auto;
  font-family: var(--mono);
  font-size: 11.5px;
  font-weight: 400;
  letter-spacing: 0;
  color: var(--ink-3);
}

.run-ins-kv {
  display: grid;
  grid-template-columns: 76px minmax(0, 1fr);
  gap: 5px 8px;
  align-items: baseline;
  font-size: 12px;
}

.run-ins-kv > span {
  color: var(--ink-3);
}

.run-ins-kv > b,
.run-ins-kv > code {
  overflow: hidden;
  font-weight: 500;
  color: var(--ink-2);
  text-overflow: ellipsis;
  white-space: nowrap;
}

.run-ins-kv > b.is-wrap {
  white-space: normal;
}

.run-ins-kv > code {
  font-family: var(--mono);
  font-size: 12px;
}

.run-ins-kv.is-four {
  grid-template-columns: 64px minmax(0, 1fr) 64px minmax(0, 1fr);
}

.run-ins-gate {
  display: inline-flex;
  margin: 0 0 8px;
  padding: 3px 8px;
  border-radius: 99px;
  font-size: 12px;
  font-weight: 600;
}

.run-ins-gate.is-pass {
  background: color-mix(in srgb, var(--green) 8%, var(--surface));
  color: color-mix(in srgb, var(--green) 82%, var(--ink));
}

.run-ins-gate.is-fail {
  background: color-mix(in srgb, var(--red) 12%, var(--paper));
  color: var(--red);
}

.run-ins-stat-grid {
  display: grid;
  grid-template-columns: repeat(5, minmax(0, 1fr));
  gap: 5px;
  margin-top: 3px;
}

.run-ins-stat-grid > div {
  display: flex;
  min-width: 0;
  flex-direction: column;
  gap: 1px;
  padding: 5px 6px;
  border-radius: 4px;
  background: color-mix(in srgb, var(--paper) 60%, var(--surface));
}

.run-ins-stat-grid span {
  font-size: 11px;
  color: var(--ink-3);
}

.run-ins-stat-grid b {
  overflow: hidden;
  font-family: var(--mono);
  font-size: 12px;
  font-weight: 500;
  color: var(--ink-2);
  text-overflow: ellipsis;
  white-space: nowrap;
}

.run-ins-stat-grid.is-three {
  grid-template-columns: repeat(3, minmax(0, 1fr));
}

.run-ins-slowest {
  display: flex;
  flex-wrap: wrap;
  gap: 3px 7px;
  margin: 7px 0 0;
  font-size: 12px;
  color: var(--ink-3);
}

.run-ins-slowest b {
  font-family: var(--mono);
  font-weight: 500;
  color: var(--ink-2);
}

.run-ins-issues {
  margin: 8px 0 0;
  padding: 7px 8px;
  border-radius: 4px;
  background: color-mix(in srgb, var(--red) 5%, var(--surface));
  font-size: 12px;
  line-height: 1.5;
  color: var(--ink-2);
}

.run-ins-brief summary {
  font-size: 12px;
  color: var(--ink-2);
  cursor: pointer;
}

.run-ins-brief pre {
  max-height: 180px;
  margin: 8px 0 0;
  overflow: auto;
  padding: 8px;
  border-radius: 4px;
  background: var(--paper);
  font-family: var(--mono);
  font-size: 12px;
  line-height: 1.55;
  color: var(--ink-2);
  white-space: pre-wrap;
}

.run-ins-muted {
  margin: 2px 0;
  font-size: 12px;
  line-height: 1.5;
  color: var(--ink-3);
}

.run-ins-muted.is-small {
  font-size: 12px;
}

@media (max-height: 800px) {
  .run-ins-token-note {
    display: none;
  }
}

.run-ins-round {
  border: 1px solid color-mix(in srgb, var(--line) 80%, transparent);
  border-radius: 6px;
  overflow: hidden;
}

.run-ins-round-head {
  display: flex;
  width: 100%;
  align-items: center;
  gap: 8px;
  padding: 8px 9px;
  border: 0;
  background: color-mix(in srgb, var(--paper) 55%, var(--surface));
  text-align: left;
  cursor: pointer;
}

.run-ins-round-index {
  flex: none;
  font-size: 12px;
  font-weight: 600;
  color: var(--ink);
}

.run-ins-round-stats {
  display: flex;
  min-width: 0;
  flex: 1;
  gap: 8px;
  overflow: hidden;
  font-family: var(--mono);
  font-size: 12px;
  color: var(--ink-3);
  white-space: nowrap;
}

.run-ins-chevron {
  flex: none;
  font-size: 13px;
  color: var(--ink-3);
  transition: transform 0.15s ease;
}

.run-ins-chevron.is-open {
  transform: rotate(180deg);
}

.run-ins-round-body {
  padding: 8px 9px 10px;
  border-top: 1px solid color-mix(in srgb, var(--line) 55%, transparent);
}

.run-ins-model {
  padding-bottom: 7px;
  border-bottom: 1px dashed color-mix(in srgb, var(--line) 70%, transparent);
}

.run-ins-model-head {
  display: flex;
  flex-wrap: wrap;
  gap: 4px 9px;
  font-family: var(--mono);
  font-size: 12px;
  color: var(--ink-3);
}

.run-ins-model-label {
  font-family: var(--sans);
  font-weight: 600;
  color: var(--ink-2);
}

.run-ins-fold {
  margin-top: 6px;
}

.run-ins-fold summary {
  font-size: 12px;
  color: var(--ink-3);
  cursor: pointer;
}

.run-ins-fold pre {
  max-height: 220px;
  margin: 5px 0 0;
  overflow: auto;
  padding: 7px;
  border-radius: 4px;
  background: var(--paper);
  font-family: var(--mono);
  font-size: 12px;
  line-height: 1.5;
  color: var(--ink-2);
  white-space: pre-wrap;
}

.run-ins-tool {
  border-top: 1px dashed color-mix(in srgb, var(--line) 60%, transparent);
}

.run-ins-tool-head {
  display: flex;
  min-height: 28px;
  align-items: center;
  gap: 6px;
  font-size: 12px;
  color: var(--ink-2);
  cursor: pointer;
  list-style: none;
}

.run-ins-tool-head::before {
  content: '▸';
  flex: none;
  font-size: 10px;
  color: var(--ink-3);
}

.run-ins-tool[open] > .run-ins-tool-head::before {
  content: '▾';
}

.run-ins-tool-head::-webkit-details-marker {
  display: none;
}

.run-ins-tool-icon {
  width: 15px;
  flex: none;
  color: var(--blue);
  font-family: var(--mono);
  text-align: center;
}

.run-ins-tool-head b {
  flex: none;
  font-family: var(--mono);
  font-size: 12px;
  font-weight: 500;
}

.run-ins-tool-arg {
  min-width: 0;
  overflow: hidden;
  font-family: var(--mono);
  font-size: 12px;
  color: var(--ink-3);
  text-overflow: ellipsis;
  white-space: nowrap;
}

.run-ins-tool-time {
  margin-left: auto;
  flex: none;
  font-family: var(--mono);
  font-size: 12px;
  color: var(--ink-3);
}

.run-ins-tool-body {
  padding: 0 0 8px 21px;
}

.run-ins-tool-obs {
  max-height: 260px;
  margin: 2px 0 0;
  overflow: auto;
  padding-right: 4px;
  font-family: var(--mono);
  font-size: 12px;
  line-height: 1.55;
  color: var(--ink-3);
  white-space: pre-wrap;
  word-break: break-word;
}

.run-ins-link {
  padding: 0;
  border: 0;
  background: transparent;
  font-size: 12px;
  color: var(--blue);
  cursor: pointer;
}

.run-ins-link:hover {
  text-decoration: underline;
}

.run-ins-logs {
  display: flex;
  min-height: 0;
  height: 100%;
  flex-direction: column;
  gap: 8px;
}

.run-ins-log-tools {
  display: flex;
  gap: 6px;
}

.run-ins-select,
.run-ins-search {
  height: 26px;
  border: 1px solid var(--line);
  border-radius: 4px;
  background: var(--surface);
  font-size: 12px;
  color: var(--ink-2);
}

.run-ins-select {
  padding: 0 5px;
}

.run-ins-search {
  min-width: 0;
  flex: 1;
  padding: 0 7px;
}

.run-ins-follow {
  height: 26px;
  padding: 0 8px;
  border: 1px solid var(--line);
  border-radius: 4px;
  background: transparent;
  font-size: 12px;
  color: var(--ink-3);
  cursor: pointer;
}

.run-ins-follow.is-on {
  border-color: color-mix(in srgb, var(--blue) 35%, var(--line));
  color: var(--blue);
}

.run-ins-log-list {
  min-height: 0;
  flex: 1;
  overflow: auto;
  border: 1px solid color-mix(in srgb, var(--line) 70%, transparent);
  border-radius: 5px;
  background: var(--paper);
}

.run-ins-log-row {
  display: grid;
  width: 100%;
  grid-template-columns: 54px 38px minmax(0, 1fr);
  gap: 6px;
  padding: 5px 7px;
  border: 0;
  border-bottom: 1px solid color-mix(in srgb, var(--line) 35%, transparent);
  background: transparent;
  text-align: left;
  cursor: pointer;
}

.run-ins-log-row:hover {
  background: var(--surface);
}

.run-ins-log-time {
  font-family: var(--mono);
  font-size: 12px;
  color: var(--ink-3);
}

.run-ins-log-level {
  font-family: var(--mono);
  font-size: 12px;
  color: var(--ink-3);
}

.run-ins-log-row.is-warn .run-ins-log-level { color: var(--gold); }
.run-ins-log-row.is-error .run-ins-log-level { color: var(--red); }

.run-ins-log-message {
  display: -webkit-box;
  overflow: hidden;
  font-family: var(--mono);
  font-size: 12px;
  line-height: 1.45;
  color: var(--ink-2);
  white-space: pre-wrap;
  -webkit-box-orient: vertical;
  -webkit-line-clamp: 2;
}

.run-ins-log-message.is-open {
  -webkit-line-clamp: unset;
}

.run-ins-artifact {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: 8px;
  padding: 8px 9px;
  border: 1px solid color-mix(in srgb, var(--line) 80%, transparent);
  border-radius: 5px;
  background: color-mix(in srgb, var(--paper) 42%, var(--surface));
  text-align: left;
  cursor: pointer;
}

.run-ins-artifact:hover {
  border-color: var(--line-2);
}

.run-ins-artifact-name {
  overflow: hidden;
  font-family: var(--mono);
  font-size: 12px;
  color: var(--ink);
  text-overflow: ellipsis;
  white-space: nowrap;
}

.run-ins-artifact-meta {
  flex: none;
  font-family: var(--mono);
  font-size: 12px;
  color: var(--ink-3);
}

.run-ins-preview {
  border: 1px solid var(--line);
  border-radius: 5px;
  overflow: hidden;
}

.run-ins-preview-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 6px 8px;
  border-bottom: 1px solid var(--line);
  background: var(--paper);
  font-size: 12px;
}

.run-ins-preview pre {
  max-height: 320px;
  margin: 0;
  overflow: auto;
  padding: 8px;
  font-family: var(--mono);
  font-size: 12px;
  line-height: 1.55;
  color: var(--ink-2);
  white-space: pre-wrap;
}
</style>
