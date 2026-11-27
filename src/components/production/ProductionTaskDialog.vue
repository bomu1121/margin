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
  LoaderCircle,
  Pencil,
  Play,
  RotateCcw,
  Sparkles,
  Square,
  Trash2,
  X,
} from 'lucide-vue-next'

import * as api from '../../lib/api'
import { renderRichText } from '../../lib/richText'
import { buildRunNodes, stepLabel, type RunNode } from '../../lib/runModel'
import {
  formatBytes,
  formatDate,
  formatDuration,
  formatTokens,
  progressOf,
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
const graphLoading = ref(false)
const artifactPreview = ref<{ name: string; text: string } | null>(null)
const artifactLoading = ref(false)
const selectedStageKey = ref('')
const stagePicked = ref(false)
const logScope = ref<'stage' | 'all'>('stage')
const logRef = ref<HTMLElement | null>(null)
const logStick = ref(true)
// Programmatic scroll also fires `scroll`; without this flag it can briefly look
// like the user scrolled away and make the "jump to latest" pill flicker.
let logAutoScrolling = false
const loading = ref(false)
const error = ref('')
const activeTab = ref('run')
const tabInitialized = ref(false)
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
const hasNote = computed(() => Boolean(result.value?.body?.trim()))
const tabs = computed(() => {
  const items: Array<{ id: string; label: string }> = []
  // The note and the run are deliberately separate: one is the reading surface,
  // the other is the evidence surface. Neither repeats the other's summary.
  if (hasNote.value) items.push({ id: 'note', label: '笔记' })
  items.push({ id: 'run', label: '运行' })
  if ((result.value?.variants.length ?? 0) >= 2) items.push({ id: 'compare', label: '对照' })
  if (result.value?.mindmap) items.push({ id: 'mindmap', label: '思维导图' })
  if ((result.value?.drill.length ?? 0) > 0) items.push({ id: 'drill', label: '练一练' })
  if ((result.value?.trace.length ?? 0) > 0) items.push({ id: 'trace', label: '溯源' })
  items.push({ id: 'usage', label: '用量' })
  if (hasNote.value && job.value?.noteId) items.push({ id: 'versions', label: '版本' })
  return items
})
const currentTab = computed(() => (
  tabs.value.some((item) => item.id === activeTab.value)
    ? activeTab.value
    : (hasNote.value ? 'note' : 'run')
))
const selectedVersion = computed(() => versions.value.find((item) => item.id === selectedVersionId.value))
const tokens = computed(() => (job.value?.usage.promptTokens ?? 0) + (job.value?.usage.completionTokens ?? 0))
const isActive = computed(() => ['running', 'queued'].includes(job.value?.status ?? ''))
const doneStepCount = computed(() => job.value?.steps.filter((step) => step.status === 'done').length ?? 0)
const runningStep = computed(() => job.value?.steps.find((step) => step.status === 'running') ?? null)
const stages = computed<RunNode[]>(() => (job.value ? buildRunNodes(job.value, runGraph.value) : []))
const autoStageKey = computed(() => {
  if (!stages.value.length) return ''
  const running = stages.value.find((stage) => stage.status === 'running')
  if (running) return running.key
  const failed = stages.value.find((stage) => ['error', 'interrupted', 'cancelled'].includes(stage.status))
  if (failed) return failed.key
  const agent = stages.value.find((stage) => stage.key === 'agent_note')
  if (agent) return agent.key
  const publish = stages.value.find((stage) => stage.key === 'publish')
  if (publish) return publish.key
  const withArtifacts = [...stages.value].reverse().find((stage) => stage.artifacts.length)
  return withArtifacts?.key ?? stages.value[stages.value.length - 1]?.key ?? ''
})
const selectedStage = computed(() => stages.value.find((stage) => stage.key === selectedStageKey.value) ?? null)
const selectedStageLogs = computed(() => {
  const stage = selectedStage.value
  if (!stage || stage.stepIndex < 0) return []
  return (detail.value?.logs ?? []).filter((log) => log.stepIndex === stage.stepIndex)
})
const shownLogs = computed(() => (logScope.value === 'all' ? (detail.value?.logs ?? []) : selectedStageLogs.value))
const stageLogCount = computed(() => shownLogs.value.length)
const stageArtifacts = computed(() => selectedStage.value?.artifacts ?? [])
const runHeroLabel = computed(() => {
  if (!job.value) return ''
  if (job.value.status === 'queued') return '排队中'
  if (job.value.status === 'running') return runningLabel.value
  if (job.value.status === 'success') return '制作完成'
  if (job.value.status === 'error') return '制作失败'
  if (job.value.status === 'cancelled') return '已取消'
  if (job.value.status === 'interrupted') return '任务被中断'
  return jobStatusLabel(job.value)
})
const runHeroDetail = computed(() => {
  if (!job.value) return ''
  if (job.value.status === 'running' || job.value.status === 'queued') return runningDetail.value
  if (job.value.status === 'success') return '每一步都保留了可查看的产物和日志，可在左侧逐段校对。'
  if (job.value.status === 'error') return displayError(job.value.error) || '某个步骤失败。'
  return job.value.status === 'cancelled' ? '这次制作被取消；已完成的阶段仍可查看。' : '应用在运行中退出；已完成的阶段仍可查看。'
})
const runningDetail = computed(() => {
  const step = runningStep.value
  if (step?.detail) return step.detail
  if (step) return `${stepLabel(step.key, step.label)}进行中`
  return job.value?.currentStep?.trim() || (job.value?.status === 'queued' ? '等待开始…' : '正在准备素材…')
})
const costText = computed(() => {
  const cost = job.value?.usage.estimatedCost ?? 0
  return cost ? `¥${cost.toFixed(2)}` : '—'
})
const costNote = computed(() => {
  const usage = job.value?.usage
  if (!usage) return ''
  if (usage.priced) return '目录价估算'
  return usage.estimatedCost ? 'ASR 手动价' : '未收录价格'
})
const sourceAuthor = computed(() => {
  const source = job.value?.source
  if (!source || (source.kind !== 'bili' && source.kind !== 'favorites')) return ''
  return source.videos[0]?.author || ''
})
const noteTitle = computed(() => {
  const body = result.value?.body ?? ''
  const line = body.split('\n').find((item) => item.startsWith('# '))
  return line?.slice(2).trim() || job.value?.title || ''
})
// Note titles often differ from the task title only by trailing punctuation;
// treat those as the same title so the note tab does not print it twice.
const noteTitleDiffers = computed(() => {
  const strip = (value: string) => value.trim().replace(/[。！？!?.,，、；;：:\s]+$/, '')
  return strip(noteTitle.value) !== strip(job.value?.title ?? '')
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

function syncPolling(): void {
  window.clearInterval(pollTimer)
  pollTimer = undefined
  if (job.value && ['running', 'queued'].includes(job.value.status)) {
    pollTimer = window.setInterval(() => {
      void loadDetail(true)
      void loadRunGraph(true)
    }, 1500)
  }
}

let graphLoadingGuard = false

async function loadRunGraph(silent = false): Promise<void> {
  if (!props.jobId || graphLoadingGuard) return
  graphLoadingGuard = true
  graphLoading.value = true
  try {
    runGraph.value = await api.productionRunGraph(props.jobId)
  } catch (cause) {
    if (!silent) error.value = api.errorText(cause)
  } finally {
    graphLoadingGuard = false
    graphLoading.value = false
  }
}

watch(autoStageKey, (key) => {
  if (!stagePicked.value && key) selectedStageKey.value = key
}, { immediate: true })

watch(stages, (items) => {
  if (!items.length) {
    selectedStageKey.value = ''
    return
  }
  if (!items.some((item) => item.key === selectedStageKey.value)) {
    selectedStageKey.value = autoStageKey.value
  }
}, { immediate: true })

function selectStage(key: string): void {
  stagePicked.value = key !== autoStageKey.value
  selectedStageKey.value = key
}

function followCurrentStage(): void {
  stagePicked.value = false
  selectedStageKey.value = autoStageKey.value
}

function stageMeta(stage: RunNode): string {
  const parts: string[] = []
  if (stage.startedAt) parts.push(nodeDuration(stage))
  // Node summaries often already include the exact split (e.g. 1986/44 tok);
  // only add the aggregate when the summary does not carry it.
  if (stage.tokens && !/tokens?/i.test(stage.summary)) parts.push(`${formatTokens(stage.tokens)} tok`)
  if (stage.cost) parts.push(`¥${stage.cost.toFixed(3)}`)
  return parts.join(' · ')
}

function stageSummary(stage: RunNode): string {
  return stage.error || stage.summary || stepStatusLabel(stage.status)
}

function stageRowSummary(stage: RunNode): string {
  return stageSummary(stage).replace(/\s*·\s*\d+\/\d+\s*tokens?$/i, '')
}

function nodeDuration(node: { startedAt: number; endedAt: number }): string {
  if (!node.startedAt) return '—'
  const end = node.endedAt || Date.now()
  const seconds = Math.max(0, Math.round((end - node.startedAt) / 1000))
  return formatDuration(seconds)
}

async function openArtifact(artifact: ProductionRunArtifact): Promise<void> {
  artifactLoading.value = true
  try {
    const text = await api.productionArtifactText(artifact.id)
    artifactPreview.value = { name: artifact.name, text }
  } catch (cause) {
    error.value = api.errorText(cause)
  } finally {
    artifactLoading.value = false
  }
}

function onLogScroll(): void {
  if (logAutoScrolling) return
  const el = logRef.value
  if (!el) return
  logStick.value = el.scrollHeight - el.scrollTop - el.clientHeight < 28
}

async function scrollLogsToEnd(): Promise<void> {
  await nextTick()
  const el = logRef.value
  if (!el) return
  logAutoScrolling = true
  el.scrollTop = el.scrollHeight
  window.requestAnimationFrame(() => {
    window.setTimeout(() => {
      logAutoScrolling = false
    }, 80)
  })
}

async function scrollLogsToStart(): Promise<void> {
  await nextTick()
  const el = logRef.value
  if (el) el.scrollTop = 0
}

function stageIsLive(): boolean {
  return isActive.value && selectedStage.value?.status === 'running'
}

watch(selectedStageKey, async () => {
  artifactPreview.value = null
  logStick.value = stageIsLive()
  if (logStick.value) await scrollLogsToEnd()
  else await scrollLogsToStart()
})

watch(logScope, async () => {
  logStick.value = stageIsLive()
  if (logStick.value) await scrollLogsToEnd()
  else await scrollLogsToStart()
})

watch(
  () => shownLogs.value.length,
  async () => {
    if (logStick.value) await scrollLogsToEnd()
  },
)

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
      if (!tabInitialized.value) {
        tabInitialized.value = true
        activeTab.value = data.job.status === 'success' && data.job.result?.body?.trim() ? 'note' : 'run'
      }
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

function selectTab(next: string): void {
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
  activeTab.value = 'run'
  tabInitialized.value = false
  quizChoices.value = {}
  void loadDetail()
}, { immediate: true })

watch(tabs, (items) => {
  if (!items.some((item) => item.id === activeTab.value)) {
    activeTab.value = hasNote.value ? 'note' : 'run'
  }
})

onMounted(() => {
  if (!production.templates.length) void refreshJobs()
  void loadRunGraph()
  syncPolling()
  void loadVersions()
})

watch(currentTab, (tab) => {
  if (tab === 'versions') void loadVersions()
  if (tab === 'run') void loadRunGraph()
  if (tab === 'trace') void loadTraceFacts()
})

watch(
  () => props.jobId,
  () => {
    confirmDelete.value = false
    renaming.value = false
    renameValue.value = ''
    activeTab.value = 'run'
    tabInitialized.value = false
    quizChoices.value = {}
    versions.value = []
    selectedVersionId.value = ''
    versionBody.value = ''
    confirmRestoreId.value = ''
    restoreNotice.value = ''
    traceFacts.value = []
    traceFactsError.value = ''
    runGraph.value = null
    artifactPreview.value = null
    selectedStageKey.value = ''
    stagePicked.value = false
    logScope.value = 'stage'
    logStick.value = true
  },
)

watch(
  () => job.value?.status,
  (next, previous) => {
    if (next && next !== previous) {
      void loadDetail(true)
      void loadRunGraph(true)
    }
  },
)

onBeforeUnmount(() => window.clearInterval(pollTimer))
</script>

<template>
  <div v-if="jobId" class="prod-overlay" @click.self="close">
    <div v-if="job" class="prod-dialog prod-task-dialog" role="dialog" aria-modal="true" aria-labelledby="prodTaskTitle">
      <div class="prod-task-top">
        <div class="prod-task-heading">
          <div class="prod-task-eyebrow">
            <span class="prod-task-template">{{ template?.name ?? job.templateId }}</span>
            <span v-if="job.templateId === 'ai-note' && job.options.prompt" class="prod-task-prompt">{{ job.options.prompt }}</span>
            <span v-if="job.options.auto" class="prod-auto-badge">UP 自动</span>
            <span class="prod-task-status" :class="statusClass(job.status)">
              <i class="prod-dot" :class="statusClass(job.status) === 'is-success' ? 'is-green' : statusClass(job.status) === 'is-error' ? 'is-red' : statusClass(job.status) === 'is-gold' ? 'is-gold' : 'is-blue'" />
              {{ jobStatusLabel(job) }}
            </span>
          </div>
          <div class="prod-task-title-row">
            <template v-if="renaming">
              <input ref="renameInput" v-model="renameValue" class="prod-rename-input" maxlength="120" @keyup.enter="commitRename" @keyup.esc="cancelRename" />
              <button type="button" class="prod-icon-btn" aria-label="保存标题" @click="commitRename"><Check :size="14" /></button>
              <button type="button" class="prod-icon-btn" aria-label="取消重命名" @click="cancelRename"><X :size="14" /></button>
            </template>
            <h2 v-else id="prodTaskTitle" :title="job.title">{{ job.title }}</h2>
          </div>
          <div class="prod-task-meta">
            <span>{{ sourceSummary(job.source) }}</span>
            <template v-if="sourceAuthor">
              <span class="prod-meta-sep">·</span>
              <span>{{ sourceAuthor }}</span>
            </template>
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
          <button v-if="job.noteId && currentTab !== 'note'" type="button" class="prod-btn is-small" @click="openNote">
            <ExternalLink :size="12" /> 打开笔记
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

      <div class="prod-task-tabbar">
        <div class="prod-task-tabs" role="tablist">
          <button v-for="tab in tabs" :key="tab.id" type="button" role="tab" :aria-selected="currentTab === tab.id" :class="{ 'is-active': currentTab === tab.id }" @click="selectTab(tab.id)">{{ tab.label }}</button>
        </div>
      </div>

      <div class="prod-task-layout">
        <div class="prod-task-main">
          <div v-if="error" class="prod-error-box" style="margin-bottom: 10px">
            <h3>操作失败</h3>
            <p>{{ error }}</p>
          </div>

          <section v-if="currentTab === 'note'" class="prod-task-panel is-active is-note">
            <div v-if="result" class="prod-result-doc">
              <div class="prod-result-bar">
                <span class="prod-result-pill"><Check :size="12" /> {{ job.templateId === 'note-trace' ? '事实已更新' : '已保存' }}</span>
                <span class="prod-note-location" :title="job.noteRelPath">
                  <FileText :size="11" />
                  {{ noteFile || '笔记已入库' }}<template v-if="noteDir"> · {{ noteDir }}/</template>
                </span>
                <button v-if="job.noteId" type="button" class="prod-note-open" @click="openNote">打开笔记 <ExternalLink :size="11" /></button>
              </div>
              <h3 v-if="noteTitleDiffers">{{ noteTitle }}</h3>
              <div class="prod-markdown" v-html="renderMarkdown(bodyWithoutTitle)" />
            </div>
          </section>

          <section v-else-if="currentTab === 'run'" class="prod-task-panel is-active is-run">
            <div class="prod-run-hero" :class="statusClass(job.status)">
              <div class="prod-run-orb" :class="{ 'is-live': job.status === 'running' }">
                <LoaderCircle v-if="isActive" :size="19" class="prod-spin" />
                <CircleAlert v-else-if="job.status === 'error' || job.status === 'interrupted'" :size="19" />
                <Check v-else-if="job.status === 'success'" :size="19" />
                <Sparkles v-else :size="19" />
              </div>
              <div class="prod-run-copy">
                <div class="prod-run-heading">
                  <b>{{ runHeroLabel }}</b>
                  <span class="prod-run-count">{{ doneStepCount }} / {{ job.steps.length }} 步</span>
                </div>
                <p class="prod-run-detail">{{ runHeroDetail }}</p>
                <div class="prod-run-meta">
                  <span><b>{{ formatDuration(elapsedSeconds) }}</b> 已运行</span>
                  <span><b>{{ formatTokens(tokens) }}</b> tok</span>
                  <span><b>{{ job.progress }}%</b></span>
                  <span v-if="isActive" class="prod-log-live"><i />实时</span>
                </div>
              </div>
            </div>
            <div class="prod-run-progress" role="progressbar" :aria-valuenow="job.progress" aria-valuemin="0" aria-valuemax="100">
              <i :style="{ width: `${progressOf(job)}%` }" />
            </div>

            <div v-if="job.status === 'error' || job.status === 'cancelled' || job.status === 'interrupted'" class="prod-run-notice" :class="`is-${job.status}`">
              <p v-if="job.auto && job.nextRetryAt">已自动安排重试：第 {{ job.retryCount }} 次失败，预计 {{ formatDate(job.nextRetryAt) }}。</p>
              <p v-else-if="job.auto && job.retryCount">已自动重试 {{ job.retryCount }} 次；依赖恢复后会自动继续。</p>
              <p v-if="resumeInfo">{{ resumeInfo.canResume ? `已保存 ${resumeInfo.checkpointCount} 个中间结果，继续会复用已完成的调用。` : resumeInfo.reason }}</p>
              <p v-if="resumeInfo?.configChanged">任务创建后模型/接口/默认提示词变过；继续使用创建时配置，重跑全部使用当前配置。</p>
              <p v-if="resumeInfo?.published">笔记已经入库；继续只会补完成状态，不会重复发布。</p>
              <p v-if="!job.auto && !resumeInfo">可以从右上角重试，或查看下面已完成的阶段产物。</p>
            </div>

            <div class="prod-run-workbench">
              <nav class="prod-stage-rail" aria-label="执行阶段">
                <div class="prod-stage-rail-head">
                  <span>阶段</span>
                  <button v-if="isActive && selectedStageKey !== autoStageKey" type="button" @click="followCurrentStage">回到当前</button>
                </div>
                <button
                  v-for="stage in stages"
                  :key="stage.key"
                  type="button"
                  class="prod-stage-row"
                  :class="[`is-${stage.status}`, { 'is-selected': selectedStageKey === stage.key }]"
                  @click="selectStage(stage.key)"
                >
                  <span class="prod-stage-mark">
                    <LoaderCircle v-if="stage.status === 'running'" :size="11" class="prod-spin" />
                    <Check v-else-if="stage.status === 'done'" :size="11" />
                    <CircleAlert v-else-if="stage.status === 'error' || stage.status === 'interrupted'" :size="11" />
                    <span v-else class="prod-stage-dot" />
                  </span>
                  <span class="prod-stage-copy">
                    <span class="prod-stage-line">
                      <b>{{ stage.label }}</b>
                      <em>{{ stepStatusLabel(stage.status) }}</em>
                    </span>
                    <small v-if="stage.error">{{ stage.error }}</small>
                    <small v-else-if="stage.summary && stage.status !== 'running'">{{ stageRowSummary(stage) }}</small>
                    <small v-else-if="stage.artifacts.length">{{ stage.artifacts.length }} 个阶段产物</small>
                    <small v-else-if="stage.status === 'running'">正在写入实时日志…</small>
                    <small v-else>{{ stageMeta(stage) || '等待执行' }}</small>
                  </span>
                  <span v-if="stage.artifacts.length" class="prod-stage-badge">{{ stage.artifacts.length }}</span>
                </button>
              </nav>

              <section v-if="selectedStage" class="prod-stage-detail">
                <header class="prod-stage-head">
                  <div>
                    <span class="prod-stage-kicker">阶段 {{ selectedStage.stepIndex >= 0 ? selectedStage.stepIndex + 1 : '·' }} / {{ job.steps.length }} · {{ stepStatusLabel(selectedStage.status) }}</span>
                    <h3>{{ selectedStage.label }}</h3>
                  </div>
                  <span class="prod-stage-stats">{{ stageMeta(selectedStage) || '—' }}</span>
                </header>
                <p v-if="selectedStage.status !== 'running'" class="prod-stage-summary" :class="{ 'is-error': selectedStage.error }">{{ stageSummary(selectedStage) }}</p>

                <div class="prod-stage-artifacts">
                  <div class="prod-stage-section-head">
                    <span>阶段产物</span>
                    <small v-if="stageArtifacts.length">{{ stageArtifacts.length }} 个 · 点击预览</small>
                    <small v-else-if="graphLoading">正在读取…</small>
                    <small v-else>还没有产物</small>
                  </div>
                  <div v-if="stageArtifacts.length" class="prod-artifact-row">
                    <button v-for="artifact in stageArtifacts" :key="artifact.id" type="button" class="prod-artifact-chip" :class="{ 'is-selected': artifactPreview?.name === artifact.name }" @click="openArtifact(artifact)">
                      <FileText :size="12" />
                      <span>{{ artifact.name }}</span>
                      <small>{{ formatBytes(artifact.bytes) }}</small>
                    </button>
                  </div>
                  <div v-if="artifactLoading" class="prod-artifact-loading"><LoaderCircle :size="12" class="prod-spin" />正在读取产物…</div>
                  <div v-else-if="artifactPreview" class="prod-artifact-preview">
                    <div class="prod-artifact-preview-head">
                      <span>{{ artifactPreview.name }}</span>
                      <button type="button" aria-label="关闭产物预览" @click="artifactPreview = null"><X :size="12" /></button>
                    </div>
                    <pre>{{ artifactPreview.text }}</pre>
                  </div>
                  <p v-else-if="!stageArtifacts.length && !graphLoading" class="prod-stage-empty">这个阶段完成后会保留可校对的结果；运行中先看下面的日志。</p>
                </div>

                <div class="prod-stage-log">
                  <div class="prod-stage-section-head">
                    <span>日志 <em>{{ stageLogCount }}</em></span>
                    <div class="prod-log-tools">
                      <span class="prod-log-scope">
                        <button type="button" :class="{ 'is-active': logScope === 'stage' }" @click="logScope = 'stage'">本阶段</button>
                        <button type="button" :class="{ 'is-active': logScope === 'all' }" @click="logScope = 'all'">全部</button>
                      </span>
                      <span v-if="isActive && selectedStage.status === 'running'" class="prod-log-live"><i />实时</span>
                    </div>
                  </div>
                  <div ref="logRef" class="prod-log" @scroll="onLogScroll">
                    <div v-for="log in shownLogs" :key="log.id" class="prod-log-row" :class="`is-${log.level}`">
                      <span class="prod-log-time">{{ formatDate(log.createdAt).slice(6) }}</span>
                      <span class="prod-log-level">{{ log.level }}</span>
                      <span class="prod-log-message" :title="log.message">{{ log.message }}</span>
                    </div>
                    <div v-if="isActive && selectedStage.status === 'running'" class="prod-log-live-row">
                      <span class="prod-log-live-dot"><i /></span>
                      <span class="prod-log-live-line">
                        <span class="prod-log-live-text">{{ selectedStage.stepDetail || '正在执行…' }}</span>
                        <em>{{ nodeDuration(selectedStage) }}</em>
                      </span>
                    </div>
                    <p v-if="!shownLogs.length && !(isActive && selectedStage.status === 'running')" class="prod-stage-empty">{{ logScope === 'stage' ? '这个阶段还没有日志；开始后会实时出现。' : '还没有日志；运行开始后逐步出现。' }}</p>
                  </div>
                  <button v-if="!logStick && stageIsLive()" type="button" class="prod-log-jump" @click="scrollLogsToEnd">跳到最新 ↓</button>
                </div>
              </section>
            </div>
            <p v-if="isActive" class="prod-run-hint">取消后已完成的阶段不会写进笔记；阶段产物和日志会保留到任务结束。</p>
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

          <section v-else class="prod-task-panel is-active is-usage">
            <div class="prod-usage-hero">
              <div class="prod-usage-stat">
                <span>总 tokens</span>
                <b>{{ formatTokens(tokens) }}</b>
                <small>输入 {{ formatTokens(job.usage.promptTokens) }} · 输出 {{ formatTokens(job.usage.completionTokens) }}</small>
              </div>
              <div class="prod-usage-stat">
                <span>估算金额</span>
                <b>{{ costText }}</b>
                <small>{{ costNote }}</small>
              </div>
              <div v-if="job.usage.audioSeconds" class="prod-usage-stat">
                <span>音频时长</span>
                <b>{{ formatDuration(job.usage.audioSeconds) }}</b>
                <small>ASR 转写</small>
              </div>
            </div>
            <div class="prod-usage-panel">
              <div class="prod-usage-panel-head">用量明细</div>
              <div class="prod-usage-table">
                <div class="prod-usage-row"><span>输入 tokens</span><b>{{ formatTokens(job.usage.promptTokens) }}</b><span>未缓存 {{ formatTokens(job.usage.uncachedInputTokens) }}</span></div>
                <div v-if="job.usage.cachedTokens" class="prod-usage-row"><span>缓存读 tokens</span><b>{{ formatTokens(job.usage.cachedTokens) }}</b><span>接口返回</span></div>
                <div v-if="job.usage.cacheWriteTokens" class="prod-usage-row"><span>缓存写 tokens</span><b>{{ formatTokens(job.usage.cacheWriteTokens) }}</b><span>接口返回</span></div>
                <div class="prod-usage-row"><span>输出 tokens</span><b>{{ formatTokens(job.usage.completionTokens) }}</b><span>接口返回</span></div>
                <div v-if="job.usage.reasoningTokens" class="prod-usage-row"><span>推理 tokens</span><b>{{ formatTokens(job.usage.reasoningTokens) }}</b><span>仅记录，不重复计费</span></div>
                <div class="prod-usage-row"><span>音频</span><b>{{ formatDuration(job.usage.audioSeconds) }}</b><span>ASR</span></div>
                <div class="prod-usage-row"><span>估算金额</span><b>{{ costText }}</b><span>{{ costNote }}</span></div>
                <div v-if="job.usage.providerCost > 0" class="prod-usage-row"><span>服务商账单</span><b>{{ job.usage.providerCurrency }} {{ job.usage.providerCost.toFixed(4) }}</b><span>接口实报</span></div>
              </div>
            </div>
            <p class="prod-usage-note">token 是接口返回值；金额 = token × 参考价，属于估算。服务商实报金额单独显示，不混入估算。</p>
          </section>
        </div>


      </div>
    </div>
  </div>
</template>
