<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { ArrowLeft, BookOpen, CheckCircle2, ExternalLink, LoaderCircle, Quote } from 'lucide-vue-next'

import { factStatusLabel } from '../lib/facts'
import * as api from '../lib/api'
import type { SourceDetail, SourcePassage } from '../types'

const route = useRoute()
const router = useRouter()

const detail = ref<SourceDetail | null>(null)
const loading = ref(false)
const error = ref('')
const notice = ref('')
let noticeTimer = 0

const sourceId = computed(() => String(route.params.id ?? ''))
const fromFactId = computed(() => (route.query.from === 'fact' ? String(route.query.factId ?? '') : ''))
const backLabel = computed(() => (fromFactId.value ? '返回事实' : '返回史料库'))

const citedFacts = computed(() => {
  const seen = new Set<string>()
  return (detail.value?.facts ?? []).filter((fact) => {
    if (seen.has(fact.factId)) return false
    seen.add(fact.factId)
    return true
  })
})

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

function relationLabel(value: string): string {
  return ({ supports: '支持', contradicts: '矛盾', variant: '异文', context: '背景' } as Record<string, string>)[value] ?? value
}

function flash(message: string): void {
  notice.value = message
  window.clearTimeout(noticeTimer)
  noticeTimer = window.setTimeout(() => { if (notice.value === message) notice.value = '' }, 3600)
}

function passageLocator(passage: SourcePassage): string {
  return passage.locator || [passage.volume, passage.chapter, passage.page].filter(Boolean).join(' · ')
}

function sourceTitle(source: SourceDetail['source']): string {
  return source.sourceType === 'web' ? source.workTitle : `《${source.workTitle}》${source.sectionTitle ? ` · ${source.sectionTitle}` : ''}`
}

function back(): void {
  if (fromFactId.value) {
    void router.push(`/facts/${fromFactId.value}`)
    return
  }
  void router.push('/sources')
}

function openFact(factId: string): void {
  if (factId) void router.push({ path: `/facts/${factId}`, query: { from: 'source', sourceId: sourceId.value } })
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

async function load(): Promise<void> {
  if (!sourceId.value) return
  loading.value = true
  error.value = ''
  try {
    detail.value = await api.productionSource(sourceId.value)
  } catch (cause) {
    detail.value = null
    error.value = api.errorText(cause)
  } finally {
    loading.value = false
  }
}

onMounted(load)
watch(sourceId, load)
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
            <div class="source-detail-section-head"><h2>原文段落</h2><span>{{ detail.passages.length }} 段</span></div>
            <div v-if="detail.passages.length" class="source-passage-list">
              <article v-for="passage in detail.passages" :key="passage.id" class="source-passage">
                <div class="source-passage-head">
                  <span>{{ passageLocator(passage) || '未标卷页' }}</span>
                  <div>
                    <button type="button" @click="copyText(passage.originalText)">复制</button>
                    <button v-if="passage.scanUrl" type="button" @click="openUrl(passage.scanUrl)">扫描页</button>
                  </div>
                </div>
                <blockquote>{{ passage.originalText }}</blockquote>
                <p v-if="passage.translation" class="source-translation">{{ passage.translation }}</p>
              </article>
            </div>
            <div v-else class="facts-empty is-compact"><Quote :size="20" /><b>还没有原文</b><p>自动溯源还没有为这份史料写入可逐字引用的原文。</p></div>
          </section>

          <section class="source-detail-section">
            <div class="source-detail-section-head"><h2>引用它的事实</h2><span>{{ detail.source.factCount }} 条</span></div>
            <div v-if="citedFacts.length" class="source-cited-facts">
              <button v-for="fact in citedFacts" :key="fact.factId" type="button" class="source-cited-fact" @click="openFact(fact.factId)">
                <span class="source-cited-relation">{{ relationLabel(fact.relation) }}</span>
                <b>{{ fact.text }}</b>
                <small>{{ factStatusLabel({ ...fact, domain: 'history', citationCount: 1 }) }} · {{ fact.noteIds.length }} 篇笔记引用</small>
                <q v-if="fact.quote">{{ fact.quote }}</q>
              </button>
            </div>
            <p v-else class="facts-empty is-compact">还没有事实引用这份史料。</p>
          </section>
        </main>
      </div>
    </template>
  </div>
</template>
