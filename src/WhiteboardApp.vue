<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue'
import { getCurrentWebview } from '@tauri-apps/api/webview'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { open } from '@tauri-apps/plugin-dialog'
import {
  ImagePlus,
  ListTodo,
  Minus,
  Pin,
  PinOff,
  Plus,
  RotateCcw,
  Sparkles,
  Trash2,
  X,
} from 'lucide-vue-next'

import AgentSheet from './components/whiteboard/AgentSheet.vue'
import BoardCanvas from './components/whiteboard/BoardCanvas.vue'
import ImageLookupSheet from './components/whiteboard/ImageLookupSheet.vue'
import * as api from './lib/api'
import * as wb from './stores/whiteboard'

const quickText = ref('')
const quickInput = ref<HTMLInputElement | null>(null)
const pinned = ref(false)
const researchOpen = ref(false)
const researchBusy = ref(false)
const researchItems = ref<api.WhiteboardItem[]>([])
const researchJobId = ref('')
const researchError = ref('')
const imageLookup = ref<{ item: api.WhiteboardItem; mode: 'source' | 'similar' } | null>(null)
const undoVisible = ref(false)
const dismissedError = ref('')
let undoTimer = 0
let unlistenDrop: (() => void) | null = null

const selection = computed(() => wb.selectedItems.value)
const zoomLabel = computed(() => `${Math.round(wb.view.scale * 100)}%`)

function isEditable(target: EventTarget | null): boolean {
  const element = target as HTMLElement | null
  if (!element) return false
  return (
    element.tagName === 'INPUT' ||
    element.tagName === 'TEXTAREA' ||
    element.isContentEditable ||
    !!element.closest('input, textarea, [contenteditable="true"]')
  )
}

function overlaps(x: number, y: number, w: number, h: number): boolean {
  return wb.items.value.some(
    (item) =>
      x < item.x + item.w + 12 &&
      x + w + 12 > item.x &&
      y < item.y + item.h + 12 &&
      y + h + 12 > item.y,
  )
}

function freePosition(w: number, h: number): { x: number; y: number } {
  const canvas = document.querySelector('.wb-canvas') as HTMLElement | null
  const rect = canvas?.getBoundingClientRect()
  const width = rect?.width || window.innerWidth
  const height = rect?.height || window.innerHeight
  const left = -wb.view.x / wb.view.scale
  const top = -wb.view.y / wb.view.scale
  const right = (width - wb.view.x) / wb.view.scale
  // Leave room for the selection bar and zoom pill; a freshly created card is
  // selected, and the bar would otherwise cover its bottom edge.
  const bottom = (height - wb.view.y) / wb.view.scale - 64
  const centerX = (width / 2 - wb.view.x) / wb.view.scale
  const centerY = (height / 2 - wb.view.y) / wb.view.scale
  const baseX = centerX - w / 2
  const baseY = centerY - h / 2
  const minX = left + 12
  const maxX = Math.max(minX, right - w - 12)
  const minY = top + 12
  const maxY = Math.max(minY, bottom - h - 12)
  const seen = new Set<string>()
  for (let radius = 0; radius < 10; radius += 1) {
    for (let row = -radius; row <= radius; row += 1) {
      for (let col = -radius; col <= radius; col += 1) {
        if (Math.max(Math.abs(row), Math.abs(col)) !== radius) continue
        const x = Math.min(Math.max(baseX + col * (w + 16), minX), maxX)
        const y = Math.min(Math.max(baseY + row * (h + 16), minY), maxY)
        const key = `${Math.round(x)},${Math.round(y)}`
        if (seen.has(key)) continue
        seen.add(key)
        if (!overlaps(x, y, w, h)) return { x, y }
      }
    }
  }
  return { x: Math.min(Math.max(baseX, minX), maxX), y: Math.min(Math.max(baseY, minY), maxY) }
}

async function createSticky(text: string): Promise<void> {
  const position = freePosition(270, 190)
  const item = await wb.createSticky(text, position.x, position.y)
  if (item && !text) quickInput.value?.focus()
}

async function createTodo(text: string): Promise<void> {
  const position = freePosition(280, 110)
  await wb.createTodo(text, position.x, position.y)
}

async function createLink(url: string, title = ''): Promise<void> {
  const position = freePosition(300, 170)
  await wb.createLink(url, position.x, position.y, title)
}

async function submitQuick(): Promise<void> {
  const value = quickText.value.trim()
  if (!value) return
  quickText.value = ''
  try {
    const link = wb.extractLink(value)
    if (link && value.length <= 600) await createLink(link.url, link.title)
    else await createSticky(value)
  } catch (cause) {
    wb.error.value = api.errorText(cause)
  }
}

async function pickImage(): Promise<void> {
  try {
    const picked = await open({
      multiple: false,
      directory: false,
      filters: [{ name: '图片', extensions: ['png', 'jpg', 'jpeg', 'webp', 'gif'] }],
    })
    if (typeof picked !== 'string') return
    const position = freePosition(260, 260)
    await wb.importImagePath(picked, position.x, position.y)
  } catch (cause) {
    wb.error.value = api.errorText(cause)
  }
}

function readFileAsDataUrl(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader()
    reader.onload = () => resolve(String(reader.result || ''))
    reader.onerror = () => reject(reader.error || new Error('读取图片失败'))
    reader.readAsDataURL(file)
  })
}

async function handlePaste(event: ClipboardEvent): Promise<void> {
  if (isEditable(event.target)) return
  const clipboard = event.clipboardData
  if (!clipboard) return
  const images = Array.from(clipboard.files).filter((file) => file.type.startsWith('image/'))
  if (!images.length) {
    for (const item of Array.from(clipboard.items)) {
      if (item.kind !== 'file' || !item.type.startsWith('image/')) continue
      const file = item.getAsFile()
      if (file) images.push(file)
    }
  }
  if (images.length) {
    event.preventDefault()
    for (const file of images) {
      try {
        const data = await readFileAsDataUrl(file)
        const position = freePosition(260, 260)
        await wb.saveImageData(data, position.x, position.y)
      } catch (cause) {
        wb.error.value = api.errorText(cause)
      }
    }
    return
  }
  const text = clipboard.getData('text/plain').trim()
  if (!text) return
  event.preventDefault()
  try {
    const link = wb.extractLink(text)
    if (link && text.length <= 600) await createLink(link.url, link.title)
    else await createSticky(text)
  } catch (cause) {
    wb.error.value = api.errorText(cause)
  }
}

function isImagePath(path: string): boolean {
  return /\.(png|jpe?g|webp|gif)$/i.test(path)
}

function baseName(path: string): string {
  return path.split(/[\\/]/).pop() || path
}

async function handleDropPaths(paths: string[]): Promise<void> {
  for (const path of paths) {
    try {
      if (isImagePath(path)) {
        const position = freePosition(260, 260)
        await wb.importImagePath(path, position.x, position.y)
      } else {
        const position = freePosition(270, 190)
        await wb.createSticky(`📎 ${baseName(path)}\n${path}`, position.x, position.y)
      }
    } catch (cause) {
      wb.error.value = api.errorText(cause)
    }
  }
}

function onQuickEnter(event: KeyboardEvent): void {
  if (event.shiftKey) return
  event.preventDefault()
  void submitQuick()
}

function createAt(x: number, y: number): void {
  void wb.createSticky('', x, y).catch((cause) => {
    wb.error.value = api.errorText(cause)
  })
}

function onSelection(id: string, additive: boolean): void {
  wb.select(id, additive)
  const item = wb.items.value.find((entry) => entry.id === id)
  if (item) wb.bringToFront(item)
}

function onPatch(item: api.WhiteboardItem, values: Partial<api.WhiteboardItem>): void {
  wb.patch(item, values)
}

function onCommit(item: api.WhiteboardItem): void {
  void wb.saveNow(item)
}

function onLookupImage(item: api.WhiteboardItem, mode: 'source' | 'similar'): void {
  imageLookup.value = { item, mode }
}

async function removeItem(item: api.WhiteboardItem): Promise<void> {
  await wb.removeItem(item.id)
  undoVisible.value = true
  window.clearTimeout(undoTimer)
  undoTimer = window.setTimeout(() => {
    undoVisible.value = false
    wb.lastDeleted.value = null
  }, 6000)
}

async function removeSelected(): Promise<void> {
  const ids = [...wb.selectedIds.value]
  for (const id of ids) {
    const item = wb.items.value.find((entry) => entry.id === id)
    if (item) await removeItem(item)
  }
  wb.clearSelection()
}

async function undo(): Promise<void> {
  await wb.undoDelete()
  undoVisible.value = false
  window.clearTimeout(undoTimer)
}

async function startResearch(prompt: string): Promise<void> {
  researchBusy.value = true
  researchError.value = ''
  try {
    const result = await wb.startResearch(prompt)
    if (result) {
      researchJobId.value = result.jobId
      researchOpen.value = true
      undoVisible.value = false
    }
  } catch (cause) {
    researchError.value = api.errorText(cause)
  } finally {
    researchBusy.value = false
  }
}

function openResearchCompose(): void {
  researchItems.value = [...selection.value]
  researchJobId.value = ''
  imageLookup.value = null
  researchOpen.value = true
}

function resetResearch(): void {
  researchJobId.value = ''
  researchOpen.value = true
}

function openResearchJob(item: api.WhiteboardItem): void {
  if (!item.jobId) return
  researchItems.value = []
  researchJobId.value = item.jobId
  researchOpen.value = true
}

function zoomBy(delta: number): void {
  wb.view.scale = Math.min(2, Math.max(0.4, wb.view.scale + delta))
  wb.saveView()
}

async function togglePin(): Promise<void> {
  pinned.value = !pinned.value
  try {
    await getCurrentWindow().setAlwaysOnTop(pinned.value)
    localStorage.setItem('yebian.whiteboard.pinned', pinned.value ? '1' : '0')
  } catch (cause) {
    pinned.value = !pinned.value
    wb.error.value = api.errorText(cause)
  }
}

async function minimize(): Promise<void> {
  await getCurrentWindow().minimize()
}

async function closeWindow(): Promise<void> {
  await getCurrentWindow().close()
}

function onGlobalKeydown(event: KeyboardEvent): void {
  if (event.key === 'Escape') {
    if (imageLookup.value) {
      imageLookup.value = null
      return
    }
    if (researchOpen.value) {
      researchOpen.value = false
      return
    }
    if (wb.selectedIds.value.length) {
      wb.clearSelection()
      return
    }
  }
  if ((event.ctrlKey || event.metaKey) && event.key === '0') {
    event.preventDefault()
    wb.resetView()
    wb.saveView()
    return
  }
  if (isEditable(event.target)) return
  if ((event.key === 'Delete' || event.key === 'Backspace') && wb.selectedIds.value.length) {
    event.preventDefault()
    void removeSelected()
    return
  }
  if (event.key === 'n' || event.key === 'N') {
    event.preventDefault()
    quickInput.value?.focus()
  }
}

onMounted(async () => {
  wb.loadView()
  try {
    pinned.value = (await getCurrentWindow().isAlwaysOnTop()) || localStorage.getItem('yebian.whiteboard.pinned') === '1'
    if (localStorage.getItem('yebian.whiteboard.pinned') === '1' && !pinned.value) {
      await getCurrentWindow().setAlwaysOnTop(true)
      pinned.value = true
    }
  } catch {
    pinned.value = localStorage.getItem('yebian.whiteboard.pinned') === '1'
  }
  await wb.load()
  window.addEventListener('keydown', onGlobalKeydown)
  window.addEventListener('paste', handlePaste)
  try {
    unlistenDrop = await getCurrentWebview().onDragDropEvent((event) => {
      if (event.payload.type !== 'drop') return
      void handleDropPaths(event.payload.paths || [])
    })
  } catch {
    unlistenDrop = null
  }
})

onUnmounted(() => {
  window.removeEventListener('keydown', onGlobalKeydown)
  window.removeEventListener('paste', handlePaste)
  window.clearTimeout(undoTimer)
  unlistenDrop?.()
})
</script>

<template>
  <div class="wb-shell">
    <header class="wb-header" data-tauri-drag-region>
      <div class="wb-title" data-tauri-drag-region>
        <span>白板</span>
        <em>收集 · 研究</em>
      </div>
      <div class="wb-window-actions">
        <button
          type="button"
          class="wb-win-btn"
          :class="{ 'is-active': pinned }"
          :title="pinned ? '取消置顶' : '置顶'"
          @click="togglePin"
        >
          <PinOff v-if="pinned" :size="13" />
          <Pin v-else :size="13" />
        </button>
        <button type="button" class="wb-win-btn" title="最小化" @click="minimize">
          <Minus :size="14" />
        </button>
        <button type="button" class="wb-win-btn is-close" title="关闭" @click="closeWindow">
          <X :size="14" />
        </button>
      </div>
    </header>

    <div class="wb-quick">
      <input
        ref="quickInput"
        v-model="quickText"
        class="wb-quick-input"
        placeholder="记一条，或粘贴链接 / 图片"
        autocomplete="off"
        spellcheck="false"
        @keydown.enter="onQuickEnter"
      />
      <button type="button" class="wb-quick-btn" title="添加图片" @click="pickImage">
        <ImagePlus :size="15" />
      </button>
      <button
        type="button"
        class="wb-quick-btn"
        title="添加待办"
        @click="createTodo(quickText.trim()); quickText = ''"
      >
        <ListTodo :size="15" />
      </button>
    </div>

    <main class="wb-board-wrap">
      <BoardCanvas
        :items="wb.items.value"
        :selected-ids="wb.selectedIds.value"
        :asset-urls="wb.assetUrls"
        :agent-states="wb.agentStates"
        :link-loading="wb.linkLoading"
        @select="onSelection"
        @clear-selection="wb.clearSelection()"
        @create-sticky="createAt"
        @patch="onPatch"
        @commit="onCommit"
        @remove="removeItem"
        @open-link="(item, url) => api.openExternalUrl(url || item.url)"
        @open-note="(item) => wb.openNote(item.noteId)"
        @open-research="openResearchJob"
        @lookup-image="onLookupImage"
        @cycle-color="(item) => wb.cycleStickyColor(item)"
      />

      <div v-if="!wb.loading.value && !wb.items.value.length" class="wb-empty">
        <div class="wb-empty-mark">白板</div>
        <p>把要做的事、点子、截图、链接先丢进来。</p>
        <span>双击空白处新建便签 · 直接 Ctrl+V 粘贴图片或链接 · 拖图片进来也可以</span>
      </div>

      <div class="wb-zoom">
        <button type="button" title="缩小" @click="zoomBy(-0.1)"><Minus :size="13" /></button>
        <span>{{ zoomLabel }}</span>
        <button type="button" title="放大" @click="zoomBy(0.1)"><Plus :size="13" /></button>
        <button type="button" title="回到默认视图" @click="wb.resetView(); wb.saveView()">
          <RotateCcw :size="12" />
        </button>
      </div>

      <Transition name="wb-bar">
        <div v-if="selection.length" class="wb-selection-bar">
          <span class="wb-selection-count">{{ selection.length }} 张</span>
          <button type="button" class="wb-btn is-primary" @click="openResearchCompose">
            <Sparkles :size="13" /> 研究这几张
          </button>
          <button type="button" class="wb-btn is-quiet" title="删除" @click="removeSelected">
            <Trash2 :size="13" /> 删除
          </button>
          <button type="button" class="wb-btn is-quiet" title="取消选择" @click="wb.clearSelection()">
            <X :size="13" />
          </button>
        </div>
      </Transition>

      <ImageLookupSheet
        :visible="!!imageLookup"
        :item="imageLookup?.item || null"
        :mode="imageLookup?.mode || 'source'"
        :asset-url="imageLookup ? wb.assetUrls[imageLookup.item.assetPath] || '' : ''"
        @close="imageLookup = null"
      />
      <AgentSheet
        :visible="researchOpen"
        :items="researchItems"
        :busy="researchBusy"
        :job-id="researchJobId"
        @close="researchOpen = false"
        @start="startResearch"
        @reset="resetResearch"
      />
      <p v-if="researchError" class="wb-error">
        {{ researchError }}
        <button type="button" @click="researchError = ''">关闭</button>
      </p>
      <p v-if="wb.error.value && dismissedError !== wb.error.value" class="wb-error">
        {{ wb.error.value }}
        <button type="button" @click="dismissedError = wb.error.value">关闭</button>
      </p>

      <Transition name="wb-toast">
        <div v-if="undoVisible" class="wb-toast">
          已删除
          <button type="button" @click="undo">撤销</button>
        </div>
      </Transition>
    </main>
  </div>
</template>
