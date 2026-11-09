<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'

import { ChevronRight, Crosshair, PanelRightClose, PanelRightOpen } from 'lucide-vue-next'

import NoteBlock from '../components/NoteBlock.vue'
import RelationGraph from '../components/RelationGraph.vue'
import * as api from '../lib/api'
import { parseNote, type Block, type ParsedNote } from '../lib/markdown'
import { planAnchorRelocations } from '../lib/annotationAnchor'
import type { Annotation, Fact, NoteDetail } from '../types'
import { factHasEvidence, factSourceSummary, factStatusClass, factStatusKey, factStatusLabel } from '../lib/facts'
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
const loading = ref(false)
const error = ref('')
const composing = ref<string | null>(null)
const composerText = ref('')
const composerKind = ref('标记')
type RailTab = 'facts' | 'notes' | 'links' | 'info'
const railTab = ref<RailTab>('facts')
const factFilter = ref<'all' | 'attention'>('all')
const RAIL_MIN = 300
const RAIL_MAX = 560
const viewportWidth = ref(window.innerWidth)
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

const note = computed(() => detail.value?.meta)
const artifact = computed(() => detail.value?.artifact ?? null)
const sourceLinkLabel = computed(() => artifact.value?.source?.kind === 'bili' ? '原视频' : '原文')
const titleParts = computed(() => {
  const title = note.value?.title ?? ''
  return title.match(/[^：:，,、；;]+[：:，,、；;]?/g)?.filter(Boolean) ?? [title]
})

const typeLabel = computed(() => {
  const tag = note.value?.tags.find((item) => item.startsWith('类型/'))
  return tag ? tag.split('/').pop() ?? '笔记' : '笔记'
})

const blockOrder = computed(() => {
  const map: Record<string, number> = {}
  parsed.value?.allBlocks.forEach((block, index) => {
    map[block.id] = index
  })
  return map
})

const sortedAnnotations = computed(() =>
  [...annotations.value].sort(
    (a, b) => (blockOrder.value[a.blockId] ?? 99999) - (blockOrder.value[b.blockId] ?? 99999),
  ),
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

const railFacts = computed(() => {
  const priority: Record<string, number> = { disputed: 0, unsupported: 1, candidate: 2, verified: 3 }
  const source = factFilter.value === 'attention'
    ? facts.value.filter((fact) => !factHasEvidence(fact))
    : facts.value
  return [...source].sort((left, right) =>
    (priority[factStatusKey(left)] ?? 9) - (priority[factStatusKey(right)] ?? 9) || right.updatedAt - left.updatedAt,
  )
})

const markMap = computed(() => {
  const map: Record<string, number> = {}
  validAnnotations.value.forEach((anno, index) => {
    if (map[anno.blockId] === undefined) map[anno.blockId] = index + 1
  })
  return map
})

const composingBlock = computed<Block | null>(() => {
  if (!composing.value || !parsed.value) return null
  return parsed.value.allBlocks.find((block) => block.id === composing.value) ?? null
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

function formatGenerated(value: string): string {
  if (!value) return ''
  if (/^\d{4}-\d{2}-\d{2}$/.test(value)) return value.slice(5)
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) return value
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

async function loadFacts(id: string, runId: string): Promise<void> {
  try {
    facts.value = await api.productionNoteFacts(id, runId)
  } catch {
    facts.value = []
  }
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
  composing.value = null
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
      return
    }
    detail.value = data
    parsed.value = parseNote(data.body)
    factLocateError.value = ''
    await Promise.all([loadAnnotations(id), loadFacts(id, data.artifact?.run?.runId ?? '')])
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

watch(composing, (value) => {
  if (value) railTab.value = 'notes'
})

function startAnnotate(blockId: string): void {
  composing.value = blockId
  composerText.value = ''
  composerKind.value = '标记'
  void nextTick(() => {
    const box = document.querySelector<HTMLTextAreaElement>('.composer textarea')
    box?.focus()
  })
}

async function saveAnnotation(): Promise<void> {
  if (!composing.value || !composingBlock.value || !note.value) return
  const block = composingBlock.value
  try {
    await api.addAnnotation({
      noteId: note.value.id,
      blockId: block.id,
      quote: block.text,
      body: composerText.value.trim(),
      kind: composerKind.value,
    })
    await loadAnnotations(note.value.id)
    await refreshNotes()
    composing.value = null
    composerText.value = ''
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
  el.scrollIntoView({ behavior: 'smooth', block: 'center' })
  flash(el)
}

function locateAnnotation(anno: Annotation): void {
  locateBlock(anno.blockId)
}

function locateCard(blockId: string): void {
  const anno = validAnnotations.value.find((item) => item.blockId === blockId)
  if (!anno) return
  const el = document.querySelector(`[data-anno="${anno.id}"]`)
  if (el) {
    el.scrollIntoView({ behavior: 'smooth', block: 'center' })
    flash(el)
  }
}

async function copyPath(): Promise<void> {
  if (!note.value) return
  await navigator.clipboard.writeText(note.value.path)
}

function onViewportResize(): void {
  viewportWidth.value = window.innerWidth
  window.dispatchEvent(new Event('yb-scroll-rescan'))
}

onMounted(() => {
  window.addEventListener('resize', onViewportResize)
})

onBeforeUnmount(() => {
  window.removeEventListener('resize', onViewportResize)
  window.removeEventListener('pointermove', onRailMove)
  window.removeEventListener('pointercancel', onRailUp)
  document.body.classList.remove('is-resizing-panel')
})
</script>

<template>
  <div class="note-layout" :class="{ 'rail-collapsed': railCollapsed, 'is-resizing': railResizing }" :style="railStyle">
    <div ref="docScroll" class="doc-scroll">
      <article v-if="detail && parsed" class="doc">
        <header>
          <div class="kicker">{{ typeLabel }} / {{ note?.domain }}</div>
          <h1 class="note-title"><span v-for="(part, index) in titleParts" :key="index" class="note-title-part">{{ part }}</span></h1>
        </header>

        <div v-if="parsed.lead.length" class="lead-zone">
          <NoteBlock
            v-for="block in parsed.lead"
            :key="block.id"
            :block="block"
            :mark="markMap[block.id]"
            :composing="composing === block.id"
            @annotate="startAnnotate"
            @locate="locateCard"
          />
        </div>

        <div class="meta note-byline">
          <span v-if="note?.chars" class="byline-count">{{ note.chars.toLocaleString('zh-CN') }} 字</span>
          <span v-if="annotations.length">{{ annotations.length }} 条标注</span>
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
        <div v-if="note?.tags?.length" class="tags">
          <router-link
            v-for="tag in note?.tags ?? []"
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
                :mark="markMap[block.id]"
                :composing="composing === block.id"
                @annotate="startAnnotate"
                @locate="locateCard"
              />
            </li>
          </ol>
        </section>

        <section v-for="(section, index) in parsed.sections" :key="`${section.heading}-${index}`" class="sec">
          <span class="sec-no">§ {{ pad(index + 1) }}</span>
          <h2 v-if="section.heading">{{ section.heading }}</h2>
          <NoteBlock
            v-for="block in section.blocks"
            :key="block.id"
            :block="block"
            :mark="markMap[block.id]"
            :composing="composing === block.id"
            @annotate="startAnnotate"
            @locate="locateCard"
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
                  :mark="markMap[item.block.id]"
                  :composing="composing === item.block.id"
                  @annotate="startAnnotate"
                  @locate="locateCard"
                />
              </div>
            </div>
          </template>
        </section>

        <div v-if="error" class="empty" style="margin-top: 18px">{{ error }}</div>
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
          <button type="button" role="tab" :aria-selected="railTab === 'facts'" :class="{ 'is-active': railTab === 'facts' }" @click="railTab = 'facts'">
            事实<span>{{ facts.length }}</span>
          </button>
          <button type="button" role="tab" :aria-selected="railTab === 'notes'" :class="{ 'is-active': railTab === 'notes' }" @click="railTab = 'notes'">
            标注<span>{{ annotations.length }}</span>
          </button>
          <button type="button" role="tab" :aria-selected="railTab === 'links'" :class="{ 'is-active': railTab === 'links' }" @click="railTab = 'links'">
            关联
          </button>
          <button type="button" role="tab" :aria-selected="railTab === 'info'" :class="{ 'is-active': railTab === 'info' }" @click="railTab = 'info'">
            信息
          </button>
        </div>
      </div>

      <div class="rail-body">
        <section v-if="railTab === 'facts'" class="rail-panel" role="tabpanel" aria-label="事实">
          <div class="rail-fact-toolbar">
            <span>{{ verifiedFactCount }} 有证据 · {{ attentionFactCount }} 需关注</span>
            <button type="button" :class="{ 'is-active': factFilter === 'attention' }" @click="factFilter = factFilter === 'attention' ? 'all' : 'attention'">
              {{ factFilter === 'attention' ? '全部事实' : '只看需关注' }}
            </button>
          </div>
          <p v-if="factLocateError" class="fact-locate-note is-error">{{ factLocateError }}</p>
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
                <span class="fact-status">{{ factStatusLabel(fact) }}</span>
                <span v-if="fact.domain === 'history'" class="rail-fact-domain">历史</span>
                <span class="rail-fact-top-actions">
                  <button type="button" class="rail-fact-locate" title="在正文里定位" aria-label="在正文里定位" @click.stop="locateFact(fact)">
                    <Crosshair :size="13" />
                  </button>
                  <ChevronRight :size="13" class="rail-fact-arrow" aria-hidden="true" />
                </span>
              </div>
              <p class="rail-fact-text">{{ fact.text }}</p>
              <div class="rail-fact-meta">
                <span>{{ factSourceSummary(fact) }}</span>
                <span v-if="fact.related.length">· {{ fact.related.length }} 个关联</span>
              </div>
            </article>
          </div>
          <div v-else class="empty">
            {{ factFilter === 'attention' ? '没有需要关注的事实。' : '这次生成没有留下事实记录；重新执行溯源后会写入事实库。' }}
          </div>
        </section>

        <section v-else-if="railTab === 'notes'" class="rail-panel" role="tabpanel" aria-label="标注">
          <div v-if="composingBlock" class="composer">
            <div class="composer-q">{{ composingBlock.text.slice(0, 64) }}</div>
            <div class="kinds">
              <label><input v-model="composerKind" type="radio" value="标记" /> 标记</label>
              <label><input v-model="composerKind" type="radio" value="存疑" /> 存疑</label>
              <label><input v-model="composerKind" type="radio" value="待办" /> 待办</label>
            </div>
            <textarea v-model="composerText" placeholder="为什么标这一条？" aria-label="标注内容" />
            <div class="composer-actions">
              <button type="button" class="save" @click="saveAnnotation">保存</button>
              <button type="button" @click="composing = null">取消</button>
            </div>
          </div>

          <div v-if="!annotations.length && !composing" class="empty">
            还没有标注。把鼠标移到正文任意一段，点右侧的「+」；标注会存进本地 SQLite。
          </div>

          <div
            v-for="anno in validAnnotations"
            :key="anno.id"
            class="anno"
            :data-anno="anno.id"
          >
            <div class="anno-top">
              <span class="anno-kind" :class="`kind-${anno.kind}`">{{ anno.kind }}</span>
              <span class="anno-time">{{ formatTime(anno.createdAt) }}</span>
            </div>
            <div class="anno-q">{{ anno.quote }}</div>
            <div v-if="anno.body" class="anno-b">{{ anno.body }}</div>
            <div v-else class="anno-b empty">（只做了高亮）</div>
            <div class="anno-foot">
              <button type="button" @click="locateAnnotation(anno)">定位</button>
              <button type="button" class="danger" @click="removeAnnotation(anno.id)">删除</button>
            </div>
          </div>

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
              <button type="button" class="danger" @click="removeAnnotation(anno.id)">删除</button>
            </div>
          </div>
        </section>

        <section v-else-if="railTab === 'links'" class="rail-panel" role="tabpanel" aria-label="关联">
          <p class="rail-panel-hint">当前笔记与标签、人物、相关笔记的连接。</p>
          <RelationGraph v-if="note" :notes="library.notes" :note-id="note.id" />
        </section>

        <section v-else class="rail-panel" role="tabpanel" aria-label="信息">
          <section v-if="artifact" class="rail-info-block">
            <div class="rail-info-head"><b>来源</b><span>只读</span></div>
            <div class="prov-source">
              <span class="prov-source-mark">{{ (artifact.source?.label || artifact.source?.kind || 'S').slice(0, 1) }}</span>
              <span class="prov-source-copy">
                <b>{{ artifact.title }}</b>
                <small>{{ artifact.source?.label || artifact.source?.kind || 'ScribeFlow' }}<template v-if="artifact.source?.author"> · {{ artifact.source.author }}</template></small>
              </span>
            </div>
            <dl class="prov-kv">
              <div v-if="note?.date"><dt>创建</dt><dd>{{ note.date }}</dd></div>
              <div v-if="artifact.source?.publishedAt"><dt>发布</dt><dd>{{ artifact.source.publishedAt }}</dd></div>
              <div v-if="artifact.run"><dt>运行</dt><dd class="mono">{{ artifact.run.runId || '—' }}</dd></div>
              <div v-if="artifact.generatedAt"><dt>生成</dt><dd>{{ formatGenerated(artifact.generatedAt) }}</dd></div>
            </dl>
            <div class="prov-actions">
              <button v-if="artifact.source?.url" type="button" class="btn prov-open" @click="openSourceVideo">打开{{ sourceLinkLabel }}</button>
              <button type="button" class="btn prov-open" @click="openArtifact">在制作台查看</button>
            </div>
          </section>
          <section class="rail-info-block">
            <div class="rail-info-head"><b>文件</b><span>只读</span></div>
            <p class="cap" style="word-break: break-all">{{ note?.path }}</p>
            <div class="rail-file-actions">
              <button type="button" class="btn" @click="copyPath">复制路径</button>
              <router-link class="btn rail-file-link" :to="{ path: '/tags' }">标签台</router-link>
            </div>
          </section>
          <p v-if="!artifact" class="rail-panel-hint">这篇笔记没有关联的制作来源。</p>
        </section>
      </div>
    </aside>
  </div>
</template>
