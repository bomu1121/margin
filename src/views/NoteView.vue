<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, reactive, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'

import { writeText } from '@tauri-apps/plugin-clipboard-manager'
import { ArrowUpRight, Check, ChevronRight, Copy, Crosshair, ExternalLink, LoaderCircle, PanelRightClose, PanelRightOpen } from 'lucide-vue-next'

import AnnotationCard from '../components/AnnotationCard.vue'
import AnnotationComposer from '../components/AnnotationComposer.vue'
import NoteBlock from '../components/NoteBlock.vue'
import RelationGraph from '../components/RelationGraph.vue'
import * as api from '../lib/api'
import { parseNote, type Block, type ParsedNote } from '../lib/markdown'
import { planAnchorRelocations } from '../lib/annotationAnchor'
import type { Annotation, AnnotationSegment, Fact, NoteCitation, NoteDetail, ProductionJob } from '../types'
import { factHasEvidence, factRailState, factSourceSummary, factStatusClass, factStatusKey } from '../lib/facts'
import { library, refreshNotes } from '../stores/library'
import { loadProduction, production, productionUi } from '../stores/production'

// Cold-start measurement: first time a note has actually painted, tell Rust.
// This is a diagnostic only; a missing command must never block the page.
let readyReported = false

const route = useRoute()
const router = useRouter()

const detail = ref<NoteDetail | null>(null)
const parsed = ref<ParsedNote | null>(null)
const annotations = ref<Annotation[]>([])
const facts = ref<Fact[]>([])
const factLocateError = ref('')
const traceTaskId = ref('')
const traceJob = ref<ProductionJob | null>(null)
const traceStarting = ref(false)
const traceError = ref('')
const traceNotice = ref('')
let tracePollTimer = 0
const loading = ref(false)
const error = ref('')
const pendingSegments = ref<AnnotationSegment[]>([])
const pendingQuote = ref('')
const pendingBlockId = ref('')
const pendingColor = ref('gold')
const pendingBody = ref('')
const activeAnnotationId = ref('')
const hoverAnnotationId = ref('')
type RailTab = 'facts' | 'citations' | 'notes' | 'links' | 'info' | 'toc'
const railTab = ref<RailTab>('citations')
const noteCitations = ref<NoteCitation[]>([])
const citeRunning = ref(false)
const citeError = ref('')
const factFilter = ref<'all' | 'attention'>('all')
const RAIL_MIN = 300
const RAIL_MAX = 560
const viewportWidth = ref(window.innerWidth)
const railBody = ref<HTMLElement | null>(null)
const railAnnoLayer = ref<HTMLElement | null>(null)
function railMaxWidth(): number {
  return viewportWidth.value <= 1180 ? 330 : RAIL_MAX
}
function storedRailNumber(): number {
  const value = Number(window.localStorage.getItem('yebian.rail.width'))
  const fallback = window.innerWidth <= 1180 ? 330 : 380
  return Number.isFinite(value) && value >= RAIL_MIN && value <= RAIL_MAX ? value : fallback
}
const railPreferredWidth = ref(storedRailNumber())
const railWidth = computed(() => Math.min(railPreferredWidth.value, railMaxWidth()))
const railCollapsed = ref(window.localStorage.getItem('yebian.rail.collapsed') === '1')
const railResizing = ref(false)
const railStyle = computed(() => ({ '--rail-width': `${railWidth.value}px` }))
let railStartX = 0
let railStartWidth = 0
const docScroll = ref<HTMLElement | null>(null)
const relocating = ref(false)
const relocateNotice = ref('')
const relocateError = ref('')
const orphanDeleteId = ref('')
let orphanDeleteTimer = 0

function armOrphanDelete(id: string): void {
  orphanDeleteId.value = id
  window.clearTimeout(orphanDeleteTimer)
  orphanDeleteTimer = window.setTimeout(() => {
    orphanDeleteId.value = ''
  }, 4000)
}

function cancelOrphanDelete(): void {
  window.clearTimeout(orphanDeleteTimer)
  orphanDeleteId.value = ''
}

function confirmOrphanDelete(id: string): void {
  window.clearTimeout(orphanDeleteTimer)
  orphanDeleteId.value = ''
  void removeAnnotation(id)
}

const note = computed(() => detail.value?.meta)
const artifact = computed(() => detail.value?.artifact ?? null)
const sourceLinkLabel = computed(() => artifact.value?.source?.kind === 'bili' ? '原视频' : '原文')
const authorTag = computed(() => (note.value?.tags ?? []).find((tag) => /^(?:UP主|作者)[/／]/.test(tag)) ?? '')
const bylineAuthor = computed(() => authorTag.value.replace(/^(?:UP主|作者)[/／]\s*/, '').trim())
const visibleTags = computed(() => (note.value?.tags ?? []).filter((tag) => tag !== authorTag.value))
const sourceTitle = computed(() => {
  const source = artifact.value?.title?.trim() ?? ''
  const current = note.value?.title?.trim() ?? ''
  const strip = (value: string) => value.replace(/[。.!！?？]+$/, '')
  return source && strip(source) !== strip(current) ? source : ''
})
const relativePath = computed(() => {
  const rel = note.value?.relPath ?? ''
  const index = rel.lastIndexOf('/')
  return index < 0
    ? { dir: '', name: rel }
    : { dir: rel.slice(0, index), name: rel.slice(index + 1) }
})
const pathCopied = ref(false)
let pathCopyTimer = 0
const titleParts = computed(() => {
  const title = note.value?.title ?? ''
  return title.match(/[^：:，,、；;]+[：:，,、；;]?/g)?.filter(Boolean) ?? [title]
})

const blockOrder = computed(() => {
  const map: Record<string, number> = {}
  parsed.value?.allBlocks.forEach((block, index) => {
    map[block.id] = index
  })
  return map
})

const sortedAnnotations = computed(() =>
  [...annotations.value].sort((a, b) => {
    const byBlock = (blockOrder.value[a.blockId] ?? 99999) - (blockOrder.value[b.blockId] ?? 99999)
    if (byBlock) return byBlock
    return (a.segments[0]?.start ?? 0) - (b.segments[0]?.start ?? 0)
  }),
)

const orphans = computed(() =>
  sortedAnnotations.value.filter((anno) => blockOrder.value[anno.blockId] === undefined),
)

const validAnnotations = computed(() =>
  sortedAnnotations.value.filter((anno) => blockOrder.value[anno.blockId] !== undefined),
)

const verifiedFactCount = computed(() =>
  facts.value.filter((fact) => factHasEvidence(fact)).length,
)

const attentionFactCount = computed(() =>
  facts.value.filter((fact) => !factHasEvidence(fact)).length,
)

const railShowsDomain = computed(() => {
  const domains = new Set(facts.value.map((fact) => (fact.domain === 'history' ? 'history' : 'other')))
  return domains.size > 1
})

const railFacts = computed(() => {
  const priority: Record<string, number> = { disputed: 0, unsupported: 1, candidate: 2, verified: 3 }
  const source = factFilter.value === 'attention'
    ? facts.value.filter((fact) => !factHasEvidence(fact))
    : facts.value
  return [...source].sort((left, right) =>
    (priority[factStatusKey(left)] ?? 9) - (priority[factStatusKey(right)] ?? 9) || right.updatedAt - left.updatedAt,
  )
})

const traceRunning = computed(() => ['queued', 'running'].includes(traceJob.value?.status ?? ''))
const traceStatusText = computed(() => {
  const status = traceJob.value?.status ?? ''
  return ({
    queued: '排队中',
    running: '进行中',
    success: '已完成',
    error: '失败',
    cancelled: '已取消',
    interrupted: '已中断',
  } as Record<string, string>)[status] ?? status
})

const glossaryGroups = computed(() => {
  if (!parsed.value) return []
  const order: string[] = []
  const map = new Map<string, typeof parsed.value.glossary>()
  for (const item of parsed.value.glossary) {
    if (!map.has(item.cat)) {
      map.set(item.cat, [])
      order.push(item.cat)
    }
    map.get(item.cat)?.push(item)
  }
  return order.map((cat) => ({ cat, items: map.get(cat) ?? [] }))
})

function pad(value: number): string {
  return String(value).padStart(2, '0')
}

function formatTime(ms: number): string {
  const date = new Date(ms)
  return `${date.getMonth() + 1}-${date.getDate()} ${String(date.getHours()).padStart(2, '0')}:${String(
    date.getMinutes(),
  ).padStart(2, '0')}`
}

async function openArtifact(): Promise<void> {
  if (!artifact.value) return
  await loadProduction()
  const job = production.jobs.find((item) => item.noteId === artifact.value?.noteId)
  productionUi.taskId = job?.id ?? ''
  productionUi.view = job ? 'home' : 'home'
  void router.push('/studio')
}

function openSourceVideo(): void {
  const url = artifact.value?.source?.url
  if (url) void api.openExternalUrl(url)
}

async function loadAnnotations(id: string): Promise<void> {
  annotations.value = await api.listAnnotations(id)
}

async function loadFacts(id: string): Promise<void> {
  try {
    facts.value = await api.productionNoteFacts(id, '')
  } catch {
    facts.value = []
  }
}

async function loadCitations(id: string): Promise<void> {
  citeError.value = ''
  citeNotice.value = ''
  citeEditingId.value = ''
  citeDraft.value = ''
  citeDeletingId.value = ''
  citeAdding.value = false
  citeBatchRunning.value = false
  citeBatchDone.value = 0
  citeBatchTotal.value = 0
  citeBatchStopping.value = false
  citeBatchAttempted.value = 0
  citeBatchFailures.value = []
  citeExpandedIds.value = []
  try {
    noteCitations.value = await api.productionNoteCitations(id)
  } catch {
    noteCitations.value = []
  }
}

async function startCite(): Promise<void> {
  const meta = note.value
  const body = detail.value?.body ?? ''
  if (!meta || !body.trim() || citeRunning.value || citeFillingId.value || citeBatchRunning.value) return
  citeRunning.value = true
  citeError.value = ''
  citeNotice.value = ''
  try {
    const report = await api.productionCiteNote({
      noteId: meta.id,
      title: meta.title,
      body,
      maxCitations: 6,
    })
    noteCitations.value = report.citations?.length
      ? report.citations
      : await api.productionNoteCitations(meta.id)
    if (report.status === 'no_sources') citeError.value = '没有找到可用出处；可以先用史料库补候选。'
  } catch (cause) {
    citeError.value = api.errorText(cause)
  } finally {
    citeRunning.value = false
  }
}

function openCitation(citation: NoteCitation): void {
  if (citation.url) void api.openExternalUrl(citation.url)
}

function citationWorkLabel(value: string): string {
  let title = (value || '').trim().replace(/（[^）]*待补原文[^）]*）/g, '').trim()
  if (!title) return ''
  return title.startsWith('《') ? title : `《${title}》`
}

function citationQualityLabel(url: string): string {
  const value = (url || '').toLowerCase()
  if (!value) return ''
  if (['zh.wikisource.org', 'ctext.org', 'shidianguji.com', 'njmuseum.org.cn'].some((host) => value.includes(host))) return '原文库'
  if (['read.nlc.cn', 'hathitrust.org', 'fhac.com.cn'].some((host) => value.includes(host))) return '目录'
  return '二手'
}

const citeEditingId = ref('')
const citeDraft = ref('')
const citeAdding = ref(false)
const citeDeletingId = ref('')
const citeFillingId = ref('')
const citeBatchRunning = ref(false)
const citeBatchDone = ref(0)
const citeBatchTotal = ref(0)
const citeBatchStopping = ref(false)
const citeBatchAttempted = ref(0)
const citeBatchBudget = ref(0.05)
const citeBatchFailures = ref<{ quote: string; reason: string }[]>([])
const citeExpandedIds = ref<string[]>([])
const citeNotice = ref('')
const citeForm = reactive({ noteQuote: '', workTitle: '', url: '', originalText: '' })
const pendingCitationCount = computed(() => noteCitations.value.filter((item) => item.status === 'candidate').length)
const linkedCitationCount = computed(() => noteCitations.value.filter((item) => Boolean(item.url)).length)
const catalogOnlyCount = computed(
  () => noteCitations.value.filter((item) => item.status === 'candidate' && item.url && citationQualityLabel(item.url) === '目录').length,
)
const citeBatchTargets = computed(() =>
  noteCitations.value.filter(
    (item) => item.status === 'candidate' && item.url && citationQualityLabel(item.url) !== '目录',
  ),
)
const citeBatchCount = computed(() => citeBatchTargets.value.length)
const editingCitation = computed(() => noteCitations.value.find((item) => item.id === citeEditingId.value) ?? null)
const citeDraftDirty = computed(() => {
  if (!citeEditingId.value) return false
  return citeDraft.value.trim() !== (editingCitation.value?.originalText ?? '').trim()
})
const citationHint = computed(() => {
  if (!pendingCitationCount.value) return ''
  if (catalogOnlyCount.value === pendingCitationCount.value) {
    return `${pendingCitationCount.value} 条候选只找到目录页；目录页没有正文，请手动补原文，或删除后加一条原文库链接。`
  }
  if (citeBatchCount.value) {
    return `${pendingCitationCount.value} 条待补原文；可单条 AI 补，或批量补齐 ${citeBatchCount.value} 条。`
  }
  return linkedCitationCount.value
    ? `${pendingCitationCount.value} 条待补原文；可打开来源读原文，或直接补原文。`
    : `${pendingCitationCount.value} 条待补原文；当前没有可打开的来源链接，可直接补原文。`
})

function shortReason(value: string): string {
  const text = (value || '').replace(/\s+/g, ' ').trim()
  return text.length > 140 ? `${text.slice(0, 140)}…` : text
}

function formatBudget(value: number): string {
  const amount = Number(value)
  if (!Number.isFinite(amount) || amount <= 0) return '0.00'
  return amount < 0.01 ? amount.toFixed(4) : amount.toFixed(2)
}

function isLongOriginal(value: string): boolean {
  return [...(value || '').trim()].length > 60
}

function isCiteExpanded(id: string): boolean {
  return citeExpandedIds.value.includes(id)
}

function toggleCiteOriginal(id: string): void {
  citeExpandedIds.value = isCiteExpanded(id)
    ? citeExpandedIds.value.filter((item) => item !== id)
    : [...citeExpandedIds.value, id]
}

function stopCiteBatch(): void {
  if (citeBatchRunning.value) citeBatchStopping.value = true
}

function focusCiteEditor(): void {
  void nextTick(() => {
    const editor = document.querySelector<HTMLTextAreaElement>('.rail-cite-edit textarea')
    if (!editor) return
    editor.focus()
    editor.setSelectionRange(editor.value.length, editor.value.length)
  })
}

function focusCiteAction(id: string): void {
  if (!id) return
  void nextTick(() => document.querySelector<HTMLElement>(`[data-cite-edit="${id}"]`)?.focus())
}

function beginCitationEdit(citation: NoteCitation): void {
  if (citeFillingId.value || citeBatchRunning.value) return
  if (citeEditingId.value === citation.id) return
  if (citeDraftDirty.value) {
    citeError.value = '当前还有未保存的原文，先保存或取消'
    return
  }
  citeAdding.value = false
  citeDeletingId.value = ''
  citeEditingId.value = citation.id
  citeDraft.value = citation.originalText || ''
  citeError.value = ''
  citeNotice.value = ''
  focusCiteEditor()
}

function cancelCitationEdit(): void {
  const id = citeEditingId.value
  citeEditingId.value = ''
  citeDraft.value = ''
  citeError.value = ''
  focusCiteAction(id)
}

function beginCiteAdd(): void {
  if (citeFillingId.value || citeBatchRunning.value) return
  if (citeDraftDirty.value) {
    citeError.value = '当前还有未保存的原文，先保存或取消'
    return
  }
  citeEditingId.value = ''
  citeDraft.value = ''
  citeDeletingId.value = ''
  citeAdding.value = true
  citeError.value = ''
  citeNotice.value = ''
  citeForm.noteQuote = ''
  citeForm.workTitle = ''
  citeForm.url = ''
  citeForm.originalText = ''
  void nextTick(() => document.querySelector<HTMLTextAreaElement>('.rail-cite-add textarea')?.focus())
}

function cancelCiteAdd(): void {
  citeAdding.value = false
  citeError.value = ''
}

async function saveCiteAdd(): Promise<void> {
  const meta = note.value
  const body = detail.value?.body ?? ''
  const noteQuote = citeForm.noteQuote.trim()
  const workTitle = citeForm.workTitle.trim()
  if (!meta || !noteQuote || !workTitle) {
    citeError.value = '笔记原句和书名都要填'
    return
  }
  if (!body.split(/\s+/).join('').includes(noteQuote.split(/\s+/).join(''))) {
    citeError.value = '原句必须逐字来自这篇笔记；不要改写'
    return
  }
  try {
    const citation = await api.productionNoteCitationAdd({
      noteId: meta.id,
      noteQuote,
      workTitle,
      url: citeForm.url.trim(),
      originalText: citeForm.originalText.trim(),
      status: citeForm.originalText.trim() ? 'found' : 'candidate',
    })
    noteCitations.value.push(citation)
    citeAdding.value = false
    citeError.value = ''
  } catch (cause) {
    citeError.value = api.errorText(cause)
  }
}

async function saveCitationOriginal(citation: NoteCitation): Promise<void> {
  const text = citeDraft.value.trim()
  if (!text) {
    citeError.value = '原文不能为空'
    return
  }
  try {
    const updated = await api.productionNoteCitationSetOriginal(citation.id, text)
    const index = noteCitations.value.findIndex((item) => item.id === citation.id)
    if (index >= 0) noteCitations.value[index] = updated
    citeError.value = ''
    cancelCitationEdit()
  } catch (cause) {
    citeError.value = api.errorText(cause)
  }
}

async function fillCitationOriginal(citation: NoteCitation): Promise<void> {
  if (citeFillingId.value || citeBatchRunning.value) return
  citeFillingId.value = citation.id
  citeError.value = ''
  citeNotice.value = ''
  try {
    const report = await api.productionCiteFillOriginal(citation.id)
    const cost = report.usage.estimatedCost > 0 ? ` · ¥${report.usage.estimatedCost.toFixed(4)}` : ''
    if (!report.citation) {
      citeError.value = `${report.error || 'AI 没有补到原文'}${cost}`
      return
    }
    const updated = report.citation
    const index = noteCitations.value.findIndex((item) => item.id === citation.id)
    if (index >= 0) noteCitations.value[index] = updated
    const seconds = (report.durationMs / 1000).toFixed(1)
    const quality =
      report.sourceQuality === 'primary'
        ? '原文库'
        : report.sourceQuality === 'secondary'
          ? '二手页面'
          : report.sourceQuality === 'catalog'
            ? '目录'
            : ''
    citeNotice.value = `AI 已补原文 · ${quality ? `${quality} · ` : ''}${seconds}s${cost}`
  } catch (cause) {
    citeError.value = api.errorText(cause)
  } finally {
    citeFillingId.value = ''
  }
}

async function fillAllCandidates(): Promise<void> {
  if (citeBatchRunning.value || citeFillingId.value) return
  const noteId = note.value?.id
  const targets = citeBatchTargets.value
  if (!noteId || !targets.length) return
  const parsedBudget = Number(citeBatchBudget.value)
  const budget = Number.isFinite(parsedBudget) && parsedBudget > 0 ? parsedBudget : 0
  citeBatchRunning.value = true
  citeBatchStopping.value = false
  citeBatchDone.value = 0
  citeBatchTotal.value = targets.length
  citeBatchAttempted.value = 0
  citeBatchFailures.value = []
  citeError.value = ''
  citeNotice.value = ''
  let ok = 0
  let fail = 0
  let cost = 0
  let stopReason: 'user' | 'budget' | '' = ''
  for (const target of targets) {
    if (note.value?.id !== noteId) break
    if (citeBatchStopping.value) {
      stopReason = 'user'
      break
    }
    if (budget > 0 && cost >= budget) {
      stopReason = 'budget'
      break
    }
    citeBatchAttempted.value += 1
    citeFillingId.value = target.id
    try {
      const report = await api.productionCiteFillOriginal(target.id)
      cost += report.usage.estimatedCost
      if (report.citation) {
        const index = noteCitations.value.findIndex((item) => item.id === target.id)
        if (index >= 0) noteCitations.value[index] = report.citation
        ok += 1
      } else {
        fail += 1
        citeBatchFailures.value.push({
          quote: target.noteQuote,
          reason: shortReason(report.error || 'AI 没有补到原文'),
        })
      }
    } catch (cause) {
      fail += 1
      citeBatchFailures.value.push({ quote: target.noteQuote, reason: shortReason(api.errorText(cause)) })
    } finally {
      citeFillingId.value = ''
    }
    citeBatchDone.value += 1
  }
  citeBatchRunning.value = false
  citeBatchStopping.value = false
  if (note.value?.id !== noteId) return
  const attempted = citeBatchAttempted.value
  const remaining = targets.length - attempted
  const costText = cost > 0 ? ` · ¥${cost.toFixed(4)}` : ''
  const stopText =
    remaining > 0 && stopReason === 'budget'
      ? `；${remaining} 条未开始（预算上限 ¥${formatBudget(budget)}）`
      : remaining > 0 && stopReason === 'user'
        ? `；${remaining} 条未开始`
        : ''
  if (ok === 0) {
    const prefix =
      stopReason === 'budget' ? 'AI 补齐已到预算上限' : stopReason === 'user' ? 'AI 补齐已停止' : 'AI 补齐失败'
    citeError.value = `${prefix}：0/${attempted} 成功，${fail} 条失败${stopText}`
  } else {
    const prefix =
      stopReason === 'budget' ? 'AI 补齐已到预算上限' : stopReason === 'user' ? 'AI 补齐已停止' : 'AI 补齐完成'
    citeNotice.value = `${prefix}：${ok}/${attempted} 成功${fail ? `，${fail} 条失败` : ''}${costText}${stopText}`
  }
}

function requestDeleteCitation(citation: NoteCitation): void {
  if (citeFillingId.value || citeBatchRunning.value) return
  if (citeDraftDirty.value) {
    citeError.value = '当前还有未保存的原文，先保存或取消'
    return
  }
  citeEditingId.value = ''
  citeDraft.value = ''
  citeAdding.value = false
  citeDeletingId.value = citation.id
  citeError.value = ''
  citeNotice.value = ''
}

function cancelDeleteCitation(): void {
  const id = citeDeletingId.value
  citeDeletingId.value = ''
  citeError.value = ''
  void nextTick(() => document.querySelector<HTMLElement>(`[data-cite-delete="${id}"]`)?.focus())
}

async function deleteCitation(citation: NoteCitation): Promise<void> {
  try {
    await api.productionNoteCitationDelete(citation.id)
    noteCitations.value = noteCitations.value.filter((item) => item.id !== citation.id)
    if (citeEditingId.value === citation.id) cancelCitationEdit()
    citeDeletingId.value = ''
    citeError.value = ''
  } catch (cause) {
    citeDeletingId.value = ''
    citeError.value = api.errorText(cause)
  }
}

function stopTracePoll(): void {
  if (tracePollTimer) window.clearInterval(tracePollTimer)
  tracePollTimer = 0
}

async function refreshTraceJob(): Promise<void> {
  const id = traceTaskId.value
  if (!id) return
  try {
    const detail = await api.productionGet(id)
    if (!detail) return
    traceJob.value = detail.job
    if (!['success', 'error', 'cancelled', 'interrupted'].includes(detail.job.status)) return
    stopTracePoll()
    if (detail.job.status === 'success') {
      const count = detail.job.result?.trace.length ?? 0
      traceNotice.value = count > 0
        ? `溯源完成，已核对 ${count} 条主张；事实已更新`
        : '溯源完成，没有提取到可外部核查的主张'
      if (note.value?.id) await loadFacts(note.value.id)
    } else if (detail.job.status === 'error') {
      traceError.value = detail.job.error || '溯源失败'
    }
  } catch (cause) {
    traceError.value = api.errorText(cause)
    stopTracePoll()
  }
}

function startTracePoll(): void {
  stopTracePoll()
  if (!traceTaskId.value) return
  tracePollTimer = window.setInterval(() => void refreshTraceJob(), 1500)
}

async function restoreActiveTrace(noteId: string): Promise<void> {
  try {
    const jobs = await api.productionList()
    const job = jobs.find((item) =>
      item.templateId === 'note-trace'
      && item.noteId === noteId
      && ['queued', 'running'].includes(item.status),
    )
    if (!job) return
    traceTaskId.value = job.id
    traceJob.value = job
    startTracePoll()
  } catch {
    /* facts still load even when the task list cannot be read */
  }
}

async function startNoteTrace(): Promise<void> {
  const meta = note.value
  const body = detail.value?.body ?? ''
  if (!meta || !body.trim() || traceStarting.value) return
  traceStarting.value = true
  traceError.value = ''
  traceNotice.value = ''
  try {
    const job = await api.productionTraceNote({
      noteId: meta.id,
      notePath: meta.relPath,
      title: meta.title,
      body,
      traceClaims: 10,
      traceMax: 5,
    })
    traceTaskId.value = job.id
    traceJob.value = job
    traceNotice.value = '溯源任务已开始；完成后事实会自动出现'
    startTracePoll()
  } catch (cause) {
    traceError.value = api.errorText(cause)
  } finally {
    traceStarting.value = false
  }
}

async function cancelTrace(): Promise<void> {
  if (!traceTaskId.value) return
  try {
    await api.productionCancel(traceTaskId.value)
  } catch (cause) {
    traceError.value = api.errorText(cause)
  }
}

async function openTraceTask(): Promise<void> {
  if (!traceTaskId.value) return
  await loadProduction()
  productionUi.taskId = traceTaskId.value
  productionUi.view = 'tasks'
  void router.push('/studio')
}

function normalizeFactText(value: string): string {
  return value
    .toLowerCase()
    .replace(/\s+/g, '')
    .replace(/[，。；：、,.!?！？；:()（）"'“”‘’[\]【】]/g, '')
}

function openFactDetail(id: string): void {
  if (!id) return
  const noteId = note.value?.id
  void router.push({
    path: `/facts/${id}`,
    query: noteId ? { from: 'note', noteId } : {},
  })
}

async function restoreFactFocus(id: string): Promise<void> {
  if (!id) return
  railTab.value = 'facts'
  factFilter.value = 'all'
  await nextTick()
  const row = document.querySelector<HTMLElement>(`[data-fact="${id}"]`)
  if (!row) return
  row.scrollIntoView({ behavior: 'smooth', block: 'center' })
  row.classList.add('is-flash')
  window.setTimeout(() => row.classList.remove('is-flash'), 1600)
}

function locateFact(fact: Fact): void {
  factLocateError.value = ''
  const blocks = parsed.value?.allBlocks ?? []
  const needles = [...fact.mentions.map((mention) => mention.quote), fact.text]
    .map(normalizeFactText)
    .filter((value) => value.length >= 4)
  let target: Block | undefined
  for (const needle of needles) {
    target = blocks.find((block) => normalizeFactText(block.text).includes(needle))
    if (!target) target = blocks.find((block) => needle.includes(normalizeFactText(block.text)) && normalizeFactText(block.text).length >= 8)
    if (target) break
  }
  if (!target) {
    factLocateError.value = '没有在正文里定位到这条事实的原句；它可能来自旧版溯源文本。'
    return
  }
  const element = document.querySelector<HTMLElement>(`[data-block="${target.id}"]`)
  if (!element) {
    factLocateError.value = '这块正文当前没有渲染出来。'
    return
  }
  element.classList.add('is-flash')
  element.scrollIntoView({ behavior: 'smooth', block: 'center' })
  window.setTimeout(() => element.classList.remove('is-flash'), 1400)
}

function persistRail(): void {
  window.localStorage.setItem('yebian.rail.width', String(railPreferredWidth.value))
  window.localStorage.setItem('yebian.rail.collapsed', railCollapsed.value ? '1' : '0')
}

function applyRailWidth(value: number): void {
  railPreferredWidth.value = Math.max(RAIL_MIN, Math.min(railMaxWidth(), Math.round(value)))
  window.dispatchEvent(new Event('yb-scroll-rescan'))
}

function toggleRail(): void {
  railCollapsed.value = !railCollapsed.value
  persistRail()
  window.dispatchEvent(new Event('yb-scroll-rescan'))
}

function resetRailWidth(): void {
  applyRailWidth(window.innerWidth <= 1180 ? 330 : 380)
  persistRail()
}

function startRailResize(event: PointerEvent): void {
  event.preventDefault()
  if (railCollapsed.value) return
  railResizing.value = true
  railStartX = event.clientX
  railStartWidth = railWidth.value
  document.body.classList.add('is-resizing-panel')
  window.addEventListener('pointermove', onRailMove)
  window.addEventListener('pointerup', onRailUp, { once: true })
  window.addEventListener('pointercancel', onRailUp, { once: true })
}

function onRailMove(event: PointerEvent): void {
  if (!railResizing.value) return
  applyRailWidth(railStartWidth + railStartX - event.clientX)
}

function onRailUp(): void {
  if (!railResizing.value) return
  railResizing.value = false
  document.body.classList.remove('is-resizing-panel')
  window.removeEventListener('pointermove', onRailMove)
  window.removeEventListener('pointercancel', onRailUp)
  persistRail()
  window.dispatchEvent(new Event('yb-scroll-rescan'))
}

function nudgeRail(delta: number): void {
  if (railCollapsed.value) return
  applyRailWidth(railWidth.value + delta)
  persistRail()
}

async function load(id: string): Promise<void> {
  if (!id) return
  loading.value = true
  error.value = ''
  resetPending()
  stopTracePoll()
  traceTaskId.value = ''
  traceJob.value = null
  traceStarting.value = false
  traceError.value = ''
  traceNotice.value = ''
  relocateNotice.value = ''
  relocateError.value = ''
  try {
    const data = await api.getNote(id)
    if (!data) {
      error.value = '没有找到这篇笔记。'
      detail.value = null
      parsed.value = null
      annotations.value = []
      facts.value = []
      noteCitations.value = []
      return
    }
    detail.value = data
    parsed.value = parseNote(data.body)
    factLocateError.value = ''
    await Promise.all([loadAnnotations(id), loadFacts(id), loadCitations(id)])
    void restoreActiveTrace(id)
    if (annotations.value.length) railTab.value = 'notes'
    await nextTick()
    docScroll.value?.scrollTo({ top: 0 })
    const factFocus = String(route.query.fact ?? '')
    if (factFocus) await restoreFactFocus(factFocus)
    if (!readyReported) {
      readyReported = true
      void api.appReady().catch(() => {})
    }
  } catch (cause) {
    error.value = api.errorText(cause)
  } finally {
    loading.value = false
  }
}

watch(
  [validAnnotations, pendingSegments],
  () => {
    void nextTick(() => {
      paintHighlights()
      layoutRailNotes()
    })
  },
  { deep: true },
)

watch(
  railTab,
  () => {
    if (railTab.value === 'notes' && railBody.value) railBody.value.scrollTop = 0
    void nextTick(layoutRailNotes)
  },
  { flush: 'post' },
)

watch(
  () => route.params.id,
  (id) => {
    if (id) {
      void load(String(id))
    }
  },
  { immediate: true },
)

watch(
  () => library.notes.length,
  () => {
    if (!route.params.id && library.notes.length) {
      void router.replace(`/note/${library.notes[0].id}`)
    }
  },
  { immediate: true },
)

function resetPending(): void {
  window.getSelection()?.removeAllRanges()
  pendingSegments.value = []
  pendingQuote.value = ''
  pendingBlockId.value = ''
  pendingBody.value = ''
  pendingColor.value = 'gold'
  paintHighlights()
}

function blockEl(blockId: string): HTMLElement | null {
  return document.querySelector<HTMLElement>(`[data-block="${CSS.escape(blockId)}"]`)
}

function offsetInBlock(block: HTMLElement, container: Node, offset: number): number {
  let total = 0
  const walker = document.createTreeWalker(block, NodeFilter.SHOW_TEXT)
  let node: Node | null
  while ((node = walker.nextNode())) {
    if (node === container) return total + offset
    if (node.contains(container)) return total
    total += (node.textContent ?? '').length
  }
  return total
}

function segmentsFromRange(range: Range): AnnotationSegment[] {
  const out: AnnotationSegment[] = []
  for (const block of document.querySelectorAll<HTMLElement>('.doc [data-block]')) {
    if (!range.intersectsNode(block)) continue
    const text = block.textContent ?? ''
    const inside = document.createRange()
    inside.selectNodeContents(block)
    const start = range.compareBoundaryPoints(Range.START_TO_START, inside) <= 0
      ? 0
      : offsetInBlock(block, range.startContainer, range.startOffset)
    const end = range.compareBoundaryPoints(Range.END_TO_END, inside) >= 0
      ? text.length
      : offsetInBlock(block, range.endContainer, range.endOffset)
    if (end > start) out.push({ blockId: block.dataset.block ?? '', start, end })
  }
  return out
}

function revealComposer(): void {
  const body = railBody.value
  const composer = railAnnoLayer.value?.querySelector<HTMLElement>('.anno-composer')
  const scroller = docScroll.value
  if (!body || !composer || !scroller) return
  const bodyRect = body.getBoundingClientRect()
  const rect = composer.getBoundingClientRect()
  const below = rect.bottom + 16 - bodyRect.bottom
  const above = bodyRect.top + 8 - rect.top
  if (below > 0) scroller.scrollBy({ top: below, behavior: 'smooth' })
  else if (above > 0) scroller.scrollBy({ top: -above, behavior: 'smooth' })
}

function captureSelection(): void {
  const selection = window.getSelection()
  if (!selection || selection.isCollapsed || !selection.rangeCount) return
  const range = selection.getRangeAt(0)
  const doc = docScroll.value
  if (!doc?.contains(range.commonAncestorContainer)) return
  const startNode = range.startContainer.nodeType === Node.TEXT_NODE ? range.startContainer.parentElement : range.startContainer as HTMLElement
  const endNode = range.endContainer.nodeType === Node.TEXT_NODE ? range.endContainer.parentElement : range.endContainer as HTMLElement
  if (!startNode?.closest('[data-block]') || !endNode?.closest('[data-block]')) return
  const segments = segmentsFromRange(range)
  const quote = range.toString().replace(/\s+/g, ' ').trim()
  if (!segments.length || quote.length < 1) return
  pendingSegments.value = segments
  pendingQuote.value = quote
  pendingBlockId.value = segments[0].blockId
  pendingBody.value = ''
  pendingColor.value = 'gold'
  activeAnnotationId.value = ''
  selection.removeAllRanges()
  railTab.value = 'notes'
  paintHighlights()
  void nextTick(() => {
    layoutRailNotes()
    document.querySelector<HTMLInputElement>('.anno-input')?.focus({ preventScroll: true })
    window.setTimeout(revealComposer, 80)
  })
}

function clearAnnotationMarks(): void {
  for (const mark of document.querySelectorAll<HTMLElement>('.anno-hl, .anno-pending-mark')) {
    const parent = mark.parentNode
    if (!parent) continue
    while (mark.firstChild) parent.insertBefore(mark.firstChild, mark)
    parent.removeChild(mark)
    parent.normalize()
  }
}

function wrapSegment(segment: AnnotationSegment, className: string, annoId: string, color = ''): void {
  const block = blockEl(segment.blockId)
  if (!block) return
  const start = Math.max(0, segment.start)
  const end = Math.max(start, segment.end)
  if (end <= start) return
  const textNodes: Array<{ node: Text; start: number; end: number }> = []
  const walker = document.createTreeWalker(block, NodeFilter.SHOW_TEXT)
  let cursor = 0
  let node: Node | null
  while ((node = walker.nextNode())) {
    const text = node as Text
    const length = text.data.length
    const nodeStart = cursor
    const nodeEnd = cursor + length
    if (nodeEnd > start && nodeStart < end) {
      textNodes.push({
        node: text,
        start: Math.max(0, start - nodeStart),
        end: Math.min(length, end - nodeStart),
      })
    }
    cursor = nodeEnd
  }
  for (const item of textNodes.reverse()) {
    let target = item.node
    if (item.start > 0) target = target.splitText(item.start)
    if (item.end - item.start < target.data.length) target.splitText(item.end - item.start)
    const mark = document.createElement('mark')
    mark.className = `${className}${className === 'anno-hl' ? ` is-${color || 'gold'}` : ''}`.trim()
    if (className === 'anno-hl') mark.dataset.anno = annoId
    target.parentNode?.insertBefore(mark, target)
    mark.appendChild(target)
  }
}

function setHoveredAnnotation(id: string): void {
  if (hoverAnnotationId.value === id) return
  hoverAnnotationId.value = id
  for (const mark of document.querySelectorAll<HTMLElement>('.anno-hl')) {
    mark.classList.toggle('is-hover', Boolean(id) && mark.dataset.anno === id)
  }
}

function onDocMouseOver(event: MouseEvent): void {
  const mark = (event.target as Element | null)?.closest<HTMLElement>('.anno-hl')
  setHoveredAnnotation(mark?.dataset.anno ?? '')
}

function segmentTop(segment: AnnotationSegment): number | null {
  const block = blockEl(segment.blockId)
  if (!block) return null
  const walker = document.createTreeWalker(block, NodeFilter.SHOW_TEXT)
  let cursor = 0
  let node: Node | null
  while ((node = walker.nextNode())) {
    const text = node as Text
    const next = cursor + text.data.length
    if (segment.end > cursor && segment.start < next) {
      const range = document.createRange()
      range.setStart(text, Math.max(0, segment.start - cursor))
      range.setEnd(text, Math.min(text.data.length, segment.end - cursor))
      return range.getBoundingClientRect().top
    }
    cursor = next
  }
  return block.getBoundingClientRect().top
}

function paintHighlights(): void {
  clearAnnotationMarks()
  for (const anno of validAnnotations.value) {
    const segments = anno.segments.length
      ? anno.segments
      : [{ blockId: anno.blockId, start: 0, end: blockEl(anno.blockId)?.textContent?.length ?? 0 }]
    for (const segment of segments) {
      wrapSegment(segment, 'anno-hl', anno.id, anno.color)
    }
  }
  const active = validAnnotations.value.find((anno) => anno.id === activeAnnotationId.value)
  if (active) {
    for (const mark of document.querySelectorAll<HTMLElement>(`.anno-hl[data-anno="${CSS.escape(active.id)}"]`)) {
      mark.classList.add('is-active')
    }
  }
  const hovered = hoverAnnotationId.value
  if (hovered) {
    for (const mark of document.querySelectorAll<HTMLElement>(`.anno-hl[data-anno="${CSS.escape(hovered)}"]`)) {
      mark.classList.add('is-hover')
    }
  }
  for (const segment of pendingSegments.value) {
    wrapSegment(segment, 'anno-pending-mark', '')
  }
}

function onDocMouseUp(): void {
  window.setTimeout(captureSelection, 0)
}

function onDocClick(event: MouseEvent): void {
  const mark = (event.target as Element | null)?.closest<HTMLElement>('.anno-hl')
  if (mark?.dataset.anno) {
    if (pendingSegments.value.length) resetPending()
    activeAnnotationId.value = mark.dataset.anno
    railTab.value = 'notes'
    paintHighlights()
    return
  }
  if (pendingSegments.value.length) {
    resetPending()
    return
  }
  const caret = (document as Document & { caretPositionFromPoint?: (x: number, y: number) => { offsetNode: Node; offset: number } | null }).caretPositionFromPoint?.(event.clientX, event.clientY)
  if (!caret) return
  const element = caret.offsetNode.nodeType === Node.TEXT_NODE ? caret.offsetNode.parentElement : caret.offsetNode as HTMLElement
  const block = element?.closest<HTMLElement>('[data-block]')
  if (!block) return
  const offset = offsetInBlock(block, caret.offsetNode, caret.offset)
  const hit = validAnnotations.value.find((anno) => {
    const segments = anno.segments.length
      ? anno.segments
      : [{ blockId: anno.blockId, start: 0, end: block.textContent?.length ?? 0 }]
    return segments.some((segment) => segment.blockId === block.dataset.block && segment.start <= offset && offset <= segment.end)
  })
  if (hit) {
    activeAnnotationId.value = hit.id
    railTab.value = 'notes'
    paintHighlights()
  }
}

async function saveAnnotation(): Promise<void> {
  if (!pendingSegments.value.length || !note.value) return
  try {
    await api.addAnnotation({
      noteId: note.value.id,
      blockId: pendingBlockId.value,
      quote: pendingQuote.value,
      body: pendingBody.value.trim(),
      kind: '',
      segments: pendingSegments.value,
      color: pendingColor.value,
    })
    await loadAnnotations(note.value.id)
    await refreshNotes()
    resetPending()
  } catch (cause) {
    error.value = api.errorText(cause)
  }
}

async function removeAnnotation(id: string): Promise<void> {
  if (!note.value) return
  try {
    await api.deleteAnnotation(id)
    await loadAnnotations(note.value.id)
    await refreshNotes()
  } catch (cause) {
    error.value = api.errorText(cause)
  }
}

async function relocateOrphans(): Promise<void> {
  if (!note.value || !parsed.value || !orphans.value.length) return
  relocating.value = true
  relocateNotice.value = ''
  relocateError.value = ''
  try {
    const plan = planAnchorRelocations(
      orphans.value.map((item) => ({ id: item.id, quote: item.quote })),
      parsed.value.allBlocks,
    )
    if (!plan.matches.length) {
      relocateNotice.value = `没有找到足够确定的匹配；${plan.unresolved} 条标注仍保留原文引用。`
      return
    }
    const count = await api.relocateAnnotations(
      note.value.id,
      plan.matches.map((item) => ({ id: item.id, blockId: item.blockId })),
    )
    await loadAnnotations(note.value.id)
    const remaining = Math.max(0, orphans.value.length)
    relocateNotice.value = `已重新定位 ${count} 条；${remaining} 条仍需手动处理。`
  } catch (cause) {
    relocateError.value = api.errorText(cause)
  } finally {
    relocating.value = false
  }
}

function flash(el: Element | null): void {
  if (!el) return
  el.classList.add('is-flash')
  window.setTimeout(() => el.classList.remove('is-flash'), 1200)
}

function locateBlock(blockId: string): void {
  const el = document.querySelector(`[data-block="${blockId}"]`)
  if (!el) return
  const scroller = docScroll.value
  if (!scroller) return
  const rect = el.getBoundingClientRect()
  const view = scroller.getBoundingClientRect()
  const margin = 72
  const visible = rect.top >= view.top + margin && rect.bottom <= view.bottom - margin
  if (!visible) {
    const offset = rect.top - view.top
    const reduce = window.matchMedia('(prefers-reduced-motion: reduce)').matches
    scroller.scrollTo({
      top: scroller.scrollTop + offset - view.height * 0.28,
      behavior: reduce ? 'auto' : 'smooth',
    })
  }
  if (visible) flash(el)
  else window.setTimeout(() => flash(el), 180)
}

function selectAnnotation(anno: Annotation): void {
  activeAnnotationId.value = anno.id
  railTab.value = 'notes'
  const first = anno.segments[0]
  locateBlock(first?.blockId ?? anno.blockId)
  paintHighlights()
}

async function copyPath(): Promise<void> {
  if (!note.value) return
  try {
    await writeText(note.value.path)
  } catch {
    await navigator.clipboard.writeText(note.value.path)
  }
  pathCopied.value = true
  window.clearTimeout(pathCopyTimer)
  pathCopyTimer = window.setTimeout(() => {
    pathCopied.value = false
  }, 1200)
}

let railLayoutFrame = 0

function layoutRailNotes(): void {
  if (railTab.value !== 'notes') return
  const body = railBody.value
  const layer = railAnnoLayer.value
  if (!body || !layer) return
  const bodyTop = body.getBoundingClientRect().top
  const items: Array<{ top: number; el: HTMLElement; priority: number }> = []
  const composer = layer.querySelector<HTMLElement>('.anno-composer')
  const firstPending = pendingSegments.value[0]
  if (composer && firstPending) {
    const top = segmentTop(firstPending)
    if (top !== null) items.push({ top: top - bodyTop, el: composer, priority: 0 })
  }
  for (const anno of validAnnotations.value) {
    const card = layer.querySelector<HTMLElement>(`[data-anno="${CSS.escape(anno.id)}"]`)
    const first = anno.segments[0]
    const top = first ? segmentTop(first) : null
    if (!card || top === null) continue
    items.push({ top: top - bodyTop, el: card, priority: 1 })
  }
  items.sort((left, right) => left.top - right.top || left.priority - right.priority)
  let bottom = 0
  for (const item of items) {
    const top = Math.max(item.top, bottom)
    item.el.style.top = `${Math.round(top)}px`
    bottom = top + item.el.offsetHeight + 12
  }
}

function scheduleRailLayout(): void {
  if (railLayoutFrame) return
  railLayoutFrame = requestAnimationFrame(() => {
    railLayoutFrame = 0
    layoutRailNotes()
  })
}

function onDocScroll(): void {
  scheduleRailLayout()
  updateTocActive()
}

const tocItems = computed(() =>
  (parsed.value?.sections ?? [])
    .map((section, index) => ({ heading: section.heading, index }))
    .filter((item) => item.heading),
)
const tocActive = ref(0)

function updateTocActive(): void {
  const scroller = docScroll.value
  if (!scroller || railTab.value !== 'toc') return
  const sections = scroller.querySelectorAll<HTMLElement>('.doc .sec')
  const top = scroller.getBoundingClientRect().top + 96
  let active = 0
  sections.forEach((element, index) => {
    if (element.getBoundingClientRect().top <= top) active = index
  })
  tocActive.value = active
}

function scrollToSection(index: number): void {
  const scroller = docScroll.value
  const target = scroller?.querySelectorAll<HTMLElement>('.doc .sec')[index]
  if (!scroller || !target) return
  const offset = target.getBoundingClientRect().top - scroller.getBoundingClientRect().top
  const reduce = window.matchMedia('(prefers-reduced-motion: reduce)').matches
  scroller.scrollTo({
    top: scroller.scrollTop + offset - 18,
    behavior: reduce ? 'auto' : 'smooth',
  })
  tocActive.value = index
}

function onRailWheel(event: WheelEvent): void {
  if (railTab.value !== 'notes' || event.ctrlKey) return
  if ((event.target as Element | null)?.closest('.rail-orphans')) return
  event.preventDefault()
  docScroll.value?.scrollBy({ top: event.deltaY })
}

let railObserver: ResizeObserver | null = null

function onViewportResize(): void {
  viewportWidth.value = window.innerWidth
  window.dispatchEvent(new Event('yb-scroll-rescan'))
  void nextTick(layoutRailNotes)
}

onMounted(() => {
  const body = railBody.value
  if (body) {
    railObserver = new ResizeObserver(() => scheduleRailLayout())
    railObserver.observe(body)
  }
  void document.fonts?.ready.then(() => {
    if (!docScroll.value) return
    paintHighlights()
    layoutRailNotes()
  })
  window.addEventListener('resize', onViewportResize)
})

onBeforeUnmount(() => {
  railObserver?.disconnect()
  if (railLayoutFrame) cancelAnimationFrame(railLayoutFrame)
  window.clearTimeout(pathCopyTimer)
  window.clearTimeout(orphanDeleteTimer)
  stopTracePoll()
  window.removeEventListener('resize', onViewportResize)
  window.removeEventListener('pointermove', onRailMove)
  window.removeEventListener('pointercancel', onRailUp)
  document.body.classList.remove('is-resizing-panel')
})
</script>

<template>
  <div class="note-layout" :class="{ 'rail-collapsed': railCollapsed, 'is-resizing': railResizing }" :style="railStyle">
    <div
      ref="docScroll"
      class="doc-scroll"
      @mouseup="onDocMouseUp"
      @click="onDocClick"
      @mouseover="onDocMouseOver"
      @mouseleave="setHoveredAnnotation('')"
      @scroll.passive="onDocScroll"
    >
      <article v-if="detail && parsed" class="doc">
        <div class="doc-main">
        <header>
          <h1 class="note-title"><span v-for="(part, index) in titleParts" :key="index" class="note-title-part">{{ part }}</span></h1>
        </header>

        <div v-if="parsed.lead.length" class="lead-zone">
          <NoteBlock
            v-for="block in parsed.lead"
            :key="block.id"
            :block="block"
          />
        </div>

        <div class="meta note-byline">
          <span v-if="note?.chars" class="byline-count">{{ note.chars.toLocaleString('zh-CN') }} 字</span>
          <span v-if="annotations.length">{{ annotations.length }} 条标注</span>
          <span v-if="bylineAuthor" class="byline-author" :title="authorTag">{{ bylineAuthor }}</span>
          <span v-if="artifact?.source?.url" class="byline-prov">
            <button
              type="button"
              class="byline-link"
              :title="[artifact.source.author, artifact.title].filter(Boolean).join(' · ')"
              @click="openSourceVideo"
            >
              {{ sourceLinkLabel }} ↗
            </button>
          </span>
        </div>
        <div v-if="visibleTags.length" class="tags">
          <router-link
            v-for="tag in visibleTags"
            :key="tag"
            class="tag"
            :to="{ path: '/tags', query: { tag } }"
          >
            {{ tag }}
          </router-link>
        </div>

        <section v-if="parsed.summary.length" class="takes">
          <div class="takes-head">
            <b>速览</b>
            <span>{{ parsed.summary.length }} 条判断</span>
          </div>
          <ol>
            <li v-for="(block, index) in parsed.summary" :key="block.id" class="take">
              <span class="take-n">{{ pad(index + 1) }}</span>
              <NoteBlock
                :block="block"
                              />
            </li>
          </ol>
        </section>

        <section
          v-for="(section, index) in parsed.sections"
          :key="`${section.heading}-${index}`"
          class="sec"
          :data-sec="index"
          :class="{
            'is-onepage': section.heading === '一页看懂',
            'is-cheat': section.heading === '三句话' || section.heading === '自检问题',
          }"
        >
          <span class="sec-no">§ {{ pad(index + 1) }}</span>
          <h2 v-if="section.heading">{{ section.heading }}</h2>
          <NoteBlock
            v-for="block in section.blocks"
            :key="block.id"
            :block="block"
          />
        </section>

        <section v-if="parsed.glossary.length" class="gloss">
          <div class="gloss-head">
            <h2>速查</h2>
            <span>{{ parsed.glossary.length }} 条</span>
          </div>
          <template v-for="group in glossaryGroups" :key="group.cat">
            <div class="gloss-cat">{{ group.cat }}</div>
            <div class="gloss-grid">
              <div v-for="item in group.items" :key="item.block.id" class="gloss-item">
                <NoteBlock
                  :block="item.block"
                            />
              </div>
            </div>
          </template>
        </section>

        <div v-if="error" class="empty" style="margin-top: 18px">{{ error }}</div>
        </div>
      </article>

      <div v-else-if="loading" class="loading">正在打开…</div>
      <div v-else class="loading">{{ error || '没有这篇笔记' }}</div>
    </div>

    <div
      class="rail-resizer"
      role="separator"
      aria-orientation="vertical"
      aria-label="调整右栏宽度"
      tabindex="0"
      title="拖动调整宽度，双击复位"
      @pointerdown="startRailResize"
      @dblclick="resetRailWidth"
      @keydown.left.prevent="nudgeRail(16)"
      @keydown.right.prevent="nudgeRail(-16)"
      @keydown.home.prevent="resetRailWidth"
    />

    <aside class="rail">
      <button type="button" class="rail-expand-edge" aria-label="展开右栏" title="展开右栏" @click="toggleRail">
        <PanelRightOpen :size="15" />
      </button>
      <div class="rail-top">
        <div class="rail-title">
          <b>本页</b>
          <button type="button" class="rail-collapse" aria-label="收起右栏" title="收起右栏" @click="toggleRail">
            <PanelRightClose :size="14" />
          </button>
        </div>
        <div class="rail-tabs" role="tablist" aria-label="笔记侧栏">
          <button type="button" role="tab" :aria-selected="railTab === 'citations'" :class="{ 'is-active': railTab === 'citations' }" @click="railTab = 'citations'">
            出处<span>{{ noteCitations.length }}</span>
          </button>
          <button type="button" role="tab" :aria-selected="railTab === 'notes'" :class="{ 'is-active': railTab === 'notes' }" @click="railTab = 'notes'">
            标注<span>{{ annotations.length }}</span>
          </button>
          <button type="button" role="tab" :aria-selected="railTab === 'links'" :class="{ 'is-active': railTab === 'links' }" @click="railTab = 'links'">
            关联
          </button>
          <button v-if="tocItems.length" type="button" role="tab" :aria-selected="railTab === 'toc'" :class="{ 'is-active': railTab === 'toc' }" @click="railTab = 'toc'; updateTocActive()">
            目录<span>{{ tocItems.length }}</span>
          </button>
          <button type="button" role="tab" :aria-selected="railTab === 'info'" :class="{ 'is-active': railTab === 'info' }" @click="railTab = 'info'">
            信息
          </button>
        </div>
      </div>

      <div
        ref="railBody"
        class="rail-body"
        :class="{ 'is-anno-margin': railTab === 'notes' }"
        @wheel="onRailWheel"
      >
        <section v-if="railTab === 'citations'" class="rail-panel" role="tabpanel" aria-label="出处">
          <div class="rail-cite-toolbar">
            <span>{{ noteCitations.length }} 条出处<template v-if="pendingCitationCount"> · {{ pendingCitationCount }} 条待补原文</template></span>
            <span class="rail-cite-tools">
              <button type="button" class="rail-cite-action" :disabled="citeBatchRunning || Boolean(citeFillingId)" @click="beginCiteAdd">加出处</button>
              <button type="button" class="rail-cite-action" :disabled="citeRunning || Boolean(citeFillingId) || citeBatchRunning" @click="startCite">
                <LoaderCircle v-if="citeRunning" :size="12" class="rail-fact-spin" />
                {{ citeRunning ? '正在找…' : noteCitations.length ? '重新找出处' : '找出处' }}
              </button>
            </span>
          </div>
          <div v-if="citeBatchCount" class="rail-cite-batch">
            <div class="rail-cite-batch-row">
              <button
                type="button"
                class="rail-cite-batch-action"
                data-cite-batch-button
                :disabled="citeBatchRunning || citeRunning || Boolean(citeFillingId)"
                @click="fillAllCandidates"
              >
                <LoaderCircle v-if="citeBatchRunning" :size="12" class="rail-fact-spin" />
                {{ citeBatchRunning ? `AI 补齐 ${citeBatchDone}/${citeBatchTotal}…` : `AI 补齐候选 ${citeBatchCount}` }}
              </button>
              <label v-if="!citeBatchRunning" class="rail-cite-budget" title="跑到上限后不再开始下一条；单条可能略超">
                预算 ¥<input v-model.number="citeBatchBudget" data-cite-batch-budget type="number" min="0" step="0.01" inputmode="decimal" aria-label="本次批量预算上限（元）" />
              </label>
              <template v-else>
                <span class="rail-cite-batch-cap">上限 ¥{{ formatBudget(citeBatchBudget) }}</span>
                <button type="button" class="rail-cite-batch-stop" data-cite-batch-stop :disabled="citeBatchStopping" @click="stopCiteBatch">
                  {{ citeBatchStopping ? '停止中…' : '停止' }}
                </button>
              </template>
            </div>
            <ul v-if="citeBatchFailures.length" class="rail-cite-batch-failures">
              <li v-for="(failure, index) in citeBatchFailures.slice(0, 3)" :key="`${index}-${failure.quote}`">
                <span>{{ failure.quote }}</span>
                <em>{{ failure.reason }}</em>
              </li>
              <li v-if="citeBatchFailures.length > 3" class="rail-cite-batch-more">另有 {{ citeBatchFailures.length - 3 }} 条失败，可单条重试</li>
            </ul>
          </div>
          <p v-if="pendingCitationCount && !citeAdding" class="rail-cite-hint">{{ citationHint }}</p>
          <p v-if="citeNotice" class="rail-cite-notice">{{ citeNotice }}</p>
          <div v-if="citeAdding" class="rail-cite-add" @keydown.esc.prevent="cancelCiteAdd">
            <label>笔记原句<textarea v-model="citeForm.noteQuote" placeholder="从正文复制一句话…"></textarea></label>
            <label>书名<input v-model="citeForm.workTitle" placeholder="例如《三国志》" /></label>
            <label>来源链接<input v-model="citeForm.url" placeholder="可选，打开原文的链接" /></label>
            <label>史料原文<textarea v-model="citeForm.originalText" placeholder="可选；填了就标为有原文"></textarea></label>
            <div class="rail-cite-edit-actions">
              <button type="button" class="rail-cite-save" :disabled="!citeForm.noteQuote.trim() || !citeForm.workTitle.trim()" @click="saveCiteAdd">保存出处</button>
              <button type="button" @click="cancelCiteAdd">取消</button>
            </div>
          </div>
          <p v-if="citeError" class="rail-fact-error">{{ citeError }}</p>
          <div v-if="noteCitations.length" class="rail-cite-list">
            <article v-for="citation in noteCitations" :key="citation.id" class="rail-cite-row">
              <p class="rail-cite-note">{{ citation.noteQuote }}</p>
              <div class="rail-cite-source">
                <b>{{ citationWorkLabel(citation.workTitle) || '出处待定' }}</b>
                <span v-if="citation.author">{{ citation.author }}</span>
                <span v-if="citation.volume">{{ citation.volume }}</span>
                <span v-if="citation.page">{{ citation.page }}</span>
                <span v-if="citationQualityLabel(citation.url)" class="rail-cite-quality">{{ citationQualityLabel(citation.url) }}</span>
                <em class="rail-cite-state" :class="citation.status === 'found' ? 'is-found' : 'is-pending'">
                  {{ citation.status === 'found' ? '已找到' : '待补原文' }}
                </em>
              </div>
              <span v-if="citeEditingId !== citation.id && citeDeletingId !== citation.id" class="rail-cite-actions">
                <button
                  v-if="citation.status === 'candidate' && citationQualityLabel(citation.url) !== '目录'"
                  type="button"
                  :disabled="Boolean(citeFillingId) || citeBatchRunning"
                  :title="citation.url ? '从链接抓取原文' : '先联网检索来源，再抓取原文'"
                  @click="fillCitationOriginal(citation)"
                >
                  <LoaderCircle v-if="citeFillingId === citation.id" :size="12" class="rail-fact-spin" />
                  {{ citeFillingId === citation.id ? 'AI 找原文…' : citation.url ? 'AI 补原文' : 'AI 找来源' }}
                </button>
                <button v-if="citation.status === 'candidate'" type="button" :data-cite-edit="citation.id" :disabled="Boolean(citeFillingId) || citeBatchRunning" @click="beginCitationEdit(citation)">手动补原文</button>
                <button v-else-if="citation.originalText" type="button" :data-cite-edit="citation.id" :disabled="Boolean(citeFillingId) || citeBatchRunning" @click="beginCitationEdit(citation)">改原文</button>
                <button v-if="citation.url" type="button" @click="openCitation(citation)">打开来源 ↗</button>
                <button type="button" class="rail-cite-delete" :data-cite-delete="citation.id" :disabled="Boolean(citeFillingId) || citeBatchRunning" @click="requestDeleteCitation(citation)">删除</button>
              </span>
              <div v-if="citeDeletingId === citation.id" class="rail-cite-confirm">
                <span>删除这条出处？</span>
                <button type="button" class="rail-cite-confirm-delete" @click="deleteCitation(citation)">删除</button>
                <button type="button" @click="cancelDeleteCitation">取消</button>
              </div>
              <div v-if="citeEditingId === citation.id" class="rail-cite-edit">
                <textarea
                  v-model="citeDraft"
                  placeholder="从来源复制原文，粘贴到这里…"
                  @keydown.esc.prevent="cancelCitationEdit"
                  @keydown.ctrl.enter.prevent="saveCitationOriginal(citation)"
                  @keydown.meta.enter.prevent="saveCitationOriginal(citation)"
                ></textarea>
                <div class="rail-cite-edit-actions">
                  <button type="button" class="rail-cite-save" :disabled="!citeDraft.trim()" @click="saveCitationOriginal(citation)">保存原文</button>
                  <button type="button" @click="cancelCitationEdit">取消</button>
                </div>
              </div>
              <div v-else-if="citation.originalText" class="rail-cite-original-wrap">
                <p
                  class="rail-cite-original"
                  :class="{ 'is-collapsed': isLongOriginal(citation.originalText) && !isCiteExpanded(citation.id) }"
                >{{ citation.originalText }}</p>
                <button
                  v-if="isLongOriginal(citation.originalText)"
                  type="button"
                  class="rail-cite-expand"
                  :data-cite-expand="citation.id"
                  :aria-expanded="isCiteExpanded(citation.id)"
                  @click="toggleCiteOriginal(citation.id)"
                >{{ isCiteExpanded(citation.id) ? '收起原文' : '展开原文' }}</button>
              </div>
            </article>
          </div>
          <div v-else class="rail-fact-empty">
            <strong>还没有出处</strong>
            <p>找出这篇笔记里提到的人物、事件和引文，关联到书名和链接。</p>
            <button type="button" class="rail-fact-add" :disabled="citeRunning" @click="startCite">
              <LoaderCircle v-if="citeRunning" :size="12" class="rail-fact-spin" />
              {{ citeRunning ? '正在找…' : '找出处' }}
            </button>
          </div>
        </section>

        <section v-else-if="railTab === 'facts'" class="rail-panel" role="tabpanel" aria-label="事实">
          <div class="rail-fact-toolbar">
            <span>{{ verifiedFactCount }} 有证据 · {{ attentionFactCount }} 需关注</span>
            <span class="rail-fact-tools">
              <button type="button" :class="{ 'is-active': factFilter === 'attention' }" @click="factFilter = factFilter === 'attention' ? 'all' : 'attention'">
                {{ factFilter === 'attention' ? '全部事实' : '只看需关注' }}
              </button>
              <button type="button" class="rail-fact-add" :disabled="traceStarting || traceRunning" @click="startNoteTrace">
                <LoaderCircle v-if="traceStarting" :size="12" class="rail-fact-spin" />
                <Sparkles v-else :size="12" />
                {{ traceStarting ? '创建中' : traceRunning ? '溯源中' : '跑溯源' }}
              </button>
            </span>
          </div>
          <p v-if="factLocateError" class="fact-locate-note is-error">{{ factLocateError }}</p>
          <p v-if="traceError && !traceTaskId" class="rail-fact-error">{{ traceError }}</p>

          <div v-if="traceTaskId" class="rail-trace-card" :class="{ 'is-running': traceRunning, 'is-error': traceJob?.status === 'error', 'is-success': traceJob?.status === 'success' }">
            <div class="rail-trace-head">
              <b>溯源任务</b>
              <span>{{ traceStatusText }}</span>
            </div>
            <div v-if="traceRunning" class="rail-trace-progress"><i :style="{ width: `${traceJob?.progress ?? 0}%` }" /></div>
            <p v-if="traceRunning">正在{{ traceJob?.currentStep || '准备' }} · {{ traceJob?.progress ?? 0 }}%</p>
            <p v-else-if="traceNotice">{{ traceNotice }}</p>
            <p v-else-if="traceError" class="rail-fact-error">{{ traceError }}</p>
            <div class="rail-trace-actions">
              <button type="button" class="btn" @click="openTraceTask">查看任务</button>
              <button v-if="traceRunning" type="button" class="btn" @click="cancelTrace">取消</button>
              <button v-else type="button" class="btn" :disabled="traceStarting" @click="startNoteTrace">再跑一次</button>
            </div>
          </div>

          <div v-if="railFacts.length" class="rail-fact-list">
            <article
              v-for="fact in railFacts"
              :key="fact.id"
              :data-fact="fact.id"
              class="rail-fact-row"
              :class="factStatusClass(fact)"
              role="button"
              tabindex="0"
              @click="openFactDetail(fact.id)"
              @keydown.enter="openFactDetail(fact.id)"
              @keydown.space.prevent="openFactDetail(fact.id)"
            >
              <div class="rail-fact-top">
                <span v-if="fact.domain === 'history' && railShowsDomain" class="rail-fact-domain">历史</span>
                <span class="rail-fact-top-actions">
                  <button type="button" class="rail-fact-locate" title="在正文里定位" aria-label="在正文里定位" @click.stop="locateFact(fact)">
                    <Crosshair :size="13" />
                  </button>
                  <ChevronRight :size="13" class="rail-fact-arrow" aria-hidden="true" />
                </span>
              </div>
              <p class="rail-fact-text">{{ fact.text }}</p>
              <div class="rail-fact-meta">
                <span v-if="factRailState(fact)" class="rail-fact-state" :class="factStatusClass(fact)">
                  <i aria-hidden="true"></i>{{ factRailState(fact) }}
                </span>
                <span v-if="factRailState(fact)" class="rail-fact-sep" aria-hidden="true">·</span>
                <span>{{ factSourceSummary(fact) }}</span>
                <span v-if="fact.related.length">· {{ fact.related.length }} 个关联</span>
              </div>
            </article>
          </div>
          <div v-else-if="!traceTaskId" class="rail-fact-empty">
            <Sparkles :size="18" />
            <strong>{{ facts.length ? '没有需要关注的事实' : '这篇笔记还没有事实' }}</strong>
            <p v-if="facts.length">切回全部事实，或再跑一次溯源。</p>
            <p v-else>跑一次溯源：从正文抽取可核查主张，联网核对后把事实和来源写入这里。</p>
            <button v-if="facts.length" type="button" class="btn" @click="factFilter = 'all'">查看全部事实</button>
            <button v-else type="button" class="btn btn-primary" :disabled="traceStarting" @click="startNoteTrace">
              <LoaderCircle v-if="traceStarting" :size="12" class="rail-fact-spin" />
              {{ traceStarting ? '正在创建…' : '跑溯源' }}
            </button>
            <p class="rail-fact-empty-note">会调用模型和联网检索，产生用量；不会重写笔记正文。</p>
          </div>
        </section>

        <section v-else-if="railTab === 'notes'" class="rail-panel rail-anno-panel" role="tabpanel" aria-label="标注">
          <div ref="railAnnoLayer" class="rail-anno-layer">
            <AnnotationComposer
              v-if="pendingSegments.length"
              :quote="pendingQuote"
              :color="pendingColor"
              :body="pendingBody"
              @color="pendingColor = $event"
              @body="pendingBody = $event"
              @save="saveAnnotation"
              @cancel="resetPending"
            />

            <div v-if="!annotations.length && !pendingSegments.length" class="empty rail-anno-empty">
              选中正文里的一句话，标注会出现在这里。
            </div>

            <AnnotationCard
              v-for="(anno, index) in validAnnotations"
              :key="anno.id"
              class="anno-rail-card"
              :annotation="anno"
              :index="index"
              :time="formatTime(anno.createdAt)"
              :active="activeAnnotationId === anno.id"
              :preview="hoverAnnotationId === anno.id"
              @select="selectAnnotation(anno)"
              @remove="removeAnnotation(anno.id)"
              @preview="setHoveredAnnotation(anno.id)"
              @unpreview="setHoveredAnnotation('')"
            />
          </div>

          <div v-if="orphans.length || relocateError || relocateNotice" class="rail-orphans">
            <div v-if="orphans.length" class="anno-orphan-head">
              <span>锚点失效 {{ orphans.length }} 条</span>
              <button type="button" :disabled="relocating" @click="relocateOrphans">{{ relocating ? '正在匹配' : '尝试重新定位' }}</button>
            </div>
            <p v-if="relocateError" class="anno-relocate-note is-error">{{ relocateError }}</p>
            <p v-else-if="relocateNotice" class="anno-relocate-note">{{ relocateNotice }}</p>
            <div v-for="anno in orphans" :key="anno.id" class="anno is-orphan" :data-anno="anno.id">
              <div class="anno-top">
                <span class="anno-kind">锚点失效</span>
                <span class="anno-time">{{ formatTime(anno.createdAt) }}</span>
              </div>
              <div class="anno-q">{{ anno.quote }}</div>
              <div v-if="anno.body" class="anno-b">{{ anno.body }}</div>
              <div class="anno-foot">
                <template v-if="orphanDeleteId === anno.id">
                  <span class="anno-confirm-q">删除这条失效标注？</span>
                  <span class="anno-actions" aria-live="polite">
                    <button type="button" @click.stop="cancelOrphanDelete">取消</button>
                    <button type="button" class="danger is-confirm" @click.stop="confirmOrphanDelete(anno.id)">确认删除</button>
                  </span>
                </template>
                <span v-else class="anno-actions">
                  <button type="button" class="danger" @click.stop="armOrphanDelete(anno.id)">删除</button>
                </span>
              </div>
            </div>
          </div>
        </section>

        <section v-else-if="railTab === 'links'" class="rail-panel" role="tabpanel" aria-label="关联">
          <p class="rail-panel-hint">当前笔记与标签、人物、相关笔记的连接。</p>
          <RelationGraph v-if="note" :notes="library.notes" :note-id="note.id" />
        </section>

        <section v-else-if="railTab === 'toc'" class="rail-panel" role="tabpanel" aria-label="目录">
          <p class="rail-panel-hint">按小节跳转；当前小节会高亮。</p>
          <nav class="rail-toc">
            <button
              v-for="item in tocItems"
              :key="item.index"
              type="button"
              :class="{ 'is-active': tocActive === item.index }"
              @click="scrollToSection(item.index)"
            >
              <span class="toc-n">{{ String(item.index + 1).padStart(2, '0') }}</span>{{ item.heading }}
            </button>
          </nav>
        </section>

        <section v-else class="rail-panel" role="tabpanel" aria-label="信息">
          <section v-if="artifact" class="rail-info-block">
            <div class="rail-info-head"><b>来源</b></div>
            <p class="rail-source-meta">
              <span class="rail-source-platform">{{ artifact.source?.label || artifact.source?.kind || '来源' }}</span>
              <span v-if="artifact.source?.author" class="rail-source-sep">·</span>
              <span v-if="artifact.source?.author">{{ artifact.source.author }}</span>
              <span v-if="artifact.source?.publishedAt" class="rail-source-sep">·</span>
              <span v-if="artifact.source?.publishedAt">{{ artifact.source.publishedAt }}</span>
            </p>
            <p v-if="sourceTitle" class="rail-source-title" :title="sourceTitle">{{ sourceTitle }}</p>
            <div class="rail-actions">
              <button v-if="artifact.source?.url" type="button" class="rail-action is-primary" @click="openSourceVideo">
                <ExternalLink :size="12" /> 打开{{ sourceLinkLabel }}
              </button>
              <button v-if="artifact.run" type="button" class="rail-action" @click="openArtifact">
                <ArrowUpRight :size="12" /> 制作记录
              </button>
            </div>
          </section>

          <section class="rail-info-block">
            <div class="rail-info-head">
              <b>文件</b>
              <span v-if="relativePath.dir" class="rail-info-scope">{{ relativePath.dir }}</span>
            </div>
            <div class="rail-file-row">
              <span class="rail-file-path" :title="note?.path">{{ relativePath.name }}</span>
              <button
                type="button"
                class="rail-copy"
                :class="{ 'is-copied': pathCopied }"
                :aria-label="pathCopied ? '已复制完整路径' : '复制完整路径'"
                :title="pathCopied ? '已复制' : '复制完整路径'"
                @click="copyPath"
              >
                <Check v-if="pathCopied" :size="13" />
                <Copy v-else :size="13" />
              </button>
            </div>
            <p v-if="note?.date" class="rail-file-meta">{{ note.date }} 入库</p>
          </section>

          <p v-if="!artifact" class="rail-panel-hint">这篇笔记没有关联的制作来源。</p>
        </section>
      </div>
    </aside>
  </div>
</template>
