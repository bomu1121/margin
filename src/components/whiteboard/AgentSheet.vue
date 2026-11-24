<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import { Check, LoaderCircle, Sparkles, Square, X } from 'lucide-vue-next'

import type { WhiteboardItem } from '../../lib/api'
import type { ProductionLog, ProductionRunSnapshot, ProductionStep } from '../../types'
import * as api from '../../lib/api'

const props = defineProps<{
  visible: boolean
  items: WhiteboardItem[]
  busy: boolean
  jobId: string
}>()

const emit = defineEmits<{
  close: []
  start: [prompt: string]
  reset: []
}>()

interface ResearchTurn {
  index: number
  tool: string
  label: string
  detail: string
}

const prompt = ref('')
const snapshot = ref<ProductionRunSnapshot | null>(null)
const runError = ref('')
const cancelling = ref(false)
const now = ref(Date.now())
let timer = 0
let disposed = false

const job = computed(() => snapshot.value?.job || null)
const running = computed(() => ['queued', 'pending', 'running'].includes(job.value?.status || ''))
const done = computed(() => ['done', 'success'].includes(job.value?.status || ''))
const failed = computed(() => ['failed', 'cancelled'].includes(job.value?.status || ''))
const steps = computed<ProductionStep[]>(() => job.value?.steps || [])
const logs = computed<ProductionLog[]>(() => snapshot.value?.logs || [])

const turns = computed<ResearchTurn[]>(() => parseTurns(logs.value))
const recentTurns = computed(() => turns.value.slice(-7))
const rounds = computed(() => turns.value.filter((turn) => turn.tool === 'model').length)
const progress = computed(() => (done.value ? 100 : Math.max(0, Math.min(100, job.value?.progress || 0))))
const tokens = computed(() => {
  const usage = job.value?.usage
  return (usage?.promptTokens || 0) + (usage?.completionTokens || 0)
})
const elapsed = computed(() => {
  const start = job.value?.startedAt || job.value?.createdAt || 0
  if (!start) return '0s'
  const end = job.value?.finishedAt || (running.value ? now.value : start)
  const seconds = Math.max(0, Math.round((end - start) / 1000))
  if (seconds < 60) return `${seconds}s`
  return `${Math.floor(seconds / 60)}m ${seconds % 60}s`
})
const runTitle = computed(() => {
  if (done.value) return '研究完成'
  if (job.value?.status === 'cancelled') return '研究已停止'
  if (failed.value) return '研究失败'
  if (job.value?.status === 'queued') return '排队中'
  return 'Agent 正在工作'
})
const runDetail = computed(() => {
  if (runError.value) return runError.value
  if (done.value) return formatTokenText(job.value?.result?.summary || '笔记已保存，可以打开查看。')
  if (job.value?.status === 'cancelled') return '任务已停止；已写入的内容不会丢。'
  if (failed.value) return formatTokenText(cleanLabel(job.value?.error || '')) || '任务失败，可以看下方日志。'
  const last = recentTurns.value[recentTurns.value.length - 1]
  if (last) return last.detail || last.label
  return cleanLabel(job.value?.currentStep || '') || '正在整理白板素材…'
})
const headerTitle = computed(() => {
  if (!props.jobId) return `研究选中的 ${props.items.length} 张卡片`
  if (done.value) return '研究完成'
  if (job.value?.status === 'cancelled') return '研究已停止'
  if (failed.value) return '研究失败'
  return '白板研究 Agent'
})

function kindLabel(item: WhiteboardItem): string {
  switch (item.kind) {
    case 'sticky':
      return '便签'
    case 'link':
      return '链接'
    case 'image':
      return '图片'
    case 'todo':
      return '待办'
    default:
      return '卡片'
  }
}

function summary(item: WhiteboardItem): string {
  return (item.title || item.text || item.url || '未命名').slice(0, 42)
}

function parseTurns(entries: ProductionLog[]): ResearchTurn[] {
  const map = new Map<number, ResearchTurn>()
  for (const log of entries) {
    const match = /^#(\d+)\s+(\S+)(?:\s+·\s+([\s\S]*))?$/.exec(log.message)
    if (!match) continue
    const index = Number(match[1]) - 1
    const tool = match[2]
    map.set(index, {
      index,
      tool,
      label: toolLabel(tool),
      detail: turnDetail(tool, match[3] || ''),
    })
  }
  return [...map.values()].sort((left, right) => left.index - right.index)
}

function formatTokens(value: number): string {
  if (value >= 1_000_000) return `${(value / 1_000_000).toFixed(1)}m`
  if (value >= 1000) return `${(value / 1000).toFixed(1)}k`
  return String(value)
}

function formatTokenText(value: string): string {
  return value
    .replace(/(\d+)\/(\d+) tokens/g, (_, input: string, output: string) => `${formatTokens(Number(input))}/${formatTokens(Number(output))} tok`)
    .replace(/(\d+) tokens/g, (_, total: string) => `${formatTokens(Number(total))} tok`)
}

function turnDetail(tool: string, raw: string): string {
  const tokens = /^(\d+)\/(\d+) tok\b/.exec(raw)
  if (tool === 'model') {
    return tokens
      ? `输入 ${formatTokens(Number(tokens[1]))} · 输出 ${formatTokens(Number(tokens[2]))}`
      : '思考完成'
  }
  if (tool === 'bash') {
    return /exit\s+0\b/.test(raw) ? '命令执行完成' : '命令执行结束'
  }
  const file = /^([^·]+?)\s*·\s*([\d,]+)\s*B/.exec(raw)
  if (['read_file', 'write_file', 'edit_file'].includes(tool) && file) {
    return `${file[1].trim()} · ${file[2]} B`
  }
  const line = raw.split(/\r?\n/)[0].replace(/\s+/g, ' ').trim()
  return line.length > 120 ? `${line.slice(0, 120)}…` : line || '完成'
}

function toolLabel(tool: string): string {
  switch (tool) {
    case 'model':
      return '模型思考'
    case 'fetch_url':
      return '读取链接'
    case 'read_file':
      return '读取素材'
    case 'write_file':
      return '写入笔记'
    case 'edit_file':
      return '修改笔记'
    case 'list_dir':
      return '查看目录'
    case 'search_web':
      return '联网检索'
    case 'bash':
      return '执行命令'
    default:
      return tool
  }
}

function cleanLabel(value: string): string {
  if (job.value?.templateId !== 'whiteboard-research') return value
  return value
    .replaceAll('AI 成稿（Rust 引擎）', '研究 Agent')
    .replaceAll('AI 成稿引擎', '研究 Agent')
    .replaceAll('AI 成稿', '研究 Agent')
    .replaceAll('AI 研究', '研究 Agent')
}

function stepLabel(step: ProductionStep): string {
  if (job.value?.templateId === 'whiteboard-research' && step.key === 'agent_note') {
    return '研究 Agent'
  }
  return step.label
}

function stepClass(step: ProductionStep): Record<string, boolean> {
  return {
    'is-done': ['done', 'success'].includes(step.status),
    'is-running': step.status === 'running',
    'is-failed': ['failed', 'error', 'interrupted', 'cancelled'].includes(step.status),
  }
}

function start(): void {
  if (props.busy || !props.items.length) return
  emit('start', prompt.value.trim())
}

function stop(): void {
  if (!props.jobId || cancelling.value) return
  cancelling.value = true
  void api
    .productionCancel(props.jobId)
    .then(() => {
      runError.value = ''
      window.setTimeout(() => void refresh(), 600)
    })
    .catch((cause) => {
      runError.value = api.errorText(cause)
    })
    .finally(() => {
      cancelling.value = false
    })
}

function openNote(): void {
  const noteId = job.value?.noteId
  if (noteId) void api.whiteboardOpenNote(noteId)
}

function newResearch(): void {
  emit('reset')
}

function stopTimer(): void {
  if (timer) {
    window.clearTimeout(timer)
    timer = 0
  }
}

function schedule(): void {
  stopTimer()
  if (disposed || !running.value) return
  timer = window.setTimeout(() => {
    void refresh()
  }, 900)
}

async function refresh(): Promise<void> {
  if (!props.jobId || !props.visible) return
  try {
    const next = await api.productionRunSnapshot(props.jobId, 0)
    snapshot.value = next
    runError.value = ''
    now.value = Date.now()
  } catch (cause) {
    runError.value = api.errorText(cause)
  }
  schedule()
}

watch(
  () => props.visible,
  (visible) => {
    if (!visible) {
      stopTimer()
      return
    }
    if (props.jobId) void refresh()
    else prompt.value = ''
  },
)

watch(
  () => props.jobId,
  (jobId) => {
    if (jobId && props.visible) {
      snapshot.value = null
      runError.value = ''
      void refresh()
    } else {
      stopTimer()
      snapshot.value = null
      runError.value = ''
      prompt.value = ''
    }
  },
)

watch(
  () => props.items,
  () => {
    if (!props.jobId) prompt.value = ''
  },
)

onBeforeUnmount(() => {
  disposed = true
  stopTimer()
})
</script>

<template>
  <Transition name="wb-sheet">
    <section v-if="visible" class="wb-agent-sheet wb-research-sheet" aria-label="白板研究">
      <header class="wb-sheet-head">
        <div>
          <div class="wb-sheet-eyebrow"><Sparkles :size="12" /> 白板研究 · Agent</div>
          <h2>{{ headerTitle }}</h2>
        </div>
        <button type="button" class="wb-sheet-close" title="关闭" @click="emit('close')">
          <X :size="14" />
        </button>
      </header>

      <template v-if="!jobId">
        <div class="wb-sheet-chips">
          <span v-for="item in items" :key="item.id" class="wb-sheet-chip">
            <em>{{ kindLabel(item) }}</em>{{ summary(item) }}
          </span>
        </div>

        <label class="wb-sheet-field">
          <span>想研究什么？</span>
          <textarea
            v-model="prompt"
            class="wb-sheet-prompt"
            rows="3"
            placeholder="例如：这些线索能拼出什么结论？哪些还需要查证？"
            :disabled="busy"
            @keydown.ctrl.enter.prevent="start"
            @keydown.meta.enter.prevent="start"
          />
        </label>

        <div class="wb-sheet-foot">
          <p>研究 Agent 会读卡片、抓取链接、按需联网检索，再写一篇新笔记；和制作台的视频笔记 / AI 成稿是两条独立链路。</p>
          <div class="wb-sheet-actions">
            <button type="button" class="wb-btn" :disabled="busy" @click="emit('close')">取消</button>
            <button type="button" class="wb-btn is-primary" :disabled="busy" @click="start">
              <LoaderCircle v-if="busy" :size="13" class="wb-spin" />
              <Sparkles v-else :size="13" />
              {{ busy ? '正在启动…' : '开始研究' }}
            </button>
          </div>
        </div>
      </template>

      <template v-else>
        <div class="wb-research-hero">
          <div
            class="wb-research-orb"
            :class="{ 'is-running': running, 'is-done': done, 'is-failed': failed }"
          >
            <Sparkles :size="18" />
          </div>
          <div class="wb-research-status">
            <b>{{ runTitle }}</b>
            <p>{{ runDetail }}</p>
            <div class="wb-research-meta">
              <span>第 {{ Math.max(1, rounds) }} 回合</span>
              <span>{{ elapsed }}</span>
              <span>{{ formatTokens(tokens) }} tok</span>
            </div>
          </div>
        </div>

        <div class="wb-research-progress" aria-hidden="true">
          <span :style="{ width: `${progress}%` }" />
        </div>

        <ol class="wb-research-stages">
          <li v-for="step in steps" :key="step.key" :class="stepClass(step)">
            <span class="wb-research-step-icon">
              <LoaderCircle v-if="step.status === 'running'" :size="12" class="wb-spin" />
              <Check v-else-if="['done', 'success'].includes(step.status)" :size="12" />
              <X v-else-if="['failed', 'error', 'interrupted', 'cancelled'].includes(step.status)" :size="12" />
              <span v-else class="wb-research-dot" />
            </span>
            <span class="wb-research-step-label">{{ stepLabel(step) }}</span>
            <em>{{ formatTokenText(cleanLabel(step.detail)) || (step.status === 'running' ? '正在处理…' : '等待中') }}</em>
          </li>
        </ol>

        <div v-if="recentTurns.length" class="wb-research-turns">
          <div v-for="turn in recentTurns" :key="turn.index" class="wb-research-turn">
            <span class="wb-research-turn-tool">{{ turn.label }}</span>
            <span class="wb-research-turn-detail">{{ turn.detail || '完成' }}</span>
          </div>
        </div>
        <p v-else-if="running" class="wb-research-empty">Agent 正在整理素材；工具调用会实时出现在这里。</p>

        <p v-if="runError" class="wb-research-error">{{ runError }}</p>

        <div class="wb-sheet-foot">
          <p>
            {{ running ? '关闭面板不会停止任务；可以在白板的 Agent 卡上继续查看。' : done ? '笔记已写入待整理，可以打开阅读。' : '任务已结束。' }}
          </p>
          <div class="wb-sheet-actions">
            <button
              v-if="running"
              type="button"
              class="wb-btn is-quiet"
              :disabled="cancelling"
              @click="stop"
            >
              <Square :size="11" /> {{ cancelling ? '停止中…' : '停止研究' }}
            </button>
            <button v-if="done && job?.noteId" type="button" class="wb-btn is-primary" @click="openNote">
              <Sparkles :size="12" /> 打开笔记
            </button>
            <button
              v-if="(done || failed) && items.length"
              type="button"
              class="wb-btn"
              @click="newResearch"
            >
              再研究一次
            </button>
          </div>
        </div>
      </template>
    </section>
  </Transition>
</template>
