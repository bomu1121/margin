<script setup lang="ts">
import { computed, onMounted, reactive, ref } from 'vue'
import { useRouter } from 'vue-router'
import { BookMarked, ChevronRight, LoaderCircle, RefreshCw, Search, Upload } from 'lucide-vue-next'

import * as api from '../lib/api'
import type { NoteCitation, NoteMeta, Source } from '../types'

const router = useRouter()
const citations = ref<NoteCitation[]>([])
const sources = ref<Source[]>([])
const notes = ref<NoteMeta[]>([])
const loading = ref(false)
const error = ref('')
const query = ref('')
const expandedIds = ref<string[]>([])
const importOpen = ref(false)
const importing = ref(false)
const importError = ref('')
const importNotice = ref('')
const importText = ref('')
const importFileLabel = ref('')
const importForm = reactive({ workTitle: '', sectionTitle: '', author: '', era: '' })

const noteById = computed(() => new Map(notes.value.map((note) => [note.id, note])))
const needle = computed(() => query.value.trim().toLowerCase())

function noteTitle(id: string): string {
  return noteById.value.get(id)?.title ?? '未找到笔记'
}

function isLongOriginal(value: string): boolean {
  return [...(value || '').trim()].length > 140
}

function sameText(left: string, right: string): boolean {
  const normalize = (value: string) => (value || '').replace(/[\s，。！？；：、,.!?;:「」『』“”‘’"'（）()《》〈〉…—]/g, '')
  return normalize(left) === normalize(right)
}

function showOriginal(item: NoteCitation): boolean {
  return Boolean(item.originalText) && !sameText(item.originalText, item.noteQuote)
}

function isExpanded(id: string): boolean {
  return expandedIds.value.includes(id)
}

function toggleOriginal(id: string): void {
  expandedIds.value = isExpanded(id)
    ? expandedIds.value.filter((item) => item !== id)
    : [...expandedIds.value, id]
}

function citationWorkLabel(value: string): string {
  const title = (value || '').trim()
  if (!title) return '出处待定'
  if (title.includes('《')) return title
  return title.startsWith('《') ? title : `《${title}》`
}

const WORK_ALIASES: Record<string, string> = {
  三國志: '三国志',
  三國誌: '三国志',
  後漢書: '后汉书',
  後汉書: '后汉书',
  宋書: '宋书',
  魏書: '魏书',
  晉書: '晋书',
  資治通鑑: '资治通鉴',
  資治通鉴: '资治通鉴',
  资治通鑑: '资治通鉴',
  通鑑紀事本末: '通鉴纪事本末',
  通鑑纪事本末: '通鉴纪事本末',
  通鉴紀事本末: '通鉴纪事本末',
}

function workKey(value: string): string {
  let title = (value || '').trim()
  title = title.replace(/（[^）]*待补原文[^）]*）/g, '').trim()
  if (title.startsWith('《')) {
    const book = title.match(/^《([^》]+)》/)
    title = book ? book[1] : title.replace(/^《/, '')
  } else if (!title.includes('《')) {
    title = title.replace(/》+$/, '')
  }
  title = title.trim()
  if (!title) return '出处待定'
  return WORK_ALIASES[title] ?? title
}

const filteredCitations = computed(() => {
  if (!needle.value) return citations.value
  return citations.value.filter((citation) =>
    [citation.workTitle, citation.noteQuote, citation.originalText, noteTitle(citation.noteId)]
      .join(' ')
      .toLowerCase()
      .includes(needle.value),
  )
})

const citationGroups = computed(() => {
  const groups = new Map<string, { work: string; items: NoteCitation[] }>()
  for (const citation of filteredCitations.value) {
    const work = workKey(citation.workTitle)
    const group = groups.get(work) ?? { work, items: [] }
    group.items.push(citation)
    groups.set(work, group)
  }
  return [...groups.values()].sort(
    (left, right) => right.items.length - left.items.length || left.work.localeCompare(right.work),
  )
})

const filteredSources = computed(() => {
  if (!needle.value) return sources.value
  return sources.value.filter((source) =>
    [source.workTitle, source.sectionTitle, source.author, source.edition, source.volume]
      .join(' ')
      .toLowerCase()
      .includes(needle.value),
  )
})

function isLocalSource(source: Source): boolean {
  return source.authenticityNote === '本地全文导入'
}

const localSources = computed(() => filteredSources.value.filter(isLocalSource))
const legacySources = computed(() => filteredSources.value.filter((source) => !isLocalSource(source)))

function sourceTitle(source: Source): string {
  return `《${source.workTitle}》${source.sectionTitle ? ` · ${source.sectionTitle}` : ''}`
}

function sourceMeta(source: Source): string {
  return [source.author, source.era, source.edition, source.volume].filter(Boolean).join(' · ')
}

function openNote(id: string): void {
  if (id) void router.push(`/note/${id}`)
}

function openSource(id: string): void {
  if (id) void router.push(`/sources/${id}`)
}

function openImport(): void {
  importOpen.value = true
  importError.value = ''
  importText.value = ''
  importFileLabel.value = ''
}

function closeImport(): void {
  if (importing.value) return
  importOpen.value = false
}

async function onImportFiles(event: Event): Promise<void> {
  const input = event.target as HTMLInputElement
  const files = Array.from(input.files ?? [])
  input.value = ''
  if (!files.length) return
  importError.value = ''
  const chunks: string[] = []
  for (const file of files) {
    const raw = await file.text()
    const name = file.name.toLowerCase()
    if (name.endsWith('.html') || name.endsWith('.htm')) {
      const doc = new DOMParser().parseFromString(raw, 'text/html')
      chunks.push((doc.body?.textContent ?? raw).trim())
    } else {
      chunks.push(raw.trim())
    }
  }
  importText.value = chunks.filter(Boolean).join('\n\n')
  importFileLabel.value = `${files.length} 个文件 · ${importText.value.length.toLocaleString()} 字符`
  if (!importForm.workTitle.trim() && files.length === 1) {
    importForm.workTitle = files[0].name.replace(/\.(txt|md|html?|htm)$/i, '')
  }
}

async function saveImport(): Promise<void> {
  if (importing.value) return
  const workTitle = importForm.workTitle.trim()
  if (!workTitle) {
    importError.value = '书名不能为空'
    return
  }
  if (importText.value.trim().length < 100) {
    importError.value = '全文至少 100 字；先选择 txt/md/html 文件'
    return
  }
  importing.value = true
  importError.value = ''
  try {
    const report = await api.productionLocalCorpusImport({
      workTitle,
      sectionTitle: importForm.sectionTitle.trim(),
      author: importForm.author.trim(),
      era: importForm.era.trim(),
      text: importText.value,
    })
    importNotice.value = `已导入《${report.workTitle}》：${report.passages.toLocaleString()} 段 / ${report.chars.toLocaleString()} 字`
    importOpen.value = false
    await load()
  } catch (cause) {
    importError.value = api.errorText(cause)
  } finally {
    importing.value = false
  }
}

async function load(): Promise<void> {
  loading.value = true
  error.value = ''
  try {
    const [nextCitations, nextSources, nextNotes] = await Promise.all([
      api.productionAllNoteCitations(500).catch(() => []),
      api.productionSources('', '', 300),
      api.listNotes(),
    ])
    citations.value = nextCitations
    sources.value = nextSources
    notes.value = nextNotes
  } catch (cause) {
    error.value = api.errorText(cause)
  } finally {
    loading.value = false
  }
}

onMounted(load)
</script>

<template>
  <div class="sources-page">
    <header class="sources-head">
      <div>
        <div class="prod-eyebrow">知识库 / 史料</div>
        <h1>史料库</h1>
        <p>「笔记出处」来自笔记右栏：一句笔记原句对应一份史料。旧来源目录保留在下方，不再作为日常入口。</p>
      </div>
      <div class="sources-actions">
        <button type="button" class="prod-btn" @click="openImport"><Upload :size="13" />导入全文</button>
        <button type="button" class="prod-btn" :disabled="loading" @click="load"><RefreshCw :size="13" :class="{ 'sub-spin': loading }" />刷新</button>
      </div>
    </header>

    <div v-if="error" class="prod-error-box"><p>{{ error }}</p></div>
    <p v-if="importNotice" class="sources-notice">{{ importNotice }}</p>

    <div class="sources-toolbar">
      <label class="sources-search">
        <Search :size="13" />
        <input v-model="query" type="search" placeholder="书名、笔记原句、原文…" aria-label="搜索史料" />
      </label>
      <span class="sources-total">{{ filteredCitations.length }} 条出处 · {{ localSources.length }} 本本地全文 · {{ legacySources.length }} 份旧来源</span>
    </div>

    <div v-if="loading" class="facts-loading"><LoaderCircle :size="16" class="sub-spin" />正在读取史料…</div>

    <template v-else>
      <section v-if="citationGroups.length" class="sources-citation-section">
        <div class="sources-section-head">
          <div>
            <div class="prod-eyebrow">笔记出处</div>
            <h2>一句原句 → 一份史料</h2>
          </div>
          <span>{{ filteredCitations.length }} 条</span>
        </div>
        <article v-for="group in citationGroups" :key="group.work" class="source-citation-group">
          <div class="source-citation-group-head">
            <b>{{ citationWorkLabel(group.work) }}</b>
            <span>{{ group.items.length }} 条出处 · {{ group.items.filter((item) => item.status === 'found').length }} 条有原文</span>
          </div>
          <div
            v-for="item in group.items"
            :key="item.id"
            class="source-citation-row"
            role="button"
            tabindex="0"
            @click="openNote(item.noteId)"
            @keydown.enter="openNote(item.noteId)"
            @keydown.space.prevent="openNote(item.noteId)"
          >
            <span class="source-citation-note">{{ item.noteQuote }}</span>
            <span class="source-citation-meta">
              <i>{{ noteTitle(item.noteId) }}</i>
              <em :class="item.status === 'found' ? 'is-found' : 'is-pending'">{{ item.status === 'found' ? '已找到' : '待补原文' }}</em>
            </span>
            <span
              v-if="showOriginal(item)"
              class="source-citation-original"
              :class="{ 'is-collapsed': isLongOriginal(item.originalText) && !isExpanded(item.id) }"
            >{{ item.originalText }}</span>
            <button
              v-if="showOriginal(item) && isLongOriginal(item.originalText)"
              type="button"
              class="source-citation-expand"
              :aria-expanded="isExpanded(item.id)"
              @click.stop="toggleOriginal(item.id)"
              @keydown.stop
            >{{ isExpanded(item.id) ? '收起原文' : '展开原文' }}</button>
          </div>
        </article>
      </section>

      <div v-else-if="!filteredSources.length" class="facts-empty">
        <BookMarked :size="22" />
        <b>还没有笔记出处</b>
        <p>在笔记右栏用「找出处 / AI 补原文 / 手动补原文」建立关系后，会出现在这里。</p>
      </div>

      <section v-if="localSources.length" class="sources-local">
        <div class="sources-section-head">
          <div>
            <div class="prod-eyebrow">本地全文</div>
            <h2>已导入全文</h2>
          </div>
          <span>{{ localSources.length }} 本</span>
        </div>
        <div class="source-index-list">
          <button v-for="source in localSources" :key="source.id" type="button" class="source-index-row" @click="openSource(source.id)">
            <span class="source-index-body">
              <span class="source-index-head"><b>{{ sourceTitle(source) }}</b><span class="source-type is-primary">本地</span></span>
              <span class="source-index-meta">{{ sourceMeta(source) || '未填作者/版本' }}</span>
              <span class="source-index-counts">{{ source.passageCount.toLocaleString() }} 段原文 · 补原文时优先查本地</span>
            </span>
            <ChevronRight :size="15" class="fact-index-arrow" />
          </button>
        </div>
      </section>

      <section v-if="legacySources.length" class="sources-old">
        <div class="sources-section-head">
          <div>
            <div class="prod-eyebrow">旧来源目录</div>
            <h2>旧事实溯源留下的书目</h2>
          </div>
          <span>{{ legacySources.length }} 份</span>
        </div>
        <p class="sources-old-note">数据保留，不再作为日常入口；新的出处关系看上面的「笔记出处」。</p>
        <div class="source-index-list">
          <button v-for="source in legacySources" :key="source.id" type="button" class="source-index-row" @click="openSource(source.id)">
            <span class="source-index-body">
              <span class="source-index-head"><b>{{ sourceTitle(source) }}</b></span>
              <span class="source-index-meta">{{ sourceMeta(source) || '未填作者/版本' }}</span>
              <span class="source-index-counts">{{ source.passageCount }} 段原文</span>
            </span>
            <ChevronRight :size="15" class="fact-index-arrow" />
          </button>
        </div>
        <router-link to="/sources/pending" class="sources-old-link">旧候选档案 →</router-link>
      </section>
    </template>

    <div v-if="importOpen" class="prod-overlay" @click.self="closeImport">
      <div class="prod-dialog source-dialog" role="dialog" aria-modal="true" aria-labelledby="localCorpusTitle">
        <div class="prod-dialog-head">
          <div>
            <h2 id="localCorpusTitle">导入本地全文</h2>
            <p class="source-dialog-sub">导入公版史书全文后，补原文会先查本地库，再走联网检索。</p>
          </div>
          <button type="button" class="fact-merge-close" aria-label="关闭" :disabled="importing" @click="closeImport">×</button>
        </div>
        <div class="prod-dialog-body">
          <div class="source-form">
            <label class="source-field">书名<input v-model="importForm.workTitle" placeholder="例如：后汉书" /></label>
            <label class="source-field">篇名 / 卷次<input v-model="importForm.sectionTitle" placeholder="可选；例如：全卷" /></label>
            <label class="source-field">作者<input v-model="importForm.author" placeholder="可选" /></label>
            <label class="source-field">朝代<input v-model="importForm.era" placeholder="可选" /></label>
            <label class="source-field is-wide">全文文件
              <input type="file" multiple accept=".txt,.md,.html,.htm" @change="onImportFiles" />
            </label>
          </div>
          <p class="source-dialog-sub">{{ importFileLabel || '支持 txt / md / html；可多选，按顺序拼接。' }}</p>
          <p v-if="importError" class="fact-merge-error">{{ importError }}</p>
        </div>
        <div class="prod-dialog-foot">
          <button type="button" class="prod-btn" :disabled="importing" @click="closeImport">取消</button>
          <button type="button" class="prod-btn is-primary" :disabled="importing || !importForm.workTitle.trim() || importText.trim().length < 100" @click="saveImport">
            <LoaderCircle v-if="importing" :size="12" class="sub-spin" />{{ importing ? '导入中…' : '导入并切分' }}
          </button>
        </div>
      </div>
    </div>
  </div>
</template>
