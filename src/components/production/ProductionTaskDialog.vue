<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { useRouter } from 'vue-router'
import DOMPurify from 'dompurify'
import { marked } from 'marked'
import {
  Check,
  CircleAlert,
  Download,
  ExternalLink,
  RotateCcw,
  Square,
  Trash2,
  X,
} from 'lucide-vue-next'

import * as api from '../../lib/api'
import {
  formatDate,
  formatDuration,
  formatTokens,
  progressOf,
  mediaUrl,
  sourceSummary,
  statusClass,
  statusLabel,
  templateById,
} from '../../lib/production'
import {
  deleteProduction,
  production,
  refreshJobs,
  retryProduction,
  upsertJob,
} from '../../stores/production'
import type { ProductionDrillItem, ProductionJob, ProductionJobDetail, ProductionTraceItem } from '../../types'

const props = defineProps<{ jobId: string }>()
const emit = defineEmits<{ close: [] }>()
const router = useRouter()

const detail = ref<ProductionJobDetail | null>(null)
const loading = ref(false)
const error = ref('')
const activeTab = ref('result')
const quizChoices = ref<Record<string, number>>({})

const job = computed<ProductionJob | null>(() => {
  return production.jobs.find((item) => item.id === props.jobId) ?? detail.value?.job ?? null
})
const template = computed(() => (job.value ? templateById(production.templates, job.value.templateId) : undefined))
const result = computed(() => job.value?.result ?? null)
const tabs = computed(() => {
  const items = [
    { id: 'result', label: '结果' },
    { id: 'steps', label: '步骤' },
  ]
  if ((result.value?.variants.length ?? 0) >= 2) items.push({ id: 'compare', label: '对照' })
  if (result.value?.mindmap) items.push({ id: 'mindmap', label: '思维导图' })
  if ((result.value?.drill.length ?? 0) > 0) items.push({ id: 'drill', label: '练一练' })
  if ((result.value?.trace.length ?? 0) > 0) items.push({ id: 'trace', label: '溯源' })
  items.push({ id: 'usage', label: '用量' })
  return items
})
const currentTab = computed(() => (tabs.value.some((item) => item.id === activeTab.value) ? activeTab.value : 'result'))
const tokens = computed(() => (job.value?.usage.promptTokens ?? 0) + (job.value?.usage.completionTokens ?? 0))
const author = computed(() => {
  const source = job.value?.source
  return source?.videos[0]?.author || '—'
})
const noteTitle = computed(() => {
  const body = result.value?.body ?? ''
  const line = body.split('\n').find((item) => item.startsWith('# '))
  return line?.slice(2).trim() || job.value?.title || ''
})

const bodyWithoutTitle = computed(() => (result.value?.body ?? '').replace(/^#\s+.*(?:\r?\n)+/, ''))

function renderMarkdown(value: string): string {
  try {
    return DOMPurify.sanitize(marked.parse(value, { async: false }) as string)
  } catch {
    return DOMPurify.sanitize(value)
  }
}

async function loadDetail(): Promise<void> {
  if (!props.jobId) return
  loading.value = true
  error.value = ''
  try {
    const data = await api.productionGet(props.jobId)
    if (data) {
      detail.value = data
      upsertJob(data.job)
    }
  } catch (cause) {
    error.value = api.errorText(cause)
  } finally {
    loading.value = false
  }
}

function close(): void {
  emit('close')
}

async function cancel(): Promise<void> {
  if (!job.value) return
  try {
    await api.productionCancel(job.value.id)
    await loadDetail()
  } catch (cause) {
    error.value = api.errorText(cause)
  }
}

async function retry(): Promise<void> {
  if (!job.value) return
  try {
    const next = await retryProduction(job.value.id)
    upsertJob(next)
    await loadDetail()
  } catch (cause) {
    error.value = api.errorText(cause)
  }
}

async function remove(): Promise<void> {
  if (!job.value) return
  if (!window.confirm('删除这条任务记录？已经入库的笔记不会被删除。')) return
  try {
    await deleteProduction(job.value.id)
    close()
  } catch (cause) {
    error.value = api.errorText(cause)
  }
}

function exportMarkdown(): void {
  if (!result.value) return
  const blob = new Blob([result.value.body], { type: 'text/markdown;charset=utf-8' })
  const link = document.createElement('a')
  link.href = URL.createObjectURL(blob)
  link.download = `${noteTitle.value || '笔记'}.md`
  link.click()
  window.setTimeout(() => URL.revokeObjectURL(link.href), 1000)
}

function openNote(): void {
  if (!job.value?.noteId) return
  void router.push(`/note/${job.value.noteId}`)
  close()
}

function chooseQuiz(item: ProductionDrillItem, index: number): void {
  quizChoices.value = { ...quizChoices.value, [item.id]: index }
}

function optionClass(item: ProductionDrillItem, index: number): string {
  if (quizChoices.value[item.id] === undefined) return ''
  if (quizChoices.value[item.id] !== index) return ''
  return String(item.answer).includes(item.options[index] ?? '') ? 'is-correct' : 'is-wrong'
}

function traceClass(item: ProductionTraceItem): string {
  if (item.verdict.includes('可印证')) return 'is-ok'
  if (item.verdict.includes('争议') || item.verdict.includes('反证')) return 'is-warn'
  return ''
}

function stepStatusLabel(status: string): string {
  return ({ pending: '等待', running: '进行中', done: '完成', error: '失败', cancelled: '已取消', skipped: '跳过' } as Record<string, string>)[status] ?? status
}

watch(() => props.jobId, () => {
  activeTab.value = 'result'
  quizChoices.value = {}
  void loadDetail()
}, { immediate: true })

watch(() => job.value?.updatedAt, () => {
  if (job.value?.status === 'running' || job.value?.status === 'queued') void loadDetail()
})

watch(tabs, (items) => {
  if (!items.some((item) => item.id === activeTab.value)) activeTab.value = 'result'
})

onMounted(() => {
  if (!production.templates.length) void refreshJobs()
})
</script>

<template>
  <div v-if="jobId" class="prod-overlay" @click.self="close">
    <div v-if="job" class="prod-dialog prod-task-dialog" role="dialog" aria-modal="true" aria-labelledby="prodTaskTitle">
      <div class="prod-task-top">
        <div>
          <div class="prod-task-title-row">
            <h2 id="prodTaskTitle">{{ job.title }}</h2>
            <span class="prod-task-status" :class="statusClass(job.status)">
              <i class="prod-dot" :class="statusClass(job.status) === 'is-success' ? 'is-green' : statusClass(job.status) === 'is-error' ? 'is-red' : statusClass(job.status) === 'is-gold' ? 'is-gold' : 'is-blue'" />
              {{ statusLabel(job.status) }}
            </span>
          </div>
          <div class="prod-task-meta">
            <span>{{ template?.name ?? job.templateId }}</span>
            <span>{{ sourceSummary(job.source) }}</span>
            <span>{{ formatDate(job.updatedAt) }}</span>
            <span v-if="job.status === 'running' || job.status === 'queued'">{{ job.progress }}%</span>
          </div>
        </div>
        <div class="prod-task-actions">
          <button v-if="job.status === 'running' || job.status === 'queued'" type="button" class="prod-btn is-small" @click="cancel">
            <Square :size="12" /> 取消
          </button>
          <button v-if="['error', 'cancelled', 'interrupted'].includes(job.status)" type="button" class="prod-btn is-small is-primary" @click="retry">
            <RotateCcw :size="12" /> 重试
          </button>
          <button v-if="result" type="button" class="prod-btn is-small" @click="exportMarkdown">
            <Download :size="12" /> 导出
          </button>
          <button v-if="job.status !== 'running' && job.status !== 'queued'" type="button" class="prod-btn is-small" @click="remove">
            <Trash2 :size="12" /> 删除
          </button>
          <button type="button" class="prod-icon-btn" aria-label="关闭" @click="close"><X :size="15" /></button>
        </div>
      </div>

      <div class="prod-task-tabs" role="tablist">
        <button v-for="tab in tabs" :key="tab.id" type="button" role="tab" :aria-selected="currentTab === tab.id" :class="{ 'is-active': currentTab === tab.id }" @click="activeTab = tab.id">{{ tab.label }}</button>
      </div>

      <div class="prod-task-layout">
        <div class="prod-task-main">
          <div v-if="error" class="prod-error-box" style="margin-bottom: 10px">
            <h3>读取任务失败</h3>
            <p>{{ error }}</p>
          </div>

          <section v-if="currentTab === 'result'" class="prod-task-panel is-active">
            <div v-if="job.status === 'running' || job.status === 'queued'" class="prod-running-box">
              <h3>{{ job.status === 'queued' ? '排队中' : `正在${job.currentStep || '处理'}` }}</h3>
              <p>{{ job.progress }}% · 已完成 {{ job.steps.filter((step) => step.status === 'done').length }} / {{ job.steps.length }} 个步骤</p>
              <div class="prod-running-bar"><i :style="{ width: `${progressOf(job)}%` }" /></div>
              <p style="margin-top: 9px">可以切到「步骤」看日志；取消后已完成的步骤不会写进笔记。</p>
            </div>
            <div v-else-if="job.status === 'error'" class="prod-error-box">
              <h3>制作失败</h3>
              <p>{{ job.error || '某个步骤失败。' }}</p>
              <p style="margin-top: 5px">已完成的步骤和日志保留；点右上角「重试」会重新执行整条模板。</p>
            </div>
            <div v-else-if="job.status === 'cancelled' || job.status === 'interrupted'" class="prod-error-box">
              <h3>{{ job.status === 'cancelled' ? '已取消' : '任务被中断' }}</h3>
              <p>{{ job.status === 'cancelled' ? '你取消了这次制作。' : '应用在运行中退出；任务已标记中断，可以重试。' }}</p>
            </div>
            <div v-else-if="result" class="prod-result-doc">
              <div class="prod-result-kicker">{{ template?.name }} · 已入库</div>
              <h3>{{ noteTitle }}</h3>
              <div class="prod-frontmatter">
                <span>类型: {{ job.sourceKind === 'text' ? '学习笔记' : '视频笔记' }}</span>
                <span>来源: {{ job.sourceKind === 'text' ? '文本' : job.sourceKind === 'file' ? '本地文件' : 'B站' }}</span>
                <span>作者: {{ author }}</span>
                <span>任务: {{ job.id }}</span>
              </div>
              <div class="prod-markdown" v-html="renderMarkdown(bodyWithoutTitle)" />
              <div class="prod-inline-actions">
                <button v-if="job.noteId" type="button" class="prod-btn is-primary" @click="openNote">
                  <ExternalLink :size="13" /> 打开笔记
                </button>
                <button type="button" class="prod-btn" @click="exportMarkdown"><Download :size="13" /> 导出 Markdown</button>
              </div>
            </div>
          </section>

          <section v-else-if="currentTab === 'steps'" class="prod-task-panel is-active">
            <div class="prod-step-list">
              <div v-for="step in job.steps" :key="step.index" class="prod-step-row" :class="`is-${step.status}`">
                <span class="prod-step-mark">
                  <Check v-if="step.status === 'done'" :size="10" />
                  <CircleAlert v-else-if="step.status === 'error'" :size="10" />
                  <span v-else>{{ step.index + 1 }}</span>
                </span>
                <span><b>{{ step.label }}</b><small>{{ step.detail || stepStatusLabel(step.status) }}</small></span>
                <span class="prod-step-time">{{ stepStatusLabel(step.status) }}</span>
              </div>
            </div>
            <div v-if="detail?.logs.length" class="prod-log">
              <div v-for="log in detail.logs" :key="log.id" class="prod-log-row" :class="{ 'is-error': log.level === 'error' }">
                <span>{{ formatDate(log.createdAt).slice(6) }}</span>
                <span>{{ log.level }}</span>
                <span>{{ log.message }}</span>
              </div>
            </div>
            <p v-else class="prod-wizard-hint" style="margin-top: 10px">还没有日志；运行开始后会逐步出现。</p>
          </section>

          <section v-else-if="currentTab === 'compare'" class="prod-task-panel is-active">
            <div class="prod-compare-grid">
              <div v-for="variant in result?.variants" :key="variant.name" class="prod-compare-col">
                <h4>{{ variant.name }} · {{ variant.summary }}</h4>
                <div class="prod-markdown" v-html="renderMarkdown(variant.body)" />
              </div>
            </div>
            <p class="prod-wizard-hint" style="margin-top: 8px">两版都来自同一份素材；入库的是第一版，第二版附在正文末尾。</p>
          </section>

          <section v-else-if="currentTab === 'mindmap'" class="prod-task-panel is-active">
            <div class="prod-mindmap">{{ result?.mindmap }}</div>
          </section>

          <section v-else-if="currentTab === 'drill'" class="prod-task-panel is-active">
            <div v-for="item in result?.drill" :key="item.id" class="prod-drill-card" style="margin-bottom: 9px">
              <h3>{{ item.question }}</h3>
              <button v-for="(option, index) in item.options" :key="option" type="button" class="prod-quiz-option" :class="optionClass(item, index)" @click="chooseQuiz(item, index)">
                {{ option }}
              </button>
              <p v-if="quizChoices[item.id] !== undefined" class="prod-wizard-hint" style="margin-top: 7px">参考答案：{{ item.answer }}<span v-if="item.note"> · 依据：{{ item.note }}</span></p>
            </div>
          </section>

          <section v-else-if="currentTab === 'trace'" class="prod-task-panel is-active">
            <div v-for="(item, index) in result?.trace" :key="index" class="prod-trace-row">
              <b>{{ item.claim }}<br /><span style="color: var(--ink-3); font-size: 9.5px">{{ item.quote }}</span></b>
              <span :class="traceClass(item)">{{ item.verdict }}</span>
            </div>
            <p v-if="!result?.trace.length" class="prod-wizard-hint">这次运行没有生成溯源条目。</p>
          </section>

          <section v-else class="prod-task-panel is-active">
            <div class="prod-usage-table">
              <div class="prod-usage-row"><span>输入 tokens</span><b>{{ formatTokens(job.usage.promptTokens) }}</b><span>未缓存 {{ formatTokens(job.usage.uncachedInputTokens) }}</span></div>
              <div v-if="job.usage.cachedTokens" class="prod-usage-row"><span>缓存读 tokens</span><b>{{ formatTokens(job.usage.cachedTokens) }}</b><span>接口返回</span></div>
              <div v-if="job.usage.cacheWriteTokens" class="prod-usage-row"><span>缓存写 tokens</span><b>{{ formatTokens(job.usage.cacheWriteTokens) }}</b><span>接口返回</span></div>
              <div class="prod-usage-row"><span>输出 tokens</span><b>{{ formatTokens(job.usage.completionTokens) }}</b><span>接口返回</span></div>
              <div v-if="job.usage.reasoningTokens" class="prod-usage-row"><span>推理 tokens</span><b>{{ formatTokens(job.usage.reasoningTokens) }}</b><span>仅记录，不重复计费</span></div>
              <div class="prod-usage-row"><span>音频</span><b>{{ formatDuration(job.usage.audioSeconds) }}</b><span>ASR</span></div>
              <div class="prod-usage-row"><span>估算金额</span><b>{{ job.usage.estimatedCost ? `¥${job.usage.estimatedCost.toFixed(2)}` : '—' }}</b><span>{{ job.usage.priced ? '目录价估算' : job.usage.estimatedCost ? 'ASR 手动价' : '未收录价格' }}</span></div>
              <div v-if="job.usage.providerCost > 0" class="prod-usage-row"><span>服务商账单</span><b>{{ job.usage.providerCurrency }} {{ job.usage.providerCost.toFixed(4) }}</b><span>接口实报</span></div>
            </div>
            <p class="prod-wizard-hint" style="margin-top: 8px">token 是接口返回值；金额 = token × 参考价，属于估算。服务商实报金额单独显示，不混入估算。</p>
          </section>
        </div>

        <aside class="prod-task-rail">
          <div class="prod-rail-section" style="margin-top: 0">
            <div class="prod-rail-label">来源</div>
            <div class="prod-account-card">
              <img v-if="job.source.videos[0]?.cover" class="prod-account-avatar prod-account-image" :src="mediaUrl(job.source.videos[0].cover)" alt="" referrerpolicy="no-referrer" loading="lazy" />
              <span v-else class="prod-account-avatar">{{ job.sourceKind === 'text' ? '文' : job.sourceKind === 'file' ? '本' : 'B' }}</span>
              <span class="prod-account-copy"><b>{{ job.sourceKind === 'text' ? '粘贴文稿' : job.sourceKind === 'file' ? '本地文件' : 'B站' }}</b><span>{{ author }}</span></span>
            </div>
          </div>
          <div class="prod-rail-section">
            <div class="prod-rail-label">模板</div>
            <div class="prod-pipeline"><span v-for="item in template?.pipeline" :key="item">{{ item }}</span></div>
          </div>
          <div class="prod-rail-section">
            <div class="prod-rail-label">运行</div>
            <dl class="prod-kv">
              <div><dt>任务</dt><dd>{{ job.id }}</dd></div>
              <div><dt>开始</dt><dd>{{ job.startedAt ? formatDate(job.startedAt) : '—' }}</dd></div>
              <div><dt>耗时</dt><dd>{{ job.finishedAt && job.startedAt ? formatDuration((job.finishedAt - job.startedAt) / 1000) : '—' }}</dd></div>
              <div><dt>tokens</dt><dd>{{ formatTokens(tokens) }}</dd></div>
            </dl>
          </div>
          <div class="prod-rail-section">
            <div class="prod-rail-label">入库</div>
            <div class="prod-path">{{ job.noteRelPath || '等待入库' }}</div>
            <div class="prod-note">稳定 noteId，重跑更新同一篇；目录、front matter、索引和标注由页边笔记决定。</div>
          </div>
          <div v-if="job.options.tags.length" class="prod-rail-section">
            <div class="prod-rail-label">标签</div>
            <div class="prod-folder-row"><span v-for="tag in job.options.tags" :key="tag" class="prod-folder-chip">{{ tag }}</span></div>
          </div>
          <div v-if="job.options.fields.length" class="prod-rail-section">
            <div class="prod-rail-label">属性</div>
            <div class="prod-folder-row"><span v-for="field in job.options.fields" :key="field" class="prod-folder-chip">{{ field }}</span></div>
          </div>
          <div v-if="result?.related.length" class="prod-rail-section">
            <div class="prod-rail-label">相关笔记</div>
            <div class="prod-note">{{ result.related.join('、') }}</div>
          </div>
        </aside>
      </div>
    </div>
  </div>
</template>

<style scoped>
.prod-error { margin: 7px 0 0; color: var(--red); font-family: var(--sans); font-size: 10.5px; line-height: 1.5; }
</style>
