<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import type { UnlistenFn } from '@tauri-apps/api/event'
import { useRoute, useRouter } from 'vue-router'

import NoteList from './components/NoteList.vue'
import StudioSidebar from './components/StudioSidebar.vue'
import * as api from './lib/api'
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
import type { NoteMeta } from './types'

const route = useRoute()
const router = useRouter()

const searchInput = ref<HTMLInputElement | null>(null)
const searchQuery = ref('')
const searchResults = ref<NoteMeta[]>([])
const searchOpen = ref(false)

let searchTimer: number | undefined
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

watch(searchQuery, (query) => {
  window.clearTimeout(searchTimer)
  if (!query.trim()) {
    searchResults.value = []
    searchOpen.value = false
    return
  }
  searchTimer = window.setTimeout(async () => {
    try {
      searchResults.value = await api.searchNotes(query)
      searchOpen.value = true
    } catch {
      searchResults.value = []
      searchOpen.value = false
    }
  }, 160)
})

function openResult(id: string): void {
  searchOpen.value = false
  searchQuery.value = ''
  searchResults.value = []
  void router.push(`/note/${id}`)
}

function closeSearch(): void {
  searchOpen.value = false
}

function onKeydown(event: KeyboardEvent): void {
  if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'k') {
    event.preventDefault()
    searchInput.value?.focus()
    searchInput.value?.select()
    return
  }
  if (event.key === 'Escape') {
    searchOpen.value = false
  }
}

function onDocumentClick(event: MouseEvent): void {
  const target = event.target as Element | null
  if (!target?.closest('.search-wrap')) searchOpen.value = false
}

// WebView2 has a known IME composition-window bug after a host resize
// (MicrosoftEdge/WebView2Feedback#5675). Blurring the focused field while the
// window resizes resets the composition window; focus is restored afterwards.
function onResize(): void {
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
  window.addEventListener('click', onDocumentClick)
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
  unlistenVault?.()
  unlistenIndex?.()
  window.removeEventListener('keydown', onKeydown)
  window.removeEventListener('click', onDocumentClick)
  window.removeEventListener('resize', onResize)
  window.clearTimeout(searchTimer)
  window.clearTimeout(resizeTimer)
})
</script>

<template>
  <div class="app" :class="{ 'graph-mode': isGraph, 'studio-mode': isStudio }">
    <header class="topbar">
      <div class="brand">页边 <span>笔记</span></div>
      <div class="crumb">{{ crumb }}</div>
      <div class="top-spacer" />

      <div class="search-wrap">
        <input
          ref="searchInput"
          v-model="searchQuery"
          type="text"
          placeholder="搜索笔记 / 标签 / 人物…"
          autocomplete="off"
          aria-label="搜索笔记"
          @focus="searchOpen = !!searchResults.length"
          @keydown.esc="closeSearch"
        />
        <div v-if="searchOpen" class="search-results">
          <button
            v-for="item in searchResults"
            :key="item.id"
            type="button"
            class="search-result"
            @click="openResult(item.id)"
          >
            <em>{{ item.domain }}</em>
            <span>{{ item.title }}</span>
          </button>
          <div v-if="!searchResults.length" class="search-empty">没有匹配</div>
          <div v-if="indexing" class="search-note">索引中，结果可能不完整</div>
        </div>
      </div>

      <nav class="nav" aria-label="视图">
        <router-link :to="noteLink" :class="{ 'is-active': route.name === 'note' }">笔记</router-link>
        <router-link to="/graph" :class="{ 'is-active': route.name === 'graph' }">图谱</router-link>
        <router-link to="/tags" :class="{ 'is-active': route.name === 'tags' }">标签</router-link>
        <router-link to="/studio" :class="{ 'is-active': route.name === 'studio' }">制作</router-link>
      </nav>

      <button type="button" class="top-btn" @click="pickVault">选择库</button>
      <button type="button" class="top-btn" @click="doReindex">重建索引</button>
      <span class="crumb">{{ library.notes.length }} 篇</span>
    </header>

    <div class="body" :class="{ 'graph-mode': isGraph }">
      <aside class="sidebar">
        <StudioSidebar v-if="isStudio" />
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
