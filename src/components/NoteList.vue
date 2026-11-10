<script setup lang="ts">
import type { ComponentPublicInstance } from 'vue'
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useRouter } from 'vue-router'

import * as api from '../lib/api'
import {
  SORT_HINT,
  SORT_LABEL,
  SORT_MODES,
  ancestorPaths,
  buildTree,
  flattenTree,
  folderPaths,
  type SortMode,
  type TreeRow,
} from '../lib/note-tree'
import { library, refreshNotes } from '../stores/library'
import type { NoteMeta } from '../types'

const props = defineProps<{
  notes: NoteMeta[]
  activeId?: string
}>()

const router = useRouter()

const COLLAPSED_KEY = 'yebian.sidebar.collapsed'
const PINNED_KEY = 'yebian.sidebar.pinned'
const SORT_KEY = 'yebian.sidebar.sort'

type Target = { kind: 'note' | 'folder' | 'root'; relPath: string; name: string; noteId?: string }

function readJson<T>(key: string, fallback: T): T {
  try {
    const raw = localStorage.getItem(key)
    return raw ? (JSON.parse(raw) as T) : fallback
  } catch {
    return fallback
  }
}

function persist(key: string, value: unknown): void {
  try {
    localStorage.setItem(key, JSON.stringify(value))
  } catch {
    /* storage unavailable: the choice just won't survive a reload */
  }
}

const collapsed = ref(new Set<string>(readJson<string[]>(COLLAPSED_KEY, [])))
const pinned = ref<string[]>(readJson<string[]>(PINNED_KEY, []))
const sort = ref<SortMode>(
  SORT_MODES.includes(readJson<SortMode>(SORT_KEY, 'time')) ? readJson<SortMode>(SORT_KEY, 'time') : 'time',
)
const query = ref('')
const searchInput = ref<HTMLInputElement | null>(null)
const searchResults = ref<NoteMeta[]>([])
const searchLoading = ref(false)
let searchTimer: number | undefined
const treeRef = ref<HTMLElement | null>(null)
const treeScrollRef = ref<HTMLElement | null>(null)
const focusedKey = ref('')
const folders = ref<string[]>([])

const menu = ref<{ x: number; y: number; target: Target } | null>(null)
const renaming = ref<{ relPath: string; kind: 'note' | 'folder' } | null>(null)
const renameValue = ref('')
const renameInput = ref<HTMLInputElement | null>(null)

function setRenameInput(el: Element | ComponentPublicInstance | null): void {
  renameInput.value = (el as HTMLInputElement | null) ?? null
}
const drag = ref<{ relPath: string; isDir: boolean } | null>(null)
const dropTarget = ref<string | null>(null)
const pendingDelete = ref<Target | null>(null)
const moving = ref<Target | null>(null)
const toast = ref<{ text: string; undo?: () => void } | null>(null)
let toastTimer: number | undefined
let menuOpen = false
let menuReturnKey = ''
let dragExpandTimer: number | undefined
let dragExpandPath = ''
let typeahead = ''
let typeaheadTimer: number | undefined

const indexStatus = computed(() => library.index)
const indexPct = computed(() => {
  const status = library.index
  if (!status || !status.total) return null
  return Math.min(100, Math.round((status.done / status.total) * 100))
})
const indexVisible = computed(() => !!library.index && library.index.state !== 'idle')
const indexLabel = computed(() => {
  const status = library.index
  if (!status) return ''
  if (status.state === 'error') return status.error || '索引失败'
  if (status.state === 'writing') {
    return `正在写入索引… ${status.done.toLocaleString()} / ${status.total.toLocaleString()}`
  }
  if (status.state === 'committing') return '正在切换索引…'
  if (status.total > 0) return `${status.done.toLocaleString()} / ${status.total.toLocaleString()}`
  return '正在整理…'
})
const filtering = computed(() => query.value.trim().length > 0)
const tree = computed(() => buildTree(props.notes, sort.value, folders.value))
// While searching the tree is replaced by the result list; clearing the box
// restores whatever the user had folded away.
const rows = computed(() =>
  flattenTree(tree.value, filtering.value ? new Set<string>() : collapsed.value),
)

// The tree can hold 10k rows; only the rows near the viewport are mounted.
// Heights must match sidebar.css exactly, or the scrollbar will jump.
const FOLDER_ROW_H = 30
const NOTE_ROW_H = 30
const EMPTY_ROW_H = 26
const TREE_OVERSCAN = 480
type EmptyRow = { key: string; depth: number; empty: true }
type DisplayRow = TreeRow | EmptyRow

const displayRows = computed<DisplayRow[]>(() => {
  const out: DisplayRow[] = []
  for (const row of rows.value) {
    out.push(row)
    if (row.folder && !collapsed.value.has(row.folder.path) && row.folder.count === 0) {
      out.push({ key: `empty:${row.folder.path}`, depth: row.depth + 1, empty: true })
    }
  }
  return out
})

function displayRowHeight(row: DisplayRow): number {
  if ('empty' in row) return EMPTY_ROW_H
  return row.folder ? FOLDER_ROW_H : NOTE_ROW_H
}

const rowOffsets = computed(() => {
  const out: number[] = []
  let top = 0
  for (const row of displayRows.value) {
    out.push(top)
    top += displayRowHeight(row)
  }
  return out
})

const totalHeight = computed(() => {
  const last = displayRows.value.length - 1
  return last < 0 ? 0 : rowOffsets.value[last] + displayRowHeight(displayRows.value[last])
})

const treeScrollTop = ref(0)
const treeViewport = ref(0)

function offsetAt(y: number): number {
  const offsets = rowOffsets.value
  let lo = 0
  let hi = offsets.length - 1
  let found = 0
  while (lo <= hi) {
    const mid = (lo + hi) >> 1
    if (offsets[mid] <= y) {
      found = mid
      lo = mid + 1
    } else {
      hi = mid - 1
    }
  }
  return found
}

const windowStart = computed(() =>
  offsetAt(Math.max(0, treeScrollTop.value - TREE_OVERSCAN)),
)
const windowEnd = computed(() => {
  const rowsAt = displayRows.value
  const offsets = rowOffsets.value
  const bottom = treeScrollTop.value + Math.max(treeViewport.value, 0) + TREE_OVERSCAN
  let index = windowStart.value
  while (index < rowsAt.length && offsets[index] < bottom) index += 1
  return Math.min(rowsAt.length, index + 1)
})
const windowRows = computed(() => displayRows.value.slice(windowStart.value, windowEnd.value))
const windowOffset = computed(() => rowOffsets.value[windowStart.value] ?? 0)

function ensureRowVisible(key: string): void {
  const index = displayRows.value.findIndex((row) => row.key === key)
  const el = treeScrollRef.value
  if (index < 0 || !el) return
  const top = rowOffsets.value[index]
  const bottom = top + displayRowHeight(displayRows.value[index])
  if (top < el.scrollTop) el.scrollTop = top
  else if (bottom > el.scrollTop + el.clientHeight) el.scrollTop = bottom - el.clientHeight
  treeScrollTop.value = el.scrollTop
}

function onTreeScroll(): void {
  const el = treeScrollRef.value
  if (el) treeScrollTop.value = el.scrollTop
}

let treeResizeObserver: ResizeObserver | undefined
const allCollapsed = computed(() => {
  const paths = folderPaths(tree.value)
  return paths.length > 0 && paths.every((path) => collapsed.value.has(path))
})
const pinnedNotes = computed(() => {
  const byId = new Map(props.notes.map((note) => [note.id, note]))
  return pinned.value.map((id) => byId.get(id)).filter((note): note is NoteMeta => Boolean(note))
})
const crumb = computed(() => {
  const note = props.activeId ? props.notes.find((item) => item.id === props.activeId) : undefined
  if (!note) return []
  return ancestorPaths(note.relPath).map((path) => ({
    path,
    name: path.split('/').pop() ?? path,
  }))
})

function indent(depth: number): Record<string, string> {
  return { '--depth': String(depth) }
}

function parentOf(relPath: string): string {
  const index = relPath.lastIndexOf('/')
  return index === -1 ? '' : relPath.slice(0, index)
}

function baseName(relPath: string): string {
  const name = relPath.split('/').pop() ?? relPath
  return name.replace(/\.md$/i, '')
}

// ---------- focus and keyboard ----------

const tabStopKey = computed(() => {
  if (focusedKey.value && rows.value.some((row) => row.key === focusedKey.value)) {
    return focusedKey.value
  }
  const active = rows.value.find((row) => row.note?.id === props.activeId)
  return active?.key ?? rows.value[0]?.key ?? ''
})

// When the roving tab stop is scrolled out of the virtual window, hand the
// single Tab entry point to the first row that is actually mounted.
const treeTabStopKey = computed(() => {
  const stop = tabStopKey.value
  if (windowRows.value.some((row) => row.key === stop)) return stop
  // Start at the first row at/after the viewport top, not the overscan row.
  const start = offsetAt(treeScrollTop.value)
  for (let i = start; i < windowEnd.value; i += 1) {
    const row = displayRows.value[i]
    if (!('empty' in row)) return row.key
  }
  const first = windowRows.value.find((row) => !('empty' in row))
  return first?.key ?? ''
})

function rowLabel(row: TreeRow): string {
  return row.folder?.name ?? row.note?.title ?? ''
}

function targetForRow(row: TreeRow): Target | null {
  if (row.folder) return { kind: 'folder', relPath: row.folder.path, name: row.folder.name }
  if (row.note) {
    return {
      kind: 'note',
      relPath: row.note.relPath,
      name: baseName(row.note.relPath),
      noteId: row.note.id,
    }
  }
  return null
}

function openNote(note: NoteMeta): void {
  void router.push(`/note/${note.id}`)
}

async function focusRow(key: string): Promise<void> {
  ensureRowVisible(key)
  focusedKey.value = key
  await nextTick()
  const el = treeRef.value?.querySelector<HTMLElement>(`[data-key="${CSS.escape(key)}"]`)
  if (!el) return
  el.focus({ preventScroll: true })
  el.scrollIntoView({ block: 'nearest' })
}

async function toggleFolder(path: string, key = `d:${path}`): Promise<void> {
  const wasCollapsed = collapsed.value.has(path)
  toggle(path)
  if (!wasCollapsed && focusedKey.value) {
    await nextTick()
    if (!rows.value.some((row) => row.key === focusedKey.value)) await focusRow(key)
  }
}

function onRowKeydown(event: KeyboardEvent, row: TreeRow): void {
  if (event.isComposing || event.metaKey || event.ctrlKey || event.altKey) return
  const index = rows.value.findIndex((item) => item.key === row.key)
  const focusAt = (next: number): void => {
    const target = rows.value[next]
    if (!target) return
    event.preventDefault()
    void focusRow(target.key)
  }

  switch (event.key) {
    case 'ArrowDown':
      focusAt(index + 1)
      return
    case 'ArrowUp':
      focusAt(index - 1)
      return
    case 'Home':
      focusAt(0)
      return
    case 'End':
      focusAt(rows.value.length - 1)
      return
    case 'ArrowRight':
      if (!row.folder) return
      event.preventDefault()
      if (collapsed.value.has(row.folder.path)) void toggleFolder(row.folder.path, row.key)
      else {
        const child = rows.value[index + 1]
        if (child && child.depth > row.depth) void focusRow(child.key)
      }
      return
    case 'ArrowLeft': {
      event.preventDefault()
      if (row.folder && !collapsed.value.has(row.folder.path)) {
        void toggleFolder(row.folder.path, row.key)
        return
      }
      for (let i = index - 1; i >= 0; i -= 1) {
        if (rows.value[i].depth < row.depth) {
          void focusRow(rows.value[i].key)
          return
        }
      }
      return
    }
    case 'Enter':
      event.preventDefault()
      if (row.folder) void toggleFolder(row.folder.path, row.key)
      else if (row.note) openNote(row.note)
      return
    case ' ':
      if (row.folder) {
        event.preventDefault()
        void toggleFolder(row.folder.path, row.key)
      }
      return
    case 'F2': {
      const target = targetForRow(row)
      if (target) {
        event.preventDefault()
        void startRename(target)
      }
      return
    }
    case 'Delete': {
      const target = targetForRow(row)
      if (target) {
        event.preventDefault()
        remove(target)
      }
      return
    }
    case 'p':
    case 'P':
      if (row.note) {
        event.preventDefault()
        togglePin(row.note.id)
      }
      return
    case 'm':
    case 'M': {
      const target = targetForRow(row)
      if (target) {
        event.preventDefault()
        startMove(target)
      }
      return
    }
    default: {
      if (event.key.length !== 1) return
      window.clearTimeout(typeaheadTimer)
      typeahead += event.key.toLowerCase()
      typeaheadTimer = window.setTimeout(() => {
        typeahead = ''
      }, 600)
      const start = index + 1
      const ordered = [...rows.value.slice(start), ...rows.value.slice(0, start)]
      const found = ordered.find((item) => rowLabel(item).toLowerCase().startsWith(typeahead))
      if (found) {
        event.preventDefault()
        void focusRow(found.key)
      }
    }
  }
}

// ---------- move picker ----------

const moveChoices = computed(() => {
  const target = moving.value
  if (!target) return []
  return ['', ...folderPaths(tree.value)].filter((path) => {
    if (path === parentOf(target.relPath)) return false
    if (target.kind === 'folder') {
      if (path === target.relPath) return false
      if (path.startsWith(`${target.relPath}/`)) return false
    }
    return true
  })
})

async function moveTo(relPath: string, folderPath: string): Promise<void> {
  const previousParent = parentOf(relPath)
  try {
    const report = await api.moveEntry(relPath, folderPath)
    expandAll([folderPath])
    await refreshNotes()
    await loadFolders()
    showToast(
      report.linksRewritten > 0 ? `已移动，更新了 ${report.linksRewritten} 处链接` : '已移动',
      async () => {
        await api.moveEntry(report.relPath, previousParent)
        expandAll([previousParent])
        await refreshNotes()
        await loadFolders()
      },
    )
  } catch (error) {
    showToast(errorText(error))
  }
}

function startMove(target: Target): void {
  menu.value = null
  menuOpen = false
  moving.value = target
}

async function commitMove(folderPath: string): Promise<void> {
  const target = moving.value
  moving.value = null
  if (!target) return
  await moveTo(target.relPath, folderPath)
}

// ---------- drag helpers ----------

function clearDragExpand(): void {
  window.clearTimeout(dragExpandTimer)
  dragExpandTimer = undefined
  dragExpandPath = ''
}

function onTreeDragOver(event: DragEvent): void {
  const el = treeScrollRef.value
  if (!el || !drag.value) return
  const rect = el.getBoundingClientRect()
  const edge = 32
  if (event.clientY < rect.top + edge) el.scrollTop -= 8
  else if (event.clientY > rect.bottom - edge) el.scrollTop += 8
}

function toggle(path: string): void {
  const next = new Set(collapsed.value)
  if (next.has(path)) next.delete(path)
  else next.add(path)
  collapsed.value = next
  persist(COLLAPSED_KEY, [...next])
}

function expandAll(paths: string[]): void {
  const next = new Set(collapsed.value)
  let changed = false
  for (const path of paths) {
    if (next.delete(path)) changed = true
  }
  if (changed) {
    collapsed.value = next
    persist(COLLAPSED_KEY, [...next])
  }
}

function toggleAll(): void {
  const next = allCollapsed.value ? new Set<string>() : new Set(folderPaths(tree.value))
  collapsed.value = next
  persist(COLLAPSED_KEY, [...next])
}

function cycleSort(): void {
  const index = SORT_MODES.indexOf(sort.value)
  sort.value = SORT_MODES[(index + 1) % SORT_MODES.length]
  persist(SORT_KEY, sort.value)
}

function togglePin(id: string): void {
  const next = pinned.value.includes(id)
    ? pinned.value.filter((item) => item !== id)
    : [...pinned.value, id]
  pinned.value = next
  persist(PINNED_KEY, next)
}

function isPinned(id: string): boolean {
  return pinned.value.includes(id)
}

async function loadFolders(): Promise<void> {
  try {
    folders.value = await api.listFolders()
  } catch {
    folders.value = []
  }
}

function errorText(error: unknown): string {
  return api.errorText(error)
}

function showToast(text: string, undo?: () => void): void {
  toast.value = { text, undo }
  window.clearTimeout(toastTimer)
  toastTimer = window.setTimeout(() => {
    toast.value = null
  }, 8000)
}

async function runUndo(): Promise<void> {
  const action = toast.value?.undo
  toast.value = null
  if (!action) return
  try {
    await action()
    showToast('已撤销')
  } catch (error) {
    showToast(errorText(error))
  }
}

// ---------- create / rename / delete ----------

async function createNote(parent: string): Promise<void> {
  try {
    const created = await api.createNote(parent, '新建笔记')
    expandAll(ancestorPaths(created.relPath))
    await refreshNotes()
    await loadFolders()
    await router.push(`/note/${created.id}`)
    await startRename({ kind: 'note', relPath: created.relPath, name: '新建笔记' })
  } catch (error) {
    showToast(errorText(error))
  }
}

async function createFolder(parent: string): Promise<void> {
  try {
    const relPath = await api.createFolder(parent, '新建文件夹')
    expandAll(parent ? [parent] : [])
    await loadFolders()
    await startRename({ kind: 'folder', relPath, name: '新建文件夹' })
  } catch (error) {
    showToast(errorText(error))
  }
}

async function startRename(target: Target): Promise<void> {
  menu.value = null
  menuOpen = false
  expandAll(ancestorPaths(target.relPath))
  await nextTick()
  if (target.kind === 'root') return
  renaming.value = { relPath: target.relPath, kind: target.kind }
  renameValue.value = baseName(target.relPath)
  await nextTick()
  renameInput.value?.focus()
  renameInput.value?.select()
}

function cancelRename(): void {
  renaming.value = null
}

async function commitRename(): Promise<void> {
  const target = renaming.value
  if (!target) return
  const value = renameValue.value.trim()
  renaming.value = null
  if (!value || value === baseName(target.relPath)) return
  try {
    const report = await api.renameEntry(target.relPath, value)
    await refreshNotes()
    await loadFolders()
    if (report.linksRewritten > 0) {
      showToast(`改名完成，更新了 ${report.linksRewritten} 处链接`)
    }
  } catch (error) {
    showToast(errorText(error))
  }
}

function remove(target: Target): void {
  menu.value = null
  menuOpen = false
  pendingDelete.value = target
}

async function confirmDelete(): Promise<void> {
  const target = pendingDelete.value
  pendingDelete.value = null
  if (!target) return
  try {
    const deleted = await api.deleteEntry(target.relPath)
    const next = props.notes.find((note) => note.id !== target.noteId)
    await refreshNotes()
    await loadFolders()
    if (target.kind === 'note' && target.noteId === props.activeId) {
      void router.push(next ? `/note/${next.id}` : '/note')
    }
    showToast(`已删除「${target.name}」`, async () => {
      await api.restoreEntry(deleted.trashPath, deleted.relPath)
      await refreshNotes()
      await loadFolders()
    })
  } catch (error) {
    showToast(errorText(error))
  }
}

// ---------- drag and drop ----------

function canDrop(current: { relPath: string; isDir: boolean } | null, folderPath: string): boolean {
  if (!current) return false
  if (current.isDir) {
    if (folderPath === current.relPath) return false
    if (folderPath.startsWith(`${current.relPath}/`)) return false
  }
  return folderPath !== parentOf(current.relPath)
}

function onDragStart(event: DragEvent, relPath: string, isDir: boolean, key: string): void {
  drag.value = { relPath, isDir }
  focusedKey.value = key
  if (event.dataTransfer) {
    event.dataTransfer.effectAllowed = 'move'
    event.dataTransfer.setData('text/plain', relPath)
  }
}

function onDragEnd(): void {
  drag.value = null
  dropTarget.value = null
  clearDragExpand()
}

function onDragOver(event: DragEvent, folderPath: string): void {
  if (!canDrop(drag.value, folderPath)) return
  event.preventDefault()
  if (event.dataTransfer) event.dataTransfer.dropEffect = 'move'
  dropTarget.value = folderPath

  // Hovering over a collapsed folder opens it after a pause, like Finder and
  // VS Code. Without it, moving into a closed folder requires a second drag.
  if (folderPath && collapsed.value.has(folderPath)) {
    if (dragExpandPath !== folderPath) {
      clearDragExpand()
      dragExpandPath = folderPath
      dragExpandTimer = window.setTimeout(() => {
        if (!drag.value || dropTarget.value !== folderPath) return
        const next = new Set(collapsed.value)
        next.delete(folderPath)
        collapsed.value = next
        persist(COLLAPSED_KEY, [...next])
        clearDragExpand()
      }, 650)
    }
  } else if (dragExpandPath && dragExpandPath !== folderPath) {
    clearDragExpand()
  }
}

function onDragLeave(folderPath: string): void {
  if (dropTarget.value === folderPath) dropTarget.value = null
  if (dragExpandPath === folderPath) clearDragExpand()
}

async function onDrop(event: DragEvent, folderPath: string): Promise<void> {
  event.preventDefault()
  const current = drag.value
  drag.value = null
  dropTarget.value = null
  clearDragExpand()
  if (!current || !canDrop(current, folderPath)) return
  await moveTo(current.relPath, folderPath)
}

// ---------- context menu ----------

function openMenu(event: MouseEvent, target: Target): void {
  event.preventDefault()
  const x = Math.min(event.clientX, window.innerWidth - 200)
  const y = Math.min(event.clientY, window.innerHeight - 240)
  menu.value = { x, y, target }
  menuOpen = true
  menuReturnKey =
    target.kind === 'note' ? `n:${target.noteId}` : target.kind === 'folder' ? `d:${target.relPath}` : ''
  void nextTick(() => document.querySelector<HTMLButtonElement>('.ctx button')?.focus())
}

function closeMenu(): void {
  if (!menuOpen) return
  menu.value = null
  menuOpen = false
}

function onMenuKeydown(event: KeyboardEvent): void {
  const items = [...document.querySelectorAll<HTMLButtonElement>('.ctx button')]
  if (!items.length) return
  const current = items.indexOf(document.activeElement as HTMLButtonElement)
  if (event.key === 'ArrowDown') {
    event.preventDefault()
    items[(current + 1) % items.length]?.focus()
  } else if (event.key === 'ArrowUp') {
    event.preventDefault()
    items[(current - 1 + items.length) % items.length]?.focus()
  } else if (event.key === 'Escape') {
    event.preventDefault()
    closeMenu()
    if (menuReturnKey) void focusRow(menuReturnKey)
  }
}

function onKeydown(event: KeyboardEvent): void {
  if (event.key !== 'Escape') return
  if (moving.value) {
    moving.value = null
    return
  }
  if (pendingDelete.value) {
    pendingDelete.value = null
    return
  }
  if (renaming.value) {
    cancelRename()
    return
  }
  if (menuOpen) {
    closeMenu()
    if (menuReturnKey) void focusRow(menuReturnKey)
  }
}

function searchMatchLabel(note: NoteMeta): string {
  const text = query.value.trim().toLowerCase()
  if (!text) return ''
  return note.title.toLowerCase().includes(text) || note.relPath.toLowerCase().includes(text) ? '' : '正文命中'
}

function focusSearch(): void {
  searchInput.value?.focus()
  searchInput.value?.select()
}

onMounted(() => {
  void loadFolders()
  document.addEventListener('click', closeMenu)
  window.addEventListener('keydown', onKeydown)
  window.addEventListener('yb-focus-search', focusSearch)
  const el = treeScrollRef.value
  if (el) {
    treeViewport.value = el.clientHeight
    treeResizeObserver = new ResizeObserver(() => {
      treeViewport.value = el.clientHeight
    })
    treeResizeObserver.observe(el)
  }
})

onBeforeUnmount(() => {
  document.removeEventListener('click', closeMenu)
  window.removeEventListener('keydown', onKeydown)
  window.removeEventListener('yb-focus-search', focusSearch)
  window.clearTimeout(toastTimer)
  window.clearTimeout(searchTimer)
  treeResizeObserver?.disconnect()
})

watch(
  () => props.notes,
  () => void loadFolders(),
)

watch(query, (value) => {
  if (treeScrollRef.value) treeScrollRef.value.scrollTop = 0
  treeScrollTop.value = 0
  window.clearTimeout(searchTimer)
  const text = value.trim()
  if (!text) {
    searchResults.value = []
    searchLoading.value = false
    return
  }
  searchLoading.value = true
  searchTimer = window.setTimeout(async () => {
    try {
      searchResults.value = await api.searchNotes(text)
    } catch {
      searchResults.value = []
    } finally {
      searchLoading.value = false
    }
  }, 140)
})

watch(pendingDelete, async (value) => {
  if (!value) return
  await nextTick()
  document.querySelector<HTMLButtonElement>('.confirm .cancel')?.focus()
})

watch(moving, async (value) => {
  if (!value) return
  await nextTick()
  document.querySelector<HTMLButtonElement>('.move-list button')?.focus()
})

// Reveal the active note, but only when the selection changes: collapsing the
// folder around the note you are reading is a legitimate thing to do.
let revealedFor = ''
watch(
  () => [props.activeId, props.notes] as const,
  () => {
    const id = props.activeId
    if (!id || id === revealedFor) return
    const note = props.notes.find((item) => item.id === id)
    if (!note) return
    revealedFor = id
    expandAll(ancestorPaths(note.relPath))
    void nextTick(() => {
      const active = rows.value.find((row) => row.note?.id === id)
      if (active) {
        focusedKey.value = active.key
        ensureRowVisible(active.key)
      }
    })
  },
  { immediate: true },
)
</script>

<template>
  <div class="side-nav">
    <div class="side-head">
      <span class="side-title">笔记库</span>
      <div class="side-tools">
        <button
          type="button"
          class="side-tool"
          title="在当前文件夹新建笔记"
          aria-label="在当前文件夹新建笔记"
          @click="createNote(crumb.length ? crumb[crumb.length - 1].path : '')"
        >
          <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M8 3v10M3 8h10" /></svg>
        </button>
        <button
          type="button"
          class="side-tool"
          :title="SORT_HINT[sort]"
          :aria-label="`排序：${SORT_LABEL[sort]}`"
          @click="cycleSort"
        >
          <svg viewBox="0 0 16 16" aria-hidden="true">
            <path d="M5 3v10M5 3 3 5M5 3l2 2M11 13V3M11 13l-2-2M11 13l2-2" />
          </svg>
        </button>
        <button
          type="button"
          class="side-tool"
          :title="allCollapsed ? '展开所有文件夹' : '收起所有文件夹'"
          :aria-label="allCollapsed ? '展开所有文件夹' : '收起所有文件夹'"
          @click="toggleAll"
        >
          <svg v-if="allCollapsed" viewBox="0 0 16 16" aria-hidden="true">
            <path d="M4 9l4 4 4-4M4 3l4 4 4-4" />
          </svg>
          <svg v-else viewBox="0 0 16 16" aria-hidden="true">
            <path d="M4 7l4-4 4 4M4 13l4-4 4 4" />
          </svg>
        </button>
      </div>
    </div>

    <div class="side-filter-wrap">
      <svg class="search-icon" viewBox="0 0 16 16" aria-hidden="true">
        <circle cx="7" cy="7" r="4.5" />
        <path d="m10.5 10.5 3 3" />
      </svg>
      <input
        ref="searchInput"
        v-model="query"
        class="side-filter"
        type="search"
        placeholder="搜索笔记…"
        aria-label="搜索笔记"
        @keydown.esc.prevent="query = ''"
      />
      <button
        v-if="query"
        type="button"
        class="side-clear"
        title="清除搜索"
        aria-label="清除搜索"
        @click="query = ''"
      >
        <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M4 4l8 8M12 4l-8 8" /></svg>
      </button>
    </div>

    <section v-if="!filtering && pinnedNotes.length" class="pinned" aria-label="收藏">
      <div class="section-label">
        <span>收藏</span>
        <span class="section-count">{{ pinnedNotes.length }}</span>
      </div>
      <div
        v-for="note in pinnedNotes"
        :key="`p:${note.id}`"
        class="tree-row note-row"
        :class="{ 'is-active': note.id === activeId }"
        role="link"
        tabindex="0"
        @click="openNote(note)"
        @keydown.enter.prevent="openNote(note)"
      >
        <span class="tree-lead" aria-hidden="true">
          <svg viewBox="0 0 16 16"><path d="M4 1.8h5.2L13 5.6v8.6H4z" /><path d="M9 1.8v4h4" /></svg>
        </span>
        <span class="tree-text">
          <span class="tree-title" :title="note.title">{{ note.title }}</span>
          <span v-if="note.annotationCount" class="tree-meta">
            <span class="tree-anno">{{ note.annotationCount }}</span>
          </span>
        </span>
        <button
          type="button"
          class="pin"
          aria-pressed="true"
          title="取消收藏"
          aria-label="取消收藏"
          tabindex="-1"
          @click.stop="togglePin(note.id)"
        >
          <svg viewBox="0 0 16 16" aria-hidden="true">
            <path d="M8 1.8l1.9 3.9 4.3.6-3.1 3 .7 4.3L8 11.6l-3.8 2 .7-4.3-3.1-3 4.3-.6z" />
          </svg>
        </button>
      </div>
    </section>

    <div v-if="indexVisible" class="index-state" :class="`is-${indexStatus?.state}`">
      <div class="index-line">
        <span>{{ indexLabel }}</span>
        <span v-if="indexPct !== null" class="index-pct">{{ indexPct }}%</span>
      </div>
      <div class="index-track">
        <i :style="{ width: `${indexPct ?? 0}%` }" />
      </div>
    </div>

    <div
      ref="treeScrollRef"
      class="tree-scroll"
      @scroll.passive="onTreeScroll"
      @dragover="onTreeDragOver"
      @contextmenu.prevent.self="openMenu($event, { kind: 'root', relPath: '', name: '笔记库' })"
    >
      <div v-if="filtering" class="side-search">
        <div class="side-search-head">
          <span>{{ searchLoading ? '搜索中…' : `${searchResults.length} 篇笔记` }}</span>
        </div>
        <button
          v-for="note in searchResults"
          :key="note.id"
          type="button"
          class="side-search-row"
          @click="openNote(note)"
        >
          <span class="side-search-title">{{ note.title }}</span>
          <span class="side-search-meta">
            <em v-if="searchMatchLabel(note)">{{ searchMatchLabel(note) }}</em>
            <span class="side-search-path">{{ note.relPath }}</span>
          </span>
        </button>
        <p v-if="!searchLoading && !searchResults.length" class="side-empty">没有匹配</p>
      </div>

      <div
        v-if="!filtering"
        ref="treeRef"
        class="tree-body"
        role="tree"
        aria-label="笔记库"
        :style="{ height: `${totalHeight}px` }"
      >
        <div class="tree-window" :style="{ transform: `translateY(${windowOffset}px)` }">
          <template v-for="row in windowRows" :key="row.key">
            <template v-if="'empty' in row">
              <div class="tree-empty" role="none" :style="indent(row.depth)">
                空文件夹
              </div>
            </template>
            <template v-else-if="row.folder">
              <div
                v-if="renaming?.relPath === row.folder.path"
                class="rename-row folder-rename"
                :style="indent(row.depth)"
              >
                <input
                  :ref="setRenameInput"
                  v-model="renameValue"
                  class="rename-input"
                  aria-label="重命名文件夹"
                  @keydown.enter.prevent="commitRename"
                  @keydown.esc.prevent="cancelRename"
                  @blur="commitRename"
                />
              </div>
              <div
                v-else
                class="tree-row folder-row"
                :class="{ 'is-collapsed': collapsed.has(row.folder.path), 'is-drop': dropTarget === row.folder.path }"
                :style="indent(row.depth)"
                role="treeitem"
                :aria-expanded="!collapsed.has(row.folder.path)"
                :aria-level="row.depth + 1"
                :data-key="row.key"
                :data-folder="row.folder.path"
                :tabindex="treeTabStopKey === row.key ? 0 : -1"
                draggable="true"
                @focus="focusedKey = row.key"
                @click="toggleFolder(row.folder.path, row.key)"
                @keydown="onRowKeydown($event, row)"
                @contextmenu.stop="openMenu($event, { kind: 'folder', relPath: row.folder.path, name: row.folder.name })"
                @dragstart="onDragStart($event, row.folder.path, true, row.key)"
                @dragend="onDragEnd"
                @dragover="onDragOver($event, row.folder.path)"
                @dragleave="onDragLeave(row.folder.path)"
                @drop="onDrop($event, row.folder.path)"
              >
                <span class="tree-lead" aria-hidden="true">
                  <svg class="chev" viewBox="0 0 16 16"><path d="m6 3.5 4.5 4.5L6 12.5" /></svg>
                </span>
                <span class="tree-name">{{ row.folder.name }}</span>
                <span v-if="dropTarget === row.folder.path" class="drop-chip">移入</span>
                <span
                  v-else-if="collapsed.has(row.folder.path) || row.folder.count"
                  class="tree-count"
                >{{ row.folder.count || '空' }}</span>
              </div>
            </template>

            <template v-else-if="row.note">
              <div
                v-if="renaming?.relPath === row.note.relPath"
                class="rename-row note-rename"
                :style="indent(row.depth)"
              >
                <input
                  :ref="setRenameInput"
                  v-model="renameValue"
                  class="rename-input"
                  aria-label="重命名笔记"
                  @keydown.enter.prevent="commitRename"
                  @keydown.esc.prevent="cancelRename"
                  @blur="commitRename"
                />
              </div>
              <div
                v-else
                class="tree-row note-row"
                :class="{ 'is-active': row.note.id === activeId, 'is-dragging': drag?.relPath === row.note.relPath }"
                :style="indent(row.depth)"
                role="treeitem"
                :aria-selected="row.note.id === activeId"
                :aria-level="row.depth + 1"
                :data-key="row.key"
                :tabindex="treeTabStopKey === row.key ? 0 : -1"
                draggable="true"
                @focus="focusedKey = row.key"
                @click="openNote(row.note)"
                @keydown="onRowKeydown($event, row)"
                @contextmenu.stop="openMenu($event, { kind: 'note', relPath: row.note.relPath, name: baseName(row.note.relPath), noteId: row.note.id })"
                @dragstart="onDragStart($event, row.note.relPath, false, row.key)"
                @dragend="onDragEnd"
              >
                <span class="tree-lead" aria-hidden="true">
                  <svg viewBox="0 0 16 16">
                    <path d="M4 1.8h5.2L13 5.6v8.6H4z" />
                    <path d="M9 1.8v4h4" />
                  </svg>
                </span>
                <span class="tree-text">
                  <span class="tree-title" :title="row.note.title">{{ row.note.title }}</span>
                  <span v-if="row.note.annotationCount" class="tree-meta">
                    <span class="tree-anno">{{ row.note.annotationCount }}</span>
                  </span>
                </span>
                <button
                  type="button"
                  class="pin"
                  :aria-pressed="isPinned(row.note.id)"
                  :title="isPinned(row.note.id) ? '取消收藏' : '收藏'"
                  :aria-label="isPinned(row.note.id) ? '取消收藏' : '收藏'"
                  tabindex="-1"
                  @click.stop="togglePin(row.note.id)"
                >
                  <svg viewBox="0 0 16 16" aria-hidden="true">
                    <path d="M8 1.8l1.9 3.9 4.3.6-3.1 3 .7 4.3L8 11.6l-3.8 2 .7-4.3-3.1-3 4.3-.6z" />
                  </svg>
                </button>
              </div>
            </template>
          </template>
        </div>
      </div>

      <div
        v-if="drag && parentOf(drag.relPath)"
        class="drop-root"
        :class="{ 'is-over': dropTarget === '' }"
        @dragover.stop="onDragOver($event, '')"
        @dragleave="onDragLeave('')"
        @drop="onDrop($event, '')"
      >
        拖到这里移到根目录
      </div>
    </div>

    <div
      v-if="menu"
      class="ctx"
      :style="{ left: `${menu.x}px`, top: `${menu.y}px` }"
      role="menu"
      aria-label="条目操作"
      @click.stop
      @keydown="onMenuKeydown"
    >
      <template v-if="menu.target.kind === 'root'">
        <button type="button" role="menuitem" @click="createNote('')">
          <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M4 3.5h5l3 3v6h-8z" /><path d="M9 3.5v3h3M8 8v4M6 10h4" /></svg>
          新建笔记
        </button>
        <button type="button" role="menuitem" @click="createFolder('')">
          <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M2 4.5h4l1.2 1.5H14v7.5H2z" /><path d="M8 8.5v3M6.5 10h3" /></svg>
          新建文件夹
        </button>
      </template>
      <template v-else-if="menu.target.kind === 'folder'">
        <button type="button" role="menuitem" @click="createNote(menu.target.relPath)">
          <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M4 3.5h5l3 3v6h-8z" /><path d="M9 3.5v3h3M8 8v4M6 10h4" /></svg>
          新建笔记
        </button>
        <button type="button" role="menuitem" @click="createFolder(menu.target.relPath)">
          <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M2 4.5h4l1.2 1.5H14v7.5H2z" /><path d="M8 8.5v3M6.5 10h3" /></svg>
          新建文件夹
        </button>
        <div class="ctx-sep" />
      </template>
      <button
        v-if="menu.target.kind !== 'root'"
        type="button"
        role="menuitem"
        @click="startRename(menu.target)"
      >
        <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M3 11.5V13h1.5L13 4.5 11.5 3z" /><path d="M10.5 4l1.5 1.5" /></svg>
        重命名
        <span class="ctx-key">F2</span>
      </button>
      <button
        v-if="menu.target.noteId"
        type="button"
        role="menuitem"
        @click="togglePin(menu.target.noteId!); closeMenu()"
      >
        <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M8 1.8l1.9 3.9 4.3.6-3.1 3 .7 4.3L8 11.6l-3.8 2 .7-4.3-3.1-3 4.3-.6z" /></svg>
        {{ isPinned(menu.target.noteId!) ? '取消收藏' : '收藏' }}
        <span class="ctx-key">P</span>
      </button>
      <button
        v-if="menu.target.kind !== 'root'"
        type="button"
        role="menuitem"
        @click="startMove(menu.target)"
      >
        <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M2 4.5h4l1.2 1.5H14v7.5H2z" /><path d="M9 10.5h4M11 8.5l2 2-2 2" /></svg>
        移动到…
        <span class="ctx-key">M</span>
      </button>
      <button
        v-if="menu.target.kind !== 'root'"
        type="button"
        role="menuitem"
        class="danger"
        @click="remove(menu.target)"
      >
        <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M3.5 5h9M6 5V3.5h4V5M5 5l.5 8h5L11 5" /><path d="M7 7v4M9 7v4" /></svg>
        删除
        <span class="ctx-key">Del</span>
      </button>
    </div>

    <div v-if="pendingDelete" class="confirm-backdrop" @click.self="pendingDelete = null">
      <div class="confirm" role="dialog" aria-modal="true" aria-labelledby="delete-title">
        <p id="delete-title" class="confirm-title">把「{{ pendingDelete.name }}」移到回收站？</p>
        <p class="confirm-note">删除后 8 秒内可以在左下角撤销。</p>
        <div class="confirm-actions">
          <button type="button" class="cancel" @click="pendingDelete = null">取消</button>
          <button type="button" class="danger" @click="confirmDelete">移到回收站</button>
        </div>
      </div>
    </div>

    <div v-if="moving" class="confirm-backdrop" @click.self="moving = null">
      <div class="confirm" role="dialog" aria-modal="true" aria-labelledby="move-title">
        <p id="move-title" class="confirm-title">把「{{ moving.name }}」移动到…</p>
        <p class="confirm-note">选一个文件夹；根目录表示移出当前文件夹。</p>
        <div class="move-list">
          <button
            v-for="path in moveChoices"
            :key="path || 'root'"
            type="button"
            @click="commitMove(path)"
          >
            <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M2 4.5h4l1.2 1.5H14v7.5H2z" /></svg>
            <span class="move-path">{{ path || '根目录（笔记库）' }}</span>
          </button>
        </div>
      </div>
    </div>

    <div v-if="toast" class="toast" role="status">
      <span>{{ toast.text }}</span>
      <button v-if="toast.undo" type="button" @click="runUndo">撤销</button>
    </div>
  </div>
</template>
