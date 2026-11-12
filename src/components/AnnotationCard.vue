<script setup lang="ts">
import { onBeforeUnmount, ref } from 'vue'

import type { Annotation } from '../types'

defineProps<{
  annotation: Annotation
  index: number
  time: string
  active: boolean
  preview: boolean
}>()

const emit = defineEmits<{
  select: []
  remove: []
  preview: []
  unpreview: []
}>()

const confirmingDelete = ref(false)
let confirmTimer = 0

function armDelete(): void {
  confirmingDelete.value = true
  window.clearTimeout(confirmTimer)
  confirmTimer = window.setTimeout(() => {
    confirmingDelete.value = false
  }, 4000)
}

function cancelDelete(): void {
  window.clearTimeout(confirmTimer)
  confirmingDelete.value = false
}

function confirmDelete(): void {
  window.clearTimeout(confirmTimer)
  confirmingDelete.value = false
  emit('remove')
}

onBeforeUnmount(() => window.clearTimeout(confirmTimer))
</script>

<template>
  <div
    class="anno anno-card"
    :class="[`is-${annotation.color || 'gold'}`, { 'is-active': active, 'is-hover': preview }]"
    :data-anno="annotation.id"
    title="点击定位原文"
    @click="$emit('select')"
    @mouseenter="$emit('preview')"
    @mouseleave="$emit('unpreview')"
  >
    <div class="anno-q">{{ annotation.quote }}</div>
    <div v-if="annotation.body" class="anno-b">{{ annotation.body }}</div>
    <div class="anno-foot">
      <template v-if="confirmingDelete">
        <span class="anno-confirm-q">删除这条标注？</span>
        <span class="anno-actions" aria-live="polite">
          <button type="button" @click.stop="cancelDelete">取消</button>
          <button type="button" class="danger is-confirm" @click.stop="confirmDelete">确认删除</button>
        </span>
      </template>
      <template v-else>
        <span class="anno-meta">
          <span class="anno-n">{{ String(index + 1).padStart(2, '0') }}</span>
          <span class="anno-sep">·</span>
          <span class="anno-time">{{ time }}</span>
        </span>
        <span class="anno-actions">
          <button type="button" class="danger" @click.stop="armDelete">删除</button>
        </span>
      </template>
    </div>
  </div>
</template>
