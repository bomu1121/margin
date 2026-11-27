<script setup lang="ts">
import { readText } from '@tauri-apps/plugin-clipboard-manager'
import { computed, onMounted, reactive, ref, watch } from 'vue'
import {
  BookOpen,
  ClipboardPaste,
  Cpu,
  FileAudio,
  FileText,
  LayoutGrid,
  Link2,
  ListVideo,
  LoaderCircle,
  Plus,
  Rss,
  Settings2,
  Sparkles,
  Star,
  X,
} from 'lucide-vue-next'

import BiliLoginDialog from '../components/production/BiliLoginDialog.vue'
import ProductionSettings from '../components/production/ProductionSettings.vue'
import ProductionSubscriptions from '../components/production/ProductionSubscriptions.vue'
import ProductionTaskDialog from '../components/production/ProductionTaskDialog.vue'
import ProductionWizard from '../components/production/ProductionWizard.vue'
import TemplateCard from '../components/production/TemplateCard.vue'
import * as api from '../lib/api'
import {
  buildQuickBiliRequest,
  extractBiliLink,
  formatDate,
  formatTokens,
  looksLikeBiliLink,
  mediaUrl,
  progressOf,
  sourceSummary,
  displayError,
  statusClass,
  jobStatusLabel,
  templateById,
} from '../lib/production'
import { refreshNotes } from '../stores/library'
import {
  activeJobs,
  loadProduction,
  production,
  productionUi,
  refreshJobs,
  refreshPrompts,
  refreshSettings,
  startProduction,
  visiblePrompts,
  visibleTemplates,
} from '../stores/production'
import type {
  BiliVideoPreview,
  ProductionConflict,
  ProductionJob,
  ProductionPrompt,
  ProductionTemplate,
  PromptVersion,
} from '../types'

const taskFilter = ref<'all' | 'running' | 'success' | 'failed' | 'auto'>('all')
const knownSuccess = new Set<string>()
const notice = ref('')
const promptOpen = ref(false)
const promptSaving = ref(false)
const promptError = ref('')
const promptDraft = reactive({ id: '', name: '', description: '', system: '' })
const promptTab = ref<'edit' | 'versions'>('edit')
const promptVersions = ref<PromptVersion[]>([])
const selectedPromptVersion = ref<PromptVersion | null>(null)
const promptVersionLoading = ref(false)
const promptVersionError = ref('')
const promptRestoreVersion = ref(0)
const promptRestoreNotice = ref('')

// Quick production: one clipboard link -> default AI 成稿, no wizard. The
// preview is also what tells the user which video is about to be processed.
const quickUrl = ref('')
const quickSource = ref<'clipboard' | 'manual' | ''>('')
const quickPreview = ref<BiliVideoPreview | null>(null)
const quickPreviewUrl = ref('')
const quickResolving = ref(false)
const quickStarting = ref(false)
const quickError = ref('')
const quickConflict = ref<ProductionConflict | null>(null)
let quickTimer: number | undefined
let quickSeq = 0
let clipboardChecked = false

const sourceCards = [
  { kind: 'bili', icon: Link2, title: 'B站链接', desc: '粘贴 BV / av / b23 链接，自动解析标题、UP 主和音频。', meta: '公开视频 · 无需登录' },
  { kind: 'favorites', icon: Star, title: 'B站收藏', desc: '登录后从收藏夹、稍后再看或历史里勾选素材。', meta: '需要登录' },
  { kind: 'file', icon: FileAudio, title: '本地音视频', desc: '拖入 mp4 / m4a / mp3，ffmpeg 只在本机提取音频。', meta: '需要 ffmpeg' },
  { kind: 'text', icon: ClipboardPaste, title: '粘贴文稿', desc: '已有转写稿、文章或会议记录，直接进入校对和加工。', meta: '最快 · 不消耗转写' },
]

const templateIcons: Record<string, unknown> = {
  'video-note': BookOpen,
  'text-note': FileText,
  'long-note': BookOpen,
  mindmap: LayoutGrid,
  drill: ListVideo,
  gameguide: BookOpen,
  'multi-style': LayoutGrid,
  series: LayoutGrid,
  trace: Sparkles,
  'multi-branch': LayoutGrid,
}

const homeTemplates = computed(() => {
  const list = visibleTemplates.value.slice(0, 6)
  // The AI 成稿 chain is the production default; keep it first instead of
  // burying it between the older note templates.
  return [...list].sort((left, right) => Number(right.id === 'ai-note') - Number(left.id === 'ai-note'))
})
const quickPromptName = computed(() => production.settings?.defaultPrompt?.trim() || '成稿 Agent')
const quickPromptSystem = computed(() => (
  production.prompts.find((prompt) => prompt.name === quickPromptName.value)?.system?.trim() ?? ''
))
const quickReady = computed(() => Boolean(
  quickPreview.value
  && quickPreviewUrl.value === quickUrl.value.trim()
  && !quickResolving.value,
))
const quickButtonLabel = computed(() => {
  if (quickStarting.value) return '正在创建任务'
  if (quickResolving.value) return '解析中'
  return quickReady.value ? '开始制作' : '解析并开始'
})
const taskFilters = [
  { id: 'all', label: '全部' },
  { id: 'running', label: '进行中' },
  { id: 'success', label: '已完成' },
  { id: 'failed', label: '失败 / 取消 / 中断' },
  { id: 'auto', label: '自动' },
] as const
const taskFilterCounts = computed(() => ({
  all: production.jobs.length,
  running: production.jobs.filter((job) => ['running', 'queued'].includes(job.status)).length,
  success: production.jobs.filter((job) => job.status === 'success').length,
  failed: production.jobs.filter((job) => ['error', 'cancelled', 'interrupted'].includes(job.status)).length,
  auto: production.jobs.filter((job) => job.options.auto).length,
}))
const taskEmptyText = computed(() => ({
  all: '还没有制作任务。从上面的来源或模板开始。',
  running: '现在没有等待或进行中的任务。',
  success: '还没有已完成的制作。',
  failed: '没有失败、取消或中断的任务。',
  auto: '还没有 UP 订阅自动创建的任务。',
}[taskFilter.value]))
const filteredJobs = computed(() => {
  if (taskFilter.value === 'all') return production.jobs
  if (taskFilter.value === 'failed') return production.jobs.filter((job) => ['error', 'cancelled', 'interrupted'].includes(job.status))
  if (taskFilter.value === 'auto') return production.jobs.filter((job) => job.options.auto)
  return production.jobs.filter((job) => (taskFilter.value === 'running' ? ['running', 'queued'].includes(job.status) : job.status === taskFilter.value))
})
const totals = computed(() => {
  let prompt = 0
  let completion = 0
  let cost = 0
  for (const job of production.jobs) {
    prompt += job.usage.promptTokens
    completion += job.usage.completionTokens
    cost += job.usage.estimatedCost || 0
  }
  return { tokens: prompt + completion, cost }
})
const promptGroups = computed(() => {
  const builtin = visiblePrompts.value.filter((item) => item.builtin)
  const custom = visiblePrompts.value.filter((item) => !item.builtin)
  return [
    { key: 'builtin', label: '内置风格', items: builtin },
    { key: 'custom', label: '自定义', items: custom },
  ].filter((group) => group.items.length > 0)
})

function notify(message: string): void {
  notice.value = message
  window.setTimeout(() => {
    if (notice.value === message) notice.value = ''
  }, 2400)
}


function beep(): void {
  try {
    const AudioCtor = window.AudioContext ?? (window as unknown as { webkitAudioContext?: typeof AudioContext }).webkitAudioContext
    if (!AudioCtor) return
    const context = new AudioCtor()
    const oscillator = context.createOscillator()
    const gain = context.createGain()
    oscillator.type = 'sine'
    oscillator.frequency.value = 880
    gain.gain.setValueAtTime(0.05, context.currentTime)
    gain.gain.exponentialRampToValueAtTime(0.0001, context.currentTime + 0.16)
    oscillator.connect(gain).connect(context.destination)
    oscillator.start()
    oscillator.stop(context.currentTime + 0.17)
  } catch {}
}

function notifyFinished(job: ProductionJob): void {
  const settings = production.settings
  if (!settings) return
  if (settings.sound) beep()
  if (!settings.notify) return
  if (!('Notification' in window)) return
  if (Notification.permission === 'granted') {
    new Notification(job.templateId === 'note-trace' ? '溯源完成' : '制作完成', { body: job.templateId === 'note-trace' ? `${job.title} 已更新事实库` : `${job.title} 已保存到笔记库` })
  }
}

function openWizard(source = '', template = '', initialUrl = ''): void {
  productionUi.wizardSource = source
  productionUi.wizardTemplate = template
  productionUi.wizardInitialUrl = initialUrl
  productionUi.wizardOpen = true
  productionUi.taskId = ''
}

function resetQuick(): void {
  window.clearTimeout(quickTimer)
  quickSeq += 1
  quickUrl.value = ''
  quickSource.value = ''
  quickPreview.value = null
  quickPreviewUrl.value = ''
  quickResolving.value = false
  quickError.value = ''
  quickConflict.value = null
}

function scheduleQuickParse(): void {
  window.clearTimeout(quickTimer)
  quickPreview.value = null
  quickPreviewUrl.value = ''
  quickError.value = ''
  quickConflict.value = null
  const value = quickUrl.value.trim()
  if (!value || !looksLikeBiliLink(value)) return
  quickTimer = window.setTimeout(() => void resolveQuick(value), 520)
}

function onQuickInput(value: string): void {
  quickUrl.value = value
  quickSource.value = value.trim() ? 'manual' : ''
  scheduleQuickParse()
}

function clearQuick(): void {
  resetQuick()
}

async function resolveQuick(input = quickUrl.value): Promise<BiliVideoPreview | null> {
  const value = input.trim()
  if (!value) return null
  if (!looksLikeBiliLink(value)) {
    quickError.value = '只识别 B 站视频链接：BV 号、av 号、b23.tv 或 bilibili.com/video'
    return null
  }
  const seq = ++quickSeq
  quickResolving.value = true
  quickError.value = ''
  quickConflict.value = null
  let timeoutId: number | undefined
  try {
    const timeout = new Promise<never>((_, reject) => {
      timeoutId = window.setTimeout(() => reject(new Error('解析超时，请检查网络后重试')), 15000)
    })
    const parsed = await Promise.race([api.productionBiliPreview(value), timeout])
    if (seq !== quickSeq || quickUrl.value.trim() !== value) return null
    quickPreview.value = parsed
    quickPreviewUrl.value = value
    return parsed
  } catch (cause) {
    if (seq !== quickSeq || quickUrl.value.trim() !== value) return null
    quickPreview.value = null
    quickPreviewUrl.value = ''
    quickError.value = api.errorText(cause)
    return null
  } finally {
    if (timeoutId) window.clearTimeout(timeoutId)
    if (seq === quickSeq) quickResolving.value = false
  }
}

async function startQuick(): Promise<void> {
  if (quickStarting.value || quickResolving.value) return
  quickStarting.value = true
  quickError.value = ''
  quickConflict.value = null
  try {
    const preview = quickReady.value ? quickPreview.value : await resolveQuick()
    if (!preview) return
    const request = buildQuickBiliRequest(preview, quickPreviewUrl.value, {
      prompt: quickPromptName.value,
      noteTask: quickPromptSystem.value,
      searchProvider: production.settings?.searchProvider ?? 'deepseek',
    })
    const conflict = await api.productionConflictCheck(request)
    if (conflict) {
      quickConflict.value = conflict
      quickError.value = `笔记库里已有同源笔记《${conflict.title}》。要覆盖或保留两份，请点「完整配置」。`
      return
    }
    await startProduction(request)
    notify(`已开始制作《${preview.title}》`)
    resetQuick()
  } catch (cause) {
    quickError.value = api.errorText(cause)
  } finally {
    quickStarting.value = false
  }
}

function quickToWizard(): void {
  openWizard('bili', 'ai-note', quickUrl.value.trim())
}

async function detectClipboardLink(): Promise<void> {
  if (productionUi.view !== 'home' || clipboardChecked || quickUrl.value.trim()) return
  clipboardChecked = true
  try {
    const text = await readText()
    if (!text || quickUrl.value.trim()) return
    const link = extractBiliLink(text)
    if (!link) return
    quickUrl.value = link
    quickSource.value = 'clipboard'
    await resolveQuick(link)
  } catch {
    // Clipboard can be empty or denied; the manual input still works.
  }
}

function templateSource(template: ProductionTemplate): string {
  if (template.sources.includes('bili')) return 'bili'
  if (template.sources.includes('text')) return 'text'
  return template.sources[0] ?? 'bili'
}

function openTask(job: ProductionJob): void {
  productionUi.taskId = job.id
}

function templateName(id: string): string {
  return templateById(production.templates, id)?.name ?? '历史任务'
}

/// Show the actual prompt on AI 成稿 tasks. The template name alone cannot tell
/// 成稿 Agent apart from 速览版/结构版, which is exactly the confusion worth avoiding.
function jobChainLabel(job: ProductionJob): string {
  if (job.templateId === 'whiteboard-research') return '白板研究 Agent'
  if (job.templateId === 'note-trace') return '笔记溯源 Agent'
  const name = templateName(job.templateId)
  if (job.templateId === 'ai-note' && job.options.prompt) return `${name} · ${job.options.prompt}`
  return name
}

function iconFor(id: string): unknown {
  return templateIcons[id] ?? BookOpen
}

function openPrompt(prompt?: ProductionPrompt): void {
  promptError.value = ''
  promptTab.value = 'edit'
  promptVersions.value = []
  selectedPromptVersion.value = null
  promptVersionError.value = ''
  promptRestoreVersion.value = 0
  promptRestoreNotice.value = ''
  if (prompt) {
    promptDraft.id = prompt.id
    promptDraft.name = prompt.name
    promptDraft.description = prompt.description
    promptDraft.system = prompt.system
  } else {
    promptDraft.id = ''
    promptDraft.name = ''
    promptDraft.description = ''
    promptDraft.system = ''
  }
  promptOpen.value = true
  if (prompt?.id) void loadPromptVersions(prompt.id)
}

async function loadPromptVersions(promptId = promptDraft.id): Promise<void> {
  if (!promptId) return
  promptVersionLoading.value = true
  promptVersionError.value = ''
  try {
    promptVersions.value = await api.productionPromptVersions(promptId)
    if (!selectedPromptVersion.value || !promptVersions.value.some((item) => item.version === selectedPromptVersion.value?.version)) {
      const latest = promptVersions.value[0]
      if (latest) await viewPromptVersion(latest)
    }
  } catch (cause) {
    promptVersionError.value = api.errorText(cause)
  } finally {
    promptVersionLoading.value = false
  }
}

async function viewPromptVersion(version: PromptVersion): Promise<void> {
  if (!promptDraft.id) return
  promptVersionError.value = ''
  try {
    selectedPromptVersion.value = await api.productionPromptVersion(promptDraft.id, version.version)
  } catch (cause) {
    promptVersionError.value = api.errorText(cause)
  }
}

async function restorePromptVersion(): Promise<void> {
  if (!promptDraft.id || !promptRestoreVersion.value) return
  try {
    const restored = await api.productionRestorePromptVersion(promptDraft.id, promptRestoreVersion.value)
    promptDraft.name = restored.name
    promptDraft.description = restored.description
    promptDraft.system = restored.system
    promptRestoreVersion.value = 0
    promptRestoreNotice.value = `已恢复为 v${selectedPromptVersion.value?.version ?? ''}，并保存为一个新版本。`
    await refreshPrompts()
    await loadPromptVersions()
    promptTab.value = 'edit'
    notify(`提示词「${restored.name}」已恢复`)
  } catch (cause) {
    promptVersionError.value = api.errorText(cause)
  }
}

function diffLines(before: string, after: string): Array<{ type: 'same' | 'add' | 'remove'; text: string }> {
  const a = before.split('\n')
  const b = after.split('\n')
  const table = Array.from({ length: a.length + 1 }, () => new Array<number>(b.length + 1).fill(0))
  for (let i = a.length - 1; i >= 0; i -= 1) {
    for (let j = b.length - 1; j >= 0; j -= 1) {
      table[i][j] = a[i] === b[j] ? table[i + 1][j + 1] + 1 : Math.max(table[i + 1][j], table[i][j + 1])
    }
  }
  const rows: Array<{ type: 'same' | 'add' | 'remove'; text: string }> = []
  let i = 0
  let j = 0
  while (i < a.length && j < b.length) {
    if (a[i] === b[j]) {
      rows.push({ type: 'same', text: a[i] })
      i += 1
      j += 1
    } else if (table[i + 1][j] >= table[i][j + 1]) {
      rows.push({ type: 'remove', text: a[i] })
      i += 1
    } else {
      rows.push({ type: 'add', text: b[j] })
      j += 1
    }
  }
  while (i < a.length) rows.push({ type: 'remove', text: a[i++] })
  while (j < b.length) rows.push({ type: 'add', text: b[j++] })
  return rows
}

const promptDiff = computed(() => diffLines(selectedPromptVersion.value?.system ?? '', promptDraft.system))
const promptShadowsBuiltin = computed(() => production.prompts.some((item) => item.builtin && item.name === promptDraft.name.trim()))

async function savePrompt(): Promise<void> {
  promptSaving.value = true
  promptError.value = ''
  try {
    const saved = await api.productionSavePrompt({
      id: promptDraft.id,
      name: promptDraft.name.trim(),
      description: promptDraft.description.trim(),
      system: promptDraft.system,
      builtin: false,
      createdAt: 0,
      updatedAt: Date.now(),
    })
    await refreshPrompts()
    production.settings = await api.productionSettings()
    promptOpen.value = false
    notify(`提示词「${saved.name}」已保存`)
  } catch (cause) {
    promptError.value = api.errorText(cause)
  } finally {
    promptSaving.value = false
  }
}

async function removePrompt(prompt: ProductionPrompt): Promise<void> {
  if (prompt.builtin) return
  if (!window.confirm(`删除提示词「${prompt.name}」？`)) return
  try {
    await api.productionDeletePrompt(prompt.id)
    await refreshPrompts()
    notify('提示词已删除')
  } catch (cause) {
    notify(api.errorText(cause))
  }
}

async function setDefaultPrompt(prompt: ProductionPrompt): Promise<void> {
  try {
    production.settings = await api.productionSaveSettings({ defaultPrompt: prompt.name })
    notify(`默认提示词已切到「${prompt.name}」`)
  } catch (cause) {
    notify(api.errorText(cause))
  }
}

function openLogin(): void {
  productionUi.loginOpen = true
}

async function onLoginSuccess(): Promise<void> {
  await refreshSettings()
  notify('B 站账号已连接')
}

async function onStarted(): Promise<void> {
  await refreshJobs()
  notify('制作已开始')
}

watch(
  () => production.jobs.map((job) => `${job.id}:${job.status}`).join('|'),
  async (_next, previous) => {
    if (!previous) {
      for (const job of production.jobs.filter((item) => item.status === 'success')) knownSuccess.add(job.id)
      return
    }
    let changed = false
    for (const job of production.jobs) {
      if (job.status === 'success' && !knownSuccess.has(job.id)) {
        knownSuccess.add(job.id)
        notifyFinished(job)
        changed = true
      }
    }
    if (changed) {
      await new Promise((resolve) => window.setTimeout(resolve, 300))
      await refreshNotes()
    }
  },
)

watch(
  () => productionUi.view,
  (view) => {
    if (view === 'home') void detectClipboardLink()
  },
)

onMounted(async () => {
  for (const job of production.jobs.filter((item) => item.status === 'success')) knownSuccess.add(job.id)
  if ('Notification' in window && Notification.permission === 'default') void Notification.requestPermission()
  if (!production.loaded) await loadProduction()
  await detectClipboardLink()
})

</script>

<template>
  <div class="prod-layout" :class="{ 'is-settings': productionUi.view === 'settings' }">
    <div class="prod-main">
      <div class="prod-main-inner">
      <div v-if="notice" class="prod-banner" style="margin-bottom: 12px"><span class="prod-banner-tag">提示</span>{{ notice }}</div>

      <template v-if="productionUi.view === 'home'">
        <div class="prod-page-head">
          <div>
            <div class="prod-eyebrow">制作台 / 本地引擎</div>
            <h1>把素材变成笔记</h1>
            <p>选一个来源，用内置模板生成笔记；结果直接进当前的笔记库。</p>
          </div>
          <div class="prod-page-actions">
            <button type="button" class="prod-btn" @click="openWizard('favorites')"><Star :size="14" />从 B 站收藏开始</button>
            <button type="button" class="prod-btn" @click="productionUi.view = 'subscriptions'"><Rss :size="14" />UP 订阅</button>
          </div>
        </div>

        <section class="prod-quick" :class="{ 'is-ready': quickReady }">
          <div class="prod-quick-main">
            <span class="prod-quick-mark"><Sparkles :size="16" /></span>
            <div class="prod-quick-copy">
              <div class="prod-quick-kicker">
                <span>快捷制作</span>
                <span v-if="quickSource === 'clipboard'" class="prod-quick-chip">来自剪贴板</span>
                <span class="prod-quick-chip is-muted">默认 AI 成稿</span>
              </div>
              <p class="prod-quick-question">
                <template v-if="quickResolving">正在解析这个 B 站链接…</template>
                <template v-else-if="quickPreview">检测到《{{ quickPreview.title }}》，直接用 AI 成稿开始？</template>
                <template v-else-if="quickUrl.trim()">这个链接还不能识别，检查一下再开始</template>
                <template v-else>粘贴 B 站链接，直接开始做笔记</template>
              </p>
              <p class="prod-quick-sub">
                <template v-if="quickPreview">
                  {{ quickPreview.author }} · {{ quickPreview.pages.length > 1 ? `${quickPreview.pages.length} 个分P` : '公开视频' }} · 使用「{{ quickPromptName }}」
                </template>
                <template v-else>不用选模板和参数；成稿直接进「待整理」，之后还能整理归档。</template>
              </p>
            </div>
            <span v-if="quickPreview" class="prod-quick-thumb">
              <img v-if="quickPreview.cover" :src="mediaUrl(quickPreview.cover)" alt="" referrerpolicy="no-referrer" loading="lazy" />
              <span v-else>B站</span>
            </span>
          </div>
          <div class="prod-quick-bar">
            <label class="prod-quick-field">
              <Link2 :size="14" />
              <input
                :value="quickUrl"
                type="text"
                spellcheck="false"
                placeholder="BV… / av… / b23.tv / bilibili.com/video"
                aria-label="快捷制作的 B 站链接"
                @input="onQuickInput(($event.target as HTMLInputElement).value)"
                @keydown.enter.prevent="startQuick"
              />
              <button v-if="quickUrl" type="button" aria-label="清空快捷链接" @click="clearQuick"><X :size="13" /></button>
            </label>
            <button
              type="button"
              class="prod-btn is-primary prod-quick-start"
              :disabled="quickStarting || quickResolving || !quickUrl.trim()"
              @click="startQuick"
            >
              <LoaderCircle v-if="quickStarting || quickResolving" :size="14" class="prod-spin" />
              <Sparkles v-else :size="14" />
              {{ quickButtonLabel }}
            </button>
            <button
              type="button"
              class="prod-btn prod-quick-config"
              :disabled="!quickUrl.trim()"
              @click="quickToWizard"
            >
              完整配置
            </button>
          </div>
          <div v-if="quickError" class="prod-quick-error" role="alert">
            <span>{{ quickError }}</span>
            <button v-if="quickConflict" type="button" @click="quickToWizard">去完整配置处理</button>
          </div>
        </section>

        <div class="prod-section-head">
          <div><h2>从什么开始</h2></div>
        </div>
        <div class="prod-source-grid">
          <button v-for="source in sourceCards" :key="source.kind" type="button" class="prod-source-card" @click="openWizard(source.kind)">
            <span class="prod-source-icon"><component :is="source.icon" :size="15" /></span>
            <span class="prod-source-title">{{ source.title }}</span>
            <span class="prod-source-desc">{{ source.desc }}</span>
            <span class="prod-source-meta">{{ source.meta }}</span>
          </button>
        </div>

        <div class="prod-section-head">
          <div><h2>内置模板</h2></div>
          <button type="button" class="prod-link" @click="productionUi.view = 'templates'">查看全部 {{ visibleTemplates.length }} 个 →</button>
        </div>
        <div class="prod-template-grid">
          <TemplateCard
            v-for="template in homeTemplates"
            :key="template.id"
            :template="template"
            :icon="iconFor(template.id)"
            :class="{ 'is-primary': template.id === 'ai-note' }"
            :badge="template.id === 'ai-note' ? '默认链路' : ''"
            :prompt="template.id === 'ai-note' ? (production.settings?.defaultPrompt || '') : ''"
            @select="openWizard(templateSource(template), template.id)"
          />
        </div>

        <div class="prod-section-head">
          <div><h2>最近制作</h2></div>
          <button type="button" class="prod-link" @click="productionUi.view = 'tasks'">全部任务 →</button>
        </div>
        <div class="prod-list">
          <button v-for="job in production.jobs.slice(0, 5)" :key="job.id" type="button" class="prod-task-row" @click="openTask(job)">
            <i class="prod-dot" :class="statusClass(job.status) === 'is-success' ? 'is-green' : statusClass(job.status) === 'is-error' ? 'is-red' : statusClass(job.status) === 'is-gold' ? 'is-gold' : 'is-blue'" />
            <span>
              <span class="prod-task-title" :title="job.title">{{ job.title }}<span v-if="job.options.auto" class="prod-auto-badge">自动</span></span>
              <span class="prod-task-sub"><span>{{ jobChainLabel(job) }}</span><span>{{ sourceSummary(job.source) }}</span><span>{{ formatDate(job.updatedAt) }}</span></span>
            </span>
            <span class="prod-task-status" :class="statusClass(job.status)">
              <span v-if="job.status === 'running' || job.status === 'queued'" class="prod-task-progress"><i :style="{ width: `${progressOf(job)}%` }" /></span>
              {{ jobStatusLabel(job) }}
            </span>
            <span class="prod-task-tokens">{{ formatTokens(job.usage.promptTokens + job.usage.completionTokens) }}</span>
          </button>
          <div v-if="!production.jobs.length" class="prod-empty">还没有制作任务。从上面的来源或模板开始。</div>
        </div>
      </template>

      <template v-else-if="productionUi.view === 'tasks'">
        <div class="prod-page-head">
          <div><div class="prod-eyebrow">制作 / 任务</div><h1>任务</h1></div>
        </div>
        <div class="prod-filterbar">
          <button v-for="filter in taskFilters" :key="filter.id" type="button" :class="{ 'is-active': taskFilter === filter.id }" @click="taskFilter = filter.id">{{ filter.label }} <span class="prod-filter-count">{{ taskFilterCounts[filter.id] }}</span></button>
        </div>
        <div class="prod-table-head"><span>任务</span><span>模板</span><span>来源</span><span>状态</span><span>用量</span><span>更新</span></div>
        <div class="prod-list">
          <button v-for="job in filteredJobs" :key="job.id" type="button" class="prod-table-row" @click="openTask(job)">
            <span class="prod-task-cell"><span class="prod-task-title" :title="job.title">{{ job.title }}<span v-if="job.options.auto" class="prod-auto-badge">自动</span></span><span v-if="job.status === 'running' || job.error" class="prod-task-sub"><span v-if="job.status === 'running'">进行到「{{ job.currentStep || '准备' }}」</span><span v-else class="prod-task-error" :title="job.error">{{ displayError(job.error) }}</span></span></span>
            <span class="prod-task-sub">{{ templateName(job.templateId) }}</span>
            <span class="prod-task-sub" :title="sourceSummary(job.source)">{{ sourceSummary(job.source) }}</span>
            <span class="prod-task-status" :class="statusClass(job.status)"><i class="prod-dot" :class="statusClass(job.status) === 'is-success' ? 'is-green' : statusClass(job.status) === 'is-error' ? 'is-red' : statusClass(job.status) === 'is-gold' ? 'is-gold' : 'is-blue'" />{{ jobStatusLabel(job) }}</span>
            <span class="prod-task-tokens">{{ formatTokens(job.usage.promptTokens + job.usage.completionTokens) }}</span>
            <span class="prod-task-sub">{{ formatDate(job.updatedAt) }}</span>
          </button>
          <div v-if="!filteredJobs.length" class="prod-empty">{{ taskEmptyText }}</div>
        </div>
      </template>

      <template v-else-if="productionUi.view === 'templates'">
        <div class="prod-page-head">
          <div><div class="prod-eyebrow">制作 / 模板库</div><h1>模板库</h1></div>
        </div>
        <div class="prod-template-grid">
          <TemplateCard
            v-for="template in visibleTemplates"
            :key="template.id"
            :template="template"
            :icon="iconFor(template.id)"
            @select="openWizard(templateSource(template), template.id)"
          />
        </div>
      </template>

      <template v-else-if="productionUi.view === 'prompts'">
        <div class="prod-page-head">
          <div><div class="prod-eyebrow">制作 / 提示词</div><h1>提示词</h1><p>内置风格开箱可用；自定义提示词可以新建、编辑和停用。</p></div>
          <div class="prod-page-actions"><button type="button" class="prod-btn" @click="openPrompt()"><Plus :size="14" />新建提示词</button></div>
        </div>
        <div v-for="group in promptGroups" :key="group.key" class="prod-prompt-group">
          <div class="prod-prompt-group-head"><h2>{{ group.label }}</h2><span>{{ group.items.length }} 个</span></div>
          <div v-for="prompt in group.items" :key="prompt.id" class="prod-prompt-row" :class="{ 'is-default': production.settings?.defaultPrompt === prompt.name }">
            <span class="prod-prompt-name">{{ prompt.name }}</span>
            <span class="prod-prompt-desc">{{ prompt.description || '自定义提示词' }}</span>
            <span style="display: flex; gap: 5px; justify-self: end">
              <button type="button" class="prod-btn is-small" :disabled="production.settings?.defaultPrompt === prompt.name" @click="setDefaultPrompt(prompt)">{{ production.settings?.defaultPrompt === prompt.name ? '默认' : '设为默认' }}</button>
              <button v-if="!prompt.builtin" type="button" class="prod-icon-btn" aria-label="编辑" @click="openPrompt(prompt)"><Settings2 :size="13" /></button>
              <button v-if="!prompt.builtin" type="button" class="prod-icon-btn" aria-label="删除" @click="removePrompt(prompt)"><X :size="13" /></button>
            </span>
          </div>
        </div>
        <div v-if="!visiblePrompts.some((prompt) => !prompt.builtin)" class="prod-prompt-group">
          <div class="prod-prompt-group-head"><h2>自定义</h2><span>0 个</span></div>
          <div class="prod-empty">还没有自定义提示词。新建一个，或从内置风格复制后修改。</div>
        </div>
      </template>

      <ProductionSubscriptions v-else-if="productionUi.view === 'subscriptions'" @login="openLogin" @changed="onStarted" />
      <ProductionSettings v-else-if="productionUi.view === 'settings'" @login="openLogin" />
    </div>

    </div>
    <aside v-if="productionUi.view !== 'settings'" class="prod-rail">
      <div class="prod-rail-head"><span>制作引擎</span><span class="prod-engine-pill">本地</span></div>
      <div class="prod-engine-card">
        <div class="prod-engine-top"><Cpu :size="14" /><strong>Rust 任务引擎</strong><i class="prod-dot" :class="production.settings?.hasAiKey ? 'is-green' : 'is-gold'" /></div>
        <p>{{ activeJobs.length ? `${activeJobs.length} 个进行中` : '空闲' }} · 不常驻 HTTP 服务、不占端口</p>
      </div>
      <div class="prod-rail-section">
        <div class="prod-rail-label"><span>模型</span><button type="button" class="prod-link" @click="productionUi.view = 'settings'">设置</button></div>
        <dl class="prod-kv">
          <div><dt>AI</dt><dd>{{ production.settings?.aiModel || '未配置' }}</dd></div>
          <div><dt>ASR</dt><dd>{{ production.settings?.asrModel || '未配置' }}</dd></div>
          <div><dt>检索</dt><dd>{{ production.settings?.searchProvider || '未配置' }}</dd></div>
        </dl>
      </div>
      <div class="prod-rail-section">
        <div class="prod-rail-label"><span>入库位置</span><button type="button" class="prod-link" @click="productionUi.view = 'settings'">设置</button></div>
        <div class="prod-note">手动制作可指定目录；自动任务先入「待整理」。</div>
      </div>
      <div class="prod-rail-section">
        <div class="prod-rail-label"><span>累计用量</span></div>
        <div class="prod-rail-usage"><strong>{{ formatTokens(totals.tokens) }}</strong><span>tokens</span><em>{{ totals.cost ? `≈ ¥ ${totals.cost.toFixed(2)}` : '—' }}</em></div>
        <div class="prod-note">金额按模型目录参考价估算。</div>
      </div>
      <div class="prod-rail-section">
        <div class="prod-rail-label"><span>连接</span></div>
        <div class="prod-note"><i class="prod-dot" :class="production.account ? 'is-green' : 'is-gold'" /> {{ production.account ? `B站：${production.account.name}` : 'B站：未登录（公开链接仍可用）' }}</div>
      </div>
    </aside>

    <ProductionWizard
      :open="productionUi.wizardOpen"
      :initial-source="productionUi.wizardSource"
      :initial-template="productionUi.wizardTemplate"
      :initial-url="productionUi.wizardInitialUrl"
      @close="productionUi.wizardOpen = false"
      @started="onStarted"
      @login="openLogin"
    />
    <ProductionTaskDialog :job-id="productionUi.taskId" @close="productionUi.taskId = ''" />
    <BiliLoginDialog :open="productionUi.loginOpen" @close="productionUi.loginOpen = false" @success="onLoginSuccess" />

    <div v-if="promptOpen" class="prod-overlay" @click.self="promptOpen = false">
      <div class="prod-dialog" role="dialog" aria-modal="true" aria-labelledby="prodPromptTitle">
        <header class="prod-dialog-head">
          <div><div class="prod-eyebrow">制作 / 提示词</div><h2 id="prodPromptTitle">{{ promptDraft.id ? '编辑提示词' : '新建提示词' }}</h2></div>
          <button type="button" class="prod-icon-btn" aria-label="关闭" @click="promptOpen = false"><X :size="15" /></button>
        </header>
        <div v-if="promptDraft.id" class="prod-prompt-tabs">
          <button type="button" :class="{ 'is-active': promptTab === 'edit' }" @click="promptTab = 'edit'">编辑</button>
          <button type="button" :class="{ 'is-active': promptTab === 'versions' }" @click="promptTab = 'versions'; if (!promptVersions.length) void loadPromptVersions()">版本 <span>{{ promptVersions.length }}</span></button>
        </div>
        <div class="prod-dialog-body">
          <p v-if="promptShadowsBuiltin" class="prod-warning-line" style="margin: 0 0 10px">这个自定义提示词与内置「{{ promptDraft.name }}」同名，会覆盖内置版本；删除自定义后会回落到内置。</p>
          <p v-if="promptRestoreNotice" class="prod-notice-line" style="margin: 0 0 10px">{{ promptRestoreNotice }}</p>
          <template v-if="promptTab === 'edit'">
            <div class="prod-options-grid">
              <label class="prod-field"><span>名称</span><input v-model="promptDraft.name" class="prod-input" placeholder="例如：我的行业观察" /></label>
              <label class="prod-field"><span>描述</span><input v-model="promptDraft.description" class="prod-input" placeholder="一句话说明风格" /></label>
              <label class="prod-field is-wide"><span>System prompt</span><textarea v-model="promptDraft.system" class="prod-textarea" placeholder="你是一名…" /></label>
            </div>
            <p v-if="promptError" class="prod-wizard-hint" style="color: var(--red)">{{ promptError }}</p>
          </template>
          <template v-else>
            <div v-if="promptVersionError" class="prod-error-box" style="margin-bottom: 10px"><p>{{ promptVersionError }}</p></div>
            <div v-if="promptVersionLoading" class="prod-wizard-hint">正在读取版本…</div>
            <div v-else-if="!promptVersions.length" class="prod-wizard-hint">还没有历史版本；保存一次后会生成 v1。</div>
            <div v-else class="prod-prompt-version-layout">
              <div class="prod-version-list">
                <button v-for="version in promptVersions" :key="version.version" type="button" class="prod-version-row" :class="{ 'is-selected': selectedPromptVersion?.version === version.version }" @click="viewPromptVersion(version)">
                  <b>v{{ version.version }} <span v-if="version.version === promptVersions[0]?.version" class="prod-badge is-blue">当前</span></b>
                  <span>{{ formatDate(version.createdAt) }}</span>
                  <small>{{ version.chars }} 字</small>
                </button>
              </div>
              <div class="prod-version-detail">
                <div v-if="selectedPromptVersion" class="prod-version-detail-head">
                  <div><b>v{{ selectedPromptVersion.version }} · {{ selectedPromptVersion.name }}</b><span>当前草稿 vs v{{ selectedPromptVersion.version }} · {{ selectedPromptVersion.chars }} 字</span></div>
                  <button type="button" class="prod-btn is-small" @click="promptRestoreVersion = selectedPromptVersion.version">恢复此版本</button>
                </div>
                <div v-if="selectedPromptVersion" class="prod-prompt-diff">
                  <div v-for="(line, index) in promptDiff" :key="index" class="prod-prompt-diff-row" :class="`is-${line.type}`"><span>{{ line.type === 'add' ? '+' : line.type === 'remove' ? '−' : ' ' }}</span><code>{{ line.text || ' ' }}</code></div>
                </div>
              </div>
            </div>
            <div v-if="promptRestoreVersion" class="prod-delete-confirm" style="margin-top: 10px">
              <div><b>恢复 v{{ promptRestoreVersion }}？</b><span>当前提示词会被替换；恢复动作会保存成一个新版本，不会删除旧版本。</span></div>
              <button type="button" class="prod-btn is-small is-danger" @click="restorePromptVersion">确认恢复</button>
              <button type="button" class="prod-btn is-small" @click="promptRestoreVersion = 0">取消</button>
            </div>
          </template>
        </div>
        <footer class="prod-dialog-foot">
          <button type="button" class="prod-btn" @click="promptOpen = false">取消</button>
          <button v-if="promptTab === 'edit'" type="button" class="prod-btn is-primary" :disabled="promptSaving || !promptDraft.name.trim() || !promptDraft.system.trim()" @click="savePrompt"><LoaderCircle v-if="promptSaving" :size="13" class="prod-spin" />保存</button>
          <button v-else type="button" class="prod-btn is-primary" @click="promptTab = 'edit'">返回编辑</button>
        </footer>
      </div>
    </div>
  </div>
</template>

<style scoped>
.prod-spin { animation: prod-spin 1s linear infinite; }
.prod-mono { font-family: var(--mono); color: var(--ink-3); font-size: 12px; }
@keyframes prod-spin { to { transform: rotate(360deg); } }
</style>
