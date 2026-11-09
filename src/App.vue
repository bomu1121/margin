<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import type { UnlistenFn } from '@tauri-apps/api/event'
import { PanelLeftClose, PanelLeftOpen } from 'lucide-vue-next'
import { useRoute, useRouter } from 'vue-router'

import NoteList from './components/NoteList.vue'
import ProductionSidebar from './components/production/ProductionSidebar.vue'
import * as api from './lib/api'
import { initOverlayScrollbars } from './lib/overlayScrollbars'
import {
  chooseVault,
  library,
  loadLibrary,
  noteById,
  setIndexStatus,
  reindexLibrary,
  refreshNotes,
} from './stores/library'
import { loadStudio } from './stores/studio'
import { initProductionUpdates, loadProduction } from './stores/production'

const route = useRoute()
const router = useRouter()

const SIDEBAR_MIN = 180
const SIDEBAR_MAX = 440
const viewportWidth = ref(window.innerWidth)
// NoteList owns `yebian.sidebar.collapsed` for folder collapse state.
// The shell panel uses its own keys so the two states never overwrite each other.
const legacyFolderCollapse = window.localStorage.getItem('yebian.sidebar.collapsed')
if (legacyFolderCollapse === '0' || legacyFolderCollapse === '1') {
  window.localStorage.setItem('yebian.sidebar.collapsed', '[]')
}
function storedNumber(key: string, fallback: number, min: number, max: number): number {
  const value = Number(window.localStorage.getItem(key))
  return Number.isFinite(value) && value >= min && value <= max ? value : fallback
}
function sidebarMaxWidth(): number {
  return viewportWidth.value <= 1180 ? 280 : SIDEBAR_MAX
}
const sidebarPreferredWidth = ref(storedNumber('yebian.shell.sidebar.width', window.innerWidth <= 1180 ? 200 : 240, SIDEBAR_MIN, SIDEBAR_MAX))
const sidebarWidth = computed(() => Math.min(sidebarPreferredWidth.value, sidebarMaxWidth()))
const sidebarCollapsed = ref(window.localStorage.getItem('yebian.shell.sidebar.collapsed') === '1')
const sidebarResizing = ref(false)
const bodyStyle = computed(() => ({
  '--sidebar-width': sidebarCollapsed.value ? '0px' : `${sidebarWidth.value}px`,
}))
let sidebarStartX = 0
let sidebarStartWidth = 0
let disposeScrollbars: (() => void) | undefined

let resizeTimer: number | undefined
let unlistenVault: UnlistenFn | undefined
let unlistenIndex: UnlistenFn | undefined
let focusBeforeResize: HTMLElement | null = null

const currentId = computed(() =>
  route.name === 'note' ? String(route.params.id ?? '') : '',
)
const currentNote = computed(() => noteById(currentId.value))
const indexing = computed(() => {
  const state = library.index?.state
  return state === 'scanning' || state === 'indexing' || state === 'committing'
})
const isGraph = computed(() => route.name === 'graph')
const isStudio = computed(() => route.name === 'studio')
const crumb = computed(() =>
  route.name === 'studio'
    ? '制作台'
    : route.name === 'tags'
      ? '标签台'
      : route.name === 'graph'
        ? '全库力场'
        : route.name === 'sources' || route.name === 'source'
          ? '史料库'
          : currentNote.value?.title ?? '笔记',
)
const noteLink = computed(() => (currentId.value ? `/note/${currentId.value}` : '/note'))

// Switching vaults (or deleting the open note elsewhere) leaves the route on
// a note id that no longer exists. The sidebar is fine, but the reading pane
// shows a dead "没有找到这篇笔记"; land on the first note instead.
watch(
  () => library.notes,
  (notes) => {
    if (route.name !== 'note' || !notes.length) return
    if (notes.some((note) => note.id === currentId.value)) return
    void router.replace(`/note/${notes[0].id}`)
  },
)

function onKeydown(event: KeyboardEvent): void {
  if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'k') {
    event.preventDefault()
    if (sidebarCollapsed.value) {
      sidebarCollapsed.value = false
      persistSidebar()
      window.dispatchEvent(new Event('yb-scroll-rescan'))
      window.setTimeout(() => window.dispatchEvent(new Event('yb-focus-search')), 0)
      return
    }
    window.dispatchEvent(new Event('yb-focus-search'))
  }
}

// WebView2 has a known IME composition-window bug after a host resize
// (MicrosoftEdge/WebView2Feedback#5675). Blurring the focused field while the
// window resizes resets the composition window; focus is restored afterwards.
function onResize(): void {
  viewportWidth.value = window.innerWidth
  window.dispatchEvent(new Event('yb-scroll-rescan'))
  const active = document.activeElement as HTMLElement | null
  if (!active) return
  if (active.tagName !== 'INPUT' && active.tagName !== 'TEXTAREA') return
  focusBeforeResize = active
  active.blur()
  window.clearTimeout(resizeTimer)
  resizeTimer = window.setTimeout(() => {
    if (focusBeforeResize && document.contains(focusBeforeResize)) {
      focusBeforeResize.focus({ preventScroll: true })
    }
    focusBeforeResize = null
  }, 160)
}

function persistSidebar(): void {
  window.localStorage.setItem('yebian.shell.sidebar.width', String(sidebarPreferredWidth.value))
  window.localStorage.setItem('yebian.shell.sidebar.collapsed', sidebarCollapsed.value ? '1' : '0')
}

function applySidebarWidth(value: number): void {
  sidebarPreferredWidth.value = Math.max(SIDEBAR_MIN, Math.min(sidebarMaxWidth(), Math.round(value)))
  window.dispatchEvent(new Event('yb-scroll-rescan'))
}

function toggleSidebar(): void {
  sidebarCollapsed.value = !sidebarCollapsed.value
  persistSidebar()
  window.dispatchEvent(new Event('yb-scroll-rescan'))
}

function resetSidebarWidth(): void {
  applySidebarWidth(window.innerWidth <= 1180 ? 200 : 240)
  persistSidebar()
}

function startSidebarResize(event: PointerEvent): void {
  event.preventDefault()
  if (sidebarCollapsed.value) return
  sidebarResizing.value = true
  sidebarStartX = event.clientX
  sidebarStartWidth = sidebarWidth.value
  document.body.classList.add('is-resizing-panel')
  window.addEventListener('pointermove', onSidebarMove)
  window.addEventListener('pointerup', onSidebarUp, { once: true })
  window.addEventListener('pointercancel', onSidebarUp, { once: true })
}

function onSidebarMove(event: PointerEvent): void {
  if (!sidebarResizing.value) return
  applySidebarWidth(sidebarStartWidth + event.clientX - sidebarStartX)
}

function onSidebarUp(): void {
  if (!sidebarResizing.value) return
  sidebarResizing.value = false
  document.body.classList.remove('is-resizing-panel')
  window.removeEventListener('pointermove', onSidebarMove)
  window.removeEventListener('pointercancel', onSidebarUp)
  persistSidebar()
  window.dispatchEvent(new Event('yb-scroll-rescan'))
}

function nudgeSidebar(delta: number): void {
  if (sidebarCollapsed.value) return
  applySidebarWidth(sidebarWidth.value + delta)
  persistSidebar()
}

async function pickVault(): Promise<void> {
  const changed = await chooseVault()
  if (changed && library.notes.length) {
    void router.push(`/note/${library.notes[0].id}`)
  }
}

async function doReindex(): Promise<void> {
  await reindexLibrary()
}

async function drainPending(): Promise<void> {
  try {
    const report = await api.drainOutbox()
    if (report.published > 0) {
      await new Promise((resolve) => window.setTimeout(resolve, 250))
      await refreshNotes()
      if (isStudio.value) await loadStudio()
    }
    if (report.errors.length) console.warn('outbox:', report.errors)
  } catch (error) {
    console.warn('outbox drain failed:', api.errorText(error))
  }
}

onMounted(async () => {
  disposeScrollbars = initOverlayScrollbars()
  initProductionUpdates()
  void loadProduction()
  await loadLibrary()
  await drainPending()
  if (isStudio.value) await loadStudio()
  if (route.name === 'note' && !route.params.id && library.notes.length) {
    void router.replace(`/note/${library.notes[0].id}`)
  }
  unlistenVault = await api.onVaultChanged(async () => {
    await refreshNotes()
    await drainPending()
    if (isStudio.value) await loadStudio()
    if (currentId.value && !noteById(currentId.value) && library.notes.length) {
      void router.replace(`/note/${library.notes[0].id}`)
    }
  })
  unlistenIndex = await api.onIndexStatus(async (status) => {
    setIndexStatus(status)
    if (status.state !== 'idle') return
    await refreshNotes()
    if (isStudio.value) await loadStudio()
    if (currentId.value && !noteById(currentId.value) && library.notes.length) {
      void router.replace(`/note/${library.notes[0].id}`)
    }
  })
  window.addEventListener('keydown', onKeydown)
  window.addEventListener('resize', onResize)
})

watch(
  () => route.name,
  (name) => {
    if (name === 'studio') {
      void drainPending()
      void loadStudio()
    }
  },
)

onBeforeUnmount(() => {
  disposeScrollbars?.()
  window.removeEventListener('pointermove', onSidebarMove)
  window.removeEventListener('pointercancel', onSidebarUp)
  document.body.classList.remove('is-resizing-panel')
  unlistenVault?.()
  unlistenIndex?.()
  window.removeEventListener('keydown', onKeydown)
  window.removeEventListener('resize', onResize)
  window.clearTimeout(resizeTimer)
})
</script>

<template>
  <div class="app" :class="{ 'graph-mode': isGraph, 'studio-mode': isStudio }">
    <header class="topbar">
      <div class="brand">页边 <span>笔记</span></div>
      <button
        type="button"
        class="sidebar-toggle"
        :aria-label="sidebarCollapsed ? '展开笔记侧栏' : '收起笔记侧栏'"
        :title="sidebarCollapsed ? '展开笔记侧栏' : '收起笔记侧栏'"
        @click="toggleSidebar"
      >
        <PanelLeftOpen v-if="sidebarCollapsed" :size="14" />
        <PanelLeftClose v-else :size="14" />
      </button>
      <div class="crumb">{{ crumb }}</div>
      <div class="top-spacer" />

      <nav class="nav" aria-label="视图">
        <router-link :to="noteLink" :class="{ 'is-active': route.name === 'note' }">笔记</router-link>
        <router-link to="/graph" :class="{ 'is-active': route.name === 'graph' }">图谱</router-link>
        <router-link to="/tags" :class="{ 'is-active': route.name === 'tags' }">标签</router-link>
        <router-link to="/facts" :class="{ 'is-active': route.name === 'facts' }">事实</router-link>
        <router-link to="/sources" :class="{ 'is-active': route.name === 'sources' || route.name === 'source' }">史料</router-link>
        <router-link to="/studio" :class="{ 'is-active': route.name === 'studio' }">制作</router-link>
      </nav>

      <button type="button" class="top-btn" @click="pickVault">选择库</button>
      <button type="button" class="top-btn" @click="doReindex">重建索引</button>
      <span class="crumb">{{ library.notes.length }} 篇</span>
    </header>

    <div class="body" :class="{ 'graph-mode': isGraph, 'sidebar-collapsed': sidebarCollapsed }" :style="bodyStyle">
      <div
        v-if="!isGraph"
        class="sidebar-resizer"
        role="separator"
        aria-orientation="vertical"
        aria-label="调整笔记侧栏宽度"
        tabindex="0"
        title="拖动调整宽度，双击复位"
        @pointerdown="startSidebarResize"
        @dblclick="resetSidebarWidth"
        @keydown.left.prevent="nudgeSidebar(-16)"
        @keydown.right.prevent="nudgeSidebar(16)"
        @keydown.home.prevent="resetSidebarWidth"
      />
      <aside class="sidebar">
        <ProductionSidebar v-if="isStudio" />
        <template v-else>
          <NoteList :notes="library.notes" :active-id="currentId" />
          <div class="side-foot">
            读在中间，关系在右边。<br />
            选中任意一段点「+」就能标住。<br />
            <router-link to="/tags">去标签台 →</router-link>
          </div>
        </template>
      </aside>

      <main class="main">
        <div v-if="library.error" class="empty" style="margin: 14px">{{ library.error }}</div>
        <router-view v-if="library.notes.length || route.name === 'tags' || route.name === 'studio'" />
        <div v-else class="loading">
          {{ library.loading ? '正在读取笔记库…' : indexing ? '正在整理笔记库…' : '还没有笔记。点「选择库」选一个文件夹。' }}
        </div>
      </main>
    </div>

    <div v-if="!library.loading && !library.vault" class="welcome">
      <div class="welcome-card">
        <div class="kicker">SCRIBE NOTES</div>
        <h1>选择一个笔记库</h1>
        <p>
          它会读取该目录下所有 Markdown 文件（递归，跳过 .obsidian / Attachments /
          90-Templates）。在侧栏新建、重命名、移动或删除会直接作用到这些文件；删除先进
          .trash/，可以撤销。标注和索引存在应用的本地 SQLite 里。
        </p>
        <div class="welcome-actions">
          <button type="button" class="btn btn-primary" @click="pickVault">选择文件夹</button>
          <button type="button" class="btn" @click="loadLibrary">重新读取</button>
        </div>
      </div>
    </div>
  </div>
</template>
