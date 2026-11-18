<script setup lang="ts">
import { computed, nextTick, ref, watch } from 'vue'

import * as api from '../../../lib/api'
import type { AgentRun, AgentTurn } from '../../../lib/agent/types'
import type { ProductionJob, ProductionLog, ProductionRunArtifact } from '../../../types'
import { nodeDuration, type RunNode } from '../../../lib/runModel'

const props = defineProps<{
  node: RunNode | null
  logs: ProductionLog[]
  job: ProductionJob | null
  agentTurns: AgentTurn[]
  agentRun: AgentRun | null
  running: boolean
}>()
const emit = defineEmits<{ close: [] }>()

type Tab = 'overview' | 'logs' | 'rounds' | 'artifacts' | 'metrics'
const tab = ref<Tab>('overview')
const level = ref<'all' | 'info' | 'warn' | 'error'>('all')
const search = ref('')
const follow = ref(true)
const logRef = ref<HTMLElement | null>(null)
const expandedLogs = ref<Set<number>>(new Set())
const expandedTurns = ref<Set<number>>(new Set())
const preview = ref<{ name: string; text: string } | null>(null)
const previewLoading = ref(false)

const turns = computed<AgentTurn[]>(() => {
  if (props.agentTurns.length) return props.agentTurns
  return props.agentRun?.turns ?? []
})
const toolTurns = computed(() => turns.value.filter((turn) => turn.tool))

const tabs = computed<Array<{ id: Tab; label: string }>>(() => {
  const list: Array<{ id: Tab; label: string }> = [
    { id: 'overview', label: '概览' },
    { id: 'logs', label: '日志' },
  ]
  if ((props.node?.key === 'agent' || props.node?.key === 'author') && (props.agentTurns.length > 0 || (props.agentRun?.turns.length ?? 0) > 0)) list.push({ id: 'rounds', label: '回合' })
  list.push({ id: 'artifacts', label: '产物' }, { id: 'metrics', label: '指标' })
  return list
})

const nodeLogs = computed(() => {
  const node = props.node
  if (!node || node.stepIndex < 0) return []
  return props.logs.filter((log) => log.stepIndex === node.stepIndex)
})

const filteredLogs = computed(() => {
  const query = search.value.trim().toLowerCase()
  return nodeLogs.value.filter((log) => {
    if (level.value !== 'all' && log.level !== level.value) return false
    if (query && !log.message.toLowerCase().includes(query)) return false
    return true
  })
})

const lastInspect = computed(() => {
  for (let index = turns.value.length - 1; index >= 0; index -= 1) {
    const turn = turns.value[index]
    if (turn.tool === 'inspect' && turn.observation) return turn.observation
  }
  return ''
})
const metrics = computed(() => parseInspect(lastInspect.value))

const duration = computed(() => (props.node ? nodeDuration(props.node) : 0))
const statusLabel = computed(() => statusText(props.node?.status ?? ''))

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

function formatDuration(seconds: number): string {
  if (seconds >= 60) return `${Math.floor(seconds / 60)}m${String(seconds % 60).padStart(2, '0')}s`
  return `${seconds}s`
}

function timeLabel(value: number): string {
  if (!value) return '—'
  return new Date(value).toLocaleTimeString('zh-CN', { hour12: false })
}

function logTime(value: number): string {
  if (!value) return '—'
  return new Date(value).toLocaleTimeString('zh-CN', { hour12: false }).slice(0, 8)
}

function parseAgentLog(message: string): { round: number; tool: string; body: string } | null {
  const match = message.match(/^#(\d+)\s+(\S+?)\(\)(?:\s*·\s*([\s\S]*))?$/)
  if (!match) return null
  return { round: Number(match[1]), tool: match[2], body: match[3] ?? '' }
}

function toggleLog(id: number): void {
  const next = new Set(expandedLogs.value)
  if (next.has(id)) next.delete(id)
  else next.add(id)
  expandedLogs.value = next
}

function toggleTurn(index: number): void {
  const next = new Set(expandedTurns.value)
  if (next.has(index)) next.delete(index)
  else next.add(index)
  expandedTurns.value = next
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

function turnDuration(turn: AgentTurn): string {
  if (!turn.startedAt) return ''
  const end = turn.endedAt || (props.running ? Date.now() : turn.startedAt)
  return formatDuration(Math.max(0, Math.round((end - turn.startedAt) / 1000)))
}

function iconFor(tool: string): string {
  if (tool === 'inspect' || tool === 'look') return '◉'
  if (tool === 'review') return '✓'
  if (tool === 'patch_figure' || tool === 'patch_section') return '✎'
  if (tool === 'finish') return '■'
  return '·'
}

interface InspectMetrics {
  sections: number | null
  figures: number | null
  values: Array<{ key: string; label: string; value: number }>
  issues: string
  clean: boolean
}

function parseInspect(text: string): InspectMetrics | null {
  if (!text) return null
  const sectionMatch = text.match(/节\s*(\d+)\s*·\s*图\s*(\d+)/)
  const summary = text.split('问题：')[0]
  const labels: Array<[string, string]> = [
    ['overflow', '溢出'],
    ['outOfBounds', '越界'],
    ['under12', '小字'],
    ['svgClipped', 'SVG 越界'],
    ['emptyBoxes', '空框'],
    ['labelOverlaps', '标签重叠'],
    ['textOverflow', '文字溢出'],
  ]
  const values = labels.map(([key, label]) => {
    const match = summary.match(new RegExp(`${label}\\s*(\\d+)`))
    return { key, label, value: match ? Number(match[1]) : 0 }
  })
  return {
    sections: sectionMatch ? Number(sectionMatch[1]) : null,
    figures: sectionMatch ? Number(sectionMatch[2]) : null,
    values,
    issues: text.split('问题：')[1]?.trim() ?? '',
    clean: values.every((item) => item.value === 0),
  }
}

async function openArtifact(artifact: ProductionRunArtifact): Promise<void> {
  const lower = artifact.name.toLowerCase()
  if (lower.endsWith('.pdf') || lower.endsWith('.png') || lower.endsWith('.html')) {
    await api.openLocalPath(artifact.path)
    return
  }
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

function bytes(value: number): string {
  if (value >= 1024 * 1024) return `${(value / 1024 / 1024).toFixed(1)} MB`
  if (value >= 1024) return `${(value / 1024).toFixed(1)} KB`
  return `${value} B`
}

watch(tabs, (items) => {
  if (!items.some((item) => item.id === tab.value)) tab.value = 'overview'
})

watch(
  () => props.node?.key,
  () => {
    preview.value = null
    expandedLogs.value = new Set()
    expandedTurns.value = new Set()
    follow.value = true
    const key = props.node?.key
    const hasRounds = key === 'agent' && (props.agentTurns.length > 0 || (props.agentRun?.turns.length ?? 0) > 0)
    tab.value = hasRounds ? 'rounds' : 'overview'
  },
  { immediate: true },
)

watch(
  () => filteredLogs.value.length,
  () => {
    void scrollLogs()
  },
)
</script>

<template>
  <aside class="run-inspector" aria-label="节点详情">
    <div v-if="!node" class="run-inspector-empty">选择一个节点，查看它的日志、产物和指标。</div>

    <template v-else>
      <header class="run-inspector-head">
        <div class="run-inspector-title">
          <span class="run-inspector-kicker">节点</span>
          <b>{{ node.label }}</b>
          <span class="run-inspector-status" :class="`is-${node.status}`">
            <i />{{ statusLabel }}
          </span>
        </div>
        <button type="button" class="run-inspector-close" title="收起" @click="emit('close')">×</button>
      </header>
      <div class="run-inspector-meta">
        <span>{{ duration ? formatDuration(duration) : '—' }}</span>
        <span v-if="node.tokens">{{ node.tokens.toLocaleString() }} tok</span>
        <span v-if="node.cost">¥{{ node.cost.toFixed(4) }}</span>
        <span v-if="node.key === 'agent' || node.key === 'author' && toolTurns.length">{{ toolTurns.length }} 回合</span>
        <span class="run-inspector-key">{{ node.key }}</span>
      </div>

      <div class="run-inspector-tabs" role="tablist">
        <button
          v-for="item in tabs"
          :key="item.id"
          type="button"
          role="tab"
          :aria-selected="tab === item.id"
          :class="{ 'is-active': tab === item.id }"
          @click="tab = item.id"
        >
          {{ item.label }}
        </button>
      </div>

      <div class="run-inspector-body">
        <section v-if="tab === 'overview'" class="run-inspector-panel">
          <dl class="run-inspector-list">
            <div><dt>状态</dt><dd>{{ statusLabel }}</dd></div>
            <div><dt>开始</dt><dd>{{ timeLabel(node.startedAt) }}</dd></div>
            <div><dt>结束</dt><dd>{{ timeLabel(node.endedAt) }}</dd></div>
            <div><dt>耗时</dt><dd>{{ duration ? formatDuration(duration) : '—' }}</dd></div>
            <div><dt>tokens</dt><dd>{{ node.tokens ? node.tokens.toLocaleString() : '—' }}</dd></div>
            <div><dt>成本</dt><dd>{{ node.cost ? `¥${node.cost.toFixed(4)}` : '—' }}</dd></div>
            <div><dt>步骤</dt><dd>{{ node.stepIndex >= 0 ? `#${node.stepIndex + 1}` : '运行节点' }}</dd></div>
          </dl>
          <p v-if="node.error" class="run-inspector-error">{{ node.error }}</p>
          <p v-if="node.summary" class="run-inspector-summary">{{ node.summary }}</p>

          <template v-if="node.key === 'agent'">
            <h4>智能体</h4>
            <dl class="run-inspector-list">
              <div><dt>目标</dt><dd>把草稿修到版面干净、结构完整，并通过独立审校</dd></div>
              <div><dt>回合</dt><dd>{{ toolTurns.length || '—' }}</dd></div>
              <div><dt>工具</dt><dd class="run-inspector-tools">read_draft / inspect / look / patch_figure / patch_section / review / todo_write / finish</dd></div>
            </dl>
          </template>

          <h4>输出</h4>
          <pre class="run-inspector-code">{{ node.summary || node.stepDetail || '还没有输出' }}</pre>
        </section>

        <section v-else-if="tab === 'logs'" class="run-inspector-panel is-logs">
          <div class="run-log-tools">
            <select v-model="level" class="run-log-select" aria-label="日志级别">
              <option value="all">全部级别</option>
              <option value="info">信息</option>
              <option value="warn">警告</option>
              <option value="error">错误</option>
            </select>
            <input v-model="search" class="run-log-search" type="search" placeholder="搜索日志…" />
            <button type="button" class="run-log-follow" :class="{ 'is-on': follow }" @click="setFollow(!follow)">
              {{ follow ? '跟随' : '已暂停' }}
            </button>
          </div>
          <div ref="logRef" class="run-log" @scroll="onLogScroll">
            <div v-for="log in filteredLogs" :key="log.id" class="run-log-row" :class="`is-${log.level}`">
              <span class="run-log-time">{{ logTime(log.createdAt) }}</span>
              <span class="run-log-dot" />
              <div class="run-log-body">
                <template v-if="parseAgentLog(log.message)">
                  <div class="run-log-agent">
                    <span class="run-log-round">#{{ parseAgentLog(log.message)?.round }}</span>
                    <span class="run-log-tool">{{ parseAgentLog(log.message)?.tool }}</span>
                    <button
                      v-if="(parseAgentLog(log.message)?.body.length ?? 0) > 100"
                      type="button"
                      class="run-log-expand"
                      @click="toggleLog(log.id)"
                    >
                      {{ expandedLogs.has(log.id) ? '收起' : '展开' }}
                    </button>
                  </div>
                  <p class="run-log-message" :class="{ 'is-clamped': !expandedLogs.has(log.id) }">
                    {{ parseAgentLog(log.message)?.body }}
                  </p>
                </template>
                <template v-else>
                  <p class="run-log-message" :class="{ 'is-clamped': !expandedLogs.has(log.id) }">{{ log.message }}</p>
                  <button
                    v-if="log.message.length > 220"
                    type="button"
                    class="run-log-expand"
                    @click="toggleLog(log.id)"
                  >
                    {{ expandedLogs.has(log.id) ? '收起' : '展开' }}
                  </button>
                </template>
              </div>
            </div>
            <p v-if="!filteredLogs.length" class="run-inspector-hint">
              {{ nodeLogs.length ? '没有匹配的日志。' : '这个节点还没有日志。' }}
            </p>
          </div>
        </section>

        <section v-else-if="tab === 'rounds'" class="run-inspector-panel is-rounds">
          <div v-if="!toolTurns.length" class="run-inspector-hint">智能体还没有产生回合记录。</div>
          <ol v-else class="run-rounds">
            <li v-for="turn in toolTurns" :key="turn.index" class="run-round">
              <span class="run-round-dot" :class="{ 'is-current': running && turn.index === toolTurns[toolTurns.length - 1].index }" />
              <div class="run-round-main">
                <button type="button" class="run-round-head" @click="toggleTurn(turn.index)">
                  <span class="run-round-n">#{{ turn.index + 1 }}</span>
                  <span class="run-round-tool">{{ turn.tool }}</span>
                  <span class="run-round-icon">{{ iconFor(turn.tool ?? '') }}</span>
                  <span class="run-round-dur">{{ turnDuration(turn) }}</span>
                  <span class="run-round-chevron">{{ expandedTurns.has(turn.index) ? '−' : '+' }}</span>
                </button>
                <p class="run-round-obs" :class="{ 'is-clamped': !expandedTurns.has(turn.index) }">
                  {{ turn.observation || turn.note || '（无观察）' }}
                </p>
              </div>
            </li>
          </ol>
        </section>

        <section v-else-if="tab === 'artifacts'" class="run-inspector-panel">
          <div v-if="!node.artifacts.length" class="run-inspector-hint">这个节点还没有产物。</div>
          <div v-else class="run-artifacts">
            <button v-for="artifact in node.artifacts" :key="artifact.id" type="button" class="run-artifact" @click="openArtifact(artifact)">
              <span class="run-artifact-name">{{ artifact.name }}</span>
              <span class="run-artifact-meta">{{ artifact.kind }} · {{ bytes(artifact.bytes) }}</span>
              <span class="run-artifact-action">{{ /\.(pdf|png|html)$/i.test(artifact.name) ? '打开 ↗' : '预览' }}</span>
            </button>
          </div>
          <div v-if="preview" class="run-artifact-preview">
            <div class="run-artifact-preview-head">
              <b>{{ preview.name }}</b>
              <button type="button" @click="preview = null">×</button>
            </div>
            <pre>{{ preview.text }}</pre>
          </div>
          <p v-if="previewLoading" class="run-inspector-hint">读取中…</p>
        </section>

        <section v-else-if="tab === 'metrics'" class="run-inspector-panel">
          <div v-if="!metrics" class="run-inspector-hint">这个节点没有版面指标；只有 inspect 会产生。</div>
          <template v-else>
            <div class="run-metrics-head">
              <b>{{ metrics.clean ? '版面干净' : '仍有问题' }}</b>
              <span v-if="metrics.sections !== null">节 {{ metrics.sections }} · 图 {{ metrics.figures }}</span>
            </div>
            <div class="run-metrics-grid">
              <div v-for="item in metrics.values" :key="item.key" class="run-metric" :class="{ 'is-bad': item.value > 0 }">
                <span>{{ item.label }}</span>
                <b>{{ item.value }}</b>
              </div>
            </div>
            <p v-if="metrics.issues" class="run-metrics-issues">{{ metrics.issues }}</p>
          </template>
        </section>
      </div>
    </template>
  </aside>
</template>

<style scoped>
.run-inspector {
  display: flex;
  flex-direction: column;
  min-height: 0;
  overflow: hidden;
  border-left: 1px solid var(--line);
  background: var(--surface);
  font-family: var(--sans);
}

.run-inspector-empty {
  padding: 24px 18px;
  color: var(--ink-3);
  font-size: 12px;
  line-height: 1.6;
}

.run-inspector-head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 8px;
  padding: 12px 14px 6px;
}

.run-inspector-title {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 6px;
  min-width: 0;
}

.run-inspector-kicker {
  font-size: 11px;
  letter-spacing: 0.08em;
  color: var(--ink-3);
}

.run-inspector-title b {
  font-size: 13px;
  font-weight: 600;
  color: var(--ink);
}

.run-inspector-status {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 1px 6px;
  border: 1px solid var(--line);
  border-radius: 3px;
  font-size: 11px;
  color: var(--ink-3);
}

.run-inspector-status i {
  width: 5px;
  height: 5px;
  border-radius: 50%;
  background: currentColor;
}

.run-inspector-status.is-running { color: var(--blue); border-color: var(--blue-line); background: var(--blue-soft); }
.run-inspector-status.is-done { color: var(--green); border-color: color-mix(in srgb, var(--green) 40%, var(--line)); }
.run-inspector-status.is-error { color: var(--red); border-color: color-mix(in srgb, var(--red) 40%, var(--line)); }
.run-inspector-status.is-interrupted { color: var(--gold); border-color: color-mix(in srgb, var(--gold) 40%, var(--line)); }
.run-inspector-status.is-cancelled { color: var(--ink-3); border-color: var(--line); }

.run-inspector-close {
  flex: none;
  width: 24px;
  height: 24px;
  border: 0;
  border-radius: 3px;
  background: transparent;
  color: var(--ink-3);
  font-size: 18px;
  line-height: 1;
  cursor: pointer;
}

.run-inspector-close:hover {
  background: var(--blue-soft);
  color: var(--ink);
}

.run-inspector-meta {
  display: flex;
  flex-wrap: wrap;
  gap: 4px 10px;
  padding: 0 14px 9px;
  font-family: var(--mono);
  font-size: 11px;
  color: var(--ink-3);
}

.run-inspector-key {
  margin-left: auto;
}

.run-inspector-tabs {
  display: flex;
  gap: 2px;
  padding: 0 10px;
  border-bottom: 1px solid var(--line);
}

.run-inspector-tabs button {
  padding: 7px 8px 6px;
  border: 0;
  border-bottom: 2px solid transparent;
  background: transparent;
  color: var(--ink-3);
  font-family: var(--sans);
  font-size: 11.5px;
  cursor: pointer;
}

.run-inspector-tabs button:hover {
  color: var(--ink);
}

.run-inspector-tabs button.is-active {
  color: var(--blue);
  border-bottom-color: var(--blue);
}

.run-inspector-body {
  flex: 1 1 auto;
  min-height: 0;
  display: flex;
  overflow: hidden;
}

.run-inspector-panel {
  flex: 1 1 auto;
  min-width: 0;
  overflow: auto;
  padding: 12px 14px 18px;
}

.run-inspector-panel.is-logs {
  display: flex;
  flex-direction: column;
  min-height: 0;
  overflow: hidden;
  padding-bottom: 10px;
}

.run-inspector-panel h4 {
  margin: 16px 0 6px;
  font-size: 11.5px;
  font-weight: 600;
  color: var(--ink-2);
}

.run-inspector-list {
  display: flex;
  flex-direction: column;
  gap: 4px;
  margin: 0;
}

.run-inspector-list div {
  display: grid;
  grid-template-columns: 68px minmax(0, 1fr);
  gap: 8px;
  font-size: 11.5px;
  line-height: 1.5;
}

.run-inspector-list dt {
  color: var(--ink-3);
}

.run-inspector-list dd {
  margin: 0;
  color: var(--ink-2);
  word-break: break-word;
}

.run-inspector-tools {
  font-family: var(--mono);
  font-size: 11px;
  line-height: 1.6;
}

.run-inspector-summary {
  margin: 10px 0 0;
  font-size: 12px;
  line-height: 1.6;
  color: var(--ink-2);
}

.run-inspector-error {
  margin: 10px 0 0;
  padding: 8px 10px;
  border-left: 2px solid var(--red);
  background: color-mix(in srgb, var(--red) 5%, transparent);
  font-size: 11.5px;
  line-height: 1.55;
  color: var(--red);
}

.run-inspector-code {
  margin: 0;
  max-height: 160px;
  overflow: auto;
  padding: 8px 10px;
  border: 1px solid var(--line);
  border-radius: 4px;
  background: var(--paper);
  font-family: var(--mono);
  font-size: 11px;
  line-height: 1.55;
  color: var(--ink-2);
  white-space: pre-wrap;
  word-break: break-word;
}

.run-inspector-hint {
  margin: 0;
  padding: 10px 0;
  color: var(--ink-3);
  font-size: 11.5px;
  line-height: 1.6;
}

.run-log-tools {
  display: flex;
  gap: 6px;
  margin-bottom: 8px;
}

.run-log-select,
.run-log-search {
  height: 26px;
  border: 1px solid var(--line);
  border-radius: 4px;
  background: var(--paper);
  color: var(--ink-2);
  font-family: var(--sans);
  font-size: 11px;
}

.run-log-select { width: 82px; padding: 0 4px; }
.run-log-search { flex: 1 1 auto; min-width: 0; padding: 0 7px; }

.run-log-follow {
  flex: none;
  height: 26px;
  padding: 0 8px;
  border: 1px solid var(--line);
  border-radius: 4px;
  background: var(--paper);
  color: var(--ink-3);
  font-family: var(--sans);
  font-size: 11px;
  cursor: pointer;
}

.run-log-follow.is-on {
  color: var(--blue);
  border-color: var(--blue-line);
  background: var(--blue-soft);
}

.run-log {
  flex: 1 1 auto;
  min-height: 0;
  overflow: auto;
  border-top: 1px solid var(--line);
}

.run-log-row {
  display: grid;
  grid-template-columns: 46px 6px minmax(0, 1fr);
  gap: 6px;
  padding: 6px 0;
  border-bottom: 1px solid color-mix(in srgb, var(--line) 55%, transparent);
}

.run-log-time {
  font-family: var(--mono);
  font-size: 11px;
  line-height: 1.7;
  color: var(--ink-3);
}

.run-log-dot {
  width: 5px;
  height: 5px;
  margin-top: 6px;
  border-radius: 50%;
  background: var(--line-2);
}

.run-log-row.is-error .run-log-dot { background: var(--red); }
.run-log-row.is-warn .run-log-dot { background: var(--gold); }

.run-log-body {
  min-width: 0;
}

.run-log-agent {
  display: flex;
  align-items: center;
  gap: 5px;
  margin-bottom: 2px;
}

.run-log-round {
  font-family: var(--mono);
  font-size: 11px;
  color: var(--ink-3);
}

.run-log-tool {
  padding: 0 5px;
  border-radius: 3px;
  background: var(--blue-soft);
  color: var(--blue);
  font-family: var(--mono);
  font-size: 11px;
}

.run-log-expand {
  border: 0;
  background: transparent;
  color: var(--blue);
  font-family: var(--sans);
  font-size: 11px;
  cursor: pointer;
}

.run-log-message {
  margin: 0;
  font-family: var(--mono);
  font-size: 11px;
  line-height: 1.55;
  color: var(--ink-2);
  white-space: pre-wrap;
  word-break: break-word;
}

.run-log-message.is-clamped {
  display: -webkit-box;
  -webkit-line-clamp: 5;
  -webkit-box-orient: vertical;
  overflow: hidden;
}

.run-rounds {
  list-style: none;
  margin: 0;
  padding: 0;
}

.run-round {
  display: grid;
  grid-template-columns: 12px minmax(0, 1fr);
  gap: 7px;
  position: relative;
  padding-bottom: 10px;
}

.run-round::before {
  content: "";
  position: absolute;
  left: 5px;
  top: 12px;
  bottom: -2px;
  width: 1px;
  background: var(--line);
}

.run-round:last-child::before { display: none; }

.run-round-dot {
  width: 7px;
  height: 7px;
  margin-top: 5px;
  border: 1px solid var(--line-2);
  border-radius: 50%;
  background: var(--surface);
  z-index: 1;
}

.run-round-dot.is-current {
  border-color: var(--blue);
  background: var(--blue);
  animation: run-round-pulse 1.2s ease-in-out infinite;
}

.run-round-main { min-width: 0; }

.run-round-head {
  display: flex;
  align-items: center;
  gap: 6px;
  width: 100%;
  padding: 0;
  border: 0;
  background: transparent;
  text-align: left;
  cursor: pointer;
}

.run-round-n {
  font-family: var(--mono);
  font-size: 11px;
  color: var(--ink-3);
}

.run-round-tool {
  font-family: var(--mono);
  font-size: 11px;
  color: var(--ink);
}

.run-round-icon {
  color: var(--blue);
  font-size: 10px;
}

.run-round-dur {
  margin-left: auto;
  font-family: var(--mono);
  font-size: 11px;
  color: var(--ink-3);
}

.run-round-chevron {
  width: 12px;
  color: var(--ink-3);
  font-family: var(--mono);
  font-size: 11px;
}

.run-round-obs {
  margin: 3px 0 0;
  font-family: var(--mono);
  font-size: 11px;
  line-height: 1.55;
  color: var(--ink-2);
  white-space: pre-wrap;
  word-break: break-word;
}

.run-round-obs.is-clamped {
  display: -webkit-box;
  -webkit-line-clamp: 3;
  -webkit-box-orient: vertical;
  overflow: hidden;
}

@keyframes run-round-pulse {
  0%, 100% { box-shadow: 0 0 0 0 color-mix(in srgb, var(--blue) 40%, transparent); }
  50% { box-shadow: 0 0 0 3px color-mix(in srgb, var(--blue) 0%, transparent); }
}

.run-artifacts {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.run-artifact {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto;
  grid-template-rows: auto auto;
  gap: 1px 8px;
  padding: 7px 8px;
  border: 1px solid var(--line);
  border-radius: 4px;
  background: var(--paper);
  text-align: left;
  cursor: pointer;
}

.run-artifact:hover {
  border-color: var(--blue-line);
}

.run-artifact-name {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-family: var(--mono);
  font-size: 11px;
  color: var(--ink);
}

.run-artifact-meta {
  font-family: var(--mono);
  font-size: 11px;
  color: var(--ink-3);
}

.run-artifact-action {
  grid-row: 1 / span 2;
  align-self: center;
  color: var(--blue);
  font-size: 11px;
}

.run-artifact-preview {
  margin-top: 10px;
  border: 1px solid var(--line);
  border-radius: 4px;
  overflow: hidden;
}

.run-artifact-preview-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 6px 8px;
  border-bottom: 1px solid var(--line);
  background: var(--paper);
  font-family: var(--mono);
  font-size: 10.5px;
}

.run-artifact-preview-head button {
  border: 0;
  background: transparent;
  color: var(--ink-3);
  font-size: 15px;
  cursor: pointer;
}

.run-artifact-preview pre {
  margin: 0;
  max-height: 220px;
  overflow: auto;
  padding: 8px;
  font-family: var(--mono);
  font-size: 10.5px;
  line-height: 1.55;
  color: var(--ink-2);
  white-space: pre-wrap;
  word-break: break-word;
}

.run-metrics-head {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: 8px;
  margin-bottom: 10px;
  font-size: 11.5px;
  color: var(--ink-3);
}

.run-metrics-head b {
  font-size: 13px;
  color: var(--green);
}

.run-metrics-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 6px;
}

.run-metric {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: 6px;
  padding: 6px 8px;
  border: 1px solid var(--line);
  border-radius: 4px;
  background: var(--paper);
  font-size: 11px;
  color: var(--ink-3);
}

.run-metric b {
  font-family: var(--mono);
  font-size: 12px;
  color: var(--green);
}

.run-metric.is-bad {
  border-color: color-mix(in srgb, var(--red) 40%, var(--line));
  background: color-mix(in srgb, var(--red) 4%, var(--paper));
}

.run-metric.is-bad b { color: var(--red); }

.run-metrics-issues {
  margin: 10px 0 0;
  padding: 8px 10px;
  border-left: 2px solid var(--red);
  background: color-mix(in srgb, var(--red) 4%, transparent);
  font-size: 11px;
  line-height: 1.6;
  color: var(--red);
}

@media (prefers-reduced-motion: reduce) {
  .run-round-dot.is-current {
    animation: none;
  }
}
</style>
