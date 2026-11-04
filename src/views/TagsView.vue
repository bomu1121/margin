<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'

import { attrList, tagValue, type GraphNode } from '../lib/graph'
import { library } from '../stores/library'
import type { NoteMeta } from '../types'

const route = useRoute()
const router = useRouter()

interface TagEntry {
  id: string
  kind: string
  label: string
  noteIds: Set<string>
}

const GROUPS = [
  { key: 'domain', name: '领域' },
  { key: 'type', name: '类型' },
  { key: 'up', name: 'UP 主' },
  { key: 'topic', name: '主题' },
  { key: 'person', name: '人物' },
  { key: 'period', name: '时期' },
  { key: 'source', name: '来源' },
  { key: 'tag', name: '其他' },
]

const index = computed(() => {
  const map = new Map<string, TagEntry>()
  for (const note of library.notes) {
    for (const attr of attrList(note, true)) {
      const existing = map.get(attr.id)
      if (existing) {
        existing.noteIds.add(note.id)
      } else {
        map.set(attr.id, {
          id: attr.id,
          kind: attr.kind,
          label: attr.label,
          noteIds: new Set([note.id]),
        })
      }
    }
  }
  return map
})

const groups = computed(() =>
  GROUPS.map((group) => {
    const entries = [...index.value.values()]
      .filter((entry) => entry.kind === group.key)
      .sort((a, b) => {
        if (b.noteIds.size !== a.noteIds.size) return b.noteIds.size - a.noteIds.size
        return label(entryToNode(a)).localeCompare(label(entryToNode(b)), 'zh')
      })
    return { name: group.name, key: group.key, entries }
  }).filter((group) => group.entries.length > 0),
)

const selected = ref<string[]>([])
const mode = ref<'all' | 'any'>('all')

function entryToNode(entry: TagEntry): GraphNode {
  return { id: entry.id, kind: entry.kind as GraphNode['kind'], label: entry.label, weight: 0 }
}

function label(entry: GraphNode): string {
  return entry.kind === 'person' || entry.kind === 'period' || entry.kind === 'source'
    ? entry.label
    : tagValue(entry.label)
}

function noteHas(note: NoteMeta, id: string): boolean {
  return attrList(note, true).some((attr) => attr.id === id)
}

const results = computed(() => {
  if (!selected.value.length) return library.notes
  return library.notes.filter((note) => {
    if (mode.value === 'any') return selected.value.some((id) => noteHas(note, id))
    return selected.value.every((id) => noteHas(note, id))
  })
})

function toggle(id: string): void {
  const position = selected.value.indexOf(id)
  if (position === -1) selected.value = [...selected.value, id]
  else selected.value = selected.value.filter((item) => item !== id)
}

function remove(id: string): void {
  selected.value = selected.value.filter((item) => item !== id)
}

function clear(): void {
  selected.value = []
}

function selectedLabel(id: string): string {
  const entry = index.value.get(id)
  return entry ? label(entryToNode(entry)) : id
}

function applyQuery(): void {
  const raw = route.query.tag
  const values = Array.isArray(raw)
    ? raw.map((value) => (value == null ? '' : String(value)))
    : raw
      ? [String(raw)]
      : []
  const next: string[] = []
  for (const value of values) {
    const direct = index.value.get(`t:${value}`) ?? index.value.get(value)
    if (direct) {
      next.push(direct.id)
      continue
    }
    const found = [...index.value.values()].find(
      (entry) => entry.label === value || label(entryToNode(entry)) === value,
    )
    if (found) next.push(found.id)
  }
  if (next.length) selected.value = [...new Set(next)]
}

watch(
  () => route.query.tag,
  () => applyQuery(),
  { immediate: true },
)

watch(
  () => library.notes.length,
  () => applyQuery(),
)
</script>

<template>
  <div class="tags-view">
    <aside class="tag-board">
      <div class="tag-board-title">
        <b>标签 / 属性</b>
        <span>{{ index.size }} 个</span>
      </div>
      <div v-for="group in groups" :key="group.key" class="tag-group">
        <div class="tag-group-head">
          <span>{{ group.name }}</span>
          <span>{{ group.entries.length }}</span>
        </div>
        <button
          v-for="entry in group.entries"
          :key="entry.id"
          type="button"
          class="tag-row"
          :aria-pressed="selected.includes(entry.id)"
          @click="toggle(entry.id)"
        >
          <span class="tag-dot" />
          <span class="tag-name">{{ label(entryToNode(entry)) }}</span>
          <span class="tag-count">{{ entry.noteIds.size }}</span>
        </button>
      </div>
    </aside>

    <main class="tag-main">
      <div class="querybar">
        <template v-if="selected.length">
          <span v-for="id in selected" :key="id" class="q-chip">
            {{ selectedLabel(id) }}
            <button type="button" aria-label="移除" @click="remove(id)">×</button>
          </span>
        </template>
        <span v-else class="q-empty">未选标签 · 显示全部</span>
        <span class="q-spacer" />
        <span class="mode">
          <button type="button" :aria-pressed="mode === 'all'" @click="mode = 'all'">全部满足</button>
          <button type="button" :aria-pressed="mode === 'any'" @click="mode = 'any'">任一满足</button>
        </span>
        <button type="button" class="q-clear" @click="clear">清空</button>
      </div>

      <div class="tag-summary">
        <b>{{ results.length }}</b>
        <span>篇命中 · 共 {{ library.notes.length }} 篇 · {{ selected.length }} 个标签</span>
      </div>

      <div v-if="results.length">
        <button
          v-for="note in results"
          :key="note.id"
          type="button"
          class="result-row"
          @click="router.push(`/note/${note.id}`)"
        >
          <span class="result-title">{{ note.title }}</span>
          <span class="result-meta">
            <span>{{ note.up }}</span>
            <span>{{ note.date }}</span>
            <span>{{ note.domain }}</span>
            <span v-if="note.annotationCount">{{ note.annotationCount }} 条标注</span>
          </span>
          <span class="result-tags">
            <span
              v-for="tag in note.tags"
              :key="tag"
              class="result-tag"
              :class="{ on: selected.includes(`t:${tag}`) }"
            >
              {{ tag }}
            </span>
          </span>
        </button>
      </div>
      <div v-else class="empty-state">
        没有同时满足的笔记。切到「任一满足」，或者去掉一个标签。
      </div>
    </main>
  </div>
</template>
