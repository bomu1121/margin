<script setup lang="ts">
import { computed } from 'vue'

import type { NoteMeta } from '../types'

const props = defineProps<{
  notes: NoteMeta[]
  activeId?: string
}>()

const groups = computed(() => {
  const order: string[] = []
  const map = new Map<string, NoteMeta[]>()
  for (const note of props.notes) {
    const group = note.domain || '未分类'
    if (!map.has(group)) {
      map.set(group, [])
      order.push(group)
    }
    map.get(group)?.push(note)
  }
  return order.map((name) => ({ name, items: map.get(name) ?? [] }))
})
</script>

<template>
  <nav aria-label="笔记库">
    <div v-for="group in groups" :key="group.name">
      <div class="group-label">{{ group.name }}</div>
      <router-link
        v-for="note in group.items"
        :key="note.id"
        class="file"
        :class="{ 'is-active': note.id === activeId }"
        :to="`/note/${note.id}`"
      >
        <span class="file-title">{{ note.title }}</span>
        <span class="file-meta">
          <span>{{ note.up }} · {{ note.date }}</span>
          <span v-if="note.annotationCount" class="file-count">{{ note.annotationCount }}</span>
        </span>
      </router-link>
    </div>
  </nav>
</template>
