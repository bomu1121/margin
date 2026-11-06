<script setup lang="ts">
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
  Settings2,
  Sparkles,
  Star,
  X,
} from 'lucide-vue-next'

import BiliLoginDialog from '../components/production/BiliLoginDialog.vue'
import ProductionSettings from '../components/production/ProductionSettings.vue'
import ProductionTaskDialog from '../components/production/ProductionTaskDialog.vue'
import ProductionWizard from '../components/production/ProductionWizard.vue'
import * as api from '../lib/api'
import {
  formatDate,
  formatTokens,
  progressOf,
  sourceSummary,
  statusClass,
  statusLabel,
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
} from '../stores/production'
import type { ProductionJob, ProductionPrompt, ProductionTemplate } from '../types'

const taskFilter = ref<'all' | 'running' | 'success' | 'failed'>('all')
const knownSuccess = new Set<string>()
const notice = ref('')
const promptOpen = ref(false)
const promptSaving = ref(false)
const promptError = ref('')
const promptDraft = reactive({ id: '', name: '', description: '', system: '' })

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

const homeTemplates = computed(() => production.templates.slice(0, 6))
const taskFilters = [
  { id: 'all', label: '全部' },
  { id: 'running', label: '进行中' },
  { id: 'success', label: '已完成' },
  { id: 'failed', label: '失败 / 中断' },
] as const
const filteredJobs = computed(() => {
  if (taskFilter.value === 'all') return production.jobs
  if (taskFilter.value === 'failed') return production.jobs.filter((job) => ['error', 'cancelled', 'interrupted'].includes(job.status))
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
  const builtin = production.prompts.filter((item) => item.builtin)
  const custom = production.prompts.filter((item) => !item.builtin)
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
    new Notification('制作完成', { body: `${job.title} 已入库页边笔记` })
  }
}

function openWizard(source = '', template = ''): void {
  productionUi.wizardSource = source
  productionUi.wizardTemplate = template
  productionUi.wizardOpen = true
  productionUi.taskId = ''
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
  return templateById(production.templates, id)?.name ?? id
}

function iconFor(id: string): unknown {
  return templateIcons[id] ?? BookOpen
}

function openPrompt(prompt?: ProductionPrompt): void {
  promptError.value = ''
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
}

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

onMounted(async () => {
  for (const job of production.jobs.filter((item) => item.status === 'success')) knownSuccess.add(job.id)
  if ('Notification' in window && Notification.permission === 'default') void Notification.requestPermission()
  if (!production.loaded) await loadProduction()
})

</script>

<template>
  <div class="prod-layout">
    <div class="prod-main">
      <div class="prod-main-inner">
      <div v-if="notice" class="prod-banner" style="margin-bottom: 12px"><span class="prod-banner-tag">提示</span>{{ notice }}</div>

      <template v-if="productionUi.view === 'home'">
        <div class="prod-page-head">
          <div>
            <div class="prod-eyebrow">制作台 / 本地引擎</div>
            <h1>把素材变成笔记</h1>
            <p>B站链接、本地音视频或一份文稿，选一条内置模板，剩下的交给页边笔记。</p>
          </div>
          <div class="prod-page-actions">
            <button type="button" class="prod-btn is-primary" @click="openWizard()"><Plus :size="14" />新建制作</button>
            <button type="button" class="prod-btn" @click="openWizard('favorites')"><Star :size="14" />从 B 站收藏开始</button>
          </div>
        </div>

        <div class="prod-banner">
          <span class="prod-banner-tag">引擎</span>
          结果只进页边笔记：制作引擎只负责生成 outbox，目录、front matter、索引和标注都由页边笔记决定。
          <button type="button" @click="productionUi.view = 'settings'">制作设置</button>
        </div>

        <div class="prod-section-head">
          <div><h2>从什么开始</h2><p>来源决定第一步；模板只画加工步骤。</p></div>
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
          <div><h2>内置模板</h2><p>模板是固定的，不需要画布；每个模板只暴露少量参数。</p></div>
          <button type="button" class="prod-link" @click="productionUi.view = 'templates'">查看全部 {{ production.templates.length }} 个 →</button>
        </div>
        <div class="prod-template-grid">
          <article v-for="template in homeTemplates" :key="template.id" class="prod-template">
            <div class="prod-template-top">
              <span class="prod-template-icon"><component :is="iconFor(template.id)" :size="14" /></span>
              <span class="prod-template-name">{{ template.name }}</span>
              <span class="prod-badge" :class="{ 'is-blue': template.badge === '基础', 'is-gold': template.badge === '垂直' }">{{ template.badge }}</span>
            </div>
            <div class="prod-template-desc">{{ template.description }}</div>
            <div class="prod-pipeline"><span v-for="item in template.pipeline" :key="item">{{ item }}</span></div>
            <div class="prod-template-foot">
              <small>{{ template.sources.map((source) => ({ bili: 'B站', favorites: 'B站收藏', file: '本地文件', text: '文本' })[source] ?? source).join(' / ') }}</small>
              <button type="button" class="prod-btn is-small" @click="openWizard(templateSource(template), template.id)">用此模板</button>
            </div>
          </article>
        </div>

        <div class="prod-section-head">
          <div><h2>最近制作</h2><p>结果只进页边笔记；失败的任务保留现场，可重试。</p></div>
          <button type="button" class="prod-link" @click="productionUi.view = 'tasks'">全部任务 →</button>
        </div>
        <div class="prod-list">
          <button v-for="job in production.jobs.slice(0, 5)" :key="job.id" type="button" class="prod-task-row" @click="openTask(job)">
            <i class="prod-dot" :class="statusClass(job.status) === 'is-success' ? 'is-green' : statusClass(job.status) === 'is-error' ? 'is-red' : statusClass(job.status) === 'is-gold' ? 'is-gold' : 'is-blue'" />
            <span>
              <span class="prod-task-title">{{ job.title }}</span>
              <span class="prod-task-sub"><span>{{ templateName(job.templateId) }}</span><span>{{ sourceSummary(job.source) }}</span><span>{{ formatDate(job.updatedAt) }}</span></span>
            </span>
            <span class="prod-task-status" :class="statusClass(job.status)">
              <span v-if="job.status === 'running' || job.status === 'queued'" class="prod-task-progress"><i :style="{ width: `${progressOf(job)}%` }" /></span>
              {{ statusLabel(job.status) }}
            </span>
            <span class="prod-task-tokens">{{ formatTokens(job.usage.promptTokens + job.usage.completionTokens) }}</span>
          </button>
          <div v-if="!production.jobs.length" class="prod-empty">还没有制作任务。从上面的来源或模板开始。</div>
        </div>
      </template>

      <template v-else-if="productionUi.view === 'tasks'">
        <div class="prod-page-head">
          <div><div class="prod-eyebrow">制作 / 任务</div><h1>任务</h1><p>扁平任务历史，不引入工程文件夹。进行中、失败和中断都在这里收口。</p></div>
          <div class="prod-page-actions"><button type="button" class="prod-btn is-primary" @click="openWizard()"><Plus :size="14" />新建制作</button></div>
        </div>
        <div class="prod-filterbar">
          <button v-for="filter in taskFilters" :key="filter.id" type="button" :class="{ 'is-active': taskFilter === filter.id }" @click="taskFilter = filter.id">{{ filter.label }}</button>
        </div>
        <div class="prod-table-head"><span>任务</span><span>模板</span><span>来源</span><span>状态</span><span>用量</span><span>更新</span><span /></div>
        <div class="prod-list">
          <button v-for="job in filteredJobs" :key="job.id" type="button" class="prod-table-row" @click="openTask(job)">
            <span class="prod-task-main"><span class="prod-task-title">{{ job.title }}</span><span class="prod-task-sub"><span v-if="job.status === 'running'">进行到「{{ job.currentStep || '准备' }}」</span><span v-else-if="job.error" class="prod-task-error" :title="job.error">{{ job.error }}</span><span v-else>已入库</span></span></span>
            <span class="prod-task-sub">{{ templateName(job.templateId) }}</span>
            <span class="prod-task-sub">{{ sourceSummary(job.source) }}</span>
            <span class="prod-task-status" :class="statusClass(job.status)"><i class="prod-dot" :class="statusClass(job.status) === 'is-success' ? 'is-green' : statusClass(job.status) === 'is-error' ? 'is-red' : statusClass(job.status) === 'is-gold' ? 'is-gold' : 'is-blue'" />{{ statusLabel(job.status) }}</span>
            <span class="prod-task-tokens">{{ formatTokens(job.usage.promptTokens + job.usage.completionTokens) }}</span>
            <span class="prod-task-sub">{{ formatDate(job.updatedAt) }}</span>
            <span class="prod-task-arrow">›</span>
          </button>
          <div v-if="!filteredJobs.length" class="prod-empty">这个筛选下没有任务。</div>
        </div>
      </template>

      <template v-else-if="productionUi.view === 'templates'">
        <div class="prod-page-head">
          <div><div class="prod-eyebrow">制作 / 模板库</div><h1>模板库</h1><p>模板是内置的；选一个来源，模板只决定加工步骤。</p></div>
          <div class="prod-page-actions"><button type="button" class="prod-btn is-primary" @click="openWizard()"><Plus :size="14" />新建制作</button></div>
        </div>
        <template v-for="group in ['基础', '进阶', '垂直']" :key="group">
          <div v-if="production.templates.some((template) => template.group === group)" class="prod-section-head">
            <div><h2>{{ group }}</h2><p>{{ group === '基础' ? '内置基础链路，开箱可用。' : group === '进阶' ? '需要额外模型/检索能力的链路。' : '垂直领域专用链路。' }}</p></div>
          </div>
          <div v-if="production.templates.some((template) => template.group === group)" class="prod-template-grid">
            <article v-for="template in production.templates.filter((item) => item.group === group)" :key="template.id" class="prod-template">
              <div class="prod-template-top"><span class="prod-template-icon"><component :is="iconFor(template.id)" :size="14" /></span><span class="prod-template-name">{{ template.name }}</span><span class="prod-badge" :class="{ 'is-blue': template.badge === '基础', 'is-gold': template.badge === '垂直' }">{{ template.badge }}</span></div>
              <div class="prod-template-desc">{{ template.description }}</div>
              <div class="prod-pipeline"><span v-for="item in template.pipeline" :key="item">{{ item }}</span></div>
              <div class="prod-template-foot"><small>{{ template.steps.length }} 个步骤</small><button type="button" class="prod-btn is-small" @click="openWizard(templateSource(template), template.id)">用此模板</button></div>
            </article>
          </div>
        </template>
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
            <span class="prod-prompt-usage">{{ prompt.builtin ? '内置' : '自定义' }}</span>
            <span style="display: flex; gap: 5px; justify-self: end">
              <button type="button" class="prod-btn is-small" :disabled="production.settings?.defaultPrompt === prompt.name" @click="setDefaultPrompt(prompt)">{{ production.settings?.defaultPrompt === prompt.name ? '默认' : '设为默认' }}</button>
              <button v-if="!prompt.builtin" type="button" class="prod-icon-btn" aria-label="编辑" @click="openPrompt(prompt)"><Settings2 :size="13" /></button>
              <button v-if="!prompt.builtin" type="button" class="prod-icon-btn" aria-label="删除" @click="removePrompt(prompt)"><X :size="13" /></button>
            </span>
          </div>
        </div>
      </template>

      <ProductionSettings v-else-if="productionUi.view === 'settings'" @login="openLogin" />
    </div>

    </div>
    <aside class="prod-rail">
      <div class="prod-rail-head"><span>制作引擎</span><span class="prod-engine-pill">本地</span></div>
      <div class="prod-engine-card">
        <div class="prod-engine-top"><Cpu :size="14" /><strong>Rust 任务引擎</strong><i class="prod-dot" :class="production.settings?.hasAiKey ? 'is-green' : 'is-gold'" /></div>
        <p>不常驻 HTTP 服务、不占端口；空闲时只保留任务表。</p>
        <dl class="prod-kv" style="margin-top: 7px">
          <div><dt>队列</dt><dd>{{ production.jobs.filter((job) => job.status === 'queued').length }} 等待</dd></div>
          <div><dt>运行</dt><dd>{{ activeJobs.length }} 进行中</dd></div>
        </dl>
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
        <div class="prod-path">笔记库 / ScribeFlow</div>
        <div class="prod-note">目录、文件名、front matter、索引和标注都由页边笔记决定。</div>
      </div>
      <div class="prod-rail-section">
        <div class="prod-rail-label"><span>累计用量</span><span class="prod-mono">{{ production.jobs.length }} 个任务</span></div>
        <div class="prod-rail-usage"><strong>{{ formatTokens(totals.tokens) }}</strong><span>tokens</span><em>{{ totals.cost ? `≈ ¥ ${totals.cost.toFixed(2)}` : '—' }}</em></div>
        <div class="prod-usage-bar"><i :style="{ width: `${Math.min(100, (totals.tokens / 500000) * 100)}%` }" /></div>
        <div class="prod-note">token 来自接口返回值；金额按模型目录参考价估算，未收录价格时不计金额。</div>
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
        <div class="prod-dialog-body">
          <div class="prod-options-grid">
            <label class="prod-field"><span>名称</span><input v-model="promptDraft.name" class="prod-input" placeholder="例如：我的行业观察" /></label>
            <label class="prod-field"><span>描述</span><input v-model="promptDraft.description" class="prod-input" placeholder="一句话说明风格" /></label>
            <label class="prod-field is-wide"><span>System prompt</span><textarea v-model="promptDraft.system" class="prod-textarea" placeholder="你是一名…" /></label>
          </div>
          <p v-if="promptError" class="prod-wizard-hint" style="color: var(--red)">{{ promptError }}</p>
        </div>
        <footer class="prod-dialog-foot">
          <button type="button" class="prod-btn" @click="promptOpen = false">取消</button>
          <button type="button" class="prod-btn is-primary" :disabled="promptSaving || !promptDraft.name.trim() || !promptDraft.system.trim()" @click="savePrompt"><LoaderCircle v-if="promptSaving" :size="13" class="prod-spin" />保存</button>
        </footer>
      </div>
    </div>
  </div>
</template>

<style scoped>
.prod-spin { animation: prod-spin 1s linear infinite; }
.prod-mono { font-family: var(--mono); color: var(--ink-3); font-size: 10px; }
@keyframes prod-spin { to { transform: rotate(360deg); } }
</style>
