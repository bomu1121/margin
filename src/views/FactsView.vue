<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { CheckCircle2, ChevronRight, History, ListChecks, LoaderCircle, RefreshCw, Search, Sparkles, TriangleAlert, Undo2 } from 'lucide-vue-next'

import * as api from '../lib/api'
import { factStatusClass, factStatusKey, factStatusLabel } from '../lib/facts'
import { library, loadLibrary } from '../stores/library'
import type { Fact, FactMerge } from '../types'

const router = useRouter()
const route = useRoute()
const facts = ref<Fact[]>([])
const loading = ref(false)
const error = ref('')
const notice = ref('')
const query = ref('')
const domainFilter = ref<'all' | 'history' | 'general'>('all')
const status = ref<'all' | 'candidate' | 'verified' | 'disputed' | 'unsupported' | 'split'>('all')
const onlyConflicts = ref(false)
const mergeHistoryOpen = ref(false)
const mergeHistoryLoading = ref(false)
const mergeHistoryError = ref('')
const merges = ref<FactMerge[]>([])
const undoing = ref('')
let noticeTimer = 0

const domainOptions = [
  { id: 'all', label: '全部事实' },
  { id: 'history', label: '历史事实' },
  { id: 'general', label: '其他事实' },
] as const

const statusOptions = [
  { id: 'all', label: '不限' },
  { id: 'candidate', label: '未核验' },
  { id: 'verified', label: '有证据' },
  { id: 'disputed', label: '有争议' },
  { id: 'unsupported', label: '未找到' },
  { id: 'split', label: '已拆分' },
] as const

const domainCounts = computed(() => ({
  all: facts.value.length,
  history: facts.value.filter((item) => item.domain === 'history').length,
  general: facts.value.filter((item) => item.domain !== 'history').length,
}))

const scopedFacts = computed(() => {
  if (domainFilter.value === 'all') return facts.value
  return facts.value.filter((fact) => (domainFilter.value === 'history' ? fact.domain === 'history' : fact.domain !== 'history'))
})

const counts = computed(() => ({
  all: scopedFacts.value.length,
  candidate: scopedFacts.value.filter((item) => factStatusKey(item) === 'candidate').length,
  verified: scopedFacts.value.filter((item) => factStatusKey(item) === 'verified').length,
  disputed: scopedFacts.value.filter((item) => factStatusKey(item) === 'disputed').length,
  unsupported: scopedFacts.value.filter((item) => factStatusKey(item) === 'unsupported').length,
  split: scopedFacts.value.filter((item) => factStatusKey(item) === 'split').length,
}))

const conflictCount = computed(() =>
  scopedFacts.value.filter((fact) => fact.related.some((relation) => relation.relation === 'contradicts')).length,
)

const activeMergeCount = computed(() => merges.value.filter((merge) => !merge.undoneAt).length)

const filtered = computed(() => {
  const needle = query.value.trim().toLowerCase()
  return scopedFacts.value.filter((fact) => {
    if (status.value !== 'all' && factStatusKey(fact) !== status.value) return false
    if (onlyConflicts.value && !fact.related.some((relation) => relation.relation === 'contradicts')) return false
    return !needle || fact.text.toLowerCase().includes(needle)
  })
})

const groups = computed(() => {
  const history = filtered.value.filter((fact) => fact.domain === 'history')
  const general = filtered.value.filter((fact) => fact.domain !== 'history')
  return [
    { key: 'history', title: '历史事实', hint: '史料引文 · 与网页证据分开', items: history },
    { key: 'general', title: '其他事实', hint: '网页证据 · 不显示史料栏', items: general },
  ].filter((group) => group.items.length)
})

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
    stale: '待复核',
  } as Record<string, string>)[value] ?? value
}

function openFact(id: string): void {
  if (id) void router.push(`/facts/${id}`)
}

function openCuration(): void {
  void router.push('/facts/curate')
}

function formatMergedAt(value: number): string {
  if (!value) return '—'
  return new Date(value).toLocaleString('zh-CN', { month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit' })
}

async function load(): Promise<void> {
  loading.value = true
  error.value = ''
  try {
    facts.value = await api.productionFacts('', '', 300)
  } catch (cause) {
    error.value = api.errorText(cause)
  } finally {
    loading.value = false
  }
}

async function loadMerges(): Promise<void> {
  mergeHistoryLoading.value = true
  mergeHistoryError.value = ''
  try {
    merges.value = await api.productionFactMerges(30)
  } catch (cause) {
    mergeHistoryError.value = api.errorText(cause)
  } finally {
    mergeHistoryLoading.value = false
  }
}

function openMergeHistory(): void {
  mergeHistoryOpen.value = true
  mergeHistoryError.value = ''
  void loadMerges()
}

function closeMergeHistory(): void {
  if (undoing.value) return
  mergeHistoryOpen.value = false
  mergeHistoryError.value = ''
}

async function undoMerge(merge: FactMerge): Promise<void> {
  if (undoing.value) return
  undoing.value = merge.id
  mergeHistoryError.value = ''
  try {
    await api.productionUndoFactMerge(merge.id)
    await Promise.all([load(), loadMerges()])
    flash('合并已撤销：源事实和它的来源、证据、提及、核验历史都已恢复。')
  } catch (cause) {
    mergeHistoryError.value = api.errorText(cause)
  } finally {
    undoing.value = ''
  }
}

onMounted(async () => {
  if (!library.notes.length) await loadLibrary()
  await load()
  await loadMerges()
  const focus = String(route.query.focus ?? '')
  if (focus) await router.replace(`/facts/${focus}`)
})
</script>

<template>
  <div class="facts-page">
    <div class="facts-head">
      <div>
        <div class="prod-eyebrow">知识库 / 事实</div>
        <h1>事实库</h1>
        <p>事实独立于笔记存在；一条事实是一页可维护的文档，列表只做索引。引用笔记、证据和关系在事实详情里。</p>
      </div>
      <div class="facts-head-actions">
        <button type="button" class="prod-btn" @click="openCuration"><ListChecks :size="13" />整理事实</button>
        <button type="button" class="prod-btn" @click="openMergeHistory"><History :size="13" />合并记录<template v-if="activeMergeCount"><span class="facts-count-badge">{{ activeMergeCount }}</span></template></button>
        <button type="button" class="prod-btn" :disabled="loading" @click="load"><RefreshCw :size="13" :class="{ 'sub-spin': loading }" />刷新</button>
      </div>
    </div>

    <div v-if="notice" class="facts-notice"><CheckCircle2 :size="14" />{{ notice }}</div>
    <div v-if="error" class="prod-error-box"><p>{{ error }}</p></div>

    <div class="facts-scope" role="group" aria-label="事实类型">
      <span class="facts-filter-label">范围</span>
      <button v-for="option in domainOptions" :key="option.id" type="button" :class="{ 'is-active': domainFilter === option.id }" @click="domainFilter = option.id">
        {{ option.label }}<span>{{ domainCounts[option.id] }}</span>
      </button>
      <p>历史事实走史料引文；其他事实只看网页证据。</p>
    </div>

    <div class="facts-toolbar">
      <div class="facts-filters" role="group" aria-label="事实状态筛选">
        <span class="facts-filter-label">状态</span>
        <button v-for="option in statusOptions" :key="option.id" type="button" :class="{ 'is-active': status === option.id }" @click="status = option.id">
          {{ option.label }}<span>{{ counts[option.id] }}</span>
        </button>
        <button type="button" class="is-conflict-filter" :class="{ 'is-active': onlyConflicts }" @click="onlyConflicts = !onlyConflicts">
          <TriangleAlert :size="11" />只看冲突<span>{{ conflictCount }}</span>
        </button>
      </div>
      <label class="facts-search">
        <Search :size="13" />
        <input v-model="query" type="search" placeholder="搜索事实文本…" aria-label="搜索事实" />
      </label>
    </div>

    <div v-if="loading" class="facts-loading"><LoaderCircle :size="16" class="sub-spin" />正在读取事实库…</div>
    <div v-else-if="!facts.length" class="facts-empty">
      <Sparkles :size="22" />
      <b>事实库还是空的</b>
      <p>执行一次「速览与溯源」后，主张会先成为事实候选，核验完成后写回状态和证据。</p>
    </div>
    <div v-else-if="!filtered.length" class="facts-empty">
      <Search :size="20" />
      <b>没有匹配的事实</b>
      <p>换个检索词，或切回「全部事实」。</p>
    </div>
    <div v-else class="fact-index-groups">
      <section v-for="group in groups" :key="group.key" class="fact-index-group" :class="`is-${group.key}`">
        <div class="fact-index-group-head">
          <h2>{{ group.title }}</h2>
          <span>{{ group.items.length }} 条 · {{ group.hint }}</span>
        </div>
        <div class="fact-index-list">
          <button v-for="fact in group.items" :key="fact.id" type="button" class="fact-index-row" :class="factStatusClass(fact)" @click="openFact(fact.id)">
            <span class="fact-index-body">
              <span class="fact-index-meta">
                <span class="fact-status">{{ factStatusLabel(fact) }}</span>
                <span v-if="fact.domain === 'history'" class="fact-domain-badge">历史</span>
                <span v-if="fact.noteCount > 1">{{ fact.noteCount }} 篇笔记</span>
                <span v-if="fact.citationCount > 0">{{ fact.citationCount }} 条史料</span>
                <span v-else-if="fact.evidence.length > 1">{{ fact.evidence.length }} 条网页</span>
                <span v-else-if="!fact.evidence.length">{{ fact.domain === 'history' ? '待史料' : '无来源' }}</span>
                <span v-if="fact.related.length">{{ fact.related.length }} 个关联</span>
              </span>
              <span class="fact-index-text">{{ fact.text }}</span>
            </span>
            <ChevronRight :size="15" class="fact-index-arrow" />
          </button>
        </div>
      </section>
    </div>

    <div v-if="mergeHistoryOpen" class="prod-overlay fact-merge-overlay" @click.self="closeMergeHistory">
      <div class="prod-dialog fact-merge-dialog fact-merge-history" role="dialog" aria-modal="true" aria-labelledby="factMergeHistoryTitle">
        <div class="prod-dialog-head">
          <div>
            <h2 id="factMergeHistoryTitle">合并记录</h2>
            <p class="fact-merge-sub">保留被合并事实的完整快照；新版记录可以直接撤销，旧版记录只能查看。</p>
          </div>
          <button type="button" class="fact-merge-close" aria-label="关闭" :disabled="Boolean(undoing)" @click="closeMergeHistory">×</button>
        </div>
        <div class="prod-dialog-body">
          <div v-if="mergeHistoryLoading" class="facts-loading"><LoaderCircle :size="16" class="sub-spin" />正在读取合并记录…</div>
          <div v-else-if="!merges.length" class="facts-empty is-compact"><History :size="20" /><b>还没有合并记录</b><p>在事实详情里确认合并重复事实后，这里会留下可审计的记录。</p></div>
          <div v-else class="fact-merge-history-list">
            <article v-for="merge in merges" :key="merge.id" class="fact-merge-log" :class="{ 'is-undone': merge.undoneAt }">
              <div class="fact-merge-log-head">
                <span>{{ merge.undoneAt ? '已撤销' : '已合并' }}</span>
                <time>{{ formatMergedAt(merge.mergedAt) }}</time>
              </div>
              <p><b>保留</b>{{ merge.targetText }}</p>
              <p><b>合并</b>{{ merge.sourceText }}</p>
              <div class="fact-merge-log-foot">
                <small>被合并状态：{{ statusLabel(merge.sourceStatus) }}</small>
                <button v-if="merge.canUndo" type="button" class="prod-btn is-small is-danger" :disabled="Boolean(undoing)" @click="undoMerge(merge)">
                  <LoaderCircle v-if="undoing === merge.id" :size="12" class="sub-spin" /><Undo2 v-else :size="12" />{{ undoing === merge.id ? '撤销中…' : '撤销这次合并' }}
                </button>
                <span v-else-if="merge.undoneAt" class="fact-merge-log-note">已恢复源事实</span>
                <span v-else class="fact-merge-log-note">旧版记录，无完整快照</span>
              </div>
            </article>
          </div>
          <p v-if="mergeHistoryError" class="fact-merge-error">{{ mergeHistoryError }}</p>
        </div>
        <div class="prod-dialog-foot">
          <button type="button" class="prod-btn" :disabled="Boolean(undoing)" @click="closeMergeHistory">关闭</button>
        </div>
      </div>
    </div>
  </div>
</template>
