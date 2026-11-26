// Dedicated Jiangshan atlas panel: one map point at a time and a note-local
// place list on the left. The generic PluginPanelHost keeps its field bar for
// other plugins; this view removes that control surface entirely.
<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { ExternalLink, LoaderCircle, RefreshCw, SlidersHorizontal, X } from 'lucide-vue-next'

import * as api from '../lib/api'
import { extractMapPoints, type MapPoint } from '../lib/mapText'
import { mapLinks, setMapLinks, setMapLinksEnabled } from '../stores/mapLinks'
import type { PluginInfo, PluginPanel } from '../types'

const props = defineProps<{
  plugin: PluginInfo
  panel: PluginPanel
  noteId?: string
  text?: string
  initial?: Record<string, string | number | boolean>
}>()

const emit = defineEmits<{ close: [] }>()

const points = ref<MapPoint[]>([])
const selectedId = ref('')
const localBase = ref('')
const src = ref('')
const loading = ref(false)
const error = ref('')
const frameKey = ref(0)
const inlineEnabled = computed({
  get: () => mapLinks.enabled,
  set: (value: boolean) => setMapLinksEnabled(value),
})

type LayerKey = 'timeline' | 'card' | 'legend' | 'badge'
const LAYERS_KEY = 'yebian.atlas.layers.v1'
const layerItems: Array<{ key: LayerKey; label: string }> = [
  { key: 'timeline', label: '朝代时间轴' },
  { key: 'card', label: '地点资料卡' },
  { key: 'legend', label: '图层图例' },
  { key: 'badge', label: '年份徽标' },
]
const layers = ref<Record<LayerKey, boolean>>(readLayers())
const layersOpen = ref(false)
const layersEl = ref<HTMLElement | null>(null)
const frameEl = ref<HTMLIFrameElement | null>(null)

function readLayers(): Record<LayerKey, boolean> {
  const value: Record<LayerKey, boolean> = {
    // 用户选的默认组合：时间轴和资料卡收起，只留图例和年份徽标。
    timeline: false,
    card: false,
    legend: true,
    badge: true,
  }
  try {
    const raw = localStorage.getItem(LAYERS_KEY)
    const saved = raw ? JSON.parse(raw) : null
    if (saved && typeof saved === 'object') {
      for (const key of Object.keys(value) as LayerKey[]) {
        if (typeof saved[key] === 'boolean') value[key] = saved[key]
      }
    }
  } catch {
    // localStorage 不可用时保持默认。
  }
  return value
}

function postLayers(): void {
  const frame = frameEl.value
  if (!frame?.contentWindow || !src.value) return
  try {
    frame.contentWindow.postMessage(
      { type: 'yebian:atlas-ui', state: { ...layers.value } },
      new URL(src.value).origin,
    )
  } catch {
    // 远端回退站点没有这个监听，开关保持无效即可。
  }
}

function onDocPointerDown(event: PointerEvent): void {
  if (!layersOpen.value) return
  const target = event.target
  if (target instanceof Node && layersEl.value?.contains(target)) return
  layersOpen.value = false
}

watch(
  layers,
  () => {
    try {
      localStorage.setItem(LAYERS_KEY, JSON.stringify(layers.value))
    } catch {
      // 忽略隐私模式/配额失败。
    }
    postLayers()
  },
  { deep: true },
)
let loadTimer = 0

const selected = computed(() => points.value.find((point) => point.id === selectedId.value) || null)
// 只匹配到年份时，清单里只有一个“全图”入口；此时说“1 处地点”会误导。
const yearOnly = computed(() => points.value.length === 1 && !points.value[0].place)

const frameStyle = computed(() => ({
  width: `${Math.min(props.panel.width, 1600)}px`,
  height: `${Math.min(props.panel.height, 1200)}px`,
}))

function pointYear(point: MapPoint): string {
  if (!point.year) return '年份未定'
  const year = Number(point.year)
  const label = year < 0 ? `前${-year}` : String(year)
  return `${point.approximate ? '约' : ''}${label}年`
}

function pointDetail(point: MapPoint): string {
  const bits = [pointYear(point)]
  if (point.place && point.place !== point.label) bits.push(point.place)
  return bits.join(' · ')
}

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
  postLayers()
}

function frameError(): void {
  if (localBase.value) {
    localBase.value = ''
    apply()
    return
  }
  loading.value = false
  error.value = '地图页面加载失败，请检查网络或在浏览器打开。'
}

async function resolveLocalBase(): Promise<void> {
  if (!props.panel.local) return
  try {
    const url = await api.pluginSiteUrl(props.plugin.id, props.panel.local)
    if (url) localBase.value = url
  } catch {
    // Keep the upstream URL.
  }
}

function addInitialPoint(list: MapPoint[]): MapPoint[] {
  const initial = props.initial || {}
  const place = String(initial.place || '').trim()
  const year = String(initial.year || '').trim()
  const event = String(initial.event || '').trim()
  // 从“三国”这类时期标签推出来的年份是代表年，不是文章里的确切年份。
  const approximate = String(initial.yearSource || '') === 'period'
  if (!place && !year) return list
  if (list.some((point) => point.place === place && point.year === year)) return list
  // 只有年份时不要伪造一个“初始地点”：那会让人以为识别到了具体位置。
  // 有正文地点时不必再插一条全图；没有时就给一个诚实的“全图”入口。
  if (!place && !event) {
    if (list.length) return list
    return [
      {
        id: 'initial-year',
        label: '全图',
        place: '',
        year,
        event: '',
        detail: '',
        approximate,
      },
      ...list,
    ]
  }
  return [
    {
      id: 'initial',
      label: place || (event ? '事件' : '全图'),
      place,
      year,
      event,
      detail: year ? `${year}年` : '标题入口',
      approximate,
    },
    ...list,
  ]
}

async function loadPoints(): Promise<void> {
  if (props.noteId) {
    let saved: { year?: string; place?: string; event?: string } | undefined
    try {
      const context = await api.mapContext(props.noteId, true)
      if (context.place || context.year) {
        saved = {
          year: context.yearText || (context.year !== null ? String(context.year) : ''),
          place: context.place,
          event: context.event,
        }
      }
    } catch {
      // No saved/cached context.
    }
    const detail = await api.getNote(props.noteId)
    if (detail) {
      const text = [
        detail.meta.title,
        ...(detail.meta.events || []),
        ...(detail.meta.periods || []),
        detail.body,
      ].join('\n')
      points.value = addInitialPoint(extractMapPoints(text, saved))
      setMapLinks(props.noteId, points.value)
    }
    const preferred = saved?.place
      ? points.value.find(
          (point) => point.place === saved?.place && point.year === String(saved?.year || ''),
        )
      : null
    const initial = props.initial || {}
    const initialPoint = initial.place
      ? points.value.find(
          (point) =>
            point.place === String(initial.place) && point.year === String(initial.year || ''),
        )
      : null
    // An explicit initial point (inline token/search row) wins over the note's
    // saved/default context; otherwise the saved point is the best default.
    selectedId.value = (initialPoint || preferred || points.value[0])?.id || ''
    return
  }

  points.value = addInitialPoint(extractMapPoints(props.text || ''))
  selectedId.value = points.value[0]?.id || ''
}

function buildUrl(): string {
  const point = selected.value
  if (!point || !point.year) throw new Error('这个地点还没有年份')
  const remote = props.panel.url
  const template = localBase.value
    ? `${localBase.value}?${remote.split('?')[1] || ''}`
    : remote
  const values: Record<string, string> = {
    year: point.year,
    place: point.place,
    person: '',
    event: point.event,
    lang: String(props.plugin.settingsValues?.lang ?? 'zh'),
    lock: props.plugin.settingsValues?.lock ? '1' : '0',
  }
  const missing: string[] = []
  const filled = template.replace(/\{([a-zA-Z0-9._-]+)\}/g, (_match, key: string) => {
    if (!(key in values)) {
      missing.push(key)
      return ''
    }
    return encodeURIComponent(values[key])
  })
  if (missing.length) throw new Error(`面板缺少参数：${missing.join('、')}`)
  const local = localBase.value && filled.startsWith(localBase.value)
  if (!/^https:\/\//i.test(filled) && !local) throw new Error('插件面板地址不合法')
  return filled
}

function apply(): void {
  error.value = ''
  const point = selected.value
  if (!point || !point.year) {
    src.value = ''
    loading.value = false
    return
  }
  try {
    src.value = buildUrl()
    frameKey.value += 1
    loading.value = true
    startLoadTimer()
  } catch (cause) {
    loading.value = false
    error.value = cause instanceof Error ? cause.message : String(cause)
  }
}

function select(point: MapPoint): void {
  if (!point.year) return
  selectedId.value = point.id
  apply()
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

function onKeydown(event: KeyboardEvent): void {
  if (event.key === 'Escape') emit('close')
}

onMounted(async () => {
  await resolveLocalBase()
  await loadPoints()
  apply()
  window.addEventListener('keydown', onKeydown)
  document.addEventListener('pointerdown', onDocPointerDown, true)
})

onBeforeUnmount(() => {
  window.clearTimeout(loadTimer)
  window.removeEventListener('keydown', onKeydown)
  document.removeEventListener('pointerdown', onDocPointerDown, true)
})
</script>

<template>
  <Teleport to="body">
    <div class="plugin-panel-overlay" @pointerdown.self="emit('close')">
      <section
        class="plugin-panel map-panel"
        role="dialog"
        aria-modal="true"
        :aria-label="panel.title"
        :style="frameStyle"
      >
        <header class="plugin-panel-head">
          <div class="plugin-panel-title">
            <div class="plugin-panel-heading">
              <h2>{{ panel.title }}</h2>
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

        <div class="map-panel-body">
          <aside class="map-panel-rail">
            <div class="map-panel-rail-head">
              <span>本篇地图</span>
              <b>{{ yearOnly ? '仅年份' : `${points.length} 处` }}</b>
            </div>
            <div class="map-panel-points">
              <button
                v-for="point in points"
                :key="point.id"
                type="button"
                class="map-panel-point"
                :class="{ 'is-active': point.id === selectedId, 'is-disabled': !point.year }"
                @click="select(point)"
              >
                <b>{{ point.label }}</b>
                <span>{{ pointDetail(point) }}</span>
              </button>
              <p v-if="!points.length" class="map-panel-rail-empty">没有识别到地点或事件。</p>
            </div>
            <div class="map-panel-layers" ref="layersEl">
              <button
                type="button"
                class="map-panel-layers-trigger"
                :aria-expanded="layersOpen"
                title="地图显示项"
                @click.stop="layersOpen = !layersOpen"
              >
                <SlidersHorizontal :size="12" />
                <span>显示</span>
              </button>
              <div v-if="layersOpen" class="map-panel-layers-menu" @pointerdown.stop>
                <label v-for="item in layerItems" :key="item.key">
                  <input v-model="layers[item.key]" type="checkbox" :disabled="!localBase" />
                  <span>{{ item.label }}</span>
                </label>
                <p v-if="!localBase" class="map-panel-layers-note">远端地图不支持这些开关</p>
              </div>
            </div>
            <label v-if="noteId" class="map-panel-inline-toggle">
              <input v-model="inlineEnabled" type="checkbox" />
              <span>正文地名可点</span>
            </label>
          </aside>

          <div class="map-panel-frame">
            <div v-if="!points.length" class="plugin-panel-empty">
              <b>没有可定位的地点</b>
              <span>这篇笔记里没有识别到地名或事件；可以先在正文里补上地点，或使用回填工具。</span>
            </div>
            <div v-else-if="!selected?.year" class="plugin-panel-empty">
              <b>这个地点缺少年份</b>
              <span>上方列表里还有地点没有年份，先补年份后再定位。</span>
            </div>
            <iframe
              v-if="src"
              ref="frameEl"
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
              <button type="button" class="prod-btn" @click="apply">重试</button>
            </div>
          </div>
        </div>
      </section>
    </div>
  </Teleport>
</template>
