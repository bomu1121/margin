<script setup lang="ts">
import { computed, nextTick, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'

import NoteBlock from '../components/NoteBlock.vue'
import RelationGraph from '../components/RelationGraph.vue'
import * as api from '../lib/api'
import { parseNote, type Block, type ParsedNote } from '../lib/markdown'
import { planAnchorRelocations } from '../lib/annotationAnchor'
import type { Annotation, NoteDetail } from '../types'
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
const loading = ref(false)
const error = ref('')
const composing = ref<string | null>(null)
const composerText = ref('')
const composerKind = ref('标记')
const docScroll = ref<HTMLElement | null>(null)
const relocating = ref(false)
const relocateNotice = ref('')
const relocateError = ref('')

const note = computed(() => detail.value?.meta)
const artifact = computed(() => detail.value?.artifact ?? null)

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

async function loadAnnotations(id: string): Promise<void> {
  annotations.value = await api.listAnnotations(id)
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
      return
    }
    detail.value = data
    parsed.value = parseNote(data.body)
    await loadAnnotations(id)
    await nextTick()
    docScroll.value?.scrollTo({ top: 0 })
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
</script>

<template>
  <div class="note-layout">
    <div ref="docScroll" class="doc-scroll">
      <article v-if="detail && parsed" class="doc">
        <header>
          <div class="kicker">{{ typeLabel }} / {{ note?.domain }}</div>
          <h1>{{ note?.title }}</h1>
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

        <div class="meta">
          <span>{{ note?.up }}</span>
          <span>{{ note?.date }}</span>
          <span v-if="note?.runId" class="run">{{ note.runId }}</span>
          <span>{{ note?.chars }} 字</span>
          <span v-if="annotations.length">{{ annotations.length }} 条标注</span>
        </div>
        <div class="tags">
          <router-link
            v-for="tag in note?.tags ?? []"
            :key="tag"
            class="tag"
            :to="{ path: '/tags', query: { tag } }"
          >
            {{ tag }}
          </router-link>
        </div>

        <div v-if="artifact" class="prov-strip">
          <span class="prov-badge">ScribeFlow 生成</span>
          <span v-if="artifact.run">运行 {{ artifact.run.runId || '—' }}<template v-if="artifact.run.prompt"> · {{ artifact.run.prompt }}</template></span>
          <span v-if="artifact.generatedAt">{{ formatGenerated(artifact.generatedAt) }}</span>
          <button type="button" @click="openArtifact">查看来源与运行</button>
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

    <aside class="rail">
      <section v-if="artifact" class="rail-sec prov-rail">
        <div class="rail-head"><b>来源与制作</b><span>只读</span></div>
        <div class="prov-source">
          <span class="prov-source-mark">{{ (artifact.source?.label || artifact.source?.kind || 'S').slice(0, 1) }}</span>
          <span class="prov-source-copy">
            <b>{{ artifact.title }}</b>
            <small>{{ artifact.source?.label || artifact.source?.kind || 'ScribeFlow' }}<template v-if="artifact.source?.author"> · {{ artifact.source.author }}</template></small>
          </span>
        </div>
        <dl class="prov-kv">
          <div v-if="artifact.run"><dt>运行</dt><dd class="mono">{{ artifact.run.runId || '—' }}</dd></div>
          <div v-if="artifact.run?.projectName"><dt>工程</dt><dd>{{ artifact.run.projectName }}</dd></div>
          <div v-if="artifact.generatedAt"><dt>生成</dt><dd>{{ formatGenerated(artifact.generatedAt) }}</dd></div>
          <div><dt>标注</dt><dd>{{ artifact.annotationCount }} 条</dd></div>
        </dl>
        <button type="button" class="btn prov-open" @click="openArtifact">在制作台查看</button>
      </section>

      <RelationGraph v-if="note" :notes="library.notes" :note-id="note.id" />

      <section class="rail-sec">
        <div class="rail-head">
          <b>标注</b>
          <span>{{ annotations.length ? `${annotations.length} 条` : '0 条' }}</span>
        </div>

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

      <section v-if="note" class="rail-sec">
        <div class="rail-head"><b>文件</b><span>只读</span></div>
        <p class="cap" style="word-break: break-all">{{ note.path }}</p>
        <div style="margin-top: 8px; display: flex; gap: 6px">
          <button type="button" class="btn" style="padding: 4px 9px; font-size: 12px" @click="copyPath">
            复制路径
          </button>
          <router-link class="btn" style="padding: 4px 9px; font-size: 12px; text-decoration: none" :to="{ path: '/tags' }">
            标签台
          </router-link>
        </div>
      </section>
    </aside>
  </div>
</template>
