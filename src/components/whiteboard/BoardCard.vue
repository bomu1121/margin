<script setup lang="ts">
import { computed, nextTick, onMounted, ref, watch } from 'vue'
import {
  ArrowUpRight,
  Check,
  Images,
  Palette,
  ScanSearch,
  X,
} from 'lucide-vue-next'

import type { AgentState } from '../../stores/whiteboard'
import type { WhiteboardItem } from '../../lib/api'
import { domainOf } from '../../stores/whiteboard'

const props = defineProps<{
  item: WhiteboardItem
  selected: boolean
  scale: number
  assetUrl: string
  agentState?: AgentState
  linkLoading?: boolean
}>()

const emit = defineEmits<{
  select: [additive: boolean]
  patch: [values: Partial<WhiteboardItem>]
  commit: []
  remove: []
  openLink: [url?: string]
  openNote: []
  openResearch: []
  lookupImage: [mode: 'source' | 'similar']
  cycleColor: []
}>()

const dragging = ref(false)
const resizing = ref(false)
const editing = ref(false)
const textarea = ref<HTMLTextAreaElement | null>(null)
const stickyView = ref<HTMLElement | null>(null)
const todoInput = ref<HTMLTextAreaElement | null>(null)

let startX = 0
let startY = 0
let originX = 0
let originY = 0
let originW = 0
let originH = 0

const stickyHtml = computed(() =>
  props.item.kind === 'sticky' ? renderStickyHtml(props.item.text) : '',
)

const domain = computed(() => {
  try {
    const data = JSON.parse(props.item.dataJson || '{}') as { domain?: string }
    if (data.domain) return data.domain
  } catch {
    // Fall through to the URL parser.
  }
  return domainOf(props.item.url)
})

const kindLabel = computed(() => {
  switch (props.item.kind) {
    case 'sticky':
      return '便签'
    case 'link':
      return '链接'
    case 'image':
      return '图片'
    case 'todo':
      return '待办'
    default:
      return '研究'
  }
})

const agentStatus = computed(() => props.agentState?.status || (props.item.noteId ? 'done' : 'queued'))
const agentLabel = computed(() => {
  switch (agentStatus.value) {
    case 'done':
      return '研究完成'
    case 'failed':
      return '研究失败'
    case 'running':
      return '正在研究'
    default:
      return '排队中'
  }
})
const agentProgress = computed(() => Math.max(0, Math.min(100, props.agentState?.progress || 0)))

const style = computed(() => ({
  left: `${props.item.x}px`,
  top: `${props.item.y}px`,
  width: `${props.item.w}px`,
  height: `${props.item.h}px`,
  zIndex: props.item.z,
}))

const tintClass = computed(() =>
  props.item.kind === 'sticky' ? `is-tint-${props.item.color || 'gold'}` : '',
)

function select(event: PointerEvent | MouseEvent): void {
  emit('select', event.shiftKey || event.metaKey || event.ctrlKey)
}

function onCardPointerDown(event: PointerEvent): void {
  if ((event.target as HTMLElement).closest('button, input, textarea, a')) return
  select(event)
}

function startDrag(event: PointerEvent): void {
  if (event.button !== 0) return
  select(event)
  dragging.value = true
  startX = event.clientX
  startY = event.clientY
  originX = props.item.x
  originY = props.item.y
  ;(event.currentTarget as HTMLElement).setPointerCapture(event.pointerId)
}

function moveDrag(event: PointerEvent): void {
  if (!dragging.value) return
  emit('patch', {
    x: originX + (event.clientX - startX) / props.scale,
    y: originY + (event.clientY - startY) / props.scale,
  })
}

function endDrag(event: PointerEvent): void {
  if (!dragging.value) return
  dragging.value = false
  try {
    ;(event.currentTarget as HTMLElement).releasePointerCapture(event.pointerId)
  } catch {
    // Pointer capture can already be gone.
  }
  emit('commit')
}

function startResize(event: PointerEvent): void {
  if (event.button !== 0) return
  select(event)
  resizing.value = true
  startX = event.clientX
  startY = event.clientY
  originW = props.item.w
  originH = props.item.h
  ;(event.currentTarget as HTMLElement).setPointerCapture(event.pointerId)
}

function moveResize(event: PointerEvent): void {
  if (!resizing.value) return
  emit('patch', {
    w: Math.max(140, originW + (event.clientX - startX) / props.scale),
    h: Math.max(props.item.kind === 'image' ? 160 : 72, originH + (event.clientY - startY) / props.scale),
  })
}

function endResize(event: PointerEvent): void {
  if (!resizing.value) return
  resizing.value = false
  try {
    ;(event.currentTarget as HTMLElement).releasePointerCapture(event.pointerId)
  } catch {
    // Pointer capture can already be gone.
  }
  emit('commit')
}

function onText(event: Event): void {
  emit('patch', { text: (event.target as HTMLTextAreaElement).value })
  if (props.item.kind === 'sticky') void nextTick(growSticky)
  if (props.item.kind === 'todo') void nextTick(growTodo)
}

function growSticky(): void {
  const element = editing.value || !props.item.text ? textarea.value : stickyView.value
  if (!element) return
  const previousHeight = element.style.height
  const previousMin = element.style.minHeight
  element.style.height = '0px'
  element.style.minHeight = '0px'
  const needed = element.scrollHeight
  element.style.height = previousHeight
  element.style.minHeight = previousMin
  const height = Math.max(190, needed + 30)
  if (height > props.item.h + 2) emit('patch', { h: height })
}

function growTodo(): void {
  const element = todoInput.value
  if (!element) return
  const previousHeight = element.style.height
  const previousMin = element.style.minHeight
  element.style.height = '0px'
  element.style.minHeight = '0px'
  const needed = element.scrollHeight
  element.style.height = previousHeight
  element.style.minHeight = previousMin
  const height = Math.max(110, needed + 50)
  if (height > props.item.h + 2) emit('patch', { h: height })
}

function onTitle(event: Event): void {
  emit('patch', { title: (event.target as HTMLInputElement).value })
}

function beginEdit(): void {
  editing.value = true
  void nextTick(() => {
    const element = textarea.value
    if (!element) return
    element.focus()
    const end = element.value.length
    element.setSelectionRange(end, end)
  })
}

function finishEdit(): void {
  emit('commit')
  editing.value = false
  void nextTick(growSticky)
}

function onStickyClick(event: MouseEvent): void {
  const target = event.target as HTMLElement | null
  const anchor = target?.closest('a[data-url]') as HTMLAnchorElement | null
  if (!anchor) return
  event.preventDefault()
  event.stopPropagation()
  emit('openLink', anchor.dataset.url || '')
}

function onStickyDoubleClick(event: MouseEvent): void {
  const target = event.target as HTMLElement | null
  if (target?.closest('a[data-url]')) return
  event.stopPropagation()
  beginEdit()
}

function escapeHtml(value: string): string {
  return value
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;')
}

/** 把便签正文里的 http(s) 渲染成蓝色链接，其余文字保持原样并转义。 */
function renderStickyHtml(text: string): string {
  const pattern = /https?:\/\/[^\s<>"']+/gi
  let cursor = 0
  let html = ''
  for (const match of text.matchAll(pattern)) {
    const start = match.index ?? 0
    const raw = match[0]
    html += escapeHtml(text.slice(cursor, start))
    const url = raw.replace(/[),.;!?，。；、！？）】》]+$/, '')
    if (url) {
      const escaped = escapeHtml(url)
      html += `<a href="${escaped}" data-url="${escaped}" class="wb-sticky-url">${escaped}</a>`
    }
    html += escapeHtml(raw.slice(url.length))
    cursor = start + raw.length
  }
  html += escapeHtml(text.slice(cursor))
  return html
}

function onChecked(event: Event): void {
  emit('patch', { checked: (event.target as HTMLInputElement).checked })
}

function stop(event: Event): void {
  event.stopPropagation()
}

onMounted(() => {
  if (props.item.kind === 'sticky' && !props.item.text) {
    editing.value = true
    void nextTick(() => textarea.value?.focus())
  }
  if (props.item.kind === 'sticky') void nextTick(growSticky)
  if (props.item.kind === 'todo') void nextTick(growTodo)
  if (document.fonts?.ready) {
    void document.fonts.ready.then(() => {
      if (props.item.kind === 'sticky') growSticky()
      if (props.item.kind === 'todo') growTodo()
    })
  }
})

watch(
  () => props.item.text,
  () => {
    if (!editing.value) void nextTick(growSticky)
  },
)

watch(
  () => props.selected,
  (selected) => {
    if (selected && props.item.kind === 'sticky' && !props.item.text) {
      void nextTick(() => textarea.value?.focus())
    }
  },
)
</script>

<template>
  <article
    class="wb-card"
    :class="[
      `is-${item.kind}`,
      tintClass,
      { 'is-selected': selected, 'is-dragging': dragging || resizing },
    ]"
    :style="style"
    @pointerdown.stop="onCardPointerDown"
  >
    <header
      class="wb-card-grip"
      title="拖动卡片"
      @pointerdown.stop="startDrag"
      @pointermove.stop="moveDrag"
      @pointerup.stop="endDrag"
      @pointercancel.stop="endDrag"
    >
      <span class="wb-card-kind">{{ kindLabel }}</span>
      <div class="wb-card-tools">
        <button
          v-if="item.kind === 'link'"
          type="button"
          class="wb-card-tool"
          title="打开链接"
          @pointerdown.stop
          @click.stop="emit('openLink')"
        >
          <ArrowUpRight :size="12" />
        </button>
        <button
          v-if="item.kind === 'sticky'"
          type="button"
          class="wb-card-tool"
          title="换颜色"
          @pointerdown.stop
          @click.stop="emit('cycleColor')"
        >
          <Palette :size="12" />
        </button>
        <button
          type="button"
          class="wb-card-tool"
          title="删除"
          @pointerdown.stop
          @click.stop="emit('remove')"
        >
          <X :size="12" />
        </button>
      </div>
    </header>

    <div v-if="item.kind === 'sticky'" class="wb-sticky-body">
      <textarea
        v-if="editing || !item.text"
        ref="textarea"
        class="wb-sticky-text"
        :value="item.text"
        placeholder="写点什么…"
        spellcheck="false"
        @pointerdown.stop="stop"
        @input="onText"
        @blur="finishEdit"
      />
      <div
        v-else
        ref="stickyView"
        class="wb-sticky-view"
        title="双击编辑"
        @pointerdown.stop="stop"
        @click.stop="onStickyClick"
        @dblclick.stop="onStickyDoubleClick"
        v-html="stickyHtml"
      />
    </div>

    <div v-else-if="item.kind === 'todo'" class="wb-todo-body">
      <label class="wb-check" @pointerdown.stop>
        <input type="checkbox" :checked="item.checked" @change="onChecked" />
        <span class="wb-check-mark"><Check :size="11" /></span>
      </label>
      <textarea
        ref="todoInput"
        class="wb-todo-text"
        :class="{ 'is-checked': item.checked }"
        :value="item.text"
        rows="2"
        placeholder="要做的事…"
        @pointerdown.stop="stop"
        @input="onText"
        @blur="emit('commit')"
      />
    </div>

    <div v-else-if="item.kind === 'link'" class="wb-link-body">
      <div class="wb-link-domain">{{ domain || '链接' }}</div>
      <input
        class="wb-link-title"
        :value="item.title"
        placeholder="链接标题"
        @pointerdown.stop="stop"
        @input="onTitle"
        @blur="emit('commit')"
      />
      <p class="wb-link-desc">
        {{ item.text || (linkLoading ? '正在读取链接…' : '没有摘要') }}
      </p>
      <button type="button" class="wb-open-link" @pointerdown.stop @click.stop="emit('openLink')">
        打开链接 <ArrowUpRight :size="12" />
      </button>
    </div>

    <div v-else-if="item.kind === 'image'" class="wb-image-body">
      <div class="wb-image-frame">
        <img v-if="assetUrl" :src="assetUrl" alt="" draggable="false" />
        <div v-else class="wb-image-placeholder">图片同步中…</div>
      </div>
      <div class="wb-image-actions">
        <button
          type="button"
          class="wb-image-action"
          title="反查这张图的来源（会先把图片上传到 360 识图）"
          @pointerdown.stop
          @click.stop="emit('lookupImage', 'source')"
        >
          <ScanSearch :size="12" /> 查来源
        </button>
        <button
          type="button"
          class="wb-image-action"
          title="查找相似图片（会上传到 360 识图）"
          @pointerdown.stop
          @click.stop="emit('lookupImage', 'similar')"
        >
          <Images :size="12" /> 搜相似
        </button>
      </div>
    </div>

    <div v-else class="wb-agent-body">
      <div class="wb-agent-status" :class="`is-${agentStatus}`">
        <span class="wb-agent-dot" />
        <b>{{ agentLabel }}</b>
        <span class="wb-agent-progress">{{ agentProgress }}%</span>
      </div>
      <p class="wb-agent-step">
        {{ agentState?.currentStep || item.text || '正在准备研究…' }}
      </p>
      <div v-if="item.jobId" class="wb-agent-actions">
        <button
          v-if="agentStatus === 'done' && (agentState?.noteId || item.noteId)"
          type="button"
          class="wb-note-btn"
          @pointerdown.stop
          @click.stop="emit('openNote')"
        >
          打开笔记 <ArrowUpRight :size="12" />
        </button>
        <button
          type="button"
          class="wb-note-btn is-quiet"
          @pointerdown.stop
          @click.stop="emit('openResearch')"
        >
          {{ agentStatus === 'done' ? '研究过程' : '查看进度' }}
        </button>
      </div>
      <p v-else-if="agentState?.error" class="wb-agent-error">{{ agentState.error }}</p>
    </div>

    <span
      class="wb-resize"
      title="调整大小"
      @pointerdown.stop="startResize"
      @pointermove.stop="moveResize"
      @pointerup.stop="endResize"
      @pointercancel.stop="endResize"
    />
  </article>
</template>
