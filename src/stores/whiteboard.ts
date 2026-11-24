import { computed, reactive, ref } from 'vue'

import * as api from '../lib/api'

export type WhiteboardItem = api.WhiteboardItem
export type WhiteboardItemInput = api.WhiteboardItemInput

export interface AgentState {
  status: 'queued' | 'running' | 'done' | 'failed'
  progress: number
  currentStep: string
  error: string
  noteId: string
}

export const board = ref<api.WhiteboardBoard | null>(null)
export const items = ref<WhiteboardItem[]>([])
export const loading = ref(true)
export const error = ref('')
export const selectedIds = ref<string[]>([])
export const agentStates = reactive<Record<string, AgentState>>({})
export const assetUrls = reactive<Record<string, string>>({})
export const linkLoading = reactive<Record<string, boolean>>({})
export const view = reactive({ x: 80, y: 80, scale: 1 })
export const lastDeleted = ref<{ item: WhiteboardItem; until: number } | null>(null)

const saveTimers = new Map<string, number>()
let pollTimer = 0

export const selectedItems = computed(() =>
  items.value.filter((item) => selectedIds.value.includes(item.id)),
)

export function resetView(): void {
  view.x = 80
  view.y = 80
  view.scale = 1
}

export function loadView(): void {
  try {
    const raw = localStorage.getItem('yebian.whiteboard.view')
    if (!raw) return
    const saved = JSON.parse(raw) as { x?: number; y?: number; scale?: number }
    if (typeof saved.x === 'number') view.x = saved.x
    if (typeof saved.y === 'number') view.y = saved.y
    if (typeof saved.scale === 'number') view.scale = Math.min(2, Math.max(0.4, saved.scale))
  } catch {
    // A broken local view is harmless; the default is fine.
  }
}

export function saveView(): void {
  try {
    localStorage.setItem(
      'yebian.whiteboard.view',
      JSON.stringify({ x: view.x, y: view.y, scale: view.scale }),
    )
  } catch {
    // localStorage can be unavailable; the board still works.
  }
}

export async function load(): Promise<void> {
  loading.value = true
  error.value = ''
  try {
    const current = await api.whiteboardBoard()
    board.value = current
    items.value = await api.whiteboardItems(current.id)
    for (const item of items.value) {
      if (item.assetPath) void ensureAsset(item.assetPath)
      if (item.jobId) {
        agentStates[item.jobId] = {
          status: 'running',
          progress: 0,
          currentStep: '',
          error: '',
          noteId: '',
        }
        void pollJob(item.jobId)
      }
    }
    startPolling()
  } catch (cause) {
    error.value = api.errorText(cause)
  } finally {
    loading.value = false
  }
}

export function select(id: string, additive = false): void {
  if (additive) {
    selectedIds.value = selectedIds.value.includes(id)
      ? selectedIds.value.filter((value) => value !== id)
      : [...selectedIds.value, id]
  } else {
    selectedIds.value = [id]
  }
}

export function clearSelection(): void {
  selectedIds.value = []
}

export function bringToFront(item: WhiteboardItem): void {
  const top = items.value.reduce((max, entry) => Math.max(max, entry.z), 0)
  if (item.z >= top) return
  item.z = top + 1
  scheduleSave(item)
}

export async function createSticky(text: string, x: number, y: number): Promise<WhiteboardItem | null> {
  if (!board.value) return null
  const item = await api.whiteboardSaveItem({
    boardId: board.value.id,
    kind: 'sticky',
    x,
    y,
    w: 270,
    h: 190,
    z: nextZ(),
    text,
    color: 'plain',
    dataJson: '{}',
  })
  items.value.push(item)
  select(item.id)
  return item
}

export async function createTodo(text: string, x: number, y: number): Promise<WhiteboardItem | null> {
  if (!board.value) return null
  const item = await api.whiteboardSaveItem({
    boardId: board.value.id,
    kind: 'todo',
    x,
    y,
    w: 280,
    h: 110,
    z: nextZ(),
    text,
    checked: false,
    dataJson: '{}',
  })
  items.value.push(item)
  select(item.id)
  return item
}

export async function createLink(
  url: string,
  x: number,
  y: number,
  providedTitle = '',
): Promise<WhiteboardItem | null> {
  if (!board.value) return null
  const domain = domainOf(url)
  const initialTitle = providedTitle.trim() || domain || '链接'
  const item = await api.whiteboardSaveItem({
    boardId: board.value.id,
    kind: 'link',
    x,
    y,
    w: 300,
    h: 170,
    z: nextZ(),
    url,
    title: initialTitle,
    text: '',
    dataJson: JSON.stringify({ domain }),
  })
  items.value.push(item)
  select(item.id)
  linkLoading[item.id] = true
  api
    .whiteboardLinkPreview(url)
    .then((preview) => {
      const previewTitle = preview.title.trim()
      const title =
        previewTitle && previewTitle !== preview.domain
          ? previewTitle
          : initialTitle
      Object.assign(item, {
        title,
        text: preview.description,
        url: preview.url || url,
        dataJson: JSON.stringify({ domain: preview.domain || domain }),
      })
      scheduleSave(item)
    })
    .catch(() => undefined)
    .finally(() => {
      delete linkLoading[item.id]
    })
  return item
}

export async function importImagePath(path: string, x: number, y: number): Promise<WhiteboardItem | null> {
  if (!board.value) return null
  const image = await api.whiteboardImportImage(path)
  return createImageItem(image, x, y)
}

export async function saveImageData(dataUrl: string, x: number, y: number): Promise<WhiteboardItem | null> {
  if (!board.value) return null
  const ext = dataUrl.startsWith('data:image/png') ? 'png' : dataUrl.startsWith('data:image/webp') ? 'webp' : 'jpg'
  const image = await api.whiteboardSaveImage(dataUrl, ext)
  return createImageItem(image, x, y)
}

async function createImageItem(
  image: api.ImportedImage,
  x: number,
  y: number,
): Promise<WhiteboardItem | null> {
  if (!board.value) return null
  const size = await imageSize(image.dataUrl)
  const naturalWidth = size.width || 512
  const naturalHeight = size.height || 512
  let width = Math.min(320, Math.max(180, naturalWidth))
  let height = (width / naturalWidth) * naturalHeight
  if (height > 360) {
    height = 360
    width = (height / naturalHeight) * naturalWidth
  }
  width = Math.max(160, width)
  const item = await api.whiteboardSaveItem({
    boardId: board.value.id,
    kind: 'image',
    x,
    y,
    w: width,
    h: Math.max(160, height + 23),
    z: nextZ(),
    assetPath: image.path,
    text: '',
    dataJson: JSON.stringify({ width: size.width, height: size.height }),
  })
  items.value.push(item)
  assetUrls[image.path] = image.dataUrl
  select(item.id)
  return item
}

export function patch(item: WhiteboardItem, values: Partial<WhiteboardItem>, save = true): void {
  Object.assign(item, values)
  if (save) scheduleSave(item)
}

export function scheduleSave(item: WhiteboardItem, delay = 450): void {
  const existing = saveTimers.get(item.id)
  if (existing) window.clearTimeout(existing)
  saveTimers.set(
    item.id,
    window.setTimeout(() => {
      saveTimers.delete(item.id)
      void saveNow(item)
    }, delay),
  )
}

export async function saveNow(item: WhiteboardItem): Promise<void> {
  const timer = saveTimers.get(item.id)
  if (timer) {
    window.clearTimeout(timer)
    saveTimers.delete(item.id)
  }
  try {
    const saved = await api.whiteboardSaveItem(toInput(item))
    Object.assign(item, saved)
  } catch (cause) {
    error.value = api.errorText(cause)
  }
}

function toInput(item: WhiteboardItem): WhiteboardItemInput {
  return {
    id: item.id,
    boardId: item.boardId,
    kind: item.kind,
    x: item.x,
    y: item.y,
    w: item.w,
    h: item.h,
    z: item.z,
    text: item.text,
    title: item.title,
    url: item.url,
    assetPath: item.assetPath,
    color: item.color,
    checked: item.checked,
    noteId: item.noteId,
    jobId: item.jobId,
    dataJson: item.dataJson,
  }
}

export async function removeItem(id: string): Promise<void> {
  const index = items.value.findIndex((item) => item.id === id)
  if (index < 0) return
  const [item] = items.value.splice(index, 1)
  selectedIds.value = selectedIds.value.filter((value) => value !== id)
  lastDeleted.value = { item, until: Date.now() + 6000 }
  try {
    await api.whiteboardDeleteItem(id)
  } catch (cause) {
    items.value.splice(index, 0, item)
    error.value = api.errorText(cause)
  }
}

export async function undoDelete(): Promise<void> {
  const deleted = lastDeleted.value
  if (!deleted) return
  lastDeleted.value = null
  try {
    const item = await api.whiteboardSaveItem(toInput(deleted.item))
    items.value.push(item)
    select(item.id)
  } catch (cause) {
    error.value = api.errorText(cause)
  }
}

export function cycleStickyColor(item: WhiteboardItem): void {
  const colors = ['plain', 'blue', 'green']
  const index = colors.indexOf(item.color)
  patch(item, { color: colors[(index + 1) % colors.length] })
}

export async function ensureAsset(path: string): Promise<string> {
  if (assetUrls[path]) return assetUrls[path]
  try {
    const data = await api.whiteboardAssetData(path)
    assetUrls[path] = data
    return data
  } catch {
    return ''
  }
}

export async function startResearch(prompt: string): Promise<api.WhiteboardResearchStart | null> {
  if (!board.value) return null
  const selected = selectedItems.value.filter((item) => item.kind !== 'agent')
  if (!selected.length) return null
  const result = await api.whiteboardResearch(
    board.value.id,
    selected.map((item) => item.id),
    prompt,
  )
  items.value.push(result.item)
  agentStates[result.jobId] = {
    status: 'queued',
    progress: 0,
    currentStep: '',
    error: '',
    noteId: '',
  }
  selectedIds.value = []
  startPolling()
  void pollJob(result.jobId)
  return result
}

function startPolling(): void {
  if (pollTimer) return
  pollTimer = window.setInterval(() => {
    for (const item of items.value) {
      if (!item.jobId) continue
      const state = agentStates[item.jobId]
      if (state?.status === 'done' || state?.status === 'failed') continue
      void pollJob(item.jobId)
    }
  }, 1200)
}

export async function pollJob(jobId: string): Promise<void> {
  try {
    const detail = await api.productionGet(jobId)
    if (!detail) {
      agentStates[jobId] = {
        status: 'failed',
        progress: 0,
        currentStep: '',
        error: '任务不存在',
        noteId: '',
      }
      return
    }
    const job = detail.job
    const status: AgentState['status'] =
      job.status === 'done' || job.status === 'success'
        ? 'done'
        : job.status === 'failed' || job.status === 'cancelled'
          ? 'failed'
          : job.status === 'queued'
            ? 'queued'
            : 'running'
    agentStates[jobId] = {
      status,
      progress: job.progress || 0,
      currentStep: job.currentStep || '',
      error: job.error || '',
      noteId: job.noteId || '',
    }
    if (job.noteId) {
      const item = items.value.find((entry) => entry.jobId === jobId)
      if (item && item.noteId !== job.noteId) {
        patch(item, { noteId: job.noteId, title: job.title || item.title })
      }
    }
  } catch {
    // A transient read failure should not turn a running job red.
  }
}

export async function openNote(noteId: string): Promise<void> {
  if (!noteId) return
  await api.whiteboardOpenNote(noteId)
}

export function domainOf(url: string): string {
  try {
    return new URL(url).hostname.replace(/^www\./, '')
  } catch {
    return ''
  }
}

export interface ExtractedLink {
  url: string
  domain: string
  title: string
}

/** 从便签/粘贴文本里取出第一个 http(s) 链接，保留前后的文字作为标题。 */
export function extractLink(text: string): ExtractedLink | null {
  const match = text.match(/https?:\/\/[^\s<>"']+/i)
  if (!match || match.index === undefined) return null
  const raw = match[0].replace(/[),.;!?，。；、！？）】》]+$/, '')
  const before = text.slice(0, match.index).trim()
  const after = text.slice(match.index + match[0].length).trim()
  const title = (before || after).replace(/\s+/g, ' ').slice(0, 240)
  return {
    url: raw,
    domain: domainOf(raw),
    title,
  }
}

export function nextZ(): number {
  return items.value.reduce((max, item) => Math.max(max, item.z), 0) + 1
}

function imageSize(dataUrl: string): Promise<{ width: number; height: number }> {
  return new Promise((resolve) => {
    const image = new Image()
    image.onload = () => resolve({ width: image.naturalWidth, height: image.naturalHeight })
    image.onerror = () => resolve({ width: 0, height: 0 })
    image.src = dataUrl
  })
}
