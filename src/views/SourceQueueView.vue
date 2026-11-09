<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useRouter } from 'vue-router'
import { ArrowLeft, BookMarked, CheckCircle2, ChevronDown, ExternalLink, LoaderCircle, RefreshCw, Search } from 'lucide-vue-next'

import * as api from '../lib/api'
import CandidatePasteDialog from '../components/CandidatePasteDialog.vue'
import { factSourceSummary, factStatusLabel } from '../lib/facts'
import { library, loadLibrary } from '../stores/library'
import type { Fact, HistoryCandidate } from '../types'

const router = useRouter()
const facts = ref<Fact[]>([])
const candidates = ref<HistoryCandidate[]>([])
const cited = ref<HistoryCandidate[]>([])
const loading = ref(false)
const error = ref('')
const notice = ref('')
const query = ref('')
const providerFilter = ref('')
const authorityFilter = ref('')
const expanded = ref<Set<string>>(new Set())
const pastingCandidate = ref<HistoryCandidate | null>(null)
let noticeTimer = 0

const factById = computed(() => {
  const map = new Map<string, Fact>()
  for (const fact of facts.value) map.set(fact.id, fact)
  return map
})

type QueueGroup = {
  fact: Fact
  items: HistoryCandidate[]
  providerSummary: string
}

const groups = computed<QueueGroup[]>(() => {
  const needle = query.value.trim().toLowerCase()
  const map = new Map<string, { fact: Fact; items: HistoryCandidate[] }>()
  for (const candidate of candidates.value) {
    const fact = factById.value.get(candidate.factId)
    if (!fact) continue
    if (providerFilter.value && candidate.provider !== providerFilter.value) continue
    if (authorityFilter.value && candidate.authority !== authorityFilter.value) continue
    const haystack = `${fact.text} ${candidate.provider} ${candidate.title} ${candidate.reason}`.toLowerCase()
    if (needle && !haystack.includes(needle)) continue
    const group = map.get(fact.id) ?? { fact, items: [] }
    group.items.push(candidate)
    map.set(fact.id, group)
  }
  return [...map.values()]
    .map((group) => {
      const counts = new Map<string, number>()
      for (const item of group.items) counts.set(item.provider, (counts.get(item.provider) ?? 0) + 1)
      const providerSummary = [...counts.entries()]
        .sort((left, right) => right[1] - left[1] || left[0].localeCompare(right[0]))
        .slice(0, 3)
        .map(([provider, count]) => `${provider} ${count}`)
        .join(' · ')
      return { ...group, providerSummary }
    })
    .sort((left, right) => {
      const leftNotes = left.fact.noteCount
      const rightNotes = right.fact.noteCount
      return rightNotes - leftNotes || right.items.length - left.items.length
    })
})

const totalFacts = computed(() => new Set(candidates.value.map((candidate) => candidate.factId)).size)
const providerOptions = computed(() => [...new Set(candidates.value.map((candidate) => candidate.provider))].sort())
const authorityOptions = computed(() => [...new Set(candidates.value.map((candidate) => candidate.authority))].sort())

function isExpanded(factId: string): boolean {
  return expanded.value.has(factId)
}

function toggleGroup(factId: string): void {
  const next = new Set(expanded.value)
  if (next.has(factId)) next.delete(factId)
  else next.add(factId)
  expanded.value = next
}

function collapseAll(): void {
  expanded.value = new Set()
}

function expandFirst(): void {
  const first = groups.value[0]
  expanded.value = new Set(first ? [first.fact.id] : [])
}

function toggleAll(): void {
  if (expanded.value.size) collapseAll()
  else expandFirst()
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

function noteTitle(noteId: string): string {
  return library.notes.find((note) => note.id === noteId)?.title ?? noteId
}

function flash(message: string): void {
  notice.value = message
  window.clearTimeout(noticeTimer)
  noticeTimer = window.setTimeout(() => { if (notice.value === message) notice.value = '' }, 3600)
}

function openFact(id: string): void {
  if (id) void router.push({ path: `/facts/${id}`, query: { from: 'queue' } })
}

function openCandidate(url: string): void {
  if (url) void api.openExternalUrl(url)
}

function openCandidatePaste(candidate: HistoryCandidate): void {
  pastingCandidate.value = candidate
}

function back(): void {
  void router.push('/sources')
}

async function load(): Promise<void> {
  loading.value = true
  error.value = ''
  try {
    const [nextFacts, nextCandidates, nextCited] = await Promise.all([
      api.productionFacts('', '', 300),
      api.productionHistoryCandidatesAll('candidate', 500),
      api.productionHistoryCandidatesAll('cited', 500),
    ])
    facts.value = nextFacts
    candidates.value = nextCandidates
    cited.value = nextCited
    if (!library.notes.length) await loadLibrary()
    expandFirst()
  } catch (cause) {
    error.value = api.errorText(cause)
  } finally {
    loading.value = false
  }
}

async function onImported(): Promise<void> {
  pastingCandidate.value = null
  await load()
  flash('已从粘贴原文写入史料引用')
}

onMounted(load)
</script>

<template>
  <div class="source-queue-page">
    <div class="curation-top">
      <button type="button" class="fact-detail-back" @click="back"><ArrowLeft :size="13" />返回史料库</button>
      <button type="button" class="prod-btn is-small" :disabled="loading" @click="load"><RefreshCw :size="12" :class="{ 'sub-spin': loading }" />刷新</button>
    </div>

    <header class="curation-head">
      <div class="prod-eyebrow">知识库 / 史料 / 待补录</div>
      <h1>待补录候选</h1>
      <p>这些是自动检索找到、但还没有全文引用的来源。打开候选页面，复制原文，程序会切 passage、逐字对齐并写入 citation；没有原文不硬写。</p>
    </header>

    <div v-if="notice" class="facts-notice"><CheckCircle2 :size="14" />{{ notice }}</div>
    <div v-if="error" class="prod-error-box"><p>{{ error }}</p></div>

    <div class="curation-summary is-three">
      <article><span>待补录候选</span><b>{{ candidates.length }}</b><small>candidate 状态，尚未引用</small></article>
      <article><span>涉及事实</span><b>{{ totalFacts }}</b><small>有候选但未引用</small></article>
      <article><span>已补录候选</span><b>{{ cited.length }}</b><small>从候选写入 citation，不含手工史料</small></article>
    </div>

    <div class="sources-toolbar source-queue-toolbar">
      <div class="source-queue-tools">
        <span class="source-queue-hint">按事实分组；展开一组再补录，不必滚完 360 条。</span>
        <div class="source-queue-filters">
          <select v-model="providerFilter" class="prod-select source-queue-select" aria-label="按 provider 筛选">
            <option value="">provider：全部</option>
            <option v-for="provider in providerOptions" :key="provider" :value="provider">{{ provider }}</option>
          </select>
          <select v-model="authorityFilter" class="prod-select source-queue-select" aria-label="按 authority 筛选">
            <option value="">authority：全部</option>
            <option v-for="authority in authorityOptions" :key="authority" :value="authority">{{ authorityLabel(authority) }}</option>
          </select>
          <button type="button" class="prod-btn is-small" @click="toggleAll">{{ expanded.size ? '全部折叠' : '展开第一组' }}</button>
        </div>
      </div>
      <label class="sources-search">
        <Search :size="13" />
        <input v-model="query" type="search" placeholder="搜索事实、书名、provider…" aria-label="搜索待补录候选" />
      </label>
    </div>

    <div v-if="loading" class="facts-loading"><LoaderCircle :size="16" class="sub-spin" />正在读取候选…</div>
    <div v-else-if="!candidates.length" class="facts-empty">
      <BookMarked :size="22" />
      <b>没有待补录候选</b>
      <p>所有候选都已经引用，或者还没有跑过自动找史料。</p>
    </div>
    <div v-else-if="!groups.length" class="facts-empty">
      <Search :size="20" />
      <b>没有匹配的候选</b>
      <p>换个关键词，或清空搜索。</p>
    </div>
    <div v-else class="source-queue-list">
      <article v-for="group in groups" :key="group.fact.id" class="source-queue-group" :class="{ 'is-collapsed': !isExpanded(group.fact.id) }">
        <div class="source-queue-head" @click="toggleGroup(group.fact.id)">
          <button type="button" class="source-queue-fact" @click.stop="openFact(group.fact.id)">
            <span class="curation-choice-meta">
              <span class="fact-status">{{ factStatusLabel(group.fact) }}</span>
              <span>{{ group.fact.noteCount }} 篇笔记</span>
              <span>{{ factSourceSummary(group.fact) }}</span>
              <span v-if="group.fact.domain === 'history'" class="fact-domain-badge">历史</span>
            </span>
            <b>{{ group.fact.text }}</b>
          </button>
          <span class="source-queue-count">{{ group.items.length }} 条 · {{ group.providerSummary }}</span>
          <button type="button" class="source-queue-toggle" :aria-expanded="isExpanded(group.fact.id)" :aria-label="isExpanded(group.fact.id) ? '收起候选' : '展开候选'" @click.stop="toggleGroup(group.fact.id)">
            <ChevronDown :size="14" :class="{ 'is-open': isExpanded(group.fact.id) }" />
          </button>
        </div>
        <div v-if="group.fact.mentions.length" class="source-queue-notes">
          <span v-for="noteId in [...new Set(group.fact.mentions.map((mention) => mention.noteId))].slice(0, 3)" :key="noteId">{{ noteTitle(noteId) }}</span>
        </div>
        <div v-if="isExpanded(group.fact.id)" class="source-queue-candidates">
          <article v-for="(candidate, index) in group.items" :key="candidate.id" class="source-queue-candidate">
            <div class="source-queue-candidate-main">
              <span class="fact-candidate-provider">{{ candidate.provider }}</span>
              <b>{{ candidate.title }}</b>
              <span class="fact-candidate-authority">{{ authorityLabel(candidate.authority) }}</span>
              <span v-if="index === 0 || group.items[index - 1].reason !== candidate.reason" class="source-queue-candidate-reason">{{ candidate.reason }}</span>
            </div>
            <div class="source-queue-candidate-actions">
              <button type="button" class="prod-btn is-small is-primary" @click="openCandidatePaste(candidate)">补录原文</button>
              <button v-if="candidate.url" type="button" class="prod-btn is-small" @click="openCandidate(candidate.url)"><ExternalLink :size="11" />打开</button>
            </div>
          </article>
        </div>
      </article>
    </div>

    <CandidatePasteDialog :candidate="pastingCandidate" @close="pastingCandidate = null" @imported="onImported" />
  </div>
</template>
