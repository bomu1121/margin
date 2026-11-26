<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, reactive, ref } from 'vue'
import { ExternalLink, LoaderCircle, RefreshCw, Save, Sparkles, X } from 'lucide-vue-next'

import * as api from '../lib/api'
import type { MapContext, PluginInfo, PluginPanel } from '../types'

const props = defineProps<{
  plugin: PluginInfo
  panel: PluginPanel
  params: Record<string, string | number | boolean>
  noteId?: string
}>()

const emit = defineEmits<{ close: [] }>()

const values = reactive<Record<string, string | number | boolean>>({})
for (const [key, value] of Object.entries(props.plugin.settingsValues || {})) {
  if (typeof value === 'string' || typeof value === 'number' || typeof value === 'boolean') {
    values[key] = value
  }
}
for (const [key, value] of Object.entries(props.params)) values[key] = value
for (const field of props.panel.fields) {
  if (!(field.key in values)) values[field.key] = ''
}
const src = ref('')
const loading = ref(true)
const error = ref('')
const emptyReason = ref('')
const localBase = ref('')
const frameKey = ref(0)
const aiBusy = ref(false)
const aiError = ref('')
const aiHint = ref('')
const aiHintTone = ref<'ai' | 'warn' | ''>('')
const saveBusy = ref(false)
const saveError = ref('')
const lastAi = ref<MapContext | null>(null)
const personMode = ref(false)
let loadTimer = 0

function startLoadTimer(): void {
  window.clearTimeout(loadTimer)
  loadTimer = window.setTimeout(() => {
    if (!loading.value) return
    loading.value = false
    error.value = '地图加载超时。可以点右上角「在浏览器打开」，或检查网络后重试。'
  }, 25_000)
}

function frameLoaded(): void {
  window.clearTimeout(loadTimer)
  loading.value = false
}

function frameError(): void {
  if (localBase.value) {
    // The bundled site is missing/unreadable: fall back to the upstream URL.
    localBase.value = ''
    apply()
    return
  }
  loading.value = false
  error.value = '地图页面加载失败，请检查网络或在浏览器打开。'
}

const domain = computed(() => {
  if (localBase.value) return '本地地图'
  try {
    return new URL(src.value || props.panel.url).host
  } catch {
    return 'invalid'
  }
})

const frameStyle = computed(() => ({
  width: `${Math.min(props.panel.width, 1600)}px`,
  height: `${Math.min(props.panel.height, 1200)}px`,
}))

const fallbackMessage = computed(() => {
  if (props.panel.context !== 'historical-map') return ''
  if (currentYear() === null) return '先填年份，或点 AI 定位。'
  if (currentPlace() === '') return '地点为空；地图只按年份显示，可补地点后定位。'
  return ''
})

const hintText = computed(
  () =>
    aiError.value ||
    saveError.value ||
    aiHint.value ||
    fallbackMessage.value ||
    '改年份或地点后按「定位」重新加载',
)

const hintClass = computed(() => ({
  'is-ai': !aiError.value && !saveError.value && aiHintTone.value === 'ai',
  'is-bad': Boolean(aiError.value || saveError.value),
  'is-warn':
    !aiError.value &&
    !saveError.value &&
    (aiHintTone.value === 'warn' || (!aiHint.value && Boolean(fallbackMessage.value))),
}))

function formatTokens(tokens: number): string {
  if (!tokens) return ''
  return tokens >= 1000 ? `${(tokens / 1000).toFixed(1)}k` : String(tokens)
}

function formatDuration(ms: number): string {
  if (!ms) return ''
  return ms >= 1000 ? `${(ms / 1000).toFixed(1)}s` : `${ms}ms`
}

function formatCost(context: MapContext): string {
  if (context.estimatedCost && context.estimatedCost > 0) {
    const currency = context.currency || 'CNY'
    if (currency === 'CNY') return `≈¥${context.estimatedCost.toFixed(4)}`
    if (currency === 'USD') return `≈$${context.estimatedCost.toFixed(4)}`
    return `≈${currency} ${context.estimatedCost.toFixed(4)}`.trim()
  }
  if (!context.providerCost || context.providerCost <= 0) return ''
  const currency = context.providerCurrency || ''
  if (currency === 'USD') return `$${context.providerCost.toFixed(4)}`
  if (currency === 'CNY') return `¥${context.providerCost.toFixed(4)}`
  return `${currency} ${context.providerCost.toFixed(4)}`.trim()
}

function originLabel(origin: string): string {
  return origin === 'ai' ? 'AI' : origin === 'manual' ? '手动' : '笔记'
}

function contextHint(context: MapContext, prefix: string): string {
  const parts = [prefix]
  if (context.confidence > 0) parts.push(`${Math.round(context.confidence * 100)}%`)
  const tokens = context.promptTokens + context.completionTokens
  if (tokens) parts.push(`${formatTokens(tokens)} tok`)
  const duration = formatDuration(context.durationMs)
  if (duration) parts.push(duration)
  const cost = formatCost(context)
  if (cost) parts.push(cost)
  if (context.reason) parts.push(context.reason)
  if (context.stale) parts.push('笔记已变更，建议重跑 AI')
  return parts.join(' · ')
}

function currentYear(): number | null {
  const first = String(values.year ?? '')
    .replace(/，/g, ',')
    .split(',')[0]
    .trim()
  const raw = Number(first)
  return Number.isFinite(raw) && raw !== 0 && raw >= -403 && raw <= 2026 ? raw : null
}

function currentYearText(): string {
  return String(values.year ?? '').trim()
}

function currentPlace(): string {
  return String(values.place ?? '').trim()
}

function onFieldEdit(): void {
  aiError.value = ''
  aiHint.value = ''
  aiHintTone.value = ''
  saveError.value = ''
}

const canSave = computed(() => Boolean(props.noteId) && currentYear() !== null)

const EVENT_KEYS: Record<string, string> = {
  安史之乱: 'anshi',
  靖康之变: 'jingkang',
  靖康之耻: 'jingkang',
  秦统一: 'qintongyi',
  楚汉相争: 'chuhan',
  长征: 'changzheng',
}

function mapEventKey(value: string): string {
  const text = value.trim()
  if (EVENT_KEYS[text]) return EVENT_KEYS[text]
  return ['anshi', 'jingkang', 'qintongyi', 'chuhan', 'changzheng'].includes(text)
    ? text
    : ''
}

function urlValue(value: string | number | boolean | undefined): string {
  if (typeof value === 'boolean') return value ? '1' : '0'
  if (value === undefined || value === null) return ''
  return String(value).replace(/，/g, ',').replace(/；/g, ';')
}

function buildUrl(): string {
  const remote = props.panel.url
  const template = localBase.value
    ? `${localBase.value}?${remote.split('?')[1] || ''}`
    : remote
  const missing: string[] = []
  const filled = template.replace(/\{([a-zA-Z0-9._-]+)\}/g, (_match, key: string) => {
    if (!(key in values)) {
      missing.push(key)
      return ''
    }
    // The upstream `event` parameter is a storyline key (`anshi`), not the
    // note's human-readable event label. Unknown labels are omitted instead of
    // being sent as a broken key.
    if (key === 'event') return encodeURIComponent(mapEventKey(String(values.event ?? '')))
    // Person mode is explicit: sending person by default replaces the place
    // card with a biography (upstream behavior). Default to the note's place.
    if (key === 'person') return personMode.value ? encodeURIComponent(urlValue(values.person)) : ''
    return encodeURIComponent(urlValue(values[key]))
  })
  if (missing.length) throw new Error(`面板缺少参数：${missing.join('、')}`)
  const localAsset = Boolean(localBase.value) && filled.startsWith(localBase.value)
  if (!/^https:\/\//i.test(filled) && !localAsset) {
    throw new Error('插件面板只允许 https 或本地插件资源')
  }
  return filled
}

async function resolveLocalBase(): Promise<void> {
  if (!props.panel.local) return
  try {
    const url = await api.pluginSiteUrl(props.plugin.id, props.panel.local)
    if (url) localBase.value = url
  } catch {
    // The online URL remains the fallback.
  }
}

const STATE_PREFIX = 'yebian.map.panel.'

function readSavedState(): {
  year?: number | string
  place?: string
  person?: string
  event?: string
} | null {
  if (!props.noteId) return null
  try {
    const raw = localStorage.getItem(STATE_PREFIX + props.noteId)
    if (!raw) return null
    const value = JSON.parse(raw) as {
      year?: number | string
      place?: string
      person?: string
      event?: string
    }
    return value && typeof value === 'object' ? value : null
  } catch {
    return null
  }
}

function saveState(): void {
  if (!props.noteId) return
  try {
    localStorage.setItem(
      STATE_PREFIX + props.noteId,
      JSON.stringify({
        year: values.year,
        place: values.place,
        person: values.person,
        event: values.event,
      }),
    )
  } catch {
    // The map still works when localStorage is unavailable.
  }
}

function apply(remember = false): void {
  error.value = ''
  if (props.panel.context === 'historical-map' && currentYear() === null) {
    src.value = ''
    emptyReason.value = '先填年份，地图才能定位；地点可以留空，也可以填古城/关隘。'
    loading.value = false
    return
  }
  emptyReason.value = ''
  try {
    src.value = buildUrl()
    frameKey.value += 1
    loading.value = true
    startLoadTimer()
    if (remember) saveState()
  } catch (cause) {
    loading.value = false
    error.value = String(cause instanceof Error ? cause.message : cause)
  }
}

function reload(): void {
  if (!src.value) {
    apply()
    return
  }
  loading.value = true
  startLoadTimer()
  frameKey.value += 1
}

function openExternal(): void {
  if (!src.value) return
  void api.openExternalUrl(src.value).catch((cause) => {
    error.value = api.errorText(cause)
  })
}

async function aiLocate(): Promise<void> {
  if (!props.noteId || aiBusy.value) return
  aiBusy.value = true
  aiError.value = ''
  aiHint.value = ''
  aiHintTone.value = ''
  try {
    const context = await api.mapContext(props.noteId)
    lastAi.value = context
    values.year = context.yearText || (context.year !== null ? context.year : '')
    values.place = context.place || ''
    values.person = context.person || ''
    values.event = context.event || ''
    if (context.year || context.place) {
      apply()
      if (context.year) {
        aiHint.value = contextHint(context, context.cached ? 'AI · 缓存' : 'AI')
        aiHintTone.value = 'ai'
      } else {
        aiHint.value = `${contextHint(context, 'AI 只找到地点')} · 请补年份`
        aiHintTone.value = 'warn'
      }
    } else {
      aiHint.value = 'AI 没有找到可靠年份或地点，请手动填写'
      aiHintTone.value = 'warn'
    }
  } catch (cause) {
    aiError.value = api.errorText(cause)
  } finally {
    aiBusy.value = false
  }
}

async function saveToNote(): Promise<void> {
  if (!props.noteId || saveBusy.value) return
  const year = currentYear()
  const yearText = currentYearText()
  const place = currentPlace()
  if (year === null) {
    saveError.value = '先填年份，再存入笔记'
    return
  }
  const ai = lastAi.value
  const aiYearText = ai?.yearText || (ai?.year != null ? String(ai.year) : '')
  const fromAi = Boolean(
    ai && aiYearText === yearText && (ai.place || '') === place,
  )
  saveBusy.value = true
  saveError.value = ''
  try {
    const saved = await api.mapContextSave(props.noteId, {
      year,
      yearText,
      place,
      person: String(values.person ?? '').trim(),
      event: String(values.event ?? '').trim(),
      source: fromAi ? 'ai' : 'manual',
      confidence: fromAi ? ai?.confidence ?? 0 : 1,
      reason: fromAi ? ai?.reason || '' : '',
    })
    lastAi.value = null
    aiHint.value = contextHint(saved, `已存入笔记 · ${originLabel(saved.origin)}`)
    aiHintTone.value = 'ai'
    apply(true)
  } catch (cause) {
    saveError.value = api.errorText(cause)
  } finally {
    saveBusy.value = false
  }
}

async function restoreContext(): Promise<void> {
  if (!props.noteId || props.panel.context !== 'historical-map') return
  try {
    const context = await api.mapContext(props.noteId, true)
    if (!context.year && !context.place) return
    if (context.source === 'note') {
      values.year = context.yearText || context.year || ''
      values.place = context.place
      values.person = context.person || ''
      values.event = context.event || ''
      aiHint.value = contextHint(context, `笔记 · ${originLabel(context.origin)}`)
      aiHintTone.value = context.stale ? 'warn' : context.origin === 'manual' ? '' : 'ai'
      apply()
      return
    }
    // AI cache fills missing rule fields but never overwrites a rule year/place
    // or injects a person/event into the URL: the rule may be more specific
    // (e.g. 1542/北京) and person mode would replace the place card.
    let filled = false
    if (currentYear() === null && (context.year !== null || context.place)) {
      values.year = context.yearText || context.year || ''
      filled = true
    }
    if (!currentPlace() && context.place) {
      values.place = context.place
      filled = true
    }
    if (!filled) return
    aiHint.value = contextHint(context, 'AI · 缓存补全')
    aiHintTone.value = 'ai'
    apply()
    return
  } catch {
    // No saved note context or cached AI result: fall through to local state.
  }
  const saved = readSavedState()
  if (saved && (saved.year !== undefined || saved.place)) {
    if (saved.year !== undefined) values.year = saved.year
    if (saved.place !== undefined) values.place = saved.place
    if (saved.person !== undefined) values.person = saved.person
    if (saved.event !== undefined) values.event = saved.event
    aiHint.value = '上次定位（本机）'
    aiHintTone.value = ''
    apply()
  }
}

function onKeydown(event: KeyboardEvent): void {
  if (event.key === 'Escape') emit('close')
}

onMounted(() => {
  void (async () => {
    await resolveLocalBase()
    await restoreContext()
    if (!src.value) apply()
  })()
  window.addEventListener('keydown', onKeydown)
})

onBeforeUnmount(() => {
  window.clearTimeout(loadTimer)
  window.removeEventListener('keydown', onKeydown)
})
</script>

<template>
  <Teleport to="body">
    <div class="plugin-panel-overlay" @pointerdown.self="emit('close')">
      <section
        class="plugin-panel"
        role="dialog"
        aria-modal="true"
        :aria-label="panel.title"
        :style="frameStyle"
      >
        <header class="plugin-panel-head">
          <div class="plugin-panel-title">
            <span class="plugin-panel-kicker">{{ plugin.name }}</span>
            <div class="plugin-panel-heading">
              <h2>{{ panel.title }}</h2>
              <span class="plugin-panel-domain">{{ domain }}</span>
              <span class="plugin-panel-isolation">跨域沙箱</span>
            </div>
          </div>
          <div class="plugin-panel-actions">
            <button type="button" class="plugin-panel-action" title="重新加载" @click="reload">
              <RefreshCw :size="14" />
            </button>
            <button type="button" class="plugin-panel-action" title="在浏览器打开" @click="openExternal">
              <ExternalLink :size="14" />
            </button>
            <button type="button" class="plugin-panel-action is-close" title="关闭" @click="emit('close')">
              <X :size="15" />
            </button>
          </div>
        </header>

        <div v-if="panel.fields.length" class="plugin-panel-fields">
          <label v-for="field in panel.fields" :key="field.key" class="plugin-panel-field">
            <span>{{ field.label }}</span>
            <input
              v-model="values[field.key]"
              :type="field.type === 'number' ? 'number' : 'text'"
              :placeholder="field.placeholder || ''"
              spellcheck="false"
              @input="onFieldEdit"
              @keydown.enter.prevent="apply(true)"
            />
          </label>
          <label
            v-if="panel.context === 'historical-map'"
            class="plugin-panel-person-mode"
            title="勾选后把人物作为地图主视角；默认优先年份/地点"
          >
            <input v-model="personMode" type="checkbox" @change="apply()" />
            <span>人物轨迹</span>
          </label>
          <button type="button" class="prod-btn is-primary plugin-panel-go" @click="apply(true)">
            定位
          </button>
          <button
            v-if="panel.context === 'historical-map' && noteId"
            type="button"
            class="prod-btn plugin-panel-ai"
            :disabled="aiBusy"
            title="用当前模型从这篇笔记抽取年份和地点"
            @click="aiLocate"
          >
            <LoaderCircle v-if="aiBusy" :size="12" class="plugin-spin" />
            <Sparkles v-else :size="12" /> AI 定位
          </button>
          <button
            v-if="panel.context === 'historical-map' && noteId"
            type="button"
            class="prod-btn plugin-panel-save"
            :disabled="saveBusy || aiBusy || !canSave"
            title="把当前年份/地点写进笔记 front matter，随笔记同步"
            @click="saveToNote"
          >
            <LoaderCircle v-if="saveBusy" :size="12" class="plugin-spin" />
            <Save v-else :size="12" /> 存入笔记
          </button>
          <span
            class="plugin-panel-hint"
            :class="hintClass"
            :title="hintText"
          >
            {{ hintText }}
          </span>
        </div>

        <div class="plugin-panel-frame">
          <div v-if="emptyReason" class="plugin-panel-empty">
            <b>还没有可定位的年份</b>
            <span>{{ emptyReason }}</span>
          </div>
          <iframe
            v-if="src"
            :key="frameKey"
            :src="src"
            class="plugin-panel-iframe"
            sandbox="allow-scripts allow-same-origin allow-popups allow-forms"
            referrerpolicy="no-referrer"
            @load="frameLoaded"
            @error="frameError"
          />
          <div v-if="loading" class="plugin-panel-loading">
            <LoaderCircle :size="18" class="plugin-spin" />
            <span>正在加载地图…首次打开可能需要几秒</span>
          </div>
          <div v-if="error" class="plugin-panel-error">
            <b>面板没有加载</b>
            <span>{{ error }}</span>
            <button type="button" class="prod-btn" @click="apply()">重试</button>
          </div>
        </div>
      </section>
    </div>
  </Teleport>
</template>
