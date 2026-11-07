<script setup lang="ts">
import { computed, onMounted, reactive, ref, watch } from 'vue'
import { writeText } from '@tauri-apps/plugin-clipboard-manager'
import { CheckCircle2, Copy, KeyRound, LoaderCircle, LogOut, RefreshCw, Trash2 } from 'lucide-vue-next'

import * as api from '../../lib/api'
import {
  production,
  productionUi,
  refreshJobs,
  refreshSettings,
} from '../../stores/production'
import type { BalanceResult, ModelOption, ModelPrice, PriceQuote, ProductionDataUsage, ProductionSaveSettings, ProviderPreset, TestModelResult, UsageSummary } from '../../types'

const emit = defineEmits<{ login: [] }>()

const active = ref('ai')
const saving = ref(false)
const notice = ref('')
const error = ref('')
const usage = ref<ProductionDataUsage | null>(null)
const fields = ref<string[]>(['人物', '事件'])
const providers = ref<ProviderPreset[]>([])
const models = ref<ModelOption[]>([])
const modelSearch = ref('')
const modelLoading = ref(false)
const modelSource = ref('')
const modelWarning = ref('')
const aiError = ref('')
const balance = ref<BalanceResult | null>(null)
const balanceLoading = ref(false)
const usageSummary = ref<UsageSummary | null>(null)
const testing = ref(false)
const testResult = ref<TestModelResult | null>(null)
const testError = ref('')
const priceQuote = ref<PriceQuote | null>(null)
const modelSort = ref<'recommended' | 'context' | 'input' | 'output'>('recommended')
const copyNotice = ref('')
const manualPrices = ref<ModelPrice[]>([])
const manualInput = ref('')
const manualOutput = ref('')
const manualSaving = ref(false)
const manualError = ref('')
const confirmClearResumable = ref(false)
let copyTimer = 0
let modelSeq = 0
let priceSeq = 0
let balanceSeq = 0

const form = reactive({
  aiProvider: 'deepseek',
  aiProtocol: 'openai',
  aiBaseUrl: '',
  aiModel: '',
  aiKey: '',
  aiExchangeRate: 7.2,
  monthlyBudgetCny: 0,
  asrEngine: 'mimo',
  asrBaseUrl: '',
  asrModel: '',
  asrKey: '',
  asrPricePerMinute: 0,
  searchProvider: 'zhipu',
  searchKey: '',
  searchMaxResults: 5,
  defaultTemplate: 'video-note',
  retries: 2,
  notify: true,
  sound: true,
  ffmpegPath: '',
  defaultPrompt: '成文版',
})

const groups = [
  { key: 'ai', label: 'AI 模型' },
  { key: 'asr', label: '语音识别' },
  { key: 'search', label: '联网检索' },
  { key: 'make', label: '制作' },
  { key: 'bili', label: 'B站账号' },
  { key: 'prompts', label: '提示词' },
  { key: 'labels', label: '标签与属性' },
  { key: 'data', label: '数据与存储' },
  { key: 'notify', label: '通知' },
]
const fieldChoices = ['人物', '事件', '时期', '公司', '概念']
const templates = computed(() => production.templates)
const selectedProvider = computed(() => providers.value.find((item) => item.id === form.aiProvider))
const hasSavedKeyForForm = computed(() => Boolean(production.settings?.hasAiKey && production.settings.aiProvider === form.aiProvider))
const selectedModel = computed(() => models.value.find((item) => item.id === form.aiModel))
const selectedPrice = computed(() => selectedModel.value?.price ?? priceQuote.value)
const displayPrice = computed(() => selectedPrice.value)
const manualPrice = computed(() => manualPrices.value.find((item) => item.provider === form.aiProvider && item.model === form.aiModel.trim()))
const hasCatalogPrice = computed(() => Boolean(selectedPrice.value && selectedPrice.value.source !== 'manual'))
const showManualPrice = computed(() => Boolean(!selectedProvider.value?.local && form.aiModel.trim() && (form.aiProvider === 'custom' || !hasCatalogPrice.value)))
const matchedModels = computed(() => {
  const query = modelSearch.value.trim().toLowerCase()
  if (!query) return models.value
  return models.value.filter((model) => model.id.toLowerCase().includes(query) || model.name.toLowerCase().includes(query))
})
const filteredModels = computed(() => {
  const list = [...matchedModels.value]
  if (modelSort.value !== 'recommended') {
    list.sort((a, b) => {
      if (modelSort.value === 'context') return compareOptional(a.contextLength > 0 ? a.contextLength : null, b.contextLength > 0 ? b.contextLength : null, 'desc') || a.id.localeCompare(b.id)
      const key = modelSort.value === 'input' ? 'inputCny' : 'outputCny'
      return compareOptional(a.price ? a.price[key] : null, b.price ? b.price[key] : null, 'asc') || a.id.localeCompare(b.id)
    })
  }
  return list.slice(0, 80)
})
const pinnedModel = computed(() => {
  const id = form.aiModel.trim()
  if (!id || modelSearch.value.trim()) return null
  const selected = models.value.find((item) => item.id === id)
  return selected && !filteredModels.value.some((item) => item.id === id) ? selected : null
})

function compareOptional(a: number | null, b: number | null, direction: 'asc' | 'desc'): number {
  if (a === null && b === null) return 0
  if (a === null) return 1
  if (b === null) return -1
  return direction === 'desc' ? b - a : a - b
}

function fill(): void {
  const settings = production.settings
  if (!settings) return
  form.aiProvider = settings.aiProvider
  form.aiProtocol = settings.aiProtocol
  form.aiBaseUrl = settings.aiBaseUrl
  form.aiModel = settings.aiModel
  form.aiExchangeRate = settings.aiExchangeRate
  form.monthlyBudgetCny = settings.monthlyBudgetCny
  priceQuote.value = null
  form.asrEngine = settings.asrEngine
  form.asrBaseUrl = settings.asrBaseUrl
  form.asrModel = settings.asrModel
  form.asrPricePerMinute = settings.asrPricePerMinute
  form.searchProvider = settings.searchProvider
  form.searchMaxResults = settings.searchMaxResults
  form.defaultTemplate = settings.defaultTemplate
  form.retries = settings.retries
  form.notify = settings.notify
  form.sound = settings.sound
  form.ffmpegPath = settings.ffmpegPath
  form.defaultPrompt = settings.defaultPrompt
  fields.value = [...(settings.defaultFields?.length ? settings.defaultFields : ['人物', '事件'])]
}

watch(() => production.settings, fill, { immediate: true })

async function save(): Promise<void> {
  saving.value = true
  error.value = ''
  notice.value = ''
  const input: ProductionSaveSettings = {
    aiProvider: form.aiProvider,
    aiProtocol: form.aiProtocol,
    aiBaseUrl: form.aiBaseUrl,
    aiModel: form.aiModel,
    aiExchangeRate: form.aiExchangeRate,
    monthlyBudgetCny: form.monthlyBudgetCny,
    asrEngine: form.asrEngine,
    asrBaseUrl: form.asrBaseUrl,
    asrModel: form.asrModel,
    asrPricePerMinute: form.asrPricePerMinute,
    searchProvider: form.searchProvider,
    searchMaxResults: form.searchMaxResults,
    defaultTemplate: form.defaultTemplate,
    retries: form.retries,
    notify: form.notify,
    sound: form.sound,
    ffmpegPath: form.ffmpegPath,
    defaultPrompt: form.defaultPrompt,
    defaultFields: [...fields.value],
  }
  if (form.aiKey.trim()) input.aiKey = form.aiKey.trim()
  if (form.asrKey.trim()) input.asrKey = form.asrKey.trim()
  if (form.searchKey.trim()) input.searchKey = form.searchKey.trim()
  try {
    production.settings = await api.productionSaveSettings(input)
    form.aiKey = ''
    form.asrKey = ''
    form.searchKey = ''
    notice.value = '设置已保存'
    void loadUsageSummary()
    window.setTimeout(() => { notice.value = '' }, 2200)
  } catch (cause) {
    error.value = api.errorText(cause)
  } finally {
    saving.value = false
  }
}

function formatCny(value: number): string {
  if (!Number.isFinite(value)) return '—'
  return `¥${value.toFixed(value < 0.01 ? 4 : 2)}`
}

function formatMoney(value: number): string {
  return `¥${value.toFixed(2)}`
}

function formatUsd(value: number): string {
  return `$${value.toFixed(value < 0.01 ? 4 : 2)}`
}

async function onProviderChange(): Promise<void> {
  const preset = selectedProvider.value
  form.aiProtocol = preset?.protocol ?? 'openai'
  form.aiBaseUrl = preset?.baseUrl ?? ''
  form.aiModel = preset?.defaultModel ?? ''
  form.aiKey = ''
  models.value = []
  modelSearch.value = ''
  modelSource.value = ''
  balance.value = null
  priceQuote.value = null
  modelWarning.value = ''
  aiError.value = ''
  testResult.value = null
  testError.value = ''
  await loadModels()
}

async function loadModels(): Promise<void> {
  if (!form.aiProvider) return
  const seq = ++modelSeq
  modelLoading.value = true
  aiError.value = ''
  try {
    const result = await api.productionModels(form.aiProvider, form.aiBaseUrl, form.aiKey)
    if (seq !== modelSeq) return
    models.value = result.models
    modelSource.value = result.source
    modelWarning.value = result.warning
    if (!form.aiModel && result.models[0]) form.aiModel = result.models[0].id
    await loadPrice()
  } catch (cause) {
    if (seq === modelSeq) aiError.value = api.errorText(cause)
  } finally {
    if (seq === modelSeq) modelLoading.value = false
  }
}

function chooseModel(model: ModelOption): void {
  form.aiModel = model.id
  priceQuote.value = model.price
  modelSearch.value = ''
  testResult.value = null
  testError.value = ''
}

watch(
  () => [form.aiProvider, form.aiBaseUrl, form.aiModel, form.aiKey],
  () => {
    testResult.value = null
    testError.value = ''
  },
)

watch(
  () => form.aiModel,
  (next) => {
    if (selectedModel.value?.id !== next) priceQuote.value = null
  },
)

async function testModel(): Promise<void> {
  if (!form.aiModel.trim()) {
    testError.value = '先选择或填写一个模型 ID'
    return
  }
  testing.value = true
  testError.value = ''
  testResult.value = null
  try {
    testResult.value = await api.productionTestModel(form.aiProvider, form.aiBaseUrl, form.aiModel, form.aiKey)
  } catch (cause) {
    testError.value = api.errorText(cause)
  } finally {
    testing.value = false
  }
}

async function loadPrice(force = false): Promise<void> {
  const model = form.aiModel.trim()
  const seq = ++priceSeq
  if (!model) {
    priceQuote.value = null
    return
  }
  const known = models.value.find((item) => item.id === model)
  if (known && !force) {
    priceQuote.value = known.price
    return
  }
  try {
    const quote = await api.productionPrice(form.aiProvider, model)
    if (seq === priceSeq) priceQuote.value = quote
  } catch {
    if (seq === priceSeq) priceQuote.value = null
  }
}

async function loadManualPrices(): Promise<void> {
  try {
    manualPrices.value = await api.productionModelPrices()
  } catch (cause) {
    manualError.value = api.errorText(cause)
  }
}

async function copyModelId(modelId: string): Promise<void> {
  // navigator.clipboard makes WebView2 show a "read clipboard" permission
  // prompt. The Tauri clipboard plugin writes directly through Rust.
  let copied = false
  try {
    await writeText(modelId)
    copied = true
  } catch {
    copied = false
  }
  copyNotice.value = copied ? `已复制 ${modelId}` : '复制失败，请手动选择模型 ID'
  window.clearTimeout(copyTimer)
  copyTimer = window.setTimeout(() => { copyNotice.value = '' }, 2200)
}

async function saveManualPrice(): Promise<void> {
  const provider = form.aiProvider
  const model = form.aiModel.trim()
  if (!provider || !model) return
  const inputText = manualInput.value.trim()
  const outputText = manualOutput.value.trim()
  if (!inputText || !outputText) {
    manualError.value = '输入价和输出价都要填；不知道就留空，不保存手动价目。'
    return
  }
  const inputCny = Number(inputText)
  const outputCny = Number(outputText)
  if (!Number.isFinite(inputCny) || !Number.isFinite(outputCny) || inputCny < 0 || outputCny < 0) {
    manualError.value = '价格必须是 0 或正数。'
    return
  }
  manualSaving.value = true
  manualError.value = ''
  try {
    const saved = await api.productionSetModelPrice(provider, model, inputCny, outputCny)
    manualPrices.value = manualPrices.value.filter((item) => !(item.provider === provider && item.model === model)).concat(saved)
    await loadPrice(true)
    const row = models.value.find((item) => item.id === model)
    if (row) row.price = priceQuote.value
    notice.value = '手动价目已保存；任务估算会优先使用它。'
  } catch (cause) {
    manualError.value = api.errorText(cause)
  } finally {
    manualSaving.value = false
  }
}

async function clearManualPrice(): Promise<void> {
  const provider = form.aiProvider
  const model = form.aiModel.trim()
  if (!provider || !model) return
  manualSaving.value = true
  manualError.value = ''
  try {
    await api.productionDeleteModelPrice(provider, model)
    manualPrices.value = manualPrices.value.filter((item) => !(item.provider === provider && item.model === model))
    await loadPrice(true)
    const row = models.value.find((item) => item.id === model)
    if (row) row.price = priceQuote.value
    notice.value = '手动价目已清除。'
  } catch (cause) {
    manualError.value = api.errorText(cause)
  } finally {
    manualSaving.value = false
  }
}

watch(manualPrice, (price) => {
  manualInput.value = price ? String(price.inputCny) : ''
  manualOutput.value = price ? String(price.outputCny) : ''
  manualError.value = ''
}, { immediate: true })

function formatContext(value: number): string {
  if (value >= 1_000_000) return `${(value / 1_000_000).toFixed(value % 1_000_000 === 0 ? 0 : 1)}M`
  if (value >= 1_000) return `${Math.round(value / 1_000)}K`
  return String(value)
}

function formatDate(value: number): string {
  if (!value) return '—'
  const date = new Date(value < 1_000_000_000_000 ? value * 1000 : value)
  if (Number.isNaN(date.getTime())) return '—'
  return `${date.getFullYear()}年${date.getMonth() + 1}月${date.getDate()}日`
}

async function queryBalance(): Promise<void> {
  const seq = ++balanceSeq
  balanceLoading.value = true
  aiError.value = ''
  try {
    const result = await api.productionBalance(form.aiProvider, form.aiBaseUrl, form.aiKey)
    if (seq === balanceSeq) balance.value = result
  } catch (cause) {
    if (seq === balanceSeq) {
      aiError.value = api.errorText(cause)
      balance.value = null
    }
  } finally {
    if (seq === balanceSeq) balanceLoading.value = false
  }
}

async function loadUsageSummary(): Promise<void> {
  try {
    usageSummary.value = await api.productionUsageSummary()
  } catch {
    usageSummary.value = null
  }
}

function toggleField(field: string): void {
  const index = fields.value.indexOf(field)
  if (index >= 0) fields.value.splice(index, 1)
  else fields.value.push(field)
}

async function logout(): Promise<void> {
  try {
    await api.productionBiliLogout()
    await refreshSettings()
    notice.value = '已退出 B 站账号'
  } catch (cause) {
    error.value = api.errorText(cause)
  }
}

async function loadUsage(): Promise<void> {
  try {
    usage.value = await api.productionDataUsage()
  } catch {
    usage.value = null
  }
}

function avatarUrl(face: string): string {
  if (face.startsWith("//")) return `https:${face}`
  if (face.startsWith("http://")) return `https://${face.slice(7)}`
  return face
}

async function clearTemp(): Promise<void> {
  try {
    const result = await api.productionClearTemp()
    const kept = result.keptJobs
      ? `；保留 ${result.keptJobs} 个运行中/可续跑任务（${(result.keptBytes / 1024 / 1024).toFixed(1)} MB）`
      : ''
    notice.value = `已清理 ${(result.clearedBytes / 1024 / 1024).toFixed(1)} MB 临时音频${kept}`
    await loadUsage()
  } catch (cause) {
    error.value = api.errorText(cause)
  }
}

async function clearFinished(): Promise<void> {
  try {
    const count = await api.productionClearFinished()
    notice.value = `已清理 ${count} 条已完成任务`
    await refreshJobs()
    await loadUsage()
  } catch (cause) {
    error.value = api.errorText(cause)
  }
}

async function clearResumable(): Promise<void> {
  try {
    const count = await api.productionClearResumable()
    confirmClearResumable.value = false
    notice.value = `已清理 ${count} 条失败/中断任务；续跑检查点已删除`
    await refreshJobs()
    await loadUsage()
  } catch (cause) {
    error.value = api.errorText(cause)
    confirmClearResumable.value = false
  }
}

onMounted(async () => {
  if (!production.loaded) await refreshSettings()
  try {
    providers.value = await api.productionProviders()
  } catch (cause) {
    error.value = api.errorText(cause)
  }
  void loadUsage()
  void loadUsageSummary()
  void loadManualPrices()
  if (production.settings) void loadModels()
})
</script>

<template>
  <div class="prod-settings-layout">
    <nav class="prod-settings-nav" aria-label="设置分类">
      <button v-for="group in groups" :key="group.key" type="button" :class="{ 'is-active': active === group.key }" @click="active = group.key">{{ group.label }}</button>
    </nav>

    <div>
      <div v-if="notice" class="prod-banner" style="margin-bottom: 10px"><span class="prod-banner-tag">完成</span>{{ notice }}</div>
      <div v-if="error" class="prod-error-box" style="margin-bottom: 10px"><p>{{ error }}</p></div>

      <section v-if="active === 'ai'" class="prod-settings-panel is-active">
        <div class="prod-panel-head"><h2>AI 模型</h2><p>选服务商、填 Key、拉取模型。目录价自动折算成人民币；目录没收录时才可选填手动价。</p></div>

        <div class="prod-provider-bar">
          <label class="prod-field"><span>服务商</span>
            <select v-model="form.aiProvider" class="prod-select" @change="onProviderChange">
              <option v-for="provider in providers" :key="provider.id" :value="provider.id">{{ provider.name }}</option>
            </select>
          </label>
          <div class="prod-provider-note">
            <span v-if="selectedProvider?.local" class="prod-badge">本地 · 不计费</span>
            <span v-else-if="selectedProvider?.catalogId" class="prod-badge is-blue">已收录价格</span>
            <span>{{ selectedProvider?.note }}</span>
          </div>
        </div>

        <div class="prod-form-grid">
          <label class="prod-field is-wide"><span>API Key <em v-if="hasSavedKeyForForm" class="prod-field-state">已保存</em></span>
            <input v-model="form.aiKey" type="password" class="prod-input" autocomplete="off" :placeholder="selectedProvider?.local ? '本地模型可留空' : hasSavedKeyForForm ? '已保存，留空不修改' : '粘贴 API Key'" />
          </label>
          <div class="prod-field is-wide prod-field-actions">
            <button type="button" class="prod-btn" :disabled="modelLoading" @click="loadModels">
              <LoaderCircle v-if="modelLoading" :size="13" class="prod-spin" /><RefreshCw v-else :size="13" />{{ modelLoading ? '正在获取' : '获取模型' }}
            </button>
            <button type="button" class="prod-btn" :disabled="testing || !form.aiModel" @click="testModel">
              <LoaderCircle v-if="testing" :size="13" class="prod-spin" /><CheckCircle2 v-else :size="13" />{{ testing ? '测试中' : '测试生成' }}
            </button>
            <button v-if="selectedProvider?.supportsBalance" type="button" class="prod-btn" :disabled="balanceLoading" @click="queryBalance">
              <LoaderCircle v-if="balanceLoading" :size="13" class="prod-spin" /><RefreshCw v-else :size="13" />查询余额
            </button>
            <button type="button" class="prod-btn is-primary" :disabled="saving" @click="save"><LoaderCircle v-if="saving" :size="13" class="prod-spin" />保存设置</button>
            <a v-if="selectedProvider?.docsUrl" class="prod-link" href="#" @click.prevent="api.openExternalUrl(selectedProvider.docsUrl)">获取 Key / 文档</a>
          </div>
          <p v-if="aiError" class="prod-warning-line is-error">{{ aiError }}</p>
          <div v-if="testResult" class="prod-test-result">
            <CheckCircle2 :size="15" />
            <span><b>连接成功</b> · {{ testResult.latencyMs }} ms · {{ testResult.model }} · 回复「{{ testResult.reply }}」 · {{ testResult.promptTokens + testResult.completionTokens }} tokens</span>
          </div>
          <p v-if="testError" class="prod-warning-line is-error">{{ testError }}</p>
        </div>

        <div class="prod-model-section">
          <div class="prod-section-line">
            <span class="prod-section-title">模型</span>
            <span class="prod-section-hint">{{ models.length ? `${models.length} 个可用` : '先获取模型，或直接填写模型 ID' }}</span>
          </div>
          <div class="prod-model-current">
            <input v-model="form.aiModel" class="prod-input" placeholder="模型 ID，例如 deepseek-chat" @change="loadPrice()" />
            <span v-if="displayPrice" class="prod-model-current-price">{{ formatCny(displayPrice.inputCny) }} / {{ formatCny(displayPrice.outputCny) }}</span>
            <span v-else-if="selectedProvider?.local" class="prod-model-current-price is-muted">本地不计费</span>
            <span v-else class="prod-model-current-price is-muted">待查价</span>
          </div>
          <div v-if="models.length" class="prod-model-filter">
            <input v-model="modelSearch" class="prod-input" placeholder="搜索模型 ID 或名称…" />
            <div class="prod-model-sortbar">
              <div class="prod-model-sort" role="group" aria-label="模型排序">
                <button type="button" :class="{ 'is-active': modelSort === 'recommended' }" @click="modelSort = 'recommended'">推荐</button>
                <button type="button" :class="{ 'is-active': modelSort === 'context' }" @click="modelSort = 'context'">上下文</button>
                <button type="button" :class="{ 'is-active': modelSort === 'input' }" @click="modelSort = 'input'">输入价</button>
                <button type="button" :class="{ 'is-active': modelSort === 'output' }" @click="modelSort = 'output'">输出价</button>
              </div>
              <span class="prod-model-count">显示 {{ filteredModels.length }} / {{ models.length }}</span>
              <span v-if="copyNotice" class="prod-model-copy-note" role="status">{{ copyNotice }}</span>
            </div>
          </div>
          <div v-if="pinnedModel" class="prod-model-pinned">
            <span class="prod-model-pin-label">当前选择</span>
            <div class="prod-model-row is-selected">
              <button type="button" class="prod-model-select" @click="chooseModel(pinnedModel)">
                <span class="prod-model-name"><b>{{ pinnedModel.id }}</b><small v-if="pinnedModel.name && pinnedModel.name !== pinnedModel.id">{{ pinnedModel.name }}</small></span>
                <span class="prod-model-meta">
                  <span v-if="pinnedModel.contextLength">{{ formatContext(pinnedModel.contextLength) }} 上下文</span>
                  <span v-if="pinnedModel.price" class="prod-model-price">{{ formatCny(pinnedModel.price.inputCny) }} / {{ formatCny(pinnedModel.price.outputCny) }}</span>
                  <span v-else class="prod-model-price is-muted">未收录价格</span>
                </span>
              </button>
              <button type="button" class="prod-model-copy" aria-label="复制模型 ID" title="复制模型 ID" @click="copyModelId(pinnedModel.id)"><Copy :size="13" /></button>
            </div>
          </div>
          <div v-if="filteredModels.length" class="prod-model-list">
            <div v-for="model in filteredModels" :key="model.id" class="prod-model-row" :class="{ 'is-selected': model.id === form.aiModel }">
              <button type="button" class="prod-model-select" @click="chooseModel(model)">
                <span class="prod-model-name"><b>{{ model.id }}</b><small v-if="model.name && model.name !== model.id">{{ model.name }}</small></span>
                <span class="prod-model-meta">
                  <span v-if="model.contextLength">{{ formatContext(model.contextLength) }} 上下文</span>
                  <span v-if="model.price" class="prod-model-price">{{ formatCny(model.price.inputCny) }} / {{ formatCny(model.price.outputCny) }}</span>
                  <span v-else class="prod-model-price is-muted">未收录价格</span>
                </span>
              </button>
              <button type="button" class="prod-model-copy" aria-label="复制模型 ID" title="复制模型 ID" @click="copyModelId(model.id)"><Copy :size="13" /></button>
            </div>
          </div>
          <p v-else-if="!modelLoading && modelSearch" class="prod-wizard-hint">没有匹配「{{ modelSearch }}」的模型。</p>
          <p v-if="modelSource" class="prod-source-line">模型列表：{{ modelSource === 'provider' ? '服务商返回' : modelSource === 'catalog' ? '内置目录' : modelSource }} · 价格：models.dev 目录（美元折算，估算值）</p>
          <p v-if="modelWarning" class="prod-warning-line">{{ modelWarning }}</p>
        </div>

        <div v-if="showManualPrice" class="prod-manual-price">
          <div class="prod-section-line">
            <span class="prod-section-title">手动价目（可选）</span>
            <span class="prod-section-hint">目录没收录时，用你填写的人民币价估算</span>
          </div>
          <p class="prod-manual-lead">只影响成本估算，不影响服务商实际扣费。单位：人民币 / 100 万 token。</p>
          <div class="prod-manual-grid">
            <label class="prod-field"><span>输入价 ¥ / 百万 token</span><input v-model="manualInput" class="prod-input" inputmode="decimal" placeholder="例如 4.75" /></label>
            <label class="prod-field"><span>输出价 ¥ / 百万 token</span><input v-model="manualOutput" class="prod-input" inputmode="decimal" placeholder="例如 14.26" /></label>
          </div>
          <div class="prod-field-actions">
            <button type="button" class="prod-btn" :disabled="manualSaving" @click="saveManualPrice"><LoaderCircle v-if="manualSaving" :size="13" class="prod-spin" />{{ manualSaving ? '保存中' : '保存手动价目' }}</button>
            <button v-if="manualPrice" type="button" class="prod-btn" :disabled="manualSaving" @click="clearManualPrice">清除手动价目</button>
            <span v-if="manualPrice" class="prod-badge is-gold">当前使用手动价</span>
          </div>
          <p v-if="manualError" class="prod-warning-line is-error">{{ manualError }}</p>
        </div>

        <div v-if="displayPrice" class="prod-price-card">
          <div class="prod-price-head">
            <b>参考价 <span v-if="displayPrice.source === 'manual'" class="prod-badge is-gold">手动</span></b>
            <span>每 100 万 token · 估算</span>
          </div>
          <div class="prod-price-grid">
            <div><span>输入</span><strong>{{ formatCny((displayPrice)!.inputCny) }}</strong><small>{{ formatUsd((displayPrice)!.inputUsd) }}</small></div>
            <div><span>输出</span><strong>{{ formatCny((displayPrice)!.outputCny) }}</strong><small>{{ formatUsd((displayPrice)!.outputUsd) }}</small></div>
            <div><span>缓存读</span><strong>{{ formatCny(displayPrice!.cacheReadPriced ? displayPrice!.cacheReadCny : displayPrice!.inputCny) }}</strong><small>{{ displayPrice!.cacheReadPriced ? '目录价' : '按输入价' }}</small></div>
            <div><span>缓存写</span><strong>{{ formatCny(displayPrice!.cacheWritePriced ? displayPrice!.cacheWriteCny : displayPrice!.inputCny) }}</strong><small>{{ displayPrice!.cacheWritePriced ? '目录价' : '按输入价' }}</small></div>
          </div>
          <p v-if="displayPrice!.note" class="prod-source-line">{{ displayPrice!.note }}</p>
          <p class="prod-source-line">来源 {{ (displayPrice)!.source }} · 更新于 {{ formatDate((displayPrice)!.updatedAt) }} · 汇率 1 USD = {{ (displayPrice)!.exchangeRate }} CNY · 实际账单以服务商为准</p>
        </div>
        <p v-else-if="form.aiModel && !selectedProvider?.local" class="prod-warning-line">目录里没有这个模型的价格，只会记录 token，不会显示假金额。</p>

        <details class="prod-advanced">
          <summary>高级：接口地址、协议与汇率</summary>
          <div class="prod-form-grid">
            <label class="prod-field is-wide"><span>接口地址</span><input v-model="form.aiBaseUrl" class="prod-input" :placeholder="selectedProvider?.baseUrl" /></label>
            <label class="prod-field"><span>协议</span>
              <select v-if="form.aiProvider === 'custom'" v-model="form.aiProtocol" class="prod-select"><option value="openai">OpenAI 兼容</option><option value="anthropic">Anthropic Messages</option><option value="gemini">Gemini</option></select>
              <input v-else class="prod-input" :value="form.aiProtocol" disabled />
            </label>
            <label class="prod-field"><span>美元兑人民币汇率</span><input v-model.number="form.aiExchangeRate" type="number" step="0.01" min="0.01" class="prod-input" /></label>
          </div>
          <p class="prod-wizard-hint">内置服务商的协议固定；自定义服务商才需要手选。汇率只影响估算展示，不参与扣费。</p>
        </details>

        <div class="prod-usage-card">
          <div class="prod-usage-head">
            <div><span class="prod-section-title">本月用量</span><small v-if="usageSummary">{{ usageSummary.month }} · 本地账本</small></div>
            <span class="prod-usage-amount">{{ formatMoney(usageSummary?.spendCny ?? 0) }}<small v-if="usageSummary?.budgetCny"> / {{ formatMoney(usageSummary.budgetCny) }}</small></span>
          </div>
          <div v-if="usageSummary?.budgetCny" class="prod-budget-track"><span :class="{ 'is-over': usageSummary.percent >= 80 }" :style="{ width: Math.min(100, usageSummary.percent) + '%' }"></span></div>
          <p v-if="usageSummary?.budgetCny" class="prod-source-line" :class="{ 'is-warning': usageSummary.percent >= 80 }">
            已用 {{ usageSummary.percent.toFixed(1) }}%{{ usageSummary.percent >= 80 ? '，接近或超过本月预算（只提醒，不拦截）' : '' }}
          </p>
          <div v-if="usageSummary" class="prod-usage-stats">
            <span><b>{{ usageSummary.tokens.toLocaleString() }}</b> tokens</span>
            <span><b>{{ usageSummary.jobs }}</b> 个任务</span>
            <span><b>{{ usageSummary.pricedJobs }}</b> 个有价任务</span>
          </div>
          <div v-if="usageSummary?.byModel.length" class="prod-usage-models">
            <div v-for="row in usageSummary.byModel" :key="row.model"><span>{{ row.model }}</span><span>{{ row.tokens.toLocaleString() }} tokens</span><b>{{ formatMoney(row.spendCny) }}</b></div>
          </div>
          <p v-else class="prod-wizard-hint">本月任务没有记录 token 用量。</p>
          <p class="prod-source-line">金额 = token × 目录参考价，属于估算；余额是账户级，不是单次任务账单。</p>
        </div>

        <div v-if="balance" class="prod-balance-card">
          <div class="prod-usage-head"><div><span class="prod-section-title">{{ balance.providerLabel }} 余额</span><small>{{ formatDate(balance.fetchedAt) }} 更新</small></div></div>
          <div class="prod-balance-entries">
            <div v-for="entry in balance.entries" :key="entry.label"><span>{{ entry.label }}</span><b>{{ entry.amount }} {{ entry.currency }}</b></div>
          </div>
        </div>

        <div class="prod-form-grid" style="margin-top: 12px">
          <label class="prod-field"><span>每月预算（元，0 = 不提醒）</span><input v-model.number="form.monthlyBudgetCny" type="number" step="1" min="0" class="prod-input" /></label>
        </div>
        <div class="prod-inline-actions"><button type="button" class="prod-btn is-primary" :disabled="saving" @click="save"><LoaderCircle v-if="saving" :size="13" class="prod-spin" />保存设置</button></div>
      </section>

      <section v-if="active === 'asr'" class="prod-settings-panel is-active">
        <div class="prod-panel-head"><h2>语音识别</h2><p>B站/本地音频的转写；文本来源不经过这里。</p></div>
        <div class="prod-form-grid">
          <label class="prod-field"><span>引擎</span><select v-model="form.asrEngine" class="prod-select"><option value="mimo">MiMo-V2.5</option><option value="openai-compatible">OpenAI 兼容</option></select></label>
          <label class="prod-field"><span>模型</span><input v-model="form.asrModel" class="prod-input" /></label>
          <label class="prod-field is-wide"><span>接口地址</span><input v-model="form.asrBaseUrl" class="prod-input" /></label>
          <label class="prod-field is-wide"><span>API Key</span><input v-model="form.asrKey" type="password" class="prod-input" :placeholder="production.settings?.hasAsrKey ? '已保存，留空不修改' : 'API Key'" /></label>
          <label class="prod-field"><span>单价（元 / 分钟）</span><input v-model.number="form.asrPricePerMinute" type="number" step="0.0001" class="prod-input" /></label>
        </div>
        <p class="prod-wizard-hint">MiMo 走 <code>chat/completions</code> 传 base64 音频；单次上限约 10MB，长音频会自动转 48kbps MP3。超过上限请缩短音频或改用 OpenAI 兼容转写。</p>
        <div class="prod-inline-actions"><button type="button" class="prod-btn is-primary" :disabled="saving" @click="save">保存设置</button></div>
      </section>

      <section v-if="active === 'search'" class="prod-settings-panel is-active">
        <div class="prod-panel-head"><h2>联网检索</h2><p>仅「速览与溯源」等研究型模板需要。</p></div>
        <div class="prod-form-grid">
          <label class="prod-field"><span>检索渠道</span><select v-model="form.searchProvider" class="prod-select"><option value="zhipu">智谱 BigModel</option><option value="tavily">Tavily</option></select></label>
          <label class="prod-field"><span>每次最多结果</span><input v-model.number="form.searchMaxResults" type="number" min="1" max="10" class="prod-input" /></label>
          <label class="prod-field is-wide"><span>API Key</span><input v-model="form.searchKey" type="password" class="prod-input" :placeholder="production.settings?.hasSearchKey ? '已保存，留空不修改' : 'API Key'" /></label>
        </div>
        <div class="prod-inline-actions"><button type="button" class="prod-btn is-primary" :disabled="saving" @click="save">保存设置</button></div>
      </section>

      <section v-if="active === 'make'" class="prod-settings-panel is-active">
        <div class="prod-panel-head"><h2>制作</h2><p>保守默认值；长任务失败可重试。</p></div>
        <div class="prod-form-grid">
          <label class="prod-field"><span>默认模板</span><select v-model="form.defaultTemplate" class="prod-select"><option v-for="template in templates" :key="template.id" :value="template.id">{{ template.name }}</option></select></label>
          <label class="prod-field"><span>失败重试</span><select v-model.number="form.retries" class="prod-select"><option :value="0">不重试</option><option :value="1">1 次</option><option :value="2">2 次</option><option :value="3">3 次</option></select></label>
          <label class="prod-field"><span>默认提示词</span><select v-model="form.defaultPrompt" class="prod-select"><option v-for="item in production.prompts" :key="item.id" :value="item.name">{{ item.name }}</option></select></label>
          <label class="prod-field"><span>ffmpeg 路径</span><input v-model="form.ffmpegPath" class="prod-input" placeholder="留空使用 PATH 里的 ffmpeg" /></label>
        </div>
        <div class="prod-check-row"><label><input v-model="form.notify" type="checkbox" /> 运行结束发系统通知</label><label><input v-model="form.sound" type="checkbox" /> 播放提示音</label></div>
        <div class="prod-inline-actions"><button type="button" class="prod-btn is-primary" :disabled="saving" @click="save">保存设置</button></div>
      </section>

      <section v-if="active === 'bili'" class="prod-settings-panel is-active">
        <div class="prod-panel-head"><h2>B站账号</h2><p>公开链接不需要登录；收藏夹、稍后再看需要。</p></div>
        <div class="prod-account-card">
          <img
            v-if="production.account?.face"
            class="prod-account-avatar prod-account-image"
            :src="avatarUrl(production.account.face)"
            alt=""
            referrerpolicy="no-referrer"
            loading="lazy"
          />
          <span v-else class="prod-account-avatar">{{ production.account?.name?.slice(0, 1) || '?' }}</span>
          <span class="prod-account-copy">
            <b>{{ production.account?.name || '未登录' }}</b>
            <span>{{ production.account ? `UID ${production.account.mid}` : '公开链接仍可用' }}</span>
          </span>
          <span v-if="production.account" class="prod-badge is-blue" style="margin-left: auto">已连接</span>
        </div>
        <div class="prod-inline-actions">
          <button type="button" class="prod-btn" @click="emit('login')"><KeyRound :size="13" />扫码登录 / 重新登录</button>
          <button v-if="production.account" type="button" class="prod-btn" @click="logout"><LogOut :size="13" />退出登录</button>
        </div>
      </section>

      <section v-if="active === 'prompts'" class="prod-settings-panel is-active">
        <div class="prod-panel-head"><h2>提示词</h2><p>内置风格不可删除；自定义提示词可以新建、编辑和停用。</p></div>
        <div class="prod-inline-actions">
          <button type="button" class="prod-btn" @click="productionUi.view = 'prompts'"><RefreshCw :size="13" />打开提示词库</button>
        </div>
        <p class="prod-wizard-hint" style="margin-top: 9px">共 {{ production.prompts.length }} 个提示词；内置 {{ production.prompts.filter((item) => item.builtin).length }} 个。</p>
      </section>

      <section v-if="active === 'labels'" class="prod-settings-panel is-active">
        <div class="prod-panel-head"><h2>标签与属性</h2><p>页边笔记负责最终 front matter；这里只决定默认抽取什么。</p></div>
        <div class="prod-folder-row">
          <label v-for="field in fieldChoices" :key="field" class="prod-check-chip"><input type="checkbox" :checked="fields.includes(field)" @change="toggleField(field)" />{{ field }}</label>
        </div>
        <p class="prod-wizard-hint">当前为界面默认值；每次新建制作时仍可单独调整。</p>
      </section>

      <section v-if="active === 'data'" class="prod-settings-panel is-active">
        <div class="prod-panel-head"><h2>数据与存储</h2><p>任务、临时音频和产物都留在本机。</p></div>
        <div class="prod-storage-grid">
          <div class="prod-storage-card"><span>任务</span><strong>{{ usage?.jobs ?? '—' }}</strong><small>其中完成 {{ usage?.finished ?? 0 }}</small></div>
          <div class="prod-storage-card"><span>临时音频</span><strong>{{ usage ? (usage.tempBytes / 1024 / 1024).toFixed(1) : '—' }} MB</strong><small>未完成任务的会保留</small></div>
          <div class="prod-storage-card"><span>Outbox</span><strong>{{ usage ? (usage.outboxBytes / 1024 / 1024).toFixed(1) : '—' }} MB</strong><small>入库包</small></div>
          <div class="prod-storage-card"><span>可续跑中间结果</span><strong>{{ usage ? (usage.checkpointBytes / 1024 / 1024).toFixed(1) : '—' }} MB</strong><small>检查点</small></div>
        </div>
        <div class="prod-inline-actions">
          <button type="button" class="prod-btn" @click="clearTemp"><Trash2 :size="13" />清理临时音频</button>
          <button type="button" class="prod-btn" @click="clearFinished"><Trash2 :size="13" />清理已完成任务</button>
          <button type="button" class="prod-btn" @click="confirmClearResumable = true"><Trash2 :size="13" />清理失败/中断任务</button>
        </div>
        <div v-if="confirmClearResumable" class="prod-delete-confirm" style="margin-top: 10px">
          <div><b>清理失败/中断任务？</b><span>会删除这些任务、日志和续跑检查点，之后不能继续制作；已经入库的笔记不会删除。</span></div>
          <div class="prod-confirm-actions">
            <button type="button" class="prod-btn is-small is-danger" @click="clearResumable">确认清理</button>
            <button type="button" class="prod-btn is-small" @click="confirmClearResumable = false">取消</button>
          </div>
        </div>
      </section>

      <section v-if="active === 'notify'" class="prod-settings-panel is-active">
        <div class="prod-panel-head"><h2>通知</h2><p>长任务结束后提醒一次。</p></div>
        <div class="prod-check-row"><label><input v-model="form.notify" type="checkbox" /> 发系统通知</label><label><input v-model="form.sound" type="checkbox" /> 播放提示音</label></div>
        <div class="prod-inline-actions"><button type="button" class="prod-btn is-primary" :disabled="saving" @click="save">保存设置</button></div>
      </section>
    </div>
  </div>
</template>

<style scoped>
.prod-spin { animation: prod-spin 1s linear infinite; }
@keyframes prod-spin { to { transform: rotate(360deg); } }
</style>
