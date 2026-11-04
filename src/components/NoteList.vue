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
  filterTree,
  flattenTree,
  folderPaths,
  type SortMode,
} from '../lib/note-tree'
import { refreshNotes } from '../stores/library'
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
const rootRef = ref<HTMLElement | null>(null)
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
const toast = ref<{ text: string; undo?: () => void } | null>(null)
let toastTimer: number | undefined
let menuOpen = false

const filtering = computed(() => query.value.trim().length > 0)
const tree = computed(() => buildTree(props.notes, sort.value, folders.value))
const view = computed(() => (filtering.value ? filterTree(tree.value, query.value) : tree.value))
// While filtering the whole result set is visible; clearing the box restores
// whatever the user had folded away.
const rows = computed(() =>
  flattenTree(view.value, filtering.value ? new Set<string>() : collapsed.value),
)
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

function pad(depth: number): Record<string, string> {
  return { paddingLeft: `${9 + depth * 12}px` }
}

function parentOf(relPath: string): string {
  const index = relPath.lastIndexOf('/')
  return index === -1 ? '' : relPath.slice(0, index)
}

function baseName(relPath: string): string {
  const name = relPath.split('/').pop() ?? relPath
  return name.replace(/\.md$/i, '')
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

// Breadcrumb click = "show me that level": expand it and its ancestors, then
// bring the row into view.
async function reveal(path: string): Promise<void> {
  const parts = path.split('/')
  const next = new Set(collapsed.value)
  for (let i = 1; i <= parts.length; i += 1) next.delete(parts.slice(0, i).join('/'))
  collapsed.value = next
  persist(COLLAPSED_KEY, [...next])
  await nextTick()
  const row = Array.from(rootRef.value?.querySelectorAll<HTMLElement>('.folder') ?? []).find(
    (el) => el.dataset.folder === path,
  )
  row?.scrollIntoView({ block: 'nearest' })
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

function onDragStart(event: DragEvent, relPath: string, isDir: boolean): void {
  drag.value = { relPath, isDir }
  if (event.dataTransfer) {
    event.dataTransfer.effectAllowed = 'move'
    event.dataTransfer.setData('text/plain', relPath)
  }
}

function onDragEnd(): void {
  drag.value = null
  dropTarget.value = null
}

function onDragOver(event: DragEvent, folderPath: string): void {
  if (!canDrop(drag.value, folderPath)) return
  event.preventDefault()
  if (event.dataTransfer) event.dataTransfer.dropEffect = 'move'
  dropTarget.value = folderPath
}

function onDragLeave(folderPath: string): void {
  if (dropTarget.value === folderPath) dropTarget.value = null
}

async function onDrop(event: DragEvent, folderPath: string): Promise<void> {
  event.preventDefault()
  const current = drag.value
  drag.value = null
  dropTarget.value = null
  if (!current || !canDrop(current, folderPath)) return
  try {
    const report = await api.moveEntry(current.relPath, folderPath)
    expandAll([folderPath])
    await refreshNotes()
    await loadFolders()
    showToast(
      report.linksRewritten > 0 ? `已移动，更新了 ${report.linksRewritten} 处链接` : '已移动',
    )
  } catch (error) {
    showToast(errorText(error))
  }
}

// ---------- context menu ----------

function openMenu(event: MouseEvent, target: Target): void {
  event.preventDefault()
  const x = Math.min(event.clientX, window.innerWidth - 190)
  const y = Math.min(event.clientY, window.innerHeight - 180)
  menu.value = { x, y, target }
  menuOpen = true
}

function closeMenu(): void {
  if (!menuOpen) return
  menu.value = null
  menuOpen = false
}

function onKeydown(event: KeyboardEvent): void {
  if (event.key === 'Escape') {
    if (renaming.value) cancelRename()
    closeMenu()
  }
}

onMounted(() => {
  void loadFolders()
  document.addEventListener('click', closeMenu)
  window.addEventListener('keydown', onKeydown)
})

onBeforeUnmount(() => {
  document.removeEventListener('click', closeMenu)
  window.removeEventListener('keydown', onKeydown)
  window.clearTimeout(toastTimer)
})

watch(
  () => props.notes,
  () => void loadFolders(),
)

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
  },
  { immediate: true },
)
</script>

<template>
  <div ref="rootRef" class="side-nav">
    <div class="side-head">
      <span class="side-title">笔记库</span>
      <div class="side-tools">
        <button
          type="button"
          class="side-tool"
          title="在当前文件夹新建笔记"
          @click="createNote(crumb.length ? crumb[crumb.length - 1].path : '')"
        >
          新建
        </button>
        <button type="button" class="side-tool" :title="SORT_HINT[sort]" @click="cycleSort">
          {{ SORT_LABEL[sort] }}
        </button>
        <button
          type="button"
          class="side-tool"
          :title="allCollapsed ? '展开所有文件夹' : '收起所有文件夹'"
          @click="toggleAll"
        >
          {{ allCollapsed ? '展开' : '收起' }}
        </button>
      </div>
    </div>

    <input
      v-model="query"
      class="side-filter"
      type="search"
      placeholder="筛选笔记…"
      aria-label="筛选笔记"
    />

    <nav v-if="!filtering && crumb.length" class="crumbs" aria-label="当前位置">
      <span class="crumb-root">根</span>
      <template v-for="segment in crumb" :key="segment.path">
        <span class="crumb-sep">›</span>
        <button type="button" class="crumb-link" @click="reveal(segment.path)">
          {{ segment.name }}
        </button>
      </template>
    </nav>

    <section v-if="!filtering && pinnedNotes.length" class="pinned" aria-label="收藏">
      <div class="section-label">
        <span>收藏</span>
        <span class="section-count">{{ pinnedNotes.length }}</span>
      </div>
      <div
        v-for="note in pinnedNotes"
        :key="`p:${note.id}`"
        class="file-row"
        :class="{ 'is-active': note.id === activeId }"
      >
        <router-link class="file" :to="`/note/${note.id}`">
          <span class="file-title">{{ note.title }}</span>
          <span class="file-meta">
            <span>{{ note.up ? `${note.up} · ${note.date}` : note.date }}</span>
            <span v-if="note.annotationCount" class="file-count">
              {{ note.annotationCount }}
            </span>
          </span>
        </router-link>
        <button
          type="button"
          class="pin"
          aria-pressed="true"
          title="取消收藏"
          @click.stop="togglePin(note.id)"
        >
          <svg viewBox="0 0 16 16" aria-hidden="true">
            <path d="M8 1.8l1.9 3.9 4.3.6-3.1 3 .7 4.3L8 11.6l-3.8 2 .7-4.3-3.1-3 4.3-.6z" />
          </svg>
        </button>
      </div>
    </section>

    <nav
      aria-label="笔记库"
      @contextmenu.prevent="openMenu($event, { kind: 'root', relPath: '', name: '笔记库' })"
    >
      <template v-for="row in rows" :key="row.key">
        <template v-if="row.folder">
          <div
            v-if="renaming?.relPath === row.folder.path"
            class="rename-row"
            :style="pad(row.depth)"
          >
            <input
              :ref="setRenameInput"
              v-model="renameValue"
              class="rename-input"
              @keydown.enter.prevent="commitRename"
              @keydown.esc.prevent="cancelRename"
              @blur="commitRename"
            />
          </div>
          <button
            v-else
            type="button"
            class="folder"
            :class="{ 'is-collapsed': collapsed.has(row.folder.path), 'is-drop': dropTarget === row.folder.path }"
            :data-folder="row.folder.path"
            :style="pad(row.depth)"
            :aria-expanded="!collapsed.has(row.folder.path)"
            draggable="true"
            @click="toggle(row.folder.path)"
            @contextmenu.stop="openMenu($event, { kind: 'folder', relPath: row.folder!.path, name: row.folder!.name })"
            @dragstart="onDragStart($event, row.folder!.path, true)"
            @dragend="onDragEnd"
            @dragover="onDragOver($event, row.folder!.path)"
            @dragleave="onDragLeave(row.folder!.path)"
            @drop="onDrop($event, row.folder!.path)"
          >
            <svg class="chev" viewBox="0 0 12 12" aria-hidden="true">
              <path d="M4.5 2.5 8 6l-3.5 3.5" />
            </svg>
            <span class="folder-name">{{ row.folder.name }}</span>
            <span class="folder-count">{{ row.folder.count }}</span>
          </button>
        </template>

        <template v-else-if="row.note">
          <div v-if="renaming?.relPath === row.note.relPath" class="rename-row" :style="pad(row.depth)">
            <input
              :ref="setRenameInput"
              v-model="renameValue"
              class="rename-input"
              @keydown.enter.prevent="commitRename"
              @keydown.esc.prevent="cancelRename"
              @blur="commitRename"
            />
          </div>
          <div
            v-else
            class="file-row"
            :class="{ 'is-active': row.note.id === activeId }"
            draggable="true"
            @dragstart="onDragStart($event, row.note!.relPath, false)"
            @dragend="onDragEnd"
            @contextmenu.stop="openMenu($event, { kind: 'note', relPath: row.note!.relPath, name: baseName(row.note!.relPath), noteId: row.note!.id })"
          >
            <router-link
              class="file"
              :style="pad(row.depth)"
              :to="`/note/${row.note.id}`"
            >
              <span class="file-title">{{ row.note.title }}</span>
              <span class="file-meta">
                <span>{{ row.note.up ? `${row.note.up} · ${row.note.date}` : row.note.date }}</span>
                <span v-if="row.note.annotationCount" class="file-count">
                  {{ row.note.annotationCount }}
                </span>
              </span>
            </router-link>
            <button
              type="button"
              class="pin"
              :aria-pressed="isPinned(row.note.id)"
              :title="isPinned(row.note.id) ? '取消收藏' : '收藏'"
              @click.stop="togglePin(row.note.id)"
            >
              <svg viewBox="0 0 16 16" aria-hidden="true">
                <path d="M8 1.8l1.9 3.9 4.3.6-3.1 3 .7 4.3L8 11.6l-3.8 2 .7-4.3-3.1-3 4.3-.6z" />
              </svg>
            </button>
          </div>
        </template>
      </template>

      <p v-if="filtering && !rows.length" class="side-empty">没有匹配的笔记</p>
      <div
        v-if="drag"
        class="drop-root"
        :class="{ 'is-over': dropTarget === '' }"
        @dragover="onDragOver($event, '')"
        @dragleave="onDragLeave('')"
        @drop="onDrop($event, '')"
      >
        拖到这里移到根目录
      </div>
    </nav>

    <div
      v-if="menu"
      class="ctx"
      :style="{ left: `${menu.x}px`, top: `${menu.y}px` }"
      role="menu"
      @click.stop
    >
      <template v-if="menu.target.kind === 'root'">
        <button type="button" role="menuitem" @click="createNote('')">新建笔记</button>
        <button type="button" role="menuitem" @click="createFolder('')">新建文件夹</button>
      </template>
      <template v-else-if="menu.target.kind === 'folder'">
        <button type="button" role="menuitem" @click="createNote(menu.target.relPath)">
          新建笔记
        </button>
        <button type="button" role="menuitem" @click="createFolder(menu.target.relPath)">
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
        重命名
      </button>
      <button
        v-if="menu.target.noteId"
        type="button"
        role="menuitem"
        @click="togglePin(menu.target.noteId!); menu = null"
      >
        {{ isPinned(menu.target.noteId!) ? '取消收藏' : '收藏' }}
      </button>
      <button
        v-if="menu.target.kind !== 'root'"
        type="button"
        role="menuitem"
        class="danger"
        @click="remove(menu.target)"
      >
        删除
      </button>
    </div>

    <div v-if="pendingDelete" class="confirm-backdrop" @click.self="pendingDelete = null">
      <div class="confirm" role="dialog" aria-modal="true">
        <p>把「{{ pendingDelete.name }}」移到回收站？</p>
        <p class="confirm-note">删除后 8 秒内可以在左下角撤销。</p>
        <div class="confirm-actions">
          <button type="button" @click="pendingDelete = null">取消</button>
          <button type="button" class="danger" @click="confirmDelete">移到回收站</button>
        </div>
      </div>
    </div>

    <div v-if="toast" class="toast" role="status">
      <span>{{ toast.text }}</span>
      <button v-if="toast.undo" type="button" @click="runUndo">撤销</button>
    </div>
  </div>
</template>
