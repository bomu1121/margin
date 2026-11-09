<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useRouter } from 'vue-router'
import { BookMarked, ChevronRight, ListChecks, LoaderCircle, RefreshCw, Search } from 'lucide-vue-next'

import * as api from '../lib/api'
import type { Source } from '../types'

const router = useRouter()
const sources = ref<Source[]>([])
const loading = ref(false)
const error = ref('')
const query = ref('')
const sourceType = ref('')
const pendingCount = ref(0)

const typeOptions = [
  { id: '', label: '全部' },
  { id: 'primary', label: '一手' },
  { id: 'secondary', label: '二手' },
  { id: 'archive', label: '档案' },
  { id: 'newspaper', label: '报刊' },
  { id: 'reference', label: '工具书' },
]

const filtered = computed(() => {
  const needle = query.value.trim().toLowerCase()
  return sources.value.filter((source) => {
    if (sourceType.value && source.sourceType !== sourceType.value) return false
    if (!needle) return true
    return [source.workTitle, source.sectionTitle, source.author, source.edition, source.volume]
      .join(' ')
      .toLowerCase()
      .includes(needle)
  })
})

const typeCounts = computed<Record<string, number>>(() => {
  const counts: Record<string, number> = { '': sources.value.length }
  for (const source of sources.value) {
    counts[source.sourceType] = (counts[source.sourceType] ?? 0) + 1
  }
  return counts
})

function typeLabel(value: string): string {
  return ({
    primary: '一手',
    secondary: '二手',
    archive: '档案',
    newspaper: '报刊',
    reference: '工具书',
    web: '网络',
  } as Record<string, string>)[value] ?? value
}

function typeClass(value: string): string {
  return `is-${value || 'primary'}`
}

function sourceTitle(source: Source): string {
  return source.sourceType === 'web' ? source.workTitle : `《${source.workTitle}》${source.sectionTitle ? ` · ${source.sectionTitle}` : ''}`
}

function sourceMeta(source: Source): string {
  if (source.sourceType === 'web') return '网页证据'
  return [source.author, source.era, source.edition, source.volume].filter(Boolean).join(' · ')
}

function openSource(id: string): void {
  if (id) void router.push(`/sources/${id}`)
}

async function load(): Promise<void> {
  loading.value = true
  error.value = ''
  try {
    const [nextSources, pending] = await Promise.all([
      api.productionSources('', '', 300),
      api.productionHistoryCandidatesAll('candidate', 500).catch(() => []),
    ])
    sources.value = nextSources
    pendingCount.value = pending.length
  } catch (cause) {
    error.value = api.errorText(cause)
  } finally {
    loading.value = false
  }
}

function openQueue(): void {
  void router.push('/sources/pending')
}

onMounted(load)
</script>

<template>
  <div class="sources-page">
    <header class="sources-head">
      <div>
        <div class="prod-eyebrow">知识库 / 史料</div>
        <h1>史料库</h1>
        <p>史料应来自自动溯源：书名、版本、卷页和逐字原文。它不是手工录入的资料柜；没有原文的记录不会进入这里。</p>
      </div>
      <div class="sources-actions">
        <button type="button" class="prod-btn is-primary" @click="openQueue"><ListChecks :size="13" />待补录<span v-if="pendingCount" class="facts-count-badge">{{ pendingCount }}</span></button>
        <button type="button" class="prod-btn" :disabled="loading" @click="load"><RefreshCw :size="13" :class="{ 'sub-spin': loading }" />刷新</button>
      </div>
    </header>

    <div v-if="error" class="prod-error-box"><p>{{ error }}</p></div>

    <div class="sources-toolbar">
      <div class="sources-filters" role="group" aria-label="史料类型">
        <button v-for="option in typeOptions" :key="option.id" type="button" :class="{ 'is-active': sourceType === option.id }" @click="sourceType = option.id">
          {{ option.label }}<span class="sources-filter-count">{{ typeCounts[option.id] ?? 0 }}</span>
        </button>
      </div>
      <label class="sources-search">
        <Search :size="13" />
        <input v-model="query" type="search" placeholder="书名、作者、篇名…" aria-label="搜索史料" />
      </label>
    </div>

    <div v-if="loading" class="facts-loading"><LoaderCircle :size="16" class="sub-spin" />正在读取史料库…</div>
    <div v-else-if="!sources.length" class="facts-empty">
      <BookMarked :size="22" />
      <b>还没有自动溯源写入的史料</b>
      <p>历史事实完成“检索 → 抓取原文 → 逐字对齐”后，史料、原文段落和引用关系会出现在这里。没有把握的候选不会入库。</p>
      <button v-if="pendingCount" type="button" class="prod-btn is-small" @click="openQueue">去待补录（{{ pendingCount }}）</button>
    </div>
    <div v-else-if="!filtered.length" class="facts-empty">
      <Search :size="20" />
      <b>没有匹配的史料</b>
      <p>换个书名/作者，或切回全部。</p>
    </div>
    <div v-else class="source-index-list">
      <button v-for="source in filtered" :key="source.id" type="button" class="source-index-row" @click="openSource(source.id)">
        <span class="source-index-body">
          <span class="source-index-head">
            <span class="source-type" :class="typeClass(source.sourceType)">{{ typeLabel(source.sourceType) }}</span>
            <b>{{ sourceTitle(source) }}</b>
          </span>
          <span class="source-index-meta">{{ sourceMeta(source) || '未填作者/版本' }}</span>
          <span class="source-index-counts">{{ source.passageCount }} 段原文 · {{ source.citationCount }} 条引用 · {{ source.factCount }} 条事实</span>
        </span>
        <ChevronRight :size="15" class="fact-index-arrow" />
      </button>
    </div>

    <section v-if="sources.length" class="sources-guide">
      <div class="sources-guide-head">
        <div>
          <div class="prod-eyebrow">史料从哪来</div>
          <h2>不是手工建库，是自动溯源走到最后一步</h2>
        </div>
        <button v-if="pendingCount" type="button" class="prod-btn is-small" @click="openQueue"><ListChecks :size="12" />去待补录（{{ pendingCount }}）</button>
      </div>
      <div class="sources-guide-steps">
        <article><b>01 · 自动找候选</b><span>历史事实在 trace 里规划书名/篇名，去 ctext、Wikisource、IA 和目录源找候选。</span></article>
        <article><b>02 · 补录原文</b><span>候选页打开后复制逐字原文；程序切 passage，不让你重填书目。</span></article>
        <article><b>03 · 写入引用</b><span>只有全文和逐字 quote 校验通过才写 citation；没有原文就停在候选池。</span></article>
      </div>
    </section>
  </div>
</template>
