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
import type { ProductionDataUsage, ProductionSaveSettings } from '../../types'

const emit = defineEmits<{ login: [] }>()

const active = ref('ai')
const saving = ref(false)
const notice = ref('')
const error = ref('')
const usage = ref<ProductionDataUsage | null>(null)
const fields = ref<string[]>(['人物', '事件'])

const form = reactive({
  aiProvider: 'deepseek',
  aiBaseUrl: '',
  aiModel: '',
  aiKey: '',
  aiInputPrice: 0,
  aiOutputPrice: 0,
  aiCurrency: 'CNY',
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

function fill(): void {
  const settings = production.settings
  if (!settings) return
  form.aiProvider = settings.aiProvider
  form.aiBaseUrl = settings.aiBaseUrl
  form.aiModel = settings.aiModel
  form.aiInputPrice = settings.aiInputPrice
  form.aiOutputPrice = settings.aiOutputPrice
  form.aiCurrency = settings.aiCurrency
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
    aiBaseUrl: form.aiBaseUrl,
    aiModel: form.aiModel,
    aiInputPrice: form.aiInputPrice,
    aiOutputPrice: form.aiOutputPrice,
    aiCurrency: form.aiCurrency,
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
    window.setTimeout(() => { notice.value = '' }, 2200)
  } catch (cause) {
    error.value = api.errorText(cause)
  } finally {
    saving.value = false
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

onMounted(() => {
  if (!production.loaded) void refreshSettings()
  void loadUsage()
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
        <div class="prod-panel-head"><h2>AI 模型</h2><p>用于校对、加工、属性抽取和标签判定。</p></div>
        <div class="prod-form-grid">
          <label class="prod-field"><span>提供商</span>
            <select v-model="form.aiProvider" class="prod-select"><option value="deepseek">DeepSeek</option><option value="openai">OpenAI</option><option value="custom">自定义</option></select>
          </label>
          <label class="prod-field"><span>模型</span><input v-model="form.aiModel" class="prod-input" placeholder="deepseek-chat" /></label>
          <label class="prod-field is-wide"><span>接口地址</span><input v-model="form.aiBaseUrl" class="prod-input" placeholder="https://api.deepseek.com/v1" /></label>
          <label class="prod-field is-wide"><span>API Key</span><input v-model="form.aiKey" type="password" class="prod-input" :placeholder="production.settings?.hasAiKey ? '已保存，留空不修改' : 'sk-…'" /></label>
          <label class="prod-field"><span>输入单价（每百万 token）</span><input v-model.number="form.aiInputPrice" type="number" step="0.0001" class="prod-input" /></label>
          <label class="prod-field"><span>输出单价（每百万 token）</span><input v-model.number="form.aiOutputPrice" type="number" step="0.0001" class="prod-input" /></label>
          <label class="prod-field"><span>币种</span><select v-model="form.aiCurrency" class="prod-select"><option value="CNY">人民币</option><option value="USD">美元</option></select></label>
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
