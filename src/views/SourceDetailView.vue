<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { ArrowLeft, BookOpen, CheckCircle2, ExternalLink, LoaderCircle, Quote, Search } from 'lucide-vue-next'

import * as api from '../lib/api'
import type { SourcePage, SourcePassage } from '../types'

const route = useRoute()
const router = useRouter()

const detail = ref<SourcePage | null>(null)
const loading = ref(false)
const error = ref('')
const notice = ref('')
const searchInput = ref('')
const query = ref('')
const pageInput = ref('1')
const pageSize = 50
let noticeTimer = 0

const sourceId = computed(() => String(route.params.id ?? ''))
const backLabel = '返回史料库'
const totalPages = computed(() => Math.max(1, Math.ceil((detail.value?.total ?? 0) / pageSize)))
const currentPage = computed(() => (detail.value ? Math.floor(detail.value.offset / pageSize) + 1 : 1))
const rangeStart = computed(() => (detail.value && detail.value.total > 0 ? detail.value.offset + 1 : 0))
const rangeEnd = computed(() => (detail.value ? detail.value.offset + detail.value.passages.length : 0))

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

function passageLocator(passage: SourcePassage): string {
  const value = passage.locator || [passage.volume, passage.chapter, passage.page].filter(Boolean).join(' · ')
  return value === '全卷' ? '' : value
}

function sourceTitle(source: SourcePage['source']): string {
  return source.sourceType === 'web' ? source.workTitle : `《${source.workTitle}》${source.sectionTitle ? ` · ${source.sectionTitle}` : ''}`
}

function back(): void {
  void router.push('/sources')
}

function openUrl(url: string): void {
  if (url) void api.openExternalUrl(url)
}

async function copyText(value: string): Promise<void> {
  try {
    await navigator.clipboard.writeText(value)
    flash('已复制原文')
  } catch {
    flash('复制失败，请手动选择文本')
  }
}

async function load(page = 1): Promise<void> {
  if (!sourceId.value) return
  const nextPage = Math.max(1, page)
  loading.value = true
  error.value = ''
  try {
    detail.value = await api.productionSource(
      sourceId.value,
      query.value,
      pageSize,
      (nextPage - 1) * pageSize,
    )
    pageInput.value = String(nextPage)
  } catch (cause) {
    detail.value = null
    error.value = api.errorText(cause)
  } finally {
    loading.value = false
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
  if (!detail.value || loading.value) return
  const nextPage = Math.min(Math.max(1, Math.round(value)), totalPages.value)
  await load(nextPage)
}

function jumpPage(): void {
  const value = Number(pageInput.value)
  if (!Number.isFinite(value)) {
    pageInput.value = String(detail.value ? Math.floor((detail.value.offset || 0) / pageSize) + 1 : 1)
    return
  }
  void goPage(value)
}

function resetAndLoad(): void {
  searchInput.value = ''
  query.value = ''
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
    <div v-if="loading" class="facts-loading"><LoaderCircle :size="16" class="sub-spin" />正在读取史料…</div>
    <div v-else-if="!detail" class="facts-empty">
      <BookOpen :size="22" /><b>找不到这份史料</b><p>它可能已经被删除。</p>
      <button type="button" class="prod-btn is-small" @click="back">{{ backLabel }}</button>
    </div>

    <template v-else>
      <header class="source-detail-head">
        <div class="fact-detail-kicker">史料 / {{ typeLabel(detail.source.sourceType) }}</div>
        <h1>{{ sourceTitle(detail.source) }}</h1>
        <p class="source-detail-byline">{{ detail.source.sourceType === 'web' ? '网页证据' : ([detail.source.author, detail.source.compiler, detail.source.era, detail.source.edition, detail.source.volume].filter(Boolean).join(' · ') || '尚未填写作者/版本') }}</p>
        <p v-if="[detail.source.publisher, detail.source.publishYear, detail.source.repository].some(Boolean)" class="source-detail-byline source-detail-publisher">{{ [detail.source.publisher, detail.source.publishYear, detail.source.repository].filter(Boolean).join(' · ') }}</p>
        <p v-if="detail.source.authenticityNote" class="source-detail-authenticity">{{ detail.source.authenticityNote }}</p>
        <div class="source-detail-links">
          <button v-if="detail.source.url" type="button" class="source-link-button" @click="openUrl(detail.source.url)"><ExternalLink :size="12" />全文/目录</button>
          <button v-if="detail.source.scanUrl" type="button" class="source-link-button" @click="openUrl(detail.source.scanUrl)"><ExternalLink :size="12" />扫描页</button>
          <span v-if="detail.source.repository">{{ detail.source.repository }}<template v-if="detail.source.callNumber"> · {{ detail.source.callNumber }}</template></span>
        </div>
      </header>

      <div class="source-detail-layout">
        <main>
          <section class="source-detail-section">
            <div class="source-detail-section-head"><h2>原文段落</h2><span>{{ rangeStart.toLocaleString() }}–{{ rangeEnd.toLocaleString() }} / {{ detail.total.toLocaleString() }} 段</span></div>
            <div v-if="detail.total > pageSize" class="source-passage-pager">
              <button type="button" class="prod-btn is-small" :disabled="loading || currentPage <= 1" @click="goPage(currentPage - 1)">上一页</button>
              <span>第 {{ currentPage.toLocaleString() }} / {{ totalPages.toLocaleString() }} 页</span>
              <button type="button" class="prod-btn is-small" :disabled="loading || currentPage >= totalPages" @click="goPage(currentPage + 1)">下一页</button>
              <label>跳至 <input v-model="pageInput" type="number" min="1" :max="totalPages" @keydown.enter.prevent="jumpPage" @blur="jumpPage" /> 页</label>
            </div>
            <form class="source-passage-search" @submit.prevent="search">
              <Search :size="13" />
              <input v-model="searchInput" type="search" placeholder="在本库中搜索原文…" aria-label="在本库中搜索原文" />
              <button type="submit" class="prod-btn is-small">搜索</button>
              <button v-if="query" type="button" class="prod-btn is-small" @click="clearSearch">清除</button>
            </form>
            <div v-if="detail.passages.length" class="source-passage-list">
              <article v-for="passage in detail.passages" :key="passage.id" class="source-passage">
                <div v-if="passageLocator(passage) || passage.scanUrl" class="source-passage-head">
                  <span>{{ passageLocator(passage) || '未标卷页' }}</span>
                  <div>
                    <button type="button" @click="copyText(passage.originalText)">复制</button>
                    <button v-if="passage.scanUrl" type="button" @click="openUrl(passage.scanUrl)">扫描页</button>
                  </div>
                </div>
                <button v-else type="button" class="source-passage-copy" @click="copyText(passage.originalText)">复制</button>
                <blockquote>{{ passage.originalText }}</blockquote>
                <p v-if="passage.translation" class="source-translation">{{ passage.translation }}</p>
              </article>
            </div>
            <div v-else class="facts-empty is-compact">
              <Quote :size="20" />
              <b>{{ query ? '没有匹配的原文' : '还没有原文' }}</b>
              <p>{{ query ? '换一个关键词，或清除搜索。' : '可从笔记右栏的出处卡片补原文。' }}</p>
            </div>
          </section>
        </main>
      </div>
    </template>
  </div>
</template>
