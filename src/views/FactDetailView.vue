<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { ArrowLeft, BookMarked, ExternalLink, GitMerge, LoaderCircle, RefreshCw, Scissors, Sparkles, Trash2, Undo2 } from 'lucide-vue-next'

import * as api from '../lib/api'
import CandidatePasteDialog from '../components/CandidatePasteDialog.vue'
import FactSplitDialog from '../components/FactSplitDialog.vue'
import { factIsHistory, factSourceSummary, factStatusClass as displayFactClass, factStatusLabel as displayFactStatus, isCompositeFact } from '../lib/facts'
import { library, loadLibrary } from '../stores/library'
import type { Fact, FactCheck, FactCitation, FactRelation, FactSplit, HistoryCandidate } from '../types'

const route = useRoute()
const router = useRouter()

const fact = ref<Fact | null>(null)
const checks = ref<FactCheck[]>([])
const citations = ref<FactCitation[]>([])
const candidates = ref<HistoryCandidate[]>([])
const citationsLoading = ref(false)
const loading = ref(false)
const checksLoading = ref(false)
const error = ref('')
const notice = ref('')
const verifying = ref(false)
const autoFinding = ref(false)
const autoFindNotice = ref('')
const domainSaving = ref(false)
const domainError = ref('')
const pastingCandidate = ref<HistoryCandidate | null>(null)
const mergeOpen = ref(false)
const mergeSourceId = ref('')
const merging = ref(false)
const mergeError = ref('')
const splitInfo = ref<FactSplit | null>(null)
const splitOpen = ref(false)
const splitUndoing = ref(false)
const splitError = ref('')
let noticeTimer = 0

const factId = computed(() => String(route.params.id ?? ''))
const isHistory = computed(() => (fact.value ? factIsHistory(fact.value) : false))
const canSplit = computed(() => {
  const value = fact.value
  if (!value || value.status === 'split' || splitInfo.value) return false
  return isCompositeFact(value)
})
const returnNoteId = computed(() => (route.query.from === 'note' ? String(route.query.noteId ?? '') : ''))
const returnSourceId = computed(() => (route.query.from === 'source' ? String(route.query.sourceId ?? '') : ''))
const returnQueue = computed(() => route.query.from === 'queue')
const backLabel = computed(() => returnNoteId.value ? '返回笔记' : returnSourceId.value ? '返回史料' : returnQueue.value ? '返回待补录' : '返回事实库')
const noteGroups = computed(() => {
  const source = fact.value
  if (!source) return []
  const groups = new Map<string, { noteId: string; title: string; up: string; date: string; quotes: string[]; runs: string[] }>()
  for (const mention of source.mentions) {
    const note = library.notes.find((item) => item.id === mention.noteId)
    const current = groups.get(mention.noteId) ?? {
      noteId: mention.noteId,
      title: note?.title ?? mention.noteId,
      up: note?.up ?? '',
      date: note?.date ?? '',
      quotes: [],
      runs: [],
    }
    if (mention.quote && !current.quotes.includes(mention.quote)) current.quotes.push(mention.quote)
    if (mention.runId && !current.runs.includes(mention.runId)) current.runs.push(mention.runId)
    groups.set(mention.noteId, current)
  }
  return [...groups.values()]
})

const sameAsCandidates = computed<FactRelation[]>(() => {
  const source = fact.value
  if (!source) return []
  return source.related
    .filter((relation) => relation.relation === 'same_as' && relation.confidence >= 0.62)
    .sort((left, right) => right.confidence - left.confidence)
})

const hasMergeCandidates = computed(() => sameAsCandidates.value.length > 0)

function flash(message: string): void {
  notice.value = message
  window.clearTimeout(noticeTimer)
  noticeTimer = window.setTimeout(() => { if (notice.value === message) notice.value = '' }, 3600)
}

function statusLabel(value: string): string {
  return ({
    candidate: '未核验',
    verified: '有证据支持',
    disputed: '有不同记载',
    unsupported: '暂无出处',
    split: '已拆分',
  } as Record<string, string>)[value] ?? value
}

function relationLabel(value: string): string {
  return ({ same_as: '同一事实', contradicts: '数值冲突', related: '可能相关' } as Record<string, string>)[value] ?? value
}

function evidenceLabel(value: string): string {
  return ({ supports: '支持', contradicts: '与事实不一致', mention: '提及' } as Record<string, string>)[value] ?? value
}

function authorityLabel(value: string): string {
  return ({
    primary: '一手全文',
    transcription: '转录全文',
    ocr: 'OCR 全文',
    secondary: '二手研究',
    reference: '工具书',
    link_only: '目录/外链',
  } as Record<string, string>)[value] ?? value
}

function candidateStatusLabel(value: string): string {
  return ({ cited: '已引用', rejected: '未采用', candidate: '候选' } as Record<string, string>)[value] ?? value
}

function openCandidate(url: string): void {
  if (url) void api.openExternalUrl(url)
}

function openCandidatePaste(candidate: HistoryCandidate): void {
  if (candidate.status === 'cited') return
  pastingCandidate.value = candidate
}

async function onCandidateImported(): Promise<void> {
  pastingCandidate.value = null
  await load()
  flash('已从粘贴原文写入史料引用')
}

function numericTokens(text: string): string[] {
  const matches = text.match(/\d+(?:\.\d+)?\s*(?:万|亿|million|billion|%|percent)?/gi) ?? []
  return [...new Set(matches.map((item) => item.trim()))]
}

function formatDate(value: number): string {
  if (!value) return '—'
  return new Date(value).toLocaleString('zh-CN', { year: 'numeric', month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit' })
}

function openNote(noteId: string): void {
  if (noteId) void router.push(`/note/${noteId}`)
}

function openRelated(id: string): void {
  if (id && id !== factId.value) void router.push({ path: `/facts/${id}`, query: route.query })
}

function openQueue(): void {
  void router.push('/sources/pending')
}

function scrollToSection(id: string): void {
  document.getElementById(id)?.scrollIntoView({ behavior: 'smooth', block: 'start' })
}

function back(): void {
  if (returnNoteId.value) {
    void router.push({ path: `/note/${returnNoteId.value}`, query: { fact: factId.value } })
    return
  }
  if (returnSourceId.value) {
    void router.push({ path: `/sources/${returnSourceId.value}`, query: { from: 'fact', factId: factId.value } })
    return
  }
  if (returnQueue.value) {
    void router.push('/sources/pending')
    return
  }
  void router.push('/facts')
}

function openSource(citation: FactCitation): void {
  if (citation.sourceId) void router.push({ path: `/sources/${citation.sourceId}`, query: { from: 'fact', factId: factId.value } })
}

async function load(): Promise<void> {
  if (!factId.value) return
  loading.value = true
  error.value = ''
  try {
    fact.value = await api.productionFact(factId.value)
    if (!library.notes.length) await loadLibrary()
    citationsLoading.value = true
    citations.value = await api.productionFactCitations(factId.value)
    candidates.value = await api.productionFactHistoryCandidates(factId.value).catch(() => [])
    splitInfo.value = await api.productionFactSplitInfo(factId.value).catch(() => null)
    checksLoading.value = true
    checks.value = await api.productionFactChecks(factId.value)
  } catch (cause) {
    fact.value = null
    checks.value = []
    candidates.value = []
    splitInfo.value = null
    error.value = api.errorText(cause)
  } finally {
    citationsLoading.value = false
    checksLoading.value = false
    loading.value = false
  }
}

async function reload(): Promise<void> {
  await load()
}

async function verify(): Promise<void> {
  if (!fact.value || verifying.value) return
  verifying.value = true
  error.value = ''
  try {
    const next = await api.productionVerifyFact(fact.value.id)
    fact.value = next
    checks.value = await api.productionFactChecks(fact.value.id)
    flash(`核验完成：${statusLabel(next.status)}`)
  } catch (cause) {
    error.value = api.errorText(cause)
  } finally {
    verifying.value = false
  }
}

async function autoFindSources(): Promise<void> {
  if (!fact.value || autoFinding.value) return
  autoFinding.value = true
  autoFindNotice.value = ''
  error.value = ''
  try {
    const report = await api.productionFactHistoryIngest(fact.value.id, true)
    await load()
    const detail = report.notes
      .filter((note) => !note.includes('检索结果是候选') && !note.includes('必须打开原文'))
      .slice(-2)
      .join('；')
    const planning = report.work
      ? `规划：${report.work}${report.section ? ` · ${report.section}` : ''}。`
      : ''
    autoFindNotice.value = report.citationsCreated > 0
      ? `自动入库 ${report.citationsCreated} 条引用（候选 ${report.candidates}，抓取 ${report.fetched} 页）。${planning}`
      : `没有新增史料（候选 ${report.candidates}，可自动抓取 ${report.autoEligible}）。${planning}${detail || '没有找到可逐字引用的原文。'}`
    flash(report.citationsCreated > 0 ? '已自动写入史料引用' : '没有找到可入库的史料原文')
    flash(report.citationsCreated > 0 ? '已自动写入史料引用' : '没有找到可入库的史料原文')
  } catch (cause) {
    error.value = api.errorText(cause)
  } finally {
    autoFinding.value = false
  }
}

async function setDomain(domain: 'history' | 'general'): Promise<void> {
  if (!fact.value || domainSaving.value || fact.value.domain === domain) return
  if (domain === 'general' && fact.value.citationCount > 0) {
    const ok = window.confirm('这条事实已有史料引用。归到「其他事实」后，史料区会隐藏，但引用不会被删除。继续？')
    if (!ok) return
  }
  domainSaving.value = true
  domainError.value = ''
  try {
    fact.value = await api.productionFactDomainSet(fact.value.id, domain)
    await load()
    flash(domain === 'history' ? '已归到历史事实；可以自动找史料了。' : '已归到其他事实。')
  } catch (cause) {
    domainError.value = api.errorText(cause)
  } finally {
    domainSaving.value = false
  }
}

async function removeCitation(citation: FactCitation): Promise<void> {
  if (!window.confirm('移除这条史料引用？史料和原文不会删除，只解除它和这条事实的关联。')) return
  try {
    await api.productionCitationDelete(citation.id)
    await load()
    flash('已移除这条史料引用')
  } catch (cause) {
    error.value = api.errorText(cause)
  }
}

function toggleMerge(): void {
  if (!sameAsCandidates.value.length) return
  mergeOpen.value = !mergeOpen.value
  mergeError.value = ''
  if (mergeOpen.value) {
    mergeSourceId.value = sameAsCandidates.value[0].otherId
  }
}

async function confirmMerge(): Promise<void> {
  if (!fact.value || !mergeSourceId.value || merging.value) return
  merging.value = true
  mergeError.value = ''
  try {
    fact.value = await api.productionMergeFact(fact.value.id, mergeSourceId.value)
    mergeOpen.value = false
    mergeSourceId.value = ''
    await reload()
    flash('已合并 1 条重复事实；来源、证据、笔记提及和核验历史已转移。')
  } catch (cause) {
    mergeError.value = api.errorText(cause)
  } finally {
    merging.value = false
  }
}

function openSplit(): void {
  if (!fact.value || !canSplit.value) return
  splitError.value = ''
  splitOpen.value = true
}

async function onSplit(split: FactSplit): Promise<void> {
  splitOpen.value = false
  await load()
  flash(`已拆成 ${split.children.length} 条子事实；原事实保留为审计记录。`)
}

async function undoSplit(): Promise<void> {
  if (!splitInfo.value || splitUndoing.value) return
  if (!window.confirm('撤销这次拆分？本次新建的子事实和复制过去的笔记提及会删除；已有原子事实不会删除。')) return
  splitUndoing.value = true
  splitError.value = ''
  try {
    await api.productionFactSplitUndo(splitInfo.value.id)
    await load()
    flash('拆分已撤销；原事实状态和整理标记已恢复。')
  } catch (cause) {
    splitError.value = api.errorText(cause)
  } finally {
    splitUndoing.value = false
  }
}

onMounted(load)
watch(factId, () => {
  autoFindNotice.value = ''
  domainError.value = ''
  splitOpen.value = false
  splitError.value = ''
  void load()
})
</script>

<template>
  <div class="fact-detail-page">
    <div class="fact-detail-top">
      <button type="button" class="fact-detail-back" @click="back"><ArrowLeft :size="13" />{{ backLabel }}</button>
      <div class="fact-detail-actions">
        <button v-if="fact && isHistory" type="button" class="prod-btn is-small" :disabled="autoFinding" @click="autoFindSources">
          <LoaderCircle v-if="autoFinding" :size="12" class="sub-spin" /><BookMarked v-else :size="12" />{{ autoFinding ? '找史料中…' : '自动找史料' }}
        </button>
        <button v-if="fact" type="button" class="prod-btn is-small" :disabled="verifying" @click="verify">
          <LoaderCircle v-if="verifying" :size="12" class="sub-spin" /><RefreshCw v-else :size="12" />{{ verifying ? '核验中…' : (isHistory ? '核验网页补充' : '重新核验') }}
        </button>
        <button v-if="fact && hasMergeCandidates" type="button" class="prod-btn is-small" :disabled="merging" @click="toggleMerge">
          <GitMerge :size="12" />合并重复…
        </button>
        <button v-if="fact && canSplit" type="button" class="prod-btn is-small" :disabled="splitUndoing" @click="openSplit">
          <Scissors :size="12" />拆分…
        </button>
      </div>
    </div>

    <div v-if="notice" class="facts-notice"><Sparkles :size="14" />{{ notice }}</div>
    <div v-if="error" class="prod-error-box"><p>{{ error }}</p></div>
    <div v-if="loading" class="facts-loading"><LoaderCircle :size="16" class="sub-spin" />正在读取事实…</div>
    <div v-else-if="!fact" class="facts-empty">
      <Sparkles :size="22" />
      <b>找不到这条事实</b>
      <p>它可能已经被合并。回到事实库查看当前事实。</p>
      <button type="button" class="prod-btn is-small" @click="back">{{ backLabel }}</button>
    </div>

    <template v-else>
      <header class="fact-detail-head">
        <div class="fact-detail-kicker">{{ isHistory ? '历史事实' : '通用事实' }} / {{ fact.id.slice(-6) }}</div>
        <div class="fact-detail-title-row">
          <h1>{{ fact.text }}</h1>
          <span class="fact-status-pill" :class="displayFactClass(fact)">{{ displayFactStatus(fact) }}</span>
        </div>
        <p v-if="fact.status === 'disputed'" class="fact-detail-status-note">有不同记载：至少有一条证据与事实不一致，或来源之间互相矛盾。展开下方{{ isHistory ? '「史料」和「网络证据」' : '「网络证据」' }}查看具体差异。</p>
        <p v-if="fact.status === 'split'" class="fact-detail-status-note">已拆分：子事实在下方；原事实保留为审计记录，证据和引用没有复制给子事实。</p>
        <div class="fact-detail-submeta">
          <span>{{ noteGroups.length }} 篇笔记引用</span>
          <span>{{ factSourceSummary(fact) }}</span>
          <span>{{ fact.related.length }} 个相关事实</span>
          <span>最近核验 {{ formatDate(fact.lastCheckedAt) }}</span>
        </div>
      </header>

      <nav class="fact-detail-anchors" aria-label="事实详情栏目">
        <button type="button" @click="scrollToSection('fact-notes')">引用笔记</button>
        <button v-if="isHistory" type="button" @click="scrollToSection('fact-sources')">史料</button>
        <button v-if="candidates.length" type="button" @click="scrollToSection('fact-candidates')">候选 {{ candidates.length }}</button>
        <button v-if="fact.evidence.length" type="button" @click="scrollToSection('fact-evidence')">网络证据 {{ fact.evidence.length }}</button>
        <button v-if="fact.related.length" type="button" @click="scrollToSection('fact-related')">相关 {{ fact.related.length }}</button>
        <button v-if="checks.length" type="button" @click="scrollToSection('fact-checks')">核验 {{ checks.length }}</button>
      </nav>

      <div class="fact-detail-layout">
        <main class="fact-detail-main">
          <section v-if="splitInfo" class="fact-detail-section">
            <div class="fact-detail-section-head">
              <h2>已拆分</h2>
              <span>{{ splitInfo.children.length }} 条子事实</span>
            </div>
            <p class="fact-detail-hint">原事实保留为审计记录；网页证据和史料引用没有复制到子事实。撤销会恢复拆分前状态。</p>
            <div class="fact-split-children">
              <button v-for="child in splitInfo.children" :key="child.id" type="button" class="fact-split-child" @click="openRelated(child.id)">
                <b>{{ child.text }}</b>
                <small>{{ statusLabel(child.status) }} · {{ child.evidenceCount }} 网页 · {{ child.citationCount }} 史料</small>
              </button>
            </div>
            <p v-if="splitError" class="fact-merge-error">{{ splitError }}</p>
            <div class="fact-detail-merge-actions">
              <button type="button" class="prod-btn is-small is-danger" :disabled="splitUndoing || !splitInfo.canUndo" @click="undoSplit">
                <LoaderCircle v-if="splitUndoing" :size="12" class="sub-spin" /><Undo2 v-else :size="12" />{{ splitUndoing ? '撤销中…' : '撤销拆分' }}
              </button>
            </div>
            <p v-if="!splitInfo.canUndo" class="fact-split-note">子事实已经有新增的证据、核验、关系或检索候选，不能直接撤销拆分。</p>
          </section>

          <section id="fact-notes" class="fact-detail-section">
            <div class="fact-detail-section-head">
              <h2>引用这篇事实的笔记</h2>
              <span>{{ noteGroups.length }} 篇</span>
            </div>
            <p class="fact-detail-hint">一篇事实可以同时被多篇笔记引用；这里按笔记聚合，点标题打开原文。</p>
            <div v-if="noteGroups.length" class="fact-detail-notes">
              <article v-for="group in noteGroups" :key="group.noteId" class="fact-detail-note">
                <button type="button" class="fact-detail-note-title" @click="openNote(group.noteId)">
                  <b>{{ group.title }}</b>
                  <small>{{ group.date }}</small>
                  <ExternalLink :size="12" />
                </button>
                <blockquote v-for="quote in group.quotes" :key="quote">{{ quote }}</blockquote>
                <small v-if="group.runs.length" class="fact-detail-run">运行 {{ group.runs.join(' · ') }}</small>
              </article>
            </div>
            <p v-else class="fact-detail-empty">还没有笔记引用这条事实。</p>
          </section>

          <section v-if="isHistory" id="fact-sources" class="fact-detail-section">
            <div class="fact-detail-section-head">
              <h2>史料</h2>
              <span>{{ citations.length }} 条</span>
            </div>
            <p class="fact-detail-hint">史料是“哪本书、哪一版、哪一页、原文怎么说”。它和网页证据分开，只有历史事实才走这条线。</p>
            <p v-if="autoFindNotice" class="fact-source-auto-note">{{ autoFindNotice }}</p>
            <div v-if="citationsLoading" class="fact-detail-empty">正在读取史料引用…</div>
            <div v-else-if="citations.length" class="fact-source-citations">
              <article v-for="citation in citations" :key="citation.id" class="fact-source-citation">
                <button type="button" class="fact-source-title" @click="openSource(citation)">
                  <b>{{ citation.sourceType === 'web' ? citation.workTitle : `《${citation.workTitle}》${citation.sectionTitle ? ` · ${citation.sectionTitle}` : ''}` }}</b>
                  <small>{{ citation.sourceType === 'web' ? '网页证据' : ([citation.author, citation.edition, citation.volume, citation.page].filter(Boolean).join(' · ') || '版本待补') }}</small>
                </button>
                <span class="fact-source-citation-actions">
                  <span class="fact-source-relation">{{ ({ supports: '支持', contradicts: '矛盾', variant: '异文', context: '背景' } as Record<string,string>)[citation.relation] ?? citation.relation }}</span>
                  <button type="button" class="fact-source-remove" title="移除引用" aria-label="移除引用" @click="removeCitation(citation)"><Trash2 :size="11" /></button>
                </span>
                <blockquote v-if="citation.quote">{{ citation.quote }}</blockquote>
                <small v-if="citation.translation" class="fact-source-translation">{{ citation.translation }}</small>
              </article>
            </div>
            <p v-else class="fact-detail-empty">还没有史料引用。点「自动找史料」会检索、抓取原文并逐字校验；找不到不会硬凑。</p>

          </section>

          <section v-if="candidates.length" id="fact-candidates" class="fact-detail-section">
            <div class="fact-detail-section-head">
              <h2>来源候选</h2>
              <span>{{ candidates.length }} 条</span>
            </div>
            <p class="fact-detail-hint">候选记录检索到了什么、为什么没有自动引用；只有「已引用」才会算史料。补录后候选会从队列消失。</p>
            <details class="fact-candidates">
              <summary>展开候选 {{ candidates.length }} 条 <span>补录原文</span></summary>
              <div class="fact-candidate-list">
                <article v-for="candidate in candidates" :key="candidate.id" class="fact-candidate" :class="`is-${candidate.status}`">
                  <div class="fact-candidate-head">
                    <span class="fact-candidate-provider">{{ candidate.provider }}</span>
                    <b>{{ candidate.title }}</b>
                    <span class="fact-candidate-authority">{{ authorityLabel(candidate.authority) }}</span>
                    <span class="fact-candidate-status">{{ candidateStatusLabel(candidate.status) }}</span>
                  </div>
                  <p v-if="candidate.snippet" class="fact-candidate-snippet">{{ candidate.snippet }}</p>
                  <p class="fact-candidate-reason">{{ candidate.reason }}</p>
                  <div class="fact-candidate-foot">
                    <span v-if="candidate.workTitle">{{ candidate.workTitle }}<template v-if="candidate.sectionTitle"> · {{ candidate.sectionTitle }}</template></span>
                    <span class="fact-candidate-actions">
                      <button v-if="candidate.url" type="button" @click="openCandidate(candidate.url)">打开来源</button>
                      <button v-if="candidate.status !== 'cited'" type="button" class="is-primary" @click="openCandidatePaste(candidate)">补录原文</button>
                    </span>
                  </div>
                </article>
              </div>
            </details>
            <button type="button" class="prod-btn is-small fact-candidates-queue" @click="openQueue">去待补录队列</button>
          </section>

          <section v-if="fact.evidence.length" id="fact-evidence" class="fact-detail-section">
            <div class="fact-detail-section-head">
              <h2>网络证据</h2>
              <span>{{ fact.evidence.length }} 条</span>
            </div>
            <p class="fact-detail-hint">{{ isHistory ? '补充的 Search/Fetch 网页证据；历史事实优先看上方「史料」。' : 'Search/Fetch 抓到的网页证据。' }}</p>
            <div v-if="fact.evidence.length" class="fact-detail-evidence">
              <article v-for="evidence in fact.evidence" :key="evidence.url + evidence.quote" class="fact-detail-evidence-row">
                <div class="fact-detail-evidence-head">
                  <a href="#" @click.prevent="api.openExternalUrl(evidence.url)">{{ evidence.domain || evidence.url }}</a>
                  <span :class="evidence.relation === 'contradicts' ? 'is-conflict' : ''">{{ evidenceLabel(evidence.relation) }}</span>
                </div>
                <blockquote v-if="evidence.quote">{{ evidence.quote }}</blockquote>
                <div v-if="evidence.relation === 'contradicts'" class="fact-detail-conflict">
                  <template v-if="numericTokens(fact.text).length && numericTokens(evidence.quote).length">
                    <b>争议点 · 两边数字不一致</b>
                    <div><span>事实里的数字</span><code v-for="token in numericTokens(fact.text)" :key="'f-' + token">{{ token }}</code></div>
                    <div><span>来源里的数字</span><code v-for="token in numericTokens(evidence.quote)" :key="'e-' + token">{{ token }}</code></div>
                    <p>来源与事实的数值集合不完全一致；具体口径以引文为准。</p>
                  </template>
                  <template v-else>
                    <b>争议点 · 来源与事实表述不一致</b>
                    <p>这条证据是文字表述冲突，不是数字口径冲突；以上方引文原文为准。</p>
                  </template>
                </div>
                <small>{{ evidence.title || evidence.url }}</small>
              </article>
            </div>
            <p v-else class="fact-detail-empty">{{ isHistory ? '没有补充的网页证据；这条历史事实以史料为准。' : '还没有来源证据。可以点「重新核验」走一遍检索和正文抓取。' }}</p>
          </section>

          <section v-if="fact.related.length" id="fact-related" class="fact-detail-section">
            <div class="fact-detail-section-head">
              <h2>相关与冲突</h2>
              <span>{{ fact.related.length }} 条</span>
            </div>
            <div v-if="fact.related.length" class="fact-detail-related">
              <button v-for="related in fact.related" :key="related.otherId" type="button" class="fact-detail-related-row" :class="{ 'is-conflict': related.relation === 'contradicts' }" @click="openRelated(related.otherId)">
                <span>{{ relationLabel(related.relation) }}</span>
                <b>{{ related.text }}</b>
                <small>{{ statusLabel(related.status) }} · 词面重合 {{ Math.round(related.confidence * 100) }}%</small>
              </button>
            </div>
            <p v-else class="fact-detail-empty">没有相关或冲突事实。</p>
          </section>

          <section v-if="checks.length" id="fact-checks" class="fact-detail-section">
            <div class="fact-detail-section-head">
              <h2>核验历史</h2>
              <span>{{ checks.length }} 次</span>
            </div>
            <div v-if="checksLoading" class="fact-detail-empty">正在读取核验历史…</div>
            <div v-else-if="checks.length" class="fact-detail-checks">
              <article v-for="entry in checks" :key="entry.id" class="fact-detail-check">
                <div><b>{{ entry.verdict }}</b><span>{{ entry.provider }} / {{ entry.model || '—' }}</span></div>
                <p>{{ formatDate(entry.checkedAt) }} · {{ entry.evidenceCount }} 条证据</p>
                <div v-if="entry.queries.length" class="fact-detail-query-list"><code v-for="query in entry.queries" :key="query">{{ query }}</code></div>
              </article>
            </div>
            <p v-else class="fact-detail-empty">还没有核验记录。</p>
          </section>
          <div v-if="!fact.evidence.length && !fact.related.length && !checks.length" class="fact-detail-empty-strip">
            <span>网络证据 0</span>
            <span>相关与冲突 0</span>
            <span>核验历史 0</span>
            <span class="fact-detail-empty-strip-hint">已收起空栏目</span>
          </div>
        </main>

        <aside class="fact-detail-rail">
          <section>
            <h3>事实状态</h3>
            <span class="fact-status-pill" :class="displayFactClass(fact)">{{ displayFactStatus(fact) }}</span>
            <dl class="fact-detail-facts">
              <div><dt>类型</dt><dd>{{ isHistory ? '历史事实' : '通用事实' }}</dd></div>
              <div><dt>引用笔记</dt><dd>{{ noteGroups.length }} 篇</dd></div>
              <div v-if="isHistory"><dt>史料</dt><dd>{{ fact.citationCount }} 条</dd></div>
              <div><dt>网页证据</dt><dd>{{ fact.evidence.length }} 条</dd></div>
              <div v-if="splitInfo"><dt>拆分</dt><dd>{{ splitInfo.children.length }} 条子事实</dd></div>
              <div><dt>相关事实</dt><dd>{{ fact.related.length }} 条</dd></div>
              <div><dt>创建</dt><dd>{{ formatDate(fact.createdAt) }}</dd></div>
              <div><dt>最近核验</dt><dd>{{ formatDate(fact.lastCheckedAt) }}</dd></div>
            </dl>
            <div class="fact-domain-control">
              <span>归类</span>
              <button type="button" :class="{ 'is-active': isHistory }" :disabled="domainSaving" @click="setDomain('history')">历史事实</button>
              <button type="button" :class="{ 'is-active': !isHistory }" :disabled="domainSaving" @click="setDomain('general')">其他事实</button>
            </div>
            <p v-if="domainError" class="fact-merge-error">{{ domainError }}</p>
          </section>

          <section v-if="fact.queries.length">
            <h3>检索词</h3>
            <div class="fact-detail-query-list"><code v-for="query in fact.queries" :key="query">{{ query }}</code></div>
          </section>

          <section v-if="mergeOpen">
            <h3>合并重复</h3>
            <p class="fact-detail-hint">保留当前事实，把另一条高相似事实合并进来。</p>
            <label v-for="candidate in sameAsCandidates" :key="candidate.otherId" class="fact-detail-merge-option">
              <input v-model="mergeSourceId" type="radio" :value="candidate.otherId" :disabled="merging" />
              <span><b>{{ candidate.text }}</b><small>{{ statusLabel(candidate.status) }} · {{ Math.round(candidate.confidence * 100) }}%</small></span>
            </label>
            <p v-if="mergeError" class="fact-merge-error">{{ mergeError }}</p>
            <div class="fact-detail-merge-actions">
              <button type="button" class="prod-btn is-small" :disabled="merging" @click="toggleMerge">取消</button>
              <button type="button" class="prod-btn is-small is-danger" :disabled="merging || !mergeSourceId" @click="confirmMerge">
                <LoaderCircle v-if="merging" :size="12" class="sub-spin" />{{ merging ? '合并中…' : '确认合并' }}
              </button>
            </div>
          </section>
        </aside>
      </div>
    </template>

    <CandidatePasteDialog :candidate="pastingCandidate" @close="pastingCandidate = null" @imported="onCandidateImported" />
    <FactSplitDialog :fact="splitOpen ? fact : null" @close="splitOpen = false" @split="onSplit" />
  </div>
</template>
