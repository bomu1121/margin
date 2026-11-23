<script setup lang="ts">
import { computed, nextTick, onMounted, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { ArrowLeft, BookOpen, CheckCircle2, ExternalLink, LoaderCircle, Search } from 'lucide-vue-next'

import * as api from '../lib/api'
import { toSimplified } from '../lib/t2s'
import type { SourceChapter, SourceReaderPage } from '../types'

const route = useRoute()
const router = useRouter()

const detail = ref<SourceReaderPage | null>(null)
const chapters = ref<SourceChapter[]>([])
const loading = ref(false)
const error = ref('')
const notice = ref('')
const searchInput = ref('')
const query = ref('')
const pageInput = ref('1')
const pageSize = 200
let noticeTimer = 0

const sourceId = computed(() => String(route.params.id ?? ''))
const backLabel = '返回史料库'
const totalPages = computed(() => detail.value?.totalPages ?? 1)
const currentPage = computed(() => detail.value?.page ?? 1)
const currentChapter = computed(() => {
  if (!chapters.value.length) return detail.value?.currentChapter ?? ''
  const passageOffset = (currentPage.value - 1) * pageSize
  let title = chapters.value[0].title
  for (const chapter of chapters.value) {
    if (chapter.offset <= passageOffset) title = chapter.title
  }
  return title
})
const currentChapterOffset = computed(() => {
  if (!chapters.value.length) return 0
  const passageOffset = (currentPage.value - 1) * pageSize
  let offset = chapters.value[0].offset
  for (const chapter of chapters.value) {
    if (chapter.offset <= passageOffset) offset = chapter.offset
  }
  return offset
})
const rangeStart = computed(() => (detail.value && detail.value.totalPassages > 0 ? (detail.value.page - 1) * detail.value.pageSize + 1 : 0))
const rangeEnd = computed(() => (detail.value ? Math.min(detail.value.page * detail.value.pageSize, detail.value.totalPassages) : 0))
const noMatch = computed(() => Boolean(query.value && detail.value && detail.value.matchCount === 0))

function typeLabel(value: string): string {
  return ({
    primary: '一手史料',
    secondary: '二手研究',
    archive: '档案',
    newspaper: '报刊',
    reference: '工具书',
    web: '网络资料',
  } as Record<string, string>)[value] ?? value
}

function flash(message: string): void {
  notice.value = message
  window.clearTimeout(noticeTimer)
  noticeTimer = window.setTimeout(() => { if (notice.value === message) notice.value = '' }, 3600)
}

function sourceTitle(source: SourceReaderPage['source']): string {
  const work = toSimplified(source.workTitle)
  const section = toSimplified(source.sectionTitle)
  return source.sourceType === 'web' ? work : `《${work}》${section ? ` · ${section}` : ''}`
}

function escapeHtml(value: string): string {
  return value.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
}

function cleanReadingText(value: string): string {
  return toSimplified(value)
    .trimStart()
    .replace(/(^|[\s。！？；])\d{1,3}\s+(?=[\u4e00-\u9fff])/g, '$1')
    .replace(/^[」』”’｣﹂﹄]+/, '')
    .trim()
}

function highlighted(text: string): string {
  const safe = escapeHtml(cleanReadingText(text))
  const term = toSimplified(query.value.trim())
  if (!term) return safe
  const needle = escapeHtml(term)
  return safe.split(needle).join(`<mark>${needle}</mark>`)
}

async function copyPage(): Promise<void> {
  if (!detail.value) return
  try {
    await navigator.clipboard.writeText(detail.value.paragraphs.map(cleanReadingText).join('\n\n'))
    flash('已复制本页正文')
  } catch {
    flash('复制失败，请手动选择文本')
  }
}

function back(): void {
  void router.push('/sources')
}

function openUrl(url: string): void {
  if (url) void api.openExternalUrl(url)
}

async function load(page = 1): Promise<void> {
  if (!sourceId.value) return
  loading.value = true
  error.value = ''
  const chaptersPromise = chapters.value.length
    ? Promise.resolve(chapters.value)
    : api.productionSourceChapters(sourceId.value).catch(() => [])
  try {
    detail.value = await api.productionSourceReader(sourceId.value, query.value, Math.max(1, page), pageSize)
    pageInput.value = String(detail.value.page)
    chapters.value = await chaptersPromise
  } catch (cause) {
    detail.value = null
    error.value = api.errorText(cause)
  } finally {
    loading.value = false
  }
  if (query.value) {
    await nextTick()
    if (document.fonts?.ready) await document.fonts.ready
    document.querySelector<HTMLElement>('.source-reader mark')?.scrollIntoView({
      block: 'center',
      inline: 'nearest',
      behavior: 'auto',
    })
  }
}

async function search(): Promise<void> {
  query.value = searchInput.value.trim()
  await load(1)
}

async function clearSearch(): Promise<void> {
  searchInput.value = ''
  query.value = ''
  await load(1)
}

async function goPage(value: number): Promise<void> {
  if (loading.value) return
  const next = Math.min(Math.max(1, Math.round(value)), totalPages.value)
  await load(next)
}

function goChapter(offset: number): void {
  void goPage(Math.floor(offset / pageSize) + 1)
}

function onChapterChange(event: Event): void {
  goChapter(Number((event.target as HTMLSelectElement).value))
}

function jumpPage(): void {
  const value = Number(pageInput.value)
  if (!Number.isFinite(value)) {
    pageInput.value = String(currentPage.value)
    return
  }
  void goPage(value)
}

function resetAndLoad(): void {
  searchInput.value = ''
  query.value = ''
  chapters.value = []
  void load(1)
}

onMounted(() => load(1))
watch(sourceId, resetAndLoad)
</script>

<template>
  <div class="source-detail-page">
    <div class="fact-detail-top">
      <button type="button" class="fact-detail-back" @click="back"><ArrowLeft :size="13" />{{ backLabel }}</button>
    </div>

    <div v-if="notice" class="facts-notice"><CheckCircle2 :size="14" />{{ notice }}</div>
    <div v-if="error" class="prod-error-box"><p>{{ error }}</p></div>
    <div v-if="loading" class="facts-loading"><LoaderCircle :size="16" class="sub-spin" />正在打开全文…</div>
    <div v-else-if="!detail" class="facts-empty">
      <BookOpen :size="22" /><b>找不到这份史料</b><p>它可能已经被删除。</p>
      <button type="button" class="prod-btn is-small" @click="back">{{ backLabel }}</button>
    </div>

    <template v-else>
      <header class="source-detail-head">
        <div class="fact-detail-kicker">史料 / {{ typeLabel(detail.source.sourceType) }}</div>
        <h1>{{ sourceTitle(detail.source) }}</h1>
        <p class="source-detail-byline">{{ detail.source.sourceType === 'web' ? '网页证据' : (toSimplified([detail.source.author, detail.source.compiler, detail.source.era, detail.source.edition, detail.source.volume].filter(Boolean).join(' · ')) || '尚未填写作者/版本') }}</p>
        <p v-if="[detail.source.publisher, detail.source.publishYear, detail.source.repository].some(Boolean)" class="source-detail-byline source-detail-publisher">{{ toSimplified([detail.source.publisher, detail.source.publishYear, detail.source.repository].filter(Boolean).join(' · ')) }}</p>
        <p v-if="detail.source.authenticityNote" class="source-detail-authenticity">{{ detail.source.authenticityNote }}</p>
        <div class="source-detail-links">
          <button v-if="detail.source.url" type="button" class="source-link-button" @click="openUrl(detail.source.url)"><ExternalLink :size="12" />全文/目录</button>
          <button v-if="detail.source.scanUrl" type="button" class="source-link-button" @click="openUrl(detail.source.scanUrl)"><ExternalLink :size="12" />扫描页</button>
          <button type="button" class="source-link-button" @click="copyPage">复制本页</button>
          <span v-if="detail.source.repository">{{ detail.source.repository }}<template v-if="detail.source.callNumber"> · {{ detail.source.callNumber }}</template></span>
        </div>
      </header>

      <div class="source-reader-toolbar">
        <form class="source-reader-search" @submit.prevent="search">
          <Search :size="14" />
          <input v-model="searchInput" type="search" placeholder="在本书中搜索原文…" aria-label="在本书中搜索原文" />
          <button type="submit" class="prod-btn is-small">搜索</button>
          <button v-if="query" type="button" class="prod-btn is-small" @click="clearSearch">清除</button>
        </form>
        <div class="source-reader-meta">
          <label v-if="chapters.length > 1" class="source-chapter-picker">卷/篇
            <select :value="currentChapterOffset" @change="onChapterChange">
              <option v-for="chapter in chapters" :key="chapter.offset" :value="chapter.offset">{{ chapter.title }} · {{ chapter.passages.toLocaleString() }} 段</option>
            </select>
          </label>
          <div v-if="detail.totalPages > 1" class="source-passage-pager">
            <button type="button" class="prod-btn is-small" :disabled="loading || currentPage <= 1" @click="goPage(currentPage - 1)">上一页</button>
            <span>第 {{ currentPage.toLocaleString() }} / {{ totalPages.toLocaleString() }} 页</span>
            <button type="button" class="prod-btn is-small" :disabled="loading || currentPage >= totalPages" @click="goPage(currentPage + 1)">下一页</button>
            <label>跳至 <input v-model="pageInput" type="number" min="1" :max="totalPages" @keydown.enter.prevent="jumpPage" @blur="jumpPage" /> 页</label>
          </div>
          <div class="source-reader-status">
            <span>范围 {{ rangeStart.toLocaleString() }}–{{ rangeEnd.toLocaleString() }} / {{ detail.totalPassages.toLocaleString() }} 段</span>
            <span v-if="query">命中 {{ detail.matchCount.toLocaleString() }} 处</span>
          </div>
        </div>
      </div>

      <article v-if="!noMatch" class="source-reader">
        <h2 v-if="currentChapter" class="source-reader-chapter">{{ currentChapter }}</h2>
        <p v-for="(paragraph, index) in detail.paragraphs" :key="index" v-html="highlighted(paragraph)"></p>
        <p v-if="!detail.paragraphs.length" class="source-reader-empty">这一页还没有正文。</p>
      </article>
      <div v-else class="source-reader-empty">
        没有找到「{{ query }}」。换一个词，或清除搜索。
      </div>
    </template>
  </div>
</template>
