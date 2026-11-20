<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useRouter } from 'vue-router'
import {
  Check,
  CircleAlert,
  Download,
  ExternalLink,
  FileText,
  History,
  Pencil,
  Play,
  RotateCcw,
  Square,
  Trash2,
  X,
} from 'lucide-vue-next'

import * as api from '../../lib/api'
import { renderRichText } from '../../lib/richText'
import { stepLabel } from '../../lib/runModel'
import WorkflowCanvas, { type FlowNode } from './WorkflowCanvas.vue'
import {
  formatDate,
  formatDuration,
  formatTokens,
  progressOf,
  mediaUrl,
  sourceSummary,
  displayError,
  statusClass,
  jobStatusLabel,
  templateById,
} from '../../lib/production'
import {
  deleteProduction,
  production,
  refreshJobs,
  resumeProduction,
  retryProduction,
  upsertJob,
} from '../../stores/production'
import type { Fact, ProductionDrillItem, ProductionJob, ProductionJobDetail, ProductionResumeInfo, ProductionRunArtifact, ProductionRunGraph, ProductionVersion } from '../../types'

const props = defineProps<{ jobId: string }>()
const emit = defineEmits<{ close: [] }>()
const router = useRouter()

const detail = ref<ProductionJobDetail | null>(null)
const runGraph = ref<ProductionRunGraph | null>(null)
const flowLoading = ref(false)
const artifactPreview = ref<{ name: string; text: string } | null>(null)
const selectedNode = ref('')
const logRef = ref<HTMLElement | null>(null)
const railRef = ref<HTMLElement | null>(null)
const loading = ref(false)
const error = ref('')
const activeTab = ref('result')
const quizChoices = ref<Record<string, number>>({})
const renaming = ref(false)
const renameValue = ref('')
const renameInput = ref<HTMLInputElement | null>(null)
const confirmDelete = ref(false)
const detailLoading = ref(false)
const versions = ref<ProductionVersion[]>([])
const versionLoading = ref(false)
const versionError = ref('')
const selectedVersionId = ref('')
const versionBody = ref('')
const versionView = ref<'preview' | 'source'>('preview')

function versionPreviewBody(value: string): string {
  if (!value.startsWith('---')) return value
  const end = value.indexOf('\n---', 3)
  if (end < 0) return value
  return value.slice(end + 4).replace(/^\s*\n/, '')
}
const versionBodyLoading = ref(false)
const confirmRestoreId = ref('')
const restoreNotice = ref('')
const resumeInfo = ref<ProductionResumeInfo | null>(null)
const resumeInfoLoading = ref(false)
const resumeLoading = ref(false)
const confirmRerun = ref(false)
const traceFacts = ref<Fact[]>([])
const traceFactsLoading = ref(false)
const traceFactsError = ref('')
let pollTimer: number | undefined

const job = computed<ProductionJob | null>(() => {
  return production.jobs.find((item) => item.id === props.jobId) ?? detail.value?.job ?? null
})
const template = computed(() => (job.value ? templateById(production.templates, job.value.templateId) : undefined))
const result = computed(() => job.value?.result ?? null)
const tabs = computed(() => {
  const items = [
    { id: 'result', label: '结果' },
    { id: 'steps', label: '步骤' },
    { id: 'flow', label: '运行 ↗' },
  ]
  if ((result.value?.variants.length ?? 0) >= 2) items.push({ id: 'compare', label: '对照' })
  if (result.value?.mindmap) items.push({ id: 'mindmap', label: '思维导图' })
  if ((result.value?.drill.length ?? 0) > 0) items.push({ id: 'drill', label: '练一练' })
  if ((result.value?.trace.length ?? 0) > 0) items.push({ id: 'trace', label: '溯源' })
  items.push({ id: 'usage', label: '用量' })
  if (job.value?.noteId) items.push({ id: 'versions', label: '版本' })
  return items
})
const currentTab = computed(() => (tabs.value.some((item) => item.id === activeTab.value) ? activeTab.value : 'result'))
const selectedVersion = computed(() => versions.value.find((item) => item.id === selectedVersionId.value))
const tokens = computed(() => (job.value?.usage.promptTokens ?? 0) + (job.value?.usage.completionTokens ?? 0))
const shortJobId = computed(() => {
  const id = job.value?.id ?? ''
  return id.length > 24 ? `${id.slice(0, 14)}…${id.slice(-6)}` : id
})
const noteTitle = computed(() => {
  const body = result.value?.body ?? ''
  const line = body.split('\n').find((item) => item.startsWith('# '))
  return line?.slice(2).trim() || job.value?.title || ''
})
// Note titles often differ from the task title only by trailing punctuation;
// treat those as the same title so the result page does not print it twice.
const noteTitleDiffers = computed(() => {
  const strip = (value: string) => value.trim().replace(/[。！？!?.,，、；;：:\s]+$/, '')
  return strip(noteTitle.value) !== strip(job.value?.title ?? '')
})
const sourceMeta = computed(() => {
  const source = job.value?.source
  if (!source) return ''
  if (source.kind === 'text') return `${source.text.trim().length.toLocaleString()} 字`
  if (source.kind === 'file') return source.path.split(/[\\/]/).pop() || '本地文件'
  return source.videos[0]?.author || ''
})
// The rail already sits next to the header, so it only repeats what the header
// cannot show: the author for B站 and the folder for local files. The rest of
// the source line stays in the header to avoid printing it twice.
const railSource = computed(() => {
  const source = job.value?.source
  if (!source) return ''
  if (source.kind === 'file') {
    const parts = source.path.split(/[\\/]/).filter(Boolean)
    parts.pop()
    return parts.pop() ?? '本地文件'
  }
  if (source.kind === 'favorites') return `${source.videos.length} 个视频`
  if (source.kind === 'bili') return sourceMeta.value
  return ''
})
const railSourceLabel = computed(() => {
  const kind = job.value?.source.kind
  if (kind === 'bili') return 'UP 主'
  if (kind === 'favorites') return '收藏'
  if (kind === 'file') return '文件位置'
  return '来源摘录'
})
const runningLabel = computed(() => {
  const current = job.value
  if (!current) return ''
  if (current.status === 'queued') return '排队中'
  const step = (current.currentStep || '').trim()
  if (!step) return '正在处理'
  return /^[A-Za-z0-9]/.test(step) ? `正在 ${step}` : `正在${step}`
})
const elapsedSeconds = computed(() => {
  const current = job.value
  if (!current?.startedAt) return 0
  const end = current.finishedAt || Date.now()
  return Math.max(0, (end - current.startedAt) / 1000)
})
const sourceExcerpt = computed(() => {
  const source = job.value?.source
  if (source?.kind !== 'text') return ''
  const text = source.text
    .replace(/```[\s\S]*?```/g, ' ')
    .replace(/!\[[^\]]*\]\([^)]*\)/g, ' ')
    .replace(/\[([^\]]+)\]\([^)]*\)/g, '$1')
    .replace(/^#{1,6}\s*/gm, '')
    .replace(/^\s*[-*+]\s+/gm, '')
    .replace(/[*_~`>#]+/g, ' ')
    .replace(/\s+/g, ' ')
    .trim()
  return text.length > 140 ? `${text.slice(0, 140)}…` : text
})
const noteFile = computed(() => {
  const rel = job.value?.noteRelPath || ''
  return rel.split('/').filter(Boolean).pop() || ''
})
const noteDir = computed(() => {
  const parts = (job.value?.noteRelPath || '').split('/').filter(Boolean)
  parts.pop()
  return parts.join('/')
})

const bodyWithoutTitle = computed(() => (result.value?.body ?? '').replace(/^#\s+.*(?:\r?\n)+/, ''))

const renderMarkdown = renderRichText

async function loadTraceFacts(): Promise<void> {
  const noteId = job.value?.noteId
  const runId = job.value?.id
  if (!noteId || !runId) {
    traceFacts.value = []
    return
  }
  traceFactsLoading.value = true
  traceFactsError.value = ''
  try {
    traceFacts.value = await api.productionNoteFacts(noteId, runId)
  } catch (cause) {
    traceFactsError.value = api.errorText(cause)
    traceFacts.value = []
  } finally {
    traceFactsLoading.value = false
  }
}

function factStatusLabel(status: string): string {
  return ({
    candidate: '未核验',
    verified: '有证据支持',
    disputed: '有不同记载',
    unsupported: '暂无出处',
  } as Record<string, string>)[status] ?? status
}

function factStatusClass(status: string): string {
  if (status === 'verified') return 'is-verified'
  if (status === 'disputed') return 'is-disputed'
  if (status === 'unsupported') return 'is-unsupported'
  return 'is-candidate'
}

async function loadVersions(): Promise<void> {
  const noteId = job.value?.noteId
  if (!noteId) {
    versions.value = []
    return
  }
  versionLoading.value = true
  versionError.value = ''
  try {
    versions.value = await api.productionVersions(noteId)
    if (!versions.value.some((item) => item.id === selectedVersionId.value)) {
      selectedVersionId.value = versions.value[0]?.id ?? ''
      versionBody.value = ''
      if (versions.value[0]) void viewVersion(versions.value[0])
    }
  } catch (cause) {
    versionError.value = api.errorText(cause)
    versions.value = []
  } finally {
    versionLoading.value = false
  }
}

async function viewVersion(version: ProductionVersion): Promise<void> {
  const noteId = job.value?.noteId
  if (!noteId) return
  selectedVersionId.value = version.id
  versionBody.value = ''
  versionBodyLoading.value = true
  versionError.value = ''
  try {
    versionBody.value = await api.productionVersionContent(noteId, version.id)
  } catch (cause) {
    versionError.value = api.errorText(cause)
  } finally {
    versionBodyLoading.value = false
  }
}

function requestRestore(version: ProductionVersion): void {
  selectedVersionId.value = version.id
  confirmRestoreId.value = version.id
}

async function restoreVersion(): Promise<void> {
  const noteId = job.value?.noteId
  const versionId = confirmRestoreId.value
  if (!noteId || !versionId) return
  try {
    await api.productionRestoreVersion(noteId, versionId)
    confirmRestoreId.value = ''
    restoreNotice.value = '已恢复。当前笔记已替换，恢复前的内容也保存成了手改备份。'
    await loadVersions()
    await loadDetail(true)
  } catch (cause) {
    versionError.value = api.errorText(cause)
    confirmRestoreId.value = ''
  }
}

function formatBytes(value: number): string {
  if (value < 1024) return `${value} B`
  if (value < 1024 * 1024) return `${(value / 1024).toFixed(1)} KB`
  return `${(value / 1024 / 1024).toFixed(1)} MB`
}

function syncPolling(): void {
  window.clearInterval(pollTimer)
  pollTimer = undefined
  if (job.value && ['running', 'queued'].includes(job.value.status)) {
    pollTimer = window.setInterval(() => {
      void loadDetail(true)
      if (currentTab.value === 'flow') void loadRunGraph(true)
    }, 1500)
  }
}

let flowLoadingGuard = false

async function loadRunGraph(silent = false): Promise<void> {
  if (!props.jobId || flowLoadingGuard) return
  flowLoadingGuard = true
  flowLoading.value = true
  try {
    runGraph.value = await api.productionRunGraph(props.jobId)
  } catch (cause) {
    if (!silent) error.value = api.errorText(cause)
  } finally {
    flowLoadingGuard = false
    flowLoading.value = false
  }
}

const flowNodes = computed<FlowNode[]>(() => {
  const steps = job.value?.steps ?? []
  const runs = runGraph.value?.nodes ?? []
  const runMap = new Map(runs.map((node) => [node.key, node]))
  const list: FlowNode[] = steps.map((step) => {
    const run = runMap.get(step.key)
    return {
      key: step.key,
      label: stepLabel(step.key, step.label),
      status: run?.status ?? (step.status === 'pending' ? 'queued' : step.status),
      startedAt: run?.startedAt ?? 0,
      endedAt: run?.endedAt ?? 0,
      tokens: run?.tokens ?? 0,
      cost: run?.cost ?? 0,
      summary: run?.summary || step.detail || '',
      error: run?.error ?? '',
      artifacts: artifactsFor(step.key),
    }
  })
  for (const run of runs) {
    if (steps.some((step) => step.key === run.key)) continue
    list.push({ ...run, artifacts: artifactsFor(run.key) })
  }
  return list
})

watch(flowNodes, (items) => {
  if (!items.length) return
  if (!items.some((item) => item.key === selectedNode.value)) selectedNode.value = items[0].key
}, { immediate: true })

const selectedFlowNode = computed(() => flowNodes.value.find((node) => node.key === selectedNode.value) ?? null)
const selectedStepIndex = computed(() => {
  const steps = job.value?.steps ?? []
  return steps.findIndex((step) => step.key === selectedNode.value)
})
const selectedLogs = computed(() => {
  if (selectedStepIndex.value < 0) return []
  return (detail.value?.logs ?? []).filter((log) => log.stepIndex === selectedStepIndex.value)
})

const nodeLogRef = ref<HTMLElement | null>(null)
const nodeLogStick = ref(true)

function onNodeLogScroll(): void {
  const el = nodeLogRef.value
  if (!el) return
  nodeLogStick.value = el.scrollHeight - el.scrollTop - el.clientHeight < 24
}

watch(selectedNode, () => {
  nodeLogStick.value = true
})

watch(
  () => selectedLogs.value.length,
  async () => {
    if (!nodeLogStick.value) return
    await nextTick()
    const el = nodeLogRef.value
    if (el) el.scrollTop = el.scrollHeight
  },
)

function artifactsFor(key: string): ProductionRunArtifact[] {
  return runGraph.value?.artifacts.filter((item) => item.nodeKey === key) ?? []
}

function nodeDuration(node: { startedAt: number; endedAt: number }): string {
  if (!node.startedAt) return '—'
  const end = node.endedAt || Date.now()
  const seconds = Math.max(0, Math.round((end - node.startedAt) / 1000))
  return formatDuration(seconds)
}

async function openArtifact(artifact: ProductionRunArtifact): Promise<void> {
  try {
    const text = await api.productionArtifactText(artifact.id)
    artifactPreview.value = { name: artifact.name, text }
  } catch (cause) {
    error.value = api.errorText(cause)
  }
}

async function loadDetail(silent = false): Promise<void> {
  if (!props.jobId || detailLoading.value) return
  detailLoading.value = true
  if (!silent) loading.value = true
  try {
    const data = await api.productionGet(props.jobId)
    if (data) {
      detail.value = data
      upsertJob(data.job)
      error.value = ''
      if (['error', 'interrupted', 'cancelled'].includes(data.job.status)) {
        resumeInfoLoading.value = true
        void loadResumeInfo()
      } else {
        resumeInfo.value = null
        resumeInfoLoading.value = false
      }
    }
  } catch (cause) {
    if (!silent) error.value = api.errorText(cause)
  } finally {
    loading.value = false
    detailLoading.value = false
    syncPolling()
  }
}

async function beginRename(): Promise<void> {
  if (!job.value) return
  renameValue.value = job.value.title
  renaming.value = true
  await nextTick()
  renameInput.value?.focus()
  renameInput.value?.select()
}

function cancelRename(): void {
  renaming.value = false
  renameValue.value = ''
}

async function commitRename(): Promise<void> {
  if (!job.value) return
  const title = renameValue.value.trim()
  if (!title) {
    cancelRename()
    return
  }
  try {
    const updated = await api.productionRename(job.value.id, title)
    upsertJob(updated)
    if (detail.value) detail.value = { ...detail.value, job: updated }
    renaming.value = false
    renameValue.value = ''
  } catch (cause) {
    error.value = api.errorText(cause)
  }
}

function requestDelete(): void {
  confirmDelete.value = true
}

function close(): void {
  emit('close')
}

async function cancel(): Promise<void> {
  if (!job.value) return
  try {
    await api.productionCancel(job.value.id)
    await loadDetail()
  } catch (cause) {
    error.value = api.errorText(cause)
  }
}

async function loadResumeInfo(): Promise<void> {
  if (!job.value) return
  try {
    resumeInfo.value = await api.productionResumeInfo(job.value.id)
  } catch {
    resumeInfo.value = null
  } finally {
    resumeInfoLoading.value = false
  }
}

async function resume(): Promise<void> {
  if (!job.value) return
  resumeLoading.value = true
  error.value = ''
  try {
    const next = await resumeProduction(job.value.id)
    upsertJob(next)
    resumeInfo.value = null
    confirmRerun.value = false
    await loadDetail()
  } catch (cause) {
    error.value = api.errorText(cause)
  } finally {
    resumeLoading.value = false
  }
}

function requestRerun(): void {
  confirmRerun.value = true
}

async function rerunAll(): Promise<void> {
  if (!job.value) return
  try {
    const next = await retryProduction(job.value.id)
    upsertJob(next)
    resumeInfo.value = null
    confirmRerun.value = false
    await loadDetail()
  } catch (cause) {
    error.value = api.errorText(cause)
    confirmRerun.value = false
  }
}

function checkpointForStep(key: string) {
  return resumeInfo.value?.steps.find((step) => step.key === key)
}

async function remove(): Promise<void> {
  if (!job.value) return
  try {
    await deleteProduction(job.value.id)
    close()
  } catch (cause) {
    error.value = api.errorText(cause)
    confirmDelete.value = false
  }
}

function exportMarkdown(): void {
  if (!result.value) return
  const blob = new Blob([result.value.body], { type: 'text/markdown;charset=utf-8' })
  const link = document.createElement('a')
  link.href = URL.createObjectURL(blob)
  link.download = `${noteTitle.value || '笔记'}.md`
  link.click()
  window.setTimeout(() => URL.revokeObjectURL(link.href), 1000)
}

function openNote(): void {
  if (!job.value?.noteId) return
  void router.push(`/note/${job.value.noteId}`)
  close()
}

function openRunPage(): void {
  if (!job.value) return
  void router.push(`/studio/run/${job.value.id}`)
}

function selectTab(next: string): void {
  if (next === 'flow') {
    openRunPage()
    return
  }
  activeTab.value = next
}

function chooseQuiz(item: ProductionDrillItem, index: number): void {
  quizChoices.value = { ...quizChoices.value, [item.id]: index }
}

function optionClass(item: ProductionDrillItem, index: number): string {
  if (quizChoices.value[item.id] === undefined) return ''
  if (quizChoices.value[item.id] !== index) return ''
  return String(item.answer).includes(item.options[index] ?? '') ? 'is-correct' : 'is-wrong'
}

function stepStatusLabel(status: string): string {
  return ({ pending: '等待', running: '进行中', done: '完成', error: '失败', cancelled: '已取消', skipped: '跳过' } as Record<string, string>)[status] ?? status
}

watch(() => props.jobId, () => {
  activeTab.value = 'result'
  quizChoices.value = {}
  void loadDetail()
}, { immediate: true })

watch(() => job.value?.updatedAt, () => {
  if (job.value?.status === 'running' || job.value?.status === 'queued') void loadDetail()
})

watch(tabs, (items) => {
  if (!items.some((item) => item.id === activeTab.value)) activeTab.value = 'result'
})

onMounted(() => {
  if (!production.templates.length) void refreshJobs()
  void loadDetail()
  syncPolling()
  void loadVersions()
  void nextTick().then(() => { if (railRef.value) railRef.value.scrollTop = 0 })
})

watch(currentTab, (tab) => {
  if (tab === 'versions') void loadVersions()
  if (tab === 'flow') void loadRunGraph()
  if (tab === 'trace') void loadTraceFacts()
})

watch(
  () => props.jobId,
  () => {
    confirmDelete.value = false
    renaming.value = false
    renameValue.value = ''
    activeTab.value = 'result'
    quizChoices.value = {}
    versions.value = []
    selectedVersionId.value = ''
    versionBody.value = ''
    confirmRestoreId.value = ''
    restoreNotice.value = ''
    traceFacts.value = []
    traceFactsError.value = ''
    void nextTick().then(() => { if (railRef.value) railRef.value.scrollTop = 0 })
  },
)

watch(
  () => job.value?.status,
  (next, previous) => {
    if (next && next !== previous) {
      void loadDetail(true)
      if (runGraph.value || currentTab.value === 'flow') void loadRunGraph(true)
    }
  },
)

watch(
  [currentTab, () => detail.value?.logs.length],
  async ([tab]) => {
    if (tab !== 'steps') return
    await nextTick()
    if (logRef.value) logRef.value.scrollTop = logRef.value.scrollHeight
  },
  { flush: 'post' },
)

onBeforeUnmount(() => window.clearInterval(pollTimer))
</script>

<template>
  <div v-if="jobId" class="prod-overlay" @click.self="close">
    <div v-if="job" class="prod-dialog prod-task-dialog" role="dialog" aria-modal="true" aria-labelledby="prodTaskTitle">
      <div class="prod-task-top">
        <div class="prod-task-heading">
          <div class="prod-task-title-row">
            <template v-if="renaming">
              <input ref="renameInput" v-model="renameValue" class="prod-rename-input" maxlength="120" @keyup.enter="commitRename" @keyup.esc="cancelRename" />
              <button type="button" class="prod-icon-btn" aria-label="保存标题" @click="commitRename"><Check :size="14" /></button>
              <button type="button" class="prod-icon-btn" aria-label="取消重命名" @click="cancelRename"><X :size="14" /></button>
            </template>
            <h2 v-else id="prodTaskTitle">{{ job.title }}</h2>
            <span v-if="job.options.auto" class="prod-auto-badge">UP 自动</span>
            <span class="prod-task-status" :class="statusClass(job.status)">
              <i class="prod-dot" :class="statusClass(job.status) === 'is-success' ? 'is-green' : statusClass(job.status) === 'is-error' ? 'is-red' : statusClass(job.status) === 'is-gold' ? 'is-gold' : 'is-blue'" />
              {{ jobStatusLabel(job) }}
            </span>
          </div>
          <div class="prod-task-meta">
            <span>{{ template?.name ?? job.templateId }}</span>
            <span class="prod-meta-sep">·</span>
            <span>{{ sourceSummary(job.source) }}</span>
            <span class="prod-meta-sep">·</span>
            <span>{{ formatDate(job.updatedAt) }}</span>
          </div>
        </div>
        <div class="prod-task-actions">
          <button v-if="job.status === 'running' || job.status === 'queued'" type="button" class="prod-btn is-small" @click="cancel">
            <Square :size="12" /> 取消
          </button>
          <template v-if="['error', 'cancelled', 'interrupted'].includes(job.status)">
            <button v-if="resumeInfoLoading" type="button" class="prod-btn is-small" disabled>检查可继续…</button>
            <template v-else-if="resumeInfo?.canResume">
              <button type="button" class="prod-btn is-small is-primary" :disabled="resumeLoading" @click="resume">
                <Play :size="12" /> {{ resumeLoading ? '启动中' : '继续制作' }}
              </button>
              <button type="button" class="prod-btn is-small" :disabled="resumeLoading" @click="requestRerun">
                <RotateCcw :size="12" /> 重跑全部
              </button>
            </template>
            <button v-else type="button" class="prod-btn is-small is-primary" @click="requestRerun">
              <RotateCcw :size="12" /> 重试
            </button>
          </template>
          <button v-if="job.status === 'success'" type="button" class="prod-btn is-small" @click="requestRerun">
            <RotateCcw :size="12" /> 重新制作
          </button>
          <button v-if="result" type="button" class="prod-icon-btn" title="导出 Markdown" aria-label="导出 Markdown" @click="exportMarkdown">
            <Download :size="14" />
          </button>
          <button v-if="!renaming" type="button" class="prod-icon-btn" title="重命名" aria-label="重命名" @click="beginRename">
            <Pencil :size="14" />
          </button>
          <button v-if="job.status !== 'running' && job.status !== 'queued'" type="button" class="prod-icon-btn" title="删除任务" aria-label="删除任务" @click="requestDelete">
            <Trash2 :size="14" />
          </button>
          <button type="button" class="prod-icon-btn" aria-label="关闭" @click="close"><X :size="15" /></button>
        </div>
      </div>

      <div v-if="confirmDelete" class="prod-delete-confirm">
        <div><b>删除这条任务记录？</b><span>会删除任务、日志和续跑检查点；已经入库的笔记文件不会被删除。</span></div>
        <button type="button" class="prod-btn is-small is-danger" @click="remove">确认删除</button>
        <button type="button" class="prod-btn is-small" @click="confirmDelete = false">取消</button>
      </div>
      <div v-if="confirmRerun" class="prod-delete-confirm">
        <div>
          <b>{{ job.status === 'success' ? '重新制作这篇笔记？' : '重跑全部？' }}</b>
          <span v-if="job.status === 'success'">会清空中间结果并重新调用模型；同一 noteId 会更新当前笔记，旧正文保留为历史版本。已有标注可能失效。</span>
          <span v-else>会清空这次任务已保存的 {{ resumeInfo?.checkpointCount ?? 0 }} 个中间结果，重新调用模型；不会删除已入库笔记，但可能生成新的版本。</span>
        </div>
        <button type="button" class="prod-btn is-small is-danger" @click="rerunAll">{{ job.status === 'success' ? '确认重新制作' : '确认重跑' }}</button>
        <button type="button" class="prod-btn is-small" @click="confirmRerun = false">取消</button>
      </div>

      <div class="prod-task-tabs" role="tablist">
        <button v-for="tab in tabs" :key="tab.id" type="button" role="tab" :aria-selected="currentTab === tab.id" :class="{ 'is-active': currentTab === tab.id }" @click="selectTab(tab.id)">{{ tab.label }}</button>
      </div>

      <div class="prod-task-layout" :class="{ 'is-versions': currentTab === 'versions' }">
        <div class="prod-task-main">
          <div v-if="error" class="prod-error-box" style="margin-bottom: 10px">
            <h3>操作失败</h3>
            <p>{{ error }}</p>
          </div>

          <section v-if="currentTab === 'result'" class="prod-task-panel is-active">
            <div v-if="job.status === 'running' || job.status === 'queued'" class="prod-running-box">
              <div class="prod-running-head">
                <h3>{{ runningLabel }}</h3>
                <span class="prod-running-count">{{ job.steps.filter((step) => step.status === 'done').length }} / {{ job.steps.length }} 步</span>
              </div>
              <div class="prod-running-bar"><i :style="{ width: `${progressOf(job)}%` }" /></div>
              <p style="margin-top: 9px">取消后已完成的步骤不会写进笔记；每个步骤的耗时和产物在「步骤」里。</p>
            </div>
            <div v-else-if="job.status === 'error'" class="prod-error-box">
              <h3>制作失败</h3>
              <p :title="job.error">{{ displayError(job.error) || '某个步骤失败。' }}</p>
              <p v-if="job.auto && job.nextRetryAt" style="margin-top: 5px">已自动安排重试：第 {{ job.retryCount }} 次失败，预计 {{ formatDate(job.nextRetryAt) }}。不需要手动点「重试」。</p>
              <p v-else-if="job.auto && job.retryCount" style="margin-top: 5px">已自动重试 {{ job.retryCount }} 次；依赖恢复后会自动继续。</p>
              <p v-if="resumeInfo" style="margin-top: 5px">{{ resumeInfo.canResume ? `已保存 ${resumeInfo.checkpointCount} 个中间结果，继续会复用已完成的调用。` : resumeInfo.reason }}</p>
              <p v-if="resumeInfo?.configChanged" style="margin-top: 4px">任务创建后模型/接口/默认提示词变过；继续使用创建时配置，重跑全部使用当前配置。</p>
              <p v-if="resumeInfo?.published" style="margin-top: 4px">笔记已经入库；继续只会补完成状态，不会重复发布。</p>
              <p v-if="resumeInfo?.canResume" style="margin-top: 4px">「继续制作」只补没完成的部分；「重跑全部」会清空检查点并从零开始。</p>
            </div>
            <div v-else-if="job.status === 'cancelled' || job.status === 'interrupted'" class="prod-error-box">
              <h3>{{ job.status === 'cancelled' ? '已取消' : '任务被中断' }}</h3>
              <p>{{ job.status === 'cancelled' ? '你取消了这次制作。' : '应用在运行中退出；任务已标记中断。' }}</p>
              <p v-if="resumeInfo" style="margin-top: 5px">{{ resumeInfo.canResume ? `已保存 ${resumeInfo.checkpointCount} 个中间结果，可以继续。` : resumeInfo.reason }}</p>
            </div>
            <div v-else-if="result" class="prod-result-doc">
              <div class="prod-result-kicker">{{ job.templateId === 'note-trace' ? '事实已更新' : '已保存' }}</div>
              <h3 v-if="noteTitleDiffers">{{ noteTitle }}</h3>
              <div class="prod-markdown" v-html="renderMarkdown(bodyWithoutTitle)" />
            </div>
            <div v-if="detail?.logs.length && (job.status === 'running' || job.status === 'queued' || (result?.body ?? '').trim().length < 200)" class="prod-result-log">
              <div class="prod-result-log-head">
                <span>最近日志</span>
                <button type="button" @click="activeTab = 'steps'">查看全部 →</button>
              </div>
              <div class="prod-result-log-list">
                <div v-for="log in detail.logs.slice(-8)" :key="log.id" class="prod-log-row" :class="`is-${log.level}`">
                  <span class="prod-log-time">{{ formatDate(log.createdAt).slice(6) }}</span>
                  <span class="prod-log-level">{{ log.level }}</span>
                  <span class="prod-log-message">{{ log.message }}</span>
                </div>
              </div>
            </div>
          </section>

          <section v-else-if="currentTab === 'steps'" class="prod-task-panel is-active is-steps">
            <div class="prod-step-progress" :class="statusClass(job.status)">
              <div class="prod-step-progress-copy">
                <b>{{ job.status === 'queued' ? '排队中' : job.status === 'running' ? runningLabel : jobStatusLabel(job) }}</b>
                <small>
                  {{ job.steps.filter((step) => step.status === 'done').length }} / {{ job.steps.length }} 步
                  <template v-if="job.status === 'running' || job.status === 'queued'"> · {{ job.progress }}%</template>
                </small>
              </div>
              <div v-if="job.status === 'running' || job.status === 'queued'" class="prod-step-progress-track"><i :style="{ width: `${job.progress}%` }" /></div>
            </div>

            <div class="prod-step-section">
              <div class="prod-step-section-head">
                <span>执行步骤</span>
              </div>
              <div class="prod-step-list">
                <div v-for="step in job.steps" :key="step.index" class="prod-step-row" :class="`is-${step.status}`">
                  <span class="prod-step-mark">
                    <Check v-if="step.status === 'done'" :size="10" />
                    <CircleAlert v-else-if="step.status === 'error'" :size="10" />
                    <span v-else>{{ step.index + 1 }}</span>
                  </span>
                  <span><b>{{ stepLabel(step.key, step.label) }}</b><small>{{ step.detail || stepStatusLabel(step.status) }}<span v-if="checkpointForStep(step.key)"> · 已保存 {{ checkpointForStep(step.key)?.units }} 段</span></small></span>
                  <span class="prod-step-time">{{ stepStatusLabel(step.status) }}</span>
                </div>
              </div>
            </div>

            <div v-if="detail?.logs.length" class="prod-log-section">
              <div class="prod-log-head">
                <span>运行日志</span>
                <small>{{ detail.logs.length }} 条 · 最近 {{ formatDate(detail.logs[detail.logs.length - 1].createdAt).slice(6) }}</small>
              </div>
              <div ref="logRef" class="prod-log">
                <div v-for="log in detail.logs" :key="log.id" class="prod-log-row" :class="`is-${log.level}`">
                  <span class="prod-log-time">{{ formatDate(log.createdAt).slice(6) }}</span>
                  <span class="prod-log-level">{{ log.level }}</span>
                  <span class="prod-log-message">{{ log.message }}</span>
                </div>
              </div>
            </div>
            <p v-else class="prod-wizard-hint" style="margin-top: 10px">还没有日志；运行开始后逐步出现。</p>
          </section>

          <section v-else-if="currentTab === 'flow'" class="prod-task-panel is-active is-flow">
            <div class="prod-flow-head">
              <span>运行流程</span>
              <small v-if="job.status === 'running'" class="prod-flow-live"><i />进行中 · {{ job.progress }}% · {{ flowNodes.length }} 节点</small>
              <small v-else-if="runGraph">{{ runGraph.nodes.length }} 个节点 · {{ runGraph.artifacts.length }} 个产物</small>
              <small v-else>还没有节点记录</small>
            </div>

            <WorkflowCanvas
              v-if="flowNodes.length"
              :nodes="flowNodes"
              :selected="selectedNode"
              @select="selectedNode = $event"
            />

            <section v-if="selectedFlowNode" class="prod-node-detail">
              <div class="prod-node-detail-head">
                <div class="prod-node-detail-title">
                  <span class="prod-node-detail-kicker">节点</span>
                  <b>{{ selectedFlowNode.label }}</b>
                  <span class="prod-node-detail-status" :class="`is-${selectedFlowNode.status}`">{{ stepStatusLabel(selectedFlowNode.status) }}</span>
                  <span v-if="job.status === 'running' && selectedFlowNode.status === 'running'" class="prod-node-live"><i />实时</span>
                </div>
                <small class="prod-node-detail-meta">
                  {{ nodeDuration(selectedFlowNode) }}<template v-if="selectedFlowNode.tokens"> · {{ formatTokens(selectedFlowNode.tokens) }}</template><template v-if="selectedFlowNode.cost"> · ¥{{ selectedFlowNode.cost.toFixed(3) }}</template>
                </small>
              </div>
              <p v-if="selectedFlowNode.summary" class="prod-node-detail-summary">{{ selectedFlowNode.summary }}</p>
              <p v-if="selectedFlowNode.error" class="prod-flow-error">{{ selectedFlowNode.error }}</p>
              <div v-if="selectedFlowNode.artifacts.length" class="prod-flow-artifacts">
                <button v-for="artifact in selectedFlowNode.artifacts" :key="artifact.id" type="button" @click="openArtifact(artifact)">
                  <FileText :size="11" />{{ artifact.name }}<span>{{ formatBytes(artifact.bytes) }}</span>
                </button>
              </div>
              <div class="prod-node-log-head">
                <span>节点日志</span>
                <small>{{ selectedLogs.length }} 条<template v-if="job.status === 'running' && selectedFlowNode.status === 'running'"> · 进行中</template></small>
              </div>
              <div ref="nodeLogRef" class="prod-node-log" @scroll="onNodeLogScroll">
                <div v-for="log in selectedLogs.slice(-60)" :key="log.id" class="prod-node-log-row" :class="`is-${log.level}`">
                  <span class="prod-node-log-time">{{ formatDate(log.createdAt).slice(6) }}</span>
                  <span class="prod-node-log-message">{{ log.message }}</span>
                </div>
                <p v-if="!selectedLogs.length" class="prod-wizard-hint">这个节点还没有日志；开始后会实时出现。</p>
              </div>
            </section>

            <ol v-if="flowNodes.length" class="prod-flow-list">
              <li
                v-for="node in flowNodes"
                :key="node.key"
                class="prod-flow-row"
                :class="[`is-${node.status}`, { 'is-selected': selectedNode === node.key }]"
                @click="selectedNode = node.key"
              >
                <span class="prod-flow-dot" />
                <div class="prod-flow-body">
                  <div class="prod-flow-line">
                    <b>{{ node.label }}</b>
                    <span class="prod-flow-meta">{{ node.status === 'queued' ? '等待' : node.tokens || node.cost ? `${nodeDuration(node)} · ${formatTokens(node.tokens)} · ¥${node.cost.toFixed(3)}` : `${nodeDuration(node)} · 无模型调用` }}</span>
                  </div>
                  <p v-if="node.summary" class="prod-flow-summary">{{ node.summary }}</p>
                  <p v-if="node.error" class="prod-flow-error">{{ node.error }}</p>
                  <div v-if="artifactsFor(node.key).length" class="prod-flow-artifacts">
                    <button v-for="artifact in artifactsFor(node.key)" :key="artifact.id" type="button" @click="openArtifact(artifact)">
                      <FileText :size="11" />{{ artifact.name }}<span>{{ formatBytes(artifact.bytes) }}</span>
                    </button>
                  </div>
                </div>
              </li>
            </ol>
            <p v-else class="prod-wizard-hint">任务开始后，每个步骤会在这里留下耗时、用量和产物。成功运行也会保留。</p>

            <div v-if="artifactPreview" class="prod-flow-preview">
              <div class="prod-flow-preview-head">
                <span>{{ artifactPreview.name }}</span>
                <button type="button" @click="artifactPreview = null"><X :size="13" /></button>
              </div>
              <pre>{{ artifactPreview.text }}</pre>
            </div>
          </section>

          <section v-else-if="currentTab === 'compare'" class="prod-task-panel is-active">
            <div class="prod-compare-grid">
              <div v-for="variant in result?.variants" :key="variant.name" class="prod-compare-col">
                <h4>{{ variant.name }} · {{ variant.summary }}</h4>
                <div class="prod-markdown" v-html="renderMarkdown(variant.body)" />
              </div>
            </div>
            <p class="prod-wizard-hint" style="margin-top: 8px">两版都来自同一份素材；入库的是第一版，第二版附在正文末尾。</p>
          </section>

          <section v-else-if="currentTab === 'mindmap'" class="prod-task-panel is-active">
            <div class="prod-mindmap">{{ result?.mindmap }}</div>
          </section>

          <section v-else-if="currentTab === 'drill'" class="prod-task-panel is-active">
            <div v-for="item in result?.drill" :key="item.id" class="prod-drill-card" style="margin-bottom: 9px">
              <h3>{{ item.question }}</h3>
              <button v-for="(option, index) in item.options" :key="option" type="button" class="prod-quiz-option" :class="optionClass(item, index)" @click="chooseQuiz(item, index)">
                {{ option }}
              </button>
              <p v-if="quizChoices[item.id] !== undefined" class="prod-wizard-hint" style="margin-top: 7px">参考答案：{{ item.answer }}<span v-if="item.note"> · 依据：{{ item.note }}</span></p>
            </div>
          </section>

          <section v-else-if="currentTab === 'trace'" class="prod-task-panel is-active">
            <div class="prod-trace-facts-head">
              <span>事实库当前状态</span>
              <button v-if="job.noteId" type="button" class="prod-trace-source" @click="router.push(`/note/${job.noteId}`)">在笔记里查看 →</button>
            </div>
            <div v-if="traceFactsLoading" class="prod-wizard-hint">正在读取事实库…</div>
            <div v-else-if="traceFactsError" class="prod-error-box"><p>{{ traceFactsError }}</p></div>
            <template v-else>
              <div v-for="fact in traceFacts" :key="fact.id" class="prod-trace-row">
                <div class="prod-trace-copy">
                  <b>{{ fact.text }}</b>
                  <span v-for="evidence in fact.evidence.slice(0, 2)" :key="evidence.url + evidence.quote" class="prod-trace-quote">{{ evidence.domain || '来源' }} · {{ evidence.quote }}</span>
                  <button v-for="evidence in fact.evidence.slice(0, 2)" :key="'open-' + evidence.url" type="button" class="prod-trace-source" @click="api.openExternalUrl(evidence.url)">查看来源 ↗</button>
                </div>
                <span class="prod-trace-verdict" :class="factStatusClass(fact.status)">{{ factStatusLabel(fact.status) }}</span>
              </div>
              <p v-if="!traceFacts.length" class="prod-wizard-hint">这次运行没有写入事实库；旧任务的溯源快照可能只存在于任务结果里。</p>
            </template>
          </section>

          <section v-else-if="currentTab === 'versions'" class="prod-task-panel is-active is-versions">
            <div class="prod-section-line">
              <span class="prod-section-title">历史版本</span>
              <span class="prod-section-hint">生成稿与覆盖前的手改备份</span>
            </div>
            <div v-if="versionError" class="prod-error-box" style="margin-bottom: 10px"><p>{{ versionError }}</p></div>
            <div v-if="versionLoading" class="prod-wizard-hint">正在读取版本…</div>
            <div v-else-if="!versions.length" class="prod-wizard-hint">还没有可恢复的历史版本。重跑或覆盖后会自动保留。</div>
            <div v-else class="prod-version-layout">
              <div class="prod-version-list">
                <button
                  v-for="version in versions"
                  :key="version.id"
                  type="button"
                  class="prod-version-row"
                  :class="{ 'is-selected': version.id === selectedVersionId }"
                  @click="viewVersion(version)"
                >
                  <b>{{ version.label }}</b>
                  <span>{{ version.kind === 'manual' ? '手改备份' : '生成稿' }} · {{ formatDate(version.createdAt) }}</span>
                  <small>{{ version.chars }} 字 · {{ formatBytes(version.bytes) }}</small>
                </button>
              </div>
              <div class="prod-version-detail">
                <div v-if="selectedVersion" class="prod-version-detail-head">
                  <div><b>{{ selectedVersion.label }}</b><span>{{ selectedVersion.kind === 'manual' ? '覆盖前的手改稿' : '历史生成稿' }} · {{ formatDate(selectedVersion.createdAt) }}</span></div>
                  <div class="prod-version-actions">
                    <span class="prod-version-toggle">
                      <button type="button" :class="{ 'is-active': versionView === 'preview' }" @click="versionView = 'preview'">预览</button>
                      <button type="button" :class="{ 'is-active': versionView === 'source' }" @click="versionView = 'source'">源码</button>
                    </span>
                    <button type="button" class="prod-btn is-small" @click="requestRestore(selectedVersion)"><History :size="12" />恢复此版本</button>
                  </div>
                </div>
                <div v-if="versionBody && versionView === 'preview'" class="prod-version-rich prod-markdown" v-html="renderMarkdown(versionPreviewBody(versionBody))" />
                <pre v-else-if="versionBody" class="prod-version-preview">{{ versionBody }}</pre>
                <div v-else-if="versionBodyLoading" class="prod-wizard-hint">正在读取内容…</div>
                <div v-else class="prod-wizard-hint">选择一个版本查看内容。</div>
              </div>
            </div>
            <div v-if="confirmRestoreId" class="prod-delete-confirm" style="margin: 10px 0 0">
              <div><b>恢复这个版本？</b><span>当前笔记会被替换；替换前会自动保存为手改备份。标注可能失效。</span></div>
              <button type="button" class="prod-btn is-small is-danger" @click="restoreVersion">确认恢复</button>
              <button type="button" class="prod-btn is-small" @click="confirmRestoreId = ''">取消</button>
            </div>
            <p v-if="restoreNotice" class="prod-wizard-hint" style="margin-top: 8px">{{ restoreNotice }}</p>
            <p class="prod-wizard-hint" style="margin-top: 8px">恢复只改当前笔记正文；其他版本继续保留。恢复旧版可能让部分标注锚点失效。</p>
          </section>

          <section v-else class="prod-task-panel is-active">
            <div class="prod-usage-table">
              <div class="prod-usage-row"><span>输入 tokens</span><b>{{ formatTokens(job.usage.promptTokens) }}</b><span>未缓存 {{ formatTokens(job.usage.uncachedInputTokens) }}</span></div>
              <div v-if="job.usage.cachedTokens" class="prod-usage-row"><span>缓存读 tokens</span><b>{{ formatTokens(job.usage.cachedTokens) }}</b><span>接口返回</span></div>
              <div v-if="job.usage.cacheWriteTokens" class="prod-usage-row"><span>缓存写 tokens</span><b>{{ formatTokens(job.usage.cacheWriteTokens) }}</b><span>接口返回</span></div>
              <div class="prod-usage-row"><span>输出 tokens</span><b>{{ formatTokens(job.usage.completionTokens) }}</b><span>接口返回</span></div>
              <div v-if="job.usage.reasoningTokens" class="prod-usage-row"><span>推理 tokens</span><b>{{ formatTokens(job.usage.reasoningTokens) }}</b><span>仅记录，不重复计费</span></div>
              <div class="prod-usage-row"><span>音频</span><b>{{ formatDuration(job.usage.audioSeconds) }}</b><span>ASR</span></div>
              <div class="prod-usage-row"><span>估算金额</span><b>{{ job.usage.estimatedCost ? `¥${job.usage.estimatedCost.toFixed(2)}` : '—' }}</b><span>{{ job.usage.priced ? '目录价估算' : job.usage.estimatedCost ? 'ASR 手动价' : '未收录价格' }}</span></div>
              <div v-if="job.usage.providerCost > 0" class="prod-usage-row"><span>服务商账单</span><b>{{ job.usage.providerCurrency }} {{ job.usage.providerCost.toFixed(4) }}</b><span>接口实报</span></div>
            </div>
            <p class="prod-wizard-hint" style="margin-top: 8px">token 是接口返回值；金额 = token × 参考价，属于估算。服务商实报金额单独显示，不混入估算。</p>
          </section>
        </div>

        <aside ref="railRef" class="prod-task-rail">
          <div class="prod-rail-head"><span>任务信息</span></div>

          <section class="prod-source-block">
            <div class="prod-info-label">{{ railSourceLabel }}</div>
            <div v-if="job.source.videos[0]?.cover || railSource" class="prod-source-main">
              <img v-if="job.source.videos[0]?.cover" class="prod-source-cover" :src="mediaUrl(job.source.videos[0].cover)" alt="" referrerpolicy="no-referrer" loading="lazy" />
              <span v-else class="prod-source-icon"><FileText :size="17" /></span>
              <div v-if="railSource" class="prod-source-copy">
                <b>{{ railSource }}</b>
              </div>
            </div>
            <p v-if="sourceExcerpt" class="prod-source-excerpt">{{ sourceExcerpt }}</p>
          </section>

          <section class="prod-rail-section">
            <div class="prod-info-label">运行数据</div>
            <dl class="prod-meta-list">
              <div><dt>开始</dt><dd>{{ job.startedAt ? formatDate(job.startedAt) : '—' }}</dd></div>
              <div v-if="job.startedAt"><dt>耗时</dt><dd>{{ formatDuration(elapsedSeconds) }}</dd></div>
              <div><dt>tokens</dt><dd>{{ formatTokens(tokens) }}</dd></div>
            </dl>
          </section>

          <section v-if="job.noteId" class="prod-rail-section">
            <div class="prod-note-card-head">
              <span class="prod-info-label">{{ job.templateId === 'note-trace' ? '关联笔记' : '笔记文件' }}</span>
              <button type="button" class="prod-rail-link" @click="openNote">打开 <ExternalLink :size="11" /></button>
            </div>
            <div class="prod-note-file">
              <FileText :size="15" />
              <div>
                <b>{{ noteFile || (job.templateId === 'note-trace' ? '已关联笔记' : '笔记路径未知') }}</b>
                <span v-if="noteFile">{{ noteDir ? `位于 ${noteDir}/` : '位于笔记库根目录' }}</span>
              </div>
            </div>
          </section>

          <section v-if="job.options.tags.length" class="prod-rail-section">
            <div class="prod-info-label">标签</div>
            <div class="prod-folder-row"><span v-for="tag in job.options.tags" :key="tag" class="prod-folder-chip">{{ tag }}</span></div>
          </section>
          <section v-if="job.options.fields.length" class="prod-rail-section">
            <div class="prod-info-label">属性</div>
            <div class="prod-folder-row"><span v-for="field in job.options.fields" :key="field" class="prod-folder-chip">{{ field }}</span></div>
          </section>
          <div class="prod-rail-foot"><span>ID</span><code :title="job.id">{{ shortJobId }}</code></div>
        </aside>
      </div>
    </div>
  </div>
</template>

<style scoped>
.prod-error { margin: 7px 0 0; color: var(--red); font-family: var(--sans); font-size: 12px; line-height: 1.5; }

.prod-flow-head {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: 10px;
  font-family: var(--sans);
  color: var(--ink-3);
  font-size: 12px;
  padding: 2px 0 10px;
  border-bottom: 1px solid var(--line);
}

.prod-flow-live {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  color: var(--blue);
}

.prod-flow-live i,
.prod-node-live i {
  width: 5px;
  height: 5px;
  border-radius: 50%;
  background: var(--blue);
  animation: prod-live-pulse 1.2s ease-in-out infinite;
}

.prod-node-detail {
  margin-top: 12px;
  padding: 10px 12px;
  border: 1px solid var(--line);
  border-radius: 5px;
  background: var(--paper);
}

.prod-node-detail-head {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: 10px;
}

.prod-node-detail-title {
  display: flex;
  align-items: center;
  gap: 7px;
  min-width: 0;
  font-family: var(--sans);
}

.prod-node-detail-title b {
  font-size: 13px;
  font-weight: 600;
  color: var(--ink);
}

.prod-node-detail-kicker {
  font-size: 10.5px;
  letter-spacing: 0.08em;
  color: var(--ink-3);
}

.prod-node-detail-status {
  flex: none;
  padding: 1px 6px;
  border: 1px solid var(--line);
  border-radius: 3px;
  font-size: 10.5px;
  color: var(--ink-3);
}

.prod-node-detail-status.is-running {
  color: var(--blue);
  border-color: var(--blue-line);
  background: var(--blue-soft);
}

.prod-node-detail-status.is-done {
  color: var(--green);
  border-color: color-mix(in srgb, var(--green) 40%, var(--line));
}

.prod-node-detail-status.is-error {
  color: var(--red);
  border-color: color-mix(in srgb, var(--red) 40%, var(--line));
}

.prod-node-live {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  font-size: 10.5px;
  color: var(--blue);
}

.prod-node-detail-meta {
  flex: none;
  font-family: var(--mono);
  font-size: 10.5px;
  color: var(--ink-3);
  white-space: nowrap;
}

.prod-node-detail-summary {
  margin: 6px 0 0;
  font-family: var(--sans);
  font-size: 12px;
  line-height: 1.55;
  color: var(--ink-2);
}

.prod-node-log-head {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: 10px;
  margin-top: 9px;
  padding-top: 8px;
  border-top: 1px solid var(--line);
  font-family: var(--sans);
  font-size: 11px;
  color: var(--ink-3);
}

.prod-node-log {
  max-height: 180px;
  margin-top: 4px;
  overflow: auto;
  font-family: var(--mono);
  font-size: 11px;
  line-height: 1.55;
}

.prod-node-log-row {
  display: grid;
  grid-template-columns: 58px minmax(0, 1fr);
  gap: 8px;
  padding: 2px 0;
  border-bottom: 1px solid color-mix(in srgb, var(--line) 55%, transparent);
}

.prod-node-log-time {
  color: var(--ink-3);
}

.prod-node-log-message {
  color: var(--ink-2);
  white-space: pre-wrap;
  word-break: break-word;
}

.prod-node-log-row.is-error .prod-node-log-message {
  color: var(--red);
}

@keyframes prod-live-pulse {
  0%, 100% { box-shadow: 0 0 0 0 color-mix(in srgb, var(--blue) 40%, transparent); }
  50% { box-shadow: 0 0 0 4px color-mix(in srgb, var(--blue) 0%, transparent); }
}

@media (prefers-reduced-motion: reduce) {
  .prod-flow-live i,
  .prod-node-live i {
    animation: none;
  }
}

.prod-flow-strip {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 6px;
  padding: 12px 0 14px;
  border-bottom: 1px solid var(--line);
}

.prod-flow-agent {
  display: flex;
  align-items: flex-start;
  gap: 8px;
  margin: 12px 0 0;
  padding: 9px 11px;
  border: 1px solid var(--blue-line);
  border-radius: 4px;
  background: var(--blue-soft);
}

.prod-flow-agent-dot {
  width: 6px;
  height: 6px;
  margin-top: 5px;
  border-radius: 50%;
  background: var(--blue);
  flex: none;
}

.prod-flow-agent-copy {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
  font-family: var(--sans);
  font-size: 11.5px;
  line-height: 1.5;
  color: var(--ink-2);
}

.prod-flow-agent-copy b { font-size: 12px; color: var(--ink); }
.prod-flow-agent.is-failed { border-color: var(--red); background: transparent; }
.prod-flow-agent.is-failed .prod-flow-agent-dot { background: var(--red); }

.prod-flow-chip {
  border: 1px solid var(--line);
  border-radius: 3px;
  padding: 3px 8px;
  font-family: var(--sans);
  font-size: 11.5px;
  color: var(--ink-2);
  background: var(--paper);
}

.prod-flow-chip.is-running { border-color: var(--blue-line); color: var(--blue); background: var(--blue-soft); }
.prod-flow-chip.is-done { border-color: var(--line-2); color: var(--ink); }
.prod-flow-chip.is-error { border-color: var(--red); color: var(--red); }

.prod-flow-arrow { color: var(--ink-3); font-size: 11px; }

.prod-flow-list {
  list-style: none;
  margin: 0;
  padding: 8px 0 0;
}

.prod-flow-row {
  display: flex;
  gap: 10px;
  padding: 10px 0 10px 2px;
  border-bottom: 1px solid var(--line);
}

.prod-flow-row.is-selected {
  background: var(--blue-soft);
}

.prod-flow-row:last-child {
  border-bottom: 0;
}

.prod-flow-dot {
  flex: none;
  width: 7px;
  height: 7px;
  margin-top: 7px;
  border-radius: 50%;
  background: var(--line-2);
}

.prod-flow-row.is-running .prod-flow-dot { background: var(--blue); box-shadow: 0 0 0 3px var(--blue-soft); }
.prod-flow-row.is-done .prod-flow-dot { background: var(--green); }
.prod-flow-row.is-error .prod-flow-dot { background: var(--red); }

.prod-flow-body {
  min-width: 0;
  flex: 1;
}

.prod-flow-line {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: 10px;
}

.prod-flow-line b {
  font-family: var(--serif);
  font-size: 14px;
  font-weight: 600;
  color: var(--ink);
}

.prod-flow-meta {
  flex: none;
  font-family: var(--mono);
  font-size: 11.5px;
  color: var(--ink-3);
}

.prod-flow-summary,
.prod-flow-error {
  margin: 3px 0 0;
  font-size: 12.5px;
  line-height: 1.55;
  color: var(--ink-2);
}

.prod-flow-error { color: var(--red); }

.prod-flow-artifacts {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  margin-top: 7px;
}

.prod-flow-artifacts button {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  border: 1px solid var(--line);
  background: var(--paper);
  color: var(--ink-2);
  border-radius: 3px;
  padding: 3px 7px;
  font-family: var(--sans);
  font-size: 11.5px;
}

.prod-flow-artifacts button span {
  color: var(--ink-3);
  font-family: var(--mono);
  font-size: 10.5px;
}

.prod-flow-artifacts button:hover {
  border-color: var(--blue-line);
  color: var(--blue);
}

.prod-flow-preview {
  margin-top: 14px;
  border: 1px solid var(--line);
  border-radius: 4px;
  background: var(--paper);
}

.prod-flow-preview-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 6px 9px;
  border-bottom: 1px solid var(--line);
  font-family: var(--sans);
  font-size: 11.5px;
  color: var(--ink-3);
}

.prod-flow-preview-head button {
  border: 0;
  background: none;
  color: var(--ink-3);
  padding: 0;
  display: grid;
  place-items: center;
}

.prod-flow-preview pre {
  margin: 0;
  padding: 10px;
  max-height: 320px;
  overflow: auto;
  font-family: var(--mono);
  font-size: 11.5px;
  line-height: 1.6;
  color: var(--ink-2);
  white-space: pre-wrap;
  word-break: break-word;
}
</style>
