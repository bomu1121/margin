<script setup lang="ts">
import type { Annotation } from '../types'

defineProps<{
  annotation: Annotation
  index: number
  time: string
  active: boolean
  preview: boolean
}>()

defineEmits<{
  select: []
  remove: []
  preview: []
  unpreview: []
}>()
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
      <span class="anno-meta">
        <span class="anno-n">{{ String(index + 1).padStart(2, '0') }}</span>
        <span class="anno-sep">·</span>
        <span class="anno-time">{{ time }}</span>
      </span>
      <span class="anno-actions">
        <button type="button" class="danger" @click.stop="$emit('remove')">删除</button>
      </span>
    </div>
  </div>
</template>
