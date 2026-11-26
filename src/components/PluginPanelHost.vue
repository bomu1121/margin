<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, reactive, ref } from 'vue'
import { ExternalLink, LoaderCircle, RefreshCw, Sparkles, X } from 'lucide-vue-next'

import * as api from '../lib/api'
import type { PluginInfo, PluginPanel } from '../types'

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
const frameKey = ref(0)
const aiBusy = ref(false)
const aiError = ref('')
const aiHint = ref('')
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

const domain = computed(() => {
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

function urlValue(value: string | number | boolean | undefined): string {
  if (typeof value === 'boolean') return value ? '1' : '0'
  return value === undefined || value === null ? '' : String(value)
}

function buildUrl(): string {
  const template = props.panel.url
  const missing: string[] = []
  const filled = template.replace(/\{([a-zA-Z0-9._-]+)\}/g, (_match, key: string) => {
    if (!(key in values)) {
      missing.push(key)
      return ''
    }
    return encodeURIComponent(urlValue(values[key]))
  })
  if (missing.length) throw new Error(`面板缺少参数：${missing.join('、')}`)
  if (!/^https:\/\//i.test(filled)) throw new Error('插件面板只允许 https 地址')
  return filled
}

const STATE_PREFIX = 'yebian.map.panel.'

function readSavedState(): { year?: number | string; place?: string } | null {
  if (!props.noteId) return null
  try {
    const raw = localStorage.getItem(STATE_PREFIX + props.noteId)
    if (!raw) return null
    const value = JSON.parse(raw) as { year?: number | string; place?: string }
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
      JSON.stringify({ year: values.year, place: values.place }),
    )
  } catch {
    // The map still works when localStorage is unavailable.
  }
}

function apply(remember = false): void {
  error.value = ''
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
  try {
    const context = await api.mapContext(props.noteId)
    if (context.year) values.year = context.year
    if (context.place) values.place = context.place
    if (context.year || context.place) {
      apply(true)
      const source = context.cached ? 'AI · 缓存' : 'AI'
      const percent = Math.round(context.confidence * 100)
      aiHint.value = `${source} · ${percent}%${context.reason ? ' · ' + context.reason : ''}`
    } else {
      aiHint.value = 'AI 没有找到可靠年份或地点，请手动填写'
    }
  } catch (cause) {
    aiError.value = api.errorText(cause)
  } finally {
    aiBusy.value = false
  }
}

async function restoreContext(): Promise<void> {
  if (!props.noteId || props.panel.context !== 'historical-map') return
  const saved = readSavedState()
  if (saved && (saved.year !== undefined || saved.place)) {
    if (saved.year !== undefined) values.year = saved.year
    if (saved.place !== undefined) values.place = saved.place
    aiHint.value = '上次定位'
    apply()
    return
  }
  try {
    const context = await api.mapContext(props.noteId, true)
    if (!context.year && !context.place) return
    if (context.year) values.year = context.year
    if (context.place) values.place = context.place
    const percent = Math.round(context.confidence * 100)
    aiHint.value = `AI · 缓存 · ${percent}%`
    apply()
  } catch {
    // No cached AI context: keep the rules' year/place.
  }
}

function onKeydown(event: KeyboardEvent): void {
  if (event.key === 'Escape') emit('close')
}

onMounted(() => {
  apply()
  void restoreContext()
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
              @keydown.enter.prevent="apply(true)"
            />
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
          <span
            class="plugin-panel-hint"
            :class="{ 'is-ai': aiHint, 'is-bad': aiError }"
            :title="aiError || aiHint || '改年份或地点后按「定位」重新加载'"
          >
            {{ aiError || aiHint || '改年份或地点后按「定位」重新加载' }}
          </span>
        </div>

        <div class="plugin-panel-frame">
          <iframe
            v-if="src"
            :key="frameKey"
            :src="src"
            class="plugin-panel-iframe"
            sandbox="allow-scripts allow-same-origin allow-popups allow-forms"
            referrerpolicy="no-referrer"
            @load="frameLoaded"
            @error="error = '地图页面加载失败，请检查网络或在浏览器打开。'; loading = false"
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
