<script setup lang="ts">
import { computed, onMounted, onUnmounted, reactive, ref, watch } from 'vue'
import { ArrowUpRight, Check, Images, LoaderCircle, RotateCcw, ScanSearch, Search, X } from 'lucide-vue-next'

import * as api from '../../lib/api'

const props = defineProps<{
  visible: boolean
  item: api.WhiteboardItem | null
  mode: 'source' | 'similar'
  assetUrl: string
}>()

const emit = defineEmits<{
  close: []
}>()

const loading = ref(false)
const error = ref('')
const result = ref<api.ImageInvestigation | null>(null)
const progress = reactive<Record<string, api.InvestigationProgress>>({})
let run = 0
let requestId = ''
let unlisten: (() => void) | null = null

const steps = [
  { id: 'vision', label: '读图' },
  { id: 'reverse', label: '反查图片' },
  { id: 'search', label: '交叉检索' },
  { id: 'synthesis', label: '汇总证据' },
] as const

const title = computed(() => {
  if (loading.value) return props.mode === 'similar' ? '正在找相似的图' : '正在查这张图的来源'
  if (error.value) return '图片调查失败'
  if (result.value) return props.mode === 'similar' ? '相似图片' : '来源线索'
  return props.mode === 'similar' ? '找相似的图' : '查这张图的来源'
})

const currentDetail = computed(() => {
  if (error.value) return error.value
  const running = steps
    .map((step) => progress[step.id])
    .find((entry) => entry?.status === 'running')
  if (running?.detail) return running.detail
  if (result.value) {
    const similar =
      props.mode === 'source' && result.value.verdict === 'not_found'
        ? 0
        : result.value.similar.length
    const similarText = similar ? ` · ${similar} 张相似图` : ''
    return `完成 · ${result.value.sources.length} 个来源候选${similarText}`
  }
  return '把图片交给反查服务和视觉模型，通常需要 20–60 秒'
})

const verdictLabel = computed(() => {
  switch (result.value?.verdict) {
    case 'found':
      return '找到可靠来源'
    case 'likely':
      return '找到来源线索'
    default:
      return '没有找到可靠来源'
  }
})

const displayLabel = computed(() => {
  if (loading.value) return '正在调查…'
  if (error.value) return '调查失败'
  if (props.mode === 'similar') {
    return result.value?.similar.length ? `找到 ${result.value.similar.length} 张相似图` : '没有找到相似图'
  }
  return verdictLabel.value
})

const displayDetail = computed(() => {
  if (props.mode === 'similar' && result.value) {
    return '下面是 360 识图 / trace.moe 返回的相似图片；点开可以核对来源页。'
  }
  return currentDetail.value
})

const providerLine = computed(() =>
  (result.value?.providers || [])
    .map((provider) => `${provider.label} ${provider.count}`)
    .join(' · '),
)

function confidenceLabel(value: string): string {
  if (value === 'high') return '较可信'
  if (value === 'low') return '仅线索'
  return '待核对'
}

function kindLabel(value: string): string {
  if (value === 'anime') return '动画场景'
  if (value === 'search') return '关键词'
  return '反查'
}

function stepClass(id: string): Record<string, boolean> {
  const status = progress[id]?.status
  return {
    'is-running': status === 'running',
    'is-done': status === 'done',
    'is-failed': status === 'failed',
    'is-empty': status === 'empty' || status === 'skipped',
  }
}

async function analyze(): Promise<void> {
  if (!props.item) return
  const current = ++run
  const token = `${Date.now()}-${Math.random().toString(36).slice(2)}`
  requestId = token
  loading.value = true
  error.value = ''
  result.value = null
  for (const key of Object.keys(progress)) delete progress[key]
  try {
    const value = await api.whiteboardImageInvestigate(props.item.id, props.mode, token)
    if (current === run) result.value = value
  } catch (cause) {
    if (current === run) error.value = api.errorText(cause)
  } finally {
    if (current === run) loading.value = false
  }
}

watch(
  () => [props.visible, props.item?.id, props.mode] as const,
  ([visible]) => {
    if (visible) {
      void analyze()
    } else {
      run += 1
      requestId = ''
      loading.value = false
      result.value = null
      error.value = ''
    }
  },
)

function open(url: string): void {
  if (url) void api.openExternalUrl(url)
}

function searchBing(query: string): void {
  if (!query) return
  void api.openExternalUrl(`https://www.bing.com/search?q=${encodeURIComponent(query)}`)
}

onMounted(async () => {
  try {
    unlisten = await api.onImageInvestigationProgress((entry) => {
      if (!requestId || entry.requestId !== requestId) return
      progress[entry.step] = entry
    })
  } catch {
    unlisten = null
  }
})

onUnmounted(() => {
  unlisten?.()
})
</script>

<template>
  <Transition name="wb-sheet">
    <section v-if="visible" class="wb-agent-sheet wb-lookup-sheet" aria-label="图片调查">
      <header class="wb-sheet-head">
        <div>
          <div class="wb-sheet-eyebrow"><ScanSearch :size="12" /> 图片调查 · 反查 + 读图 + 检索</div>
          <h2>{{ title }}</h2>
        </div>
        <button type="button" class="wb-sheet-close" title="关闭" @click="emit('close')">
          <X :size="14" />
        </button>
      </header>

      <div class="wb-lookup-hero">
        <div class="wb-lookup-preview is-large" :class="{ 'is-scanning': loading }">
          <img v-if="assetUrl" :src="assetUrl" alt="" draggable="false" />
          <span v-if="loading" class="wb-lookup-scan" />
        </div>
        <div class="wb-lookup-status">
          <b>{{ displayLabel }}</b>
          <p>{{ displayDetail }}</p>
        </div>
      </div>

      <ol v-if="loading" class="wb-investigate-steps">
        <li v-for="step in steps" :key="step.id" :class="stepClass(step.id)">
          <span class="wb-step-icon">
            <LoaderCircle v-if="progress[step.id]?.status === 'running'" :size="12" class="wb-spin" />
            <Check v-else-if="progress[step.id]?.status === 'done'" :size="12" />
            <X v-else-if="progress[step.id]?.status === 'failed'" :size="12" />
            <span v-else class="wb-step-dot" />
          </span>
          <span class="wb-step-label">{{ step.label }}</span>
          <em>{{ progress[step.id]?.detail || '等待中' }}</em>
        </li>
      </ol>

      <div v-else-if="error" class="wb-lookup-error">
        <p>{{ error }}</p>
        <button type="button" class="wb-btn" @click="analyze">
          <RotateCcw :size="12" /> 重试
        </button>
        <p class="wb-lookup-note">反查依赖 360 识图、trace.moe 和当前网络；失败不会写回卡片。</p>
      </div>

      <template v-else-if="result">
        <div class="wb-verdict" :class="mode === 'similar' ? 'is-likely' : `is-${result.verdict}`">
          <b>{{ mode === 'similar' ? '相似图' : '判断依据' }}</b>
          <p v-if="mode === 'source'">{{ result.summary }}</p>
          <p v-else>下面是 360 识图 / trace.moe 返回的相似图片；来源判断见下方。</p>
        </div>

        <section v-if="result.sources.length" class="wb-investigate-section">
          <h3>{{ mode === 'similar' ? '来源线索' : '来源候选' }}</h3>
          <div class="wb-source-list">
            <button
              v-for="source in result.sources"
              :key="source.url"
              type="button"
              class="wb-source-card"
              @click="open(source.url)"
            >
              <span v-if="source.thumbnail" class="wb-source-thumb">
                <img :src="source.thumbnail" alt="" draggable="false" />
              </span>
              <span class="wb-source-main">
                <span class="wb-source-site">{{ source.site }}</span>
                <b>{{ source.title }}</b>
                <span v-if="source.evidence" class="wb-source-evidence">{{ source.evidence }}</span>
                <span class="wb-source-meta">
                  {{ confidenceLabel(source.confidence) }} · {{ kindLabel(source.kind) }}
                  <ArrowUpRight :size="11" />
                </span>
              </span>
            </button>
          </div>
        </section>

        <section v-if="result.similar.length && (mode === 'similar' || result.verdict !== 'not_found')" class="wb-investigate-section">
          <h3>相似图片 <em>{{ result.similar.length }}</em></h3>
          <div class="wb-similar-grid">
            <button
              v-for="similar in result.similar"
              :key="similar.url + similar.title"
              type="button"
              class="wb-similar-card"
              @click="open(similar.url)"
            >
              <span class="wb-similar-thumb">
                <img v-if="similar.thumbnail" :src="similar.thumbnail" alt="" draggable="false" />
                <span v-else class="wb-similar-placeholder"><Images :size="16" /></span>
              </span>
              <b>{{ similar.title }}</b>
              <em>{{ similar.site }}</em>
            </button>
          </div>
        </section>

        <details v-if="mode === 'similar'" class="wb-investigate-details">
          <summary>来源判断</summary>
          <p>{{ result.summary }}</p>
        </details>

        <details class="wb-investigate-details">
          <summary>画面识别与文字</summary>
          <p>{{ result.description || '没有解析出画面描述。' }}</p>
          <div v-if="result.visibleText.length" class="wb-investigate-chips">
            <span v-for="text in result.visibleText" :key="text">{{ text }}</span>
          </div>
          <div v-if="result.entities.length" class="wb-investigate-chips is-muted">
            <span v-for="entity in result.entities" :key="entity">{{ entity }}</span>
          </div>
        </details>

        <details v-if="result.queries.length" class="wb-investigate-details">
          <summary>检索词（可手动复核）</summary>
          <div class="wb-investigate-chips">
            <button v-for="query in result.queries" :key="query" type="button" @click="searchBing(query)">
              {{ query }} <Search :size="10" />
            </button>
          </div>
        </details>

        <div class="wb-lookup-foot">
          <p>
            反查会把这张图上传到 360 识图；trace.moe 只用于动画场景。结果是线索，不是逐像素确认的出处。
          </p>
          <em>
            {{ providerLine }} · 读图 {{ result.usage.promptTokens }} + {{ result.usage.completionTokens }} tokens
            <template v-if="result.usage.searchCalls"> · 检索 {{ result.usage.searchCalls }} 次 ¥{{ result.usage.searchCostCny.toFixed(4) }}</template>
            · {{ (result.usage.totalMs / 1000).toFixed(1) }}s
          </em>
        </div>
      </template>
    </section>
  </Transition>
</template>
