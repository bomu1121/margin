<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import {
  BookOpen,
  ChevronLeft,
  ChevronRight,
  FileAudio,
  FileText,
  Link2,
  LoaderCircle,
  Star,
  X,
} from 'lucide-vue-next'

import * as api from '../../lib/api'
import { mediaUrl, templateById } from '../../lib/production'
import {
  loadProduction,
  production,
  startProduction,
  startProductionBatch,
} from '../../stores/production'
import type {
  BiliFolder,
  BiliVideoPreview,
  BiliVideoRef,
  ProductionSource,
} from '../../types'

const props = defineProps<{
  open: boolean
  initialSource?: string
  initialTemplate?: string
}>()
const emit = defineEmits<{ close: []; started: []; login: [] }>()

const step = ref(1)
const sourceKind = ref('bili')
const templateId = ref('video-note')
const biliUrl = ref('')
const preview = ref<BiliVideoPreview | null>(null)
const selectedPages = ref<number[]>([])
const biliLoading = ref(false)
const biliError = ref('')
const favFolders = ref<BiliFolder[]>([])
const favVideos = ref<BiliVideoRef[]>([])
const selectedFavIds = ref<string[]>([])
const favFolderId = ref(0)
const favPage = ref(0)
const favHasMore = ref(false)
const favTotal = ref(0)
const favLoading = ref(false)
const favError = ref('')
const filePath = ref('')
const text = ref('')
const title = ref('')
const titleAuto = ref(false)
const prompt = ref('成文版')
const tags = ref('')
const fields = ref<string[]>(['人物', '事件'])
const split = ref(true)
const exportMarkdown = ref(false)
const chapter = ref('medium')
const maxChapters = ref(20)
const traceMax = ref(5)
const drillCount = ref(6)
const mindDepth = ref(3)
const styleA = ref('成文版')
const styleB = ref('速览版')
const starting = ref(false)
const error = ref('')
let parseTimer: number | undefined
let parseSeq = 0

const fieldChoices = ['人物', '事件', '时期', '公司', '概念']
const promptChoices = computed(() => production.prompts.map((item) => item.name))
const selectedTemplate = computed(() => templateById(production.templates, templateId.value))
const selectedVideos = computed(() => favVideos.value.filter((video) => selectedFavIds.value.includes(video.bvid)))
const splitByVideo = computed(() => sourceKind.value === 'favorites' && selectedVideos.value.length > 1 && templateId.value !== 'series')
const availableTemplates = computed(() => {
  const kind = sourceKind.value === 'favorites' ? 'favorites' : sourceKind.value
  return production.templates.filter((template) => template.sources.includes(kind))
})
const sourceLabel = computed(() => ({ text: '粘贴文稿', file: '本地音视频', bili: 'B站链接', favorites: 'B站收藏' })[sourceKind.value] ?? sourceKind.value)
const sourceReady = computed(() => {
  if (sourceKind.value === 'bili') return Boolean(preview.value && biliUrl.value.trim())
  if (sourceKind.value === 'favorites') return selectedFavIds.value.length > 0
  if (sourceKind.value === 'file') return Boolean(filePath.value)
  if (sourceKind.value === 'text') return text.value.trim().length > 0
  return false
})

function reset(): void {
  step.value = 1
  sourceKind.value = props.initialSource || 'bili'
  templateId.value = props.initialTemplate || production.settings?.defaultTemplate || 'video-note'
  biliUrl.value = ''
  preview.value = null
  selectedPages.value = []
  biliError.value = ''
  biliLoading.value = false
  favFolders.value = []
  favVideos.value = []
  selectedFavIds.value = []
  favFolderId.value = 0
  favPage.value = 0
  favHasMore.value = false
  favTotal.value = 0
  favError.value = ''
  filePath.value = ''
  text.value = ''
  title.value = ''
  titleAuto.value = false
  window.clearTimeout(parseTimer)
  parseSeq += 1
  prompt.value = production.settings?.defaultPrompt || '成文版'
  tags.value = ''
  fields.value = [...(production.settings?.defaultFields?.length ? production.settings.defaultFields : ['人物', '事件'])]
  split.value = true
  exportMarkdown.value = false
  chapter.value = 'medium'
  maxChapters.value = 20
  traceMax.value = 5
  drillCount.value = 6
  mindDepth.value = 3
  styleA.value = '成文版'
  styleB.value = '速览版'
  starting.value = false
  error.value = ''
}

function close(): void {
  if (starting.value) return
  emit('close')
}

function chooseSource(kind: string): void {
  sourceKind.value = kind
  error.value = ''
  if (kind === 'favorites' && production.account) void loadFolders()
}

async function loadFolders(): Promise<void> {
  favLoading.value = true
  favError.value = ''
  try {
    favFolders.value = await api.productionBiliFavorites()
    if (favFolders.value.length > 0 && favFolderId.value === 0) {
      await chooseFolder(favFolders.value[0]!.id)
    }
  } catch (cause) {
    favError.value = api.errorText(cause)
  } finally {
    favLoading.value = false
  }
}

function appendFavVideos(items: BiliVideoRef[]): void {
  for (const video of items) {
    if (!favVideos.value.some((item) => item.bvid === video.bvid)) favVideos.value.push(video)
  }
}

async function loadFolderInitial(id: number): Promise<void> {
  favFolderId.value = id
  favLoading.value = true
  favError.value = ''
  favVideos.value = []
  selectedFavIds.value = []
  favPage.value = 0
  favHasMore.value = false
  favTotal.value = 0
  try {
    for (let page = 1; page <= 5; page += 1) {
      const result = await api.productionBiliFolderVideos(id, page)
      appendFavVideos(result.items)
      favPage.value = result.page
      favHasMore.value = result.hasMore
      favTotal.value = result.total
      if (!result.hasMore || result.items.length === 0) break
    }
  } catch (cause) {
    favError.value = api.errorText(cause)
  } finally {
    favLoading.value = false
  }
}

async function chooseFolder(id: number): Promise<void> {
  await loadFolderInitial(id)
}

async function loadMoreFolder(): Promise<void> {
  if (favLoading.value || !favHasMore.value || favFolderId.value <= 0) return
  favLoading.value = true
  favError.value = ''
  try {
    const result = await api.productionBiliFolderVideos(favFolderId.value, favPage.value + 1)
    appendFavVideos(result.items)
    favPage.value = result.page
    favHasMore.value = result.hasMore
    favTotal.value = result.total
  } catch (cause) {
    favError.value = api.errorText(cause)
  } finally {
    favLoading.value = false
  }
}

async function loadWatchLater(): Promise<void> {
  favFolderId.value = -1
  favLoading.value = true
  favError.value = ''
  selectedFavIds.value = []
  favPage.value = 0
  favHasMore.value = false
  favTotal.value = 0
  try {
    favVideos.value = await api.productionBiliWatchLater()
    favTotal.value = favVideos.value.length
  } catch (cause) {
    favVideos.value = []
    favError.value = api.errorText(cause)
  } finally {
    favLoading.value = false
  }
}

async function loadHistory(): Promise<void> {
  favFolderId.value = -2
  favLoading.value = true
  favError.value = ''
  selectedFavIds.value = []
  favPage.value = 0
  favHasMore.value = false
  favTotal.value = 0
  try {
    favVideos.value = await api.productionBiliHistory()
    favTotal.value = favVideos.value.length
  } catch (cause) {
    favVideos.value = []
    favError.value = api.errorText(cause)
  } finally {
    favLoading.value = false
  }
}

function toggleFav(bvid: string): void {
  const index = selectedFavIds.value.indexOf(bvid)
  if (index >= 0) selectedFavIds.value.splice(index, 1)
  else selectedFavIds.value.push(bvid)
}

function looksLikeBili(value: string): boolean {
  const text = value.trim()
  return /BV[0-9A-Za-z]{10}/.test(text)
    || /(?:^|[^a-z0-9])av\d+/i.test(text)
    || /b23\.tv/i.test(text)
    || /bilibili\.com\/video/i.test(text)
}

function clearBiliPreview(clearAutoTitle = true): void {
  parseSeq += 1
  window.clearTimeout(parseTimer)
  biliLoading.value = false
  preview.value = null
  selectedPages.value = []
  biliError.value = ''
  if (clearAutoTitle && titleAuto.value) {
    title.value = ''
    titleAuto.value = false
  }
}

async function parseBili(input: string): Promise<void> {
  const value = input.trim()
  if (!value) return
  const seq = ++parseSeq
  biliLoading.value = true
  biliError.value = ''
  try {
    const parsed = await api.productionBiliPreview(value)
    if (seq !== parseSeq || biliUrl.value.trim() !== value) return
    preview.value = parsed
    selectedPages.value = parsed.pages.map((page) => page.page)
    if (!title.value.trim() || titleAuto.value) {
      title.value = parsed.title
      titleAuto.value = true
    }
    if (!templateId.value || !availableTemplates.value.some((item) => item.id === templateId.value)) {
      templateId.value = availableTemplates.value[0]?.id ?? 'video-note'
    }
  } catch (cause) {
    if (seq !== parseSeq || biliUrl.value.trim() !== value) return
    preview.value = null
    biliError.value = api.errorText(cause)
  } finally {
    if (seq === parseSeq && biliUrl.value.trim() === value) biliLoading.value = false
  }
}

function scheduleBiliParse(): void {
  window.clearTimeout(parseTimer)
  preview.value = null
  selectedPages.value = []
  biliError.value = ''
  biliLoading.value = false
  if (titleAuto.value) {
    title.value = ''
    titleAuto.value = false
  }
  if (!biliUrl.value.trim()) {
    clearBiliPreview()
    return
  }
  if (!looksLikeBili(biliUrl.value)) {
    parseSeq += 1
    return
  }
  const value = biliUrl.value
  parseTimer = window.setTimeout(() => void parseBili(value), 520)
}

function onTitleInput(): void {
  titleAuto.value = false
}

async function chooseFile(): Promise<void> {
  const selected = await api.chooseMediaFile()
  if (selected) {
    filePath.value = selected
    if (!title.value.trim()) {
      title.value = selected.split(/[\\/]/).pop()?.replace(/\.[^.]+$/, '') ?? ''
    }
  }
}

function togglePage(page: number): void {
  const index = selectedPages.value.indexOf(page)
  if (index >= 0) selectedPages.value.splice(index, 1)
  else selectedPages.value.push(page)
}

function toggleField(field: string): void {
  const index = fields.value.indexOf(field)
  if (index >= 0) fields.value.splice(index, 1)
  else fields.value.push(field)
}

function nextStep(): void {
  error.value = ''
  if (step.value === 1 && !sourceReady.value) {
    error.value = '先把来源准备好'
    return
  }
  if (step.value === 2 && !selectedTemplate.value) {
    error.value = '先选择一个模板'
    return
  }
  if (step.value === 2 && !availableTemplates.value.some((item) => item.id === templateId.value)) {
    templateId.value = availableTemplates.value[0]?.id ?? ''
  }
  step.value = Math.min(4, step.value + 1)
}

function prevStep(): void {
  error.value = ''
  step.value = Math.max(1, step.value - 1)
}

function buildSource(): ProductionSource {
  if (sourceKind.value === 'favorites') {
    return {
      kind: 'favorites',
      text: '',
      path: '',
      url: '',
      videos: favVideos.value.filter((video) => selectedFavIds.value.includes(video.bvid)),
      pages: [],
    }
  }
  if (sourceKind.value === 'bili') {
    return {
      kind: 'bili',
      text: '',
      path: '',
      url: biliUrl.value.trim(),
      videos: [],
      pages: selectedPages.value,
    }
  }
  if (sourceKind.value === 'file') {
    return { kind: 'file', text: '', path: filePath.value, url: '', videos: [], pages: [] }
  }
  return { kind: 'text', text: text.value, path: '', url: '', videos: [], pages: [] }
}

function displayTitle(): string {
  if (title.value.trim()) return title.value.trim()
  if (sourceKind.value === 'bili' && preview.value) return preview.value.title
  if (sourceKind.value === 'favorites') {
    if (selectedFavIds.value.length === 1) {
      return favVideos.value.find((video) => video.bvid === selectedFavIds.value[0])?.title ?? 'B站收藏'
    }
    return `B站收藏 · ${selectedFavIds.value.length} 个视频`
  }
  if (sourceKind.value === 'file') return filePath.value.split(/[\\/]/).pop()?.replace(/\.[^.]+$/, '') ?? '本地音视频'
  return '粘贴文稿'
}

async function start(): Promise<void> {
  if (!sourceReady.value || !selectedTemplate.value) return
  starting.value = true
  error.value = ''
  const options = {
    prompt: prompt.value,
    tags: tags.value.split(/[,，\s]+/).map((item) => item.trim()).filter(Boolean),
    fields: fields.value,
    split: split.value,
    exportMarkdown: exportMarkdown.value,
    chapter: chapter.value,
    maxChapters: maxChapters.value,
    traceProvider: production.settings?.searchProvider ?? 'zhipu',
    traceMax: traceMax.value,
    drillCount: drillCount.value,
    mindDepth: mindDepth.value,
    styles: [styleA.value, styleB.value],
  }
  try {
    if (splitByVideo.value) {
      const requests = selectedVideos.value.map((video) => ({
        templateId: templateId.value,
        title: video.title,
        source: {
          kind: 'bili',
          text: '',
          path: '',
          url: `https://www.bilibili.com/video/${video.bvid}`,
          videos: [video],
          pages: [],
        },
        options,
      }))
      await startProductionBatch(requests)
    } else {
      await startProduction({
        templateId: templateId.value,
        title: displayTitle(),
        source: buildSource(),
        options,
      })
    }
    emit('started')
  } catch (cause) {
    error.value = api.errorText(cause)
  } finally {
    starting.value = false
  }
}

watch(
  () => biliUrl.value,
  () => scheduleBiliParse(),
)

watch(
  () => props.open,
  (open) => {
    if (open) {
      reset()
      if (!production.loaded) void loadProduction()
      if (sourceKind.value === 'favorites' && production.account) void loadFolders()
    }
  },
  { immediate: true },
)

watch(
  () => production.account,
  (account) => {
    if (account && sourceKind.value === 'favorites') void loadFolders()
  },
)

onMounted(() => {
  if (!production.loaded) void loadProduction()
})

onBeforeUnmount(() => window.clearTimeout(parseTimer))
</script>

<template>
  <div v-if="open" class="prod-overlay" @click.self="close">
    <div class="prod-dialog prod-wizard" role="dialog" aria-modal="true" aria-labelledby="prodWizardTitle">
      <header class="prod-dialog-head">
        <div>
          <div class="prod-eyebrow">制作 / 新建</div>
          <h2 id="prodWizardTitle">新建制作</h2>
        </div>
        <button type="button" class="prod-icon-btn" aria-label="关闭" @click="close">
          <X :size="15" />
        </button>
      </header>

      <div class="prod-wizard-steps">
        <span v-for="(label, index) in ['来源', '模板', '选项', '确认']" :key="label" class="prod-wizard-step" :class="{ 'is-active': step === index + 1, 'is-done': step > index + 1 }">
          <i>{{ step > index + 1 ? '✓' : index + 1 }}</i>
          {{ label }}
        </span>
      </div>

      <div class="prod-wizard-body">
        <template v-if="step === 1">
          <div class="prod-source-picker">
            <button type="button" class="prod-source-option" :class="{ 'is-selected': sourceKind === 'bili' }" @click="chooseSource('bili')">
              <Link2 :size="15" /> B站链接
            </button>
            <button type="button" class="prod-source-option" :class="{ 'is-selected': sourceKind === 'favorites' }" @click="chooseSource('favorites')">
              <Star :size="15" /> B站收藏
            </button>
            <button type="button" class="prod-source-option" :class="{ 'is-selected': sourceKind === 'file' }" @click="chooseSource('file')">
              <FileAudio :size="15" /> 本地音视频
            </button>
            <button type="button" class="prod-source-option" :class="{ 'is-selected': sourceKind === 'text' }" @click="chooseSource('text')">
              <FileText :size="15" /> 粘贴文稿
            </button>
          </div>

          <div class="prod-source-panel">
            <template v-if="sourceKind === 'bili'">
              <h3>粘贴 B 站链接</h3>
              <p>支持 BV / av / b23.tv；粘贴后自动解析，只读取公开信息，不下载视频。</p>
              <div class="prod-url-row">
                <input v-model="biliUrl" class="prod-input" placeholder="粘贴 BV / av / b23 链接，自动解析" />
                <span v-if="biliLoading" class="prod-parse-loading"><LoaderCircle :size="13" class="prod-spin" />解析中</span>
              </div>
              <p v-if="biliError" class="prod-error">{{ biliError }}</p>
              <div v-if="preview" class="prod-parsed">
                <img v-if="preview.cover" class="prod-parsed-cover" :src="mediaUrl(preview.cover)" alt="" referrerpolicy="no-referrer" loading="lazy" />
                <span v-else class="prod-parsed-thumb">B站</span>
                <span class="prod-parsed-copy">
                  <b>{{ preview.title }}</b>
                  <span>{{ preview.author }} · {{ preview.pages.length > 1 ? `${preview.pages.length} 个分P` : '公开视频' }}</span>
                </span>
                <span class="prod-badge is-blue">已解析</span>
              </div>
              <div v-if="preview && preview.pages.length > 1" class="prod-option-section">
                <h4>选择分P</h4>
                <div class="prod-folder-row">
                  <button
                    v-for="page in preview.pages"
                    :key="page.cid"
                    type="button"
                    class="prod-folder-chip"
                    :class="{ 'is-active': selectedPages.includes(page.page) }"
                    @click="togglePage(page.page)"
                  >
                    P{{ page.page }} · {{ page.part }}
                  </button>
                </div>
              </div>
            </template>

            <template v-else-if="sourceKind === 'favorites'">
              <template v-if="!production.account">
                <div class="prod-login-inline">
                  <span>收藏夹需要先登录 B 站。公开链接不受影响。</span>
                  <button type="button" class="prod-btn" @click="emit('login')">扫码登录</button>
                </div>
              </template>
              <template v-else>
                <h3>{{ favFolderId === -1 ? '稍后再看' : favFolderId === -2 ? '观看历史' : '从 B 站收藏选择' }}</h3>
                <p>已登录「{{ production.account.name }}」。勾选 1 个或多个视频；多个会作为一个来源任务。</p>
                <div class="prod-folder-row">
                  <button
                    v-for="folder in favFolders"
                    :key="folder.id"
                    type="button"
                    class="prod-folder-chip"
                    :class="{ 'is-active': favFolderId === folder.id }"
                    @click="chooseFolder(folder.id)"
                  >
                    {{ folder.title }} · {{ folder.count }}
                  </button>
                  <button type="button" class="prod-folder-chip" :class="{ 'is-active': favFolderId === -1 }" @click="loadWatchLater">稍后再看</button>
                  <button type="button" class="prod-folder-chip" :class="{ 'is-active': favFolderId === -2 }" @click="loadHistory">观看历史</button>
                </div>
                <p v-if="favError" class="prod-error">{{ favError }}</p>
                <div v-if="favLoading" class="prod-wizard-hint">正在读取收藏夹…</div>
                <label v-for="video in favVideos" v-else :key="video.bvid" class="prod-video-row">
                  <input type="checkbox" :checked="selectedFavIds.includes(video.bvid)" @change="toggleFav(video.bvid)" />
                  <img v-if="video.cover" class="prod-video-cover" :src="mediaUrl(video.cover)" alt="" referrerpolicy="no-referrer" loading="lazy" />
                  <span>
                    <b style="font-weight: 400; color: var(--ink-2)">{{ video.title }}</b>
                    <br /><small>{{ video.author }} · {{ video.duration }}s</small>
                  </span>
                  <small>{{ video.bvid }}</small>
                </label>
                <div v-if="favHasMore" class="prod-inline-actions">
                  <button type="button" class="prod-btn is-small" :disabled="favLoading" @click="loadMoreFolder">
                    {{ favLoading ? '正在加载…' : `加载更多（已显示 ${favVideos.length} / ${favTotal || '…'}）` }}
                  </button>
                </div>
                <p v-else-if="favVideos.length" class="prod-wizard-hint">已显示 {{ favVideos.length }} 条</p>
                <div v-if="!favLoading && !favVideos.length" class="prod-wizard-hint">这个收藏夹没有视频。</div>
                <p class="prod-wizard-hint" style="margin-top: 8px">已选 {{ selectedFavIds.length }} 个</p>
              </template>
            </template>

            <template v-else-if="sourceKind === 'file'">
              <h3>选择本地音视频</h3>
              <p>mp4 / m4a / mp3 / mov / webm。需要本机 ffmpeg 提取音频。</p>
              <div class="prod-file-box" :class="{ 'is-selected': filePath }" @click="chooseFile">
                <strong>{{ filePath ? filePath.split(/[\\\\/]/).pop() : '点击选择文件' }}</strong>
                <span>{{ filePath ? '已选择 · 将在本机提取音频' : '不上传服务器' }}</span>
              </div>
            </template>

            <template v-else>
              <h3>粘贴已有文稿</h3>
              <p>适合已有转写稿、文章、会议记录；不经过转写，直接校对和加工。</p>
              <textarea v-model="text" class="prod-textarea" placeholder="把文稿粘贴到这里…" />
              <div class="prod-inline-actions">
                <button type="button" class="prod-btn is-small" @click="text = '这是一份示例文稿。\n\n## 要点\n\n- 制度设计决定了扩面的天花板。\n- 分配问题比总量问题更值得看。\n- 生产力与商品服务供给才是长期变量。'">填入示例文稿</button>
                <span class="prod-wizard-hint">{{ text.trim().length }} 字</span>
              </div>
            </template>
          </div>
        </template>

        <template v-else-if="step === 2">
          <div v-if="!availableTemplates.length" class="prod-wizard-empty">当前来源没有可用模板。</div>
          <div v-else class="prod-wizard-template-grid">
            <button
              v-for="template in availableTemplates"
              :key="template.id"
              type="button"
              class="prod-wizard-template"
              :class="{ 'is-selected': templateId === template.id }"
              @click="templateId = template.id"
            >
              <span class="prod-wizard-template-top">
                <BookOpen :size="14" />
                <b>{{ template.name }}</b>
                <span class="prod-badge" :class="{ 'is-blue': template.badge === '基础', 'is-gold': template.badge === '垂直' }">{{ template.badge }}</span>
              </span>
              <p>{{ template.description }}</p>
              <span class="prod-pipeline">
                <span v-for="item in template.pipeline" :key="item">{{ item }}</span>
              </span>
            </button>
          </div>
        </template>

        <template v-else-if="step === 3">
          <div class="prod-options-grid">
            <label class="prod-field is-wide">
              <span>笔记标题</span>
              <input v-model="title" class="prod-input" :placeholder="displayTitle()" @input="onTitleInput" />
            </label>
            <label class="prod-field">
              <span>提示词风格</span>
              <select v-model="prompt" class="prod-select">
                <option v-for="name in promptChoices" :key="name" :value="name">{{ name }}</option>
              </select>
            </label>
            <label class="prod-field">
              <span>标签</span>
              <input v-model="tags" class="prod-input" placeholder="逗号分隔" />
            </label>
            <div class="prod-field is-wide">
              <span>抽取属性</span>
              <div class="prod-folder-row">
                <label v-for="field in fieldChoices" :key="field" class="prod-check-chip">
                  <input type="checkbox" :checked="fields.includes(field)" @change="toggleField(field)" />
                  {{ field }}
                </label>
              </div>
            </div>
          </div>

          <div v-if="selectedTemplate?.id === 'long-note'" class="prod-option-section">
            <h4>分章参数</h4>
            <div class="prod-options-grid">
              <label class="prod-field"><span>切分粒度</span>
                <select v-model="chapter" class="prod-select">
                  <option value="coarse">粗（少章节）</option>
                  <option value="medium">中（推荐）</option>
                  <option value="fine">细（多章节）</option>
                </select>
              </label>
              <label class="prod-field"><span>最多章节</span><input v-model.number="maxChapters" type="number" min="2" max="60" class="prod-input" /></label>
            </div>
          </div>
          <div v-if="selectedTemplate?.id === 'multi-style' || selectedTemplate?.id === 'multi-branch'" class="prod-option-section">
            <h4>两套风格</h4>
            <div class="prod-options-grid">
              <label class="prod-field"><span>风格 A</span><select v-model="styleA" class="prod-select"><option v-for="name in promptChoices" :key="name">{{ name }}</option></select></label>
              <label class="prod-field"><span>风格 B</span><select v-model="styleB" class="prod-select"><option v-for="name in promptChoices" :key="name">{{ name }}</option></select></label>
            </div>
          </div>
          <div v-if="selectedTemplate?.id === 'trace'" class="prod-option-section">
            <h4>溯源参数</h4>
            <div class="prod-options-grid">
              <label class="prod-field"><span>每个检索词最多结果</span><input v-model.number="traceMax" type="number" min="1" max="10" class="prod-input" /></label>
            </div>
          </div>
          <div v-if="selectedTemplate?.id === 'drill'" class="prod-option-section">
            <h4>练习参数</h4>
            <div class="prod-options-grid">
              <label class="prod-field"><span>题目数量</span><input v-model.number="drillCount" type="number" min="3" max="20" class="prod-input" /></label>
            </div>
          </div>
          <div v-if="selectedTemplate?.id === 'mindmap'" class="prod-option-section">
            <h4>导图参数</h4>
            <div class="prod-options-grid">
              <label class="prod-field"><span>展开层级</span>
                <select v-model.number="mindDepth" class="prod-select"><option :value="2">2 层</option><option :value="3">3 层</option><option :value="4">4 层</option></select>
              </label>
            </div>
          </div>

          <div class="prod-option-section">
            <h4>输出</h4>
            <p class="prod-wizard-hint">完成后自动入库页边笔记；需要单独留存时，在任务结果页点「导出 Markdown」。</p>
          </div>
        </template>

        <template v-else>
          <dl class="prod-summary">
            <div class="prod-summary-row"><dt>来源</dt><dd>{{ splitByVideo ? `每个视频一篇 · ${selectedVideos.length} 个视频` : sourceLabel + ' · ' + (sourceKind === 'bili' ? preview?.title || biliUrl : sourceKind === 'favorites' ? `${selectedFavIds.length} 个视频${selectedFavIds.length > 1 ? '（合并为一篇）' : ''}` : sourceKind === 'file' ? filePath : `${text.trim().length} 字`) }}</dd></div>
            <div class="prod-summary-row"><dt>模板</dt><dd>{{ selectedTemplate?.name }} · {{ selectedTemplate?.pipeline.join(' → ') }}</dd></div>
            <div class="prod-summary-row"><dt>标题</dt><dd>{{ splitByVideo ? '每个视频使用自己的标题' : displayTitle() }}</dd></div>
            <div class="prod-summary-row"><dt>提示词</dt><dd>{{ prompt }}</dd></div>
            <div class="prod-summary-row"><dt>抽取</dt><dd>{{ fields.length ? fields.join('、') : '不抽取' }}</dd></div>
            <div class="prod-summary-row"><dt>输出</dt><dd>{{ splitByVideo ? `创建 ${selectedVideos.length} 个任务，每个视频一篇笔记` : '入库页边笔记 · ScribeFlow/' }}</dd></div>
          </dl>
          <p v-if="error" class="prod-error" style="margin-top: 10px">{{ error }}</p>
        </template>
      </div>

      <footer class="prod-wizard-foot">
        <span class="prod-wizard-hint">
          {{ step === 1 ? '先选来源；B站公开链接不需要登录。' : step === 2 ? '模板只决定步骤，不会生成画布。' : step === 3 ? '这些参数会写进本次任务。' : '开始后可以取消；失败会保留步骤日志。' }}
        </span>
        <span class="prod-spacer" />
        <button type="button" class="prod-btn" :disabled="starting" @click="close">取消</button>
        <button v-if="step > 1" type="button" class="prod-btn" :disabled="starting" @click="prevStep">
          <ChevronLeft :size="13" /> 上一步
        </button>
        <button v-if="step < 4" type="button" class="prod-btn is-primary" :disabled="starting || (step === 1 && !sourceReady) || (step === 2 && !selectedTemplate)" @click="nextStep">
          下一步 <ChevronRight :size="13" />
        </button>
        <button v-else type="button" class="prod-btn is-primary" :disabled="starting" @click="start">
          <LoaderCircle v-if="starting" :size="13" class="prod-spin" />
          {{ starting ? '正在创建…' : '开始制作' }}
        </button>
      </footer>
    </div>
  </div>
</template>

<style scoped>
.prod-spin { animation: prod-spin 1s linear infinite; }
.prod-error { margin: 7px 0 0; color: var(--red); font-family: var(--sans); font-size: 10.5px; line-height: 1.5; }
@keyframes prod-spin { to { transform: rotate(360deg); } }
</style>
