<script setup lang="ts">
import { computed, onMounted, reactive, ref, watch } from 'vue'
import { KeyRound, LoaderCircle, LogOut, RefreshCw, Trash2 } from 'lucide-vue-next'

import * as api from '../../lib/api'
import {
  production,
  productionUi,
  refreshJobs,
  refreshSettings,
} from '../../stores/production'
import type { BalanceResult, ModelOption, PriceQuote, ProductionDataUsage, ProductionSaveSettings, ProviderPreset, UsageSummary } from '../../types'

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
const priceQuote = ref<PriceQuote | null>(null)
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
const displayPrice = computed(() => selectedModel.value?.price ?? priceQuote.value)
const filteredModels = computed(() => {
  const query = modelSearch.value.trim().toLowerCase()
  if (!query) return models.value.slice(0, 80)
  return models.value.filter((model) => model.id.toLowerCase().includes(query) || model.name.toLowerCase().includes(query)).slice(0, 80)
})

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
}

async function loadPrice(): Promise<void> {
  const model = form.aiModel.trim()
  const seq = ++priceSeq
  if (!model) {
    priceQuote.value = null
    return
  }
  const known = models.value.find((item) => item.id === model)
  if (known) {
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
    const bytes = await api.productionClearTemp()
    notice.value = `已清理 ${(bytes / 1024 / 1024).toFixed(1)} MB 临时音频`
    await loadUsage()
  } catch (cause) {
    error.value = api.errorText(cause)
  }
}

async function clearFinished(): Promise<void> {
  try {
    const count = await api.productionClearFinished()
    notice.value = `已清理 ${count} 条已完成/失败任务`
    await refreshJobs()
    await loadUsage()
  } catch (cause) {
    error.value = api.errorText(cause)
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
        <div class="prod-panel-head"><h2>AI 模型</h2><p>选服务商、填 Key、拉取模型。价格自动按目录价折算成人民币，不需要手填单价。</p></div>

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
              <LoaderCircle v-if="modelLoading" :size="13" class="prod-spin" /><RefreshCw v-else :size="13" />{{ modelLoading ? '正在连接' : '获取模型 / 测试连接' }}
            </button>
            <button v-if="selectedProvider?.supportsBalance" type="button" class="prod-btn" :disabled="balanceLoading" @click="queryBalance">
              <LoaderCircle v-if="balanceLoading" :size="13" class="prod-spin" /><RefreshCw v-else :size="13" />查询余额
            </button>
            <button type="button" class="prod-btn is-primary" :disabled="saving" @click="save"><LoaderCircle v-if="saving" :size="13" class="prod-spin" />保存设置</button>
            <a v-if="selectedProvider?.docsUrl" class="prod-link" href="#" @click.prevent="api.openExternalUrl(selectedProvider.docsUrl)">获取 Key / 文档</a>
          </div>
          <p v-if="aiError" class="prod-warning-line is-error">{{ aiError }}</p>
        </div>

        <div class="prod-model-section">
          <div class="prod-section-line">
            <span class="prod-section-title">模型</span>
            <span class="prod-section-hint">{{ models.length ? `${models.length} 个可用` : '先获取模型，或直接填写模型 ID' }}</span>
          </div>
          <div class="prod-model-current">
            <input v-model="form.aiModel" class="prod-input" placeholder="模型 ID，例如 deepseek-chat" @change="loadPrice" />
            <span v-if="displayPrice" class="prod-model-current-price">{{ formatCny(displayPrice.inputCny) }} / {{ formatCny(displayPrice.outputCny) }}</span>
            <span v-else-if="selectedProvider?.local" class="prod-model-current-price is-muted">本地不计费</span>
            <span v-else class="prod-model-current-price is-muted">待查价</span>
          </div>
          <div v-if="models.length" class="prod-model-filter">
            <input v-model="modelSearch" class="prod-input" placeholder="搜索模型 ID 或名称…" />
          </div>
          <div v-if="filteredModels.length" class="prod-model-list">
            <button v-for="model in filteredModels" :key="model.id" type="button" class="prod-model-row" :class="{ 'is-selected': model.id === form.aiModel }" @click="chooseModel(model)">
              <span class="prod-model-name"><b>{{ model.id }}</b><small v-if="model.name && model.name !== model.id">{{ model.name }}</small></span>
              <span class="prod-model-meta">
                <span v-if="model.contextLength">{{ formatContext(model.contextLength) }} 上下文</span>
                <span v-if="model.price" class="prod-model-price">{{ formatCny(model.price.inputCny) }} / {{ formatCny(model.price.outputCny) }}</span>
                <span v-else class="prod-model-price is-muted">未收录价格</span>
              </span>
            </button>
          </div>
          <p v-else-if="!modelLoading && modelSearch" class="prod-wizard-hint">没有匹配「{{ modelSearch }}」的模型。</p>
          <p v-if="modelSource" class="prod-source-line">模型列表：{{ modelSource === 'provider' ? '服务商返回' : modelSource === 'catalog' ? '内置目录' : modelSource }} · 价格：models.dev 目录（美元折算，估算值）</p>
          <p v-if="modelWarning" class="prod-warning-line">{{ modelWarning }}</p>
        </div>

        <div v-if="displayPrice" class="prod-price-card">
          <div class="prod-price-head">
            <b>参考价</b>
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
          <div class="prod-storage-card"><span>临时音频</span><strong>{{ usage ? (usage.tempBytes / 1024 / 1024).toFixed(1) : '—' }} MB</strong><small>可清理</small></div>
          <div class="prod-storage-card"><span>Outbox</span><strong>{{ usage ? (usage.outboxBytes / 1024 / 1024).toFixed(1) : '—' }} MB</strong><small>入库包</small></div>
        </div>
        <div class="prod-inline-actions">
          <button type="button" class="prod-btn" @click="clearTemp"><Trash2 :size="13" />清理临时音频</button>
          <button type="button" class="prod-btn" @click="clearFinished"><Trash2 :size="13" />清理已完成任务</button>
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
