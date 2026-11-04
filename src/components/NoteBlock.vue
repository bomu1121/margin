<script setup lang="ts">
import type { Block } from '../lib/markdown'

defineProps<{
  block: Block
  mark?: number
  composing?: boolean
}>()

defineEmits<{
  annotate: [id: string]
  locate: [id: string]
}>()
</script>

<template>
  <div
    class="unit"
    :class="{ 'is-marked': !!mark, 'is-composing': composing }"
    :data-block="block.id"
  >
    <div class="prose" :class="{ quote: block.kind === 'quote' }" v-html="block.html" />
    <button
      v-if="mark"
      type="button"
      class="mark"
      :aria-label="`查看第 ${mark} 条标注`"
      @click="$emit('locate', block.id)"
    >
      {{ mark }}
    </button>
    <button
      type="button"
      class="add"
      aria-label="标注这一段"
      @click="$emit('annotate', block.id)"
    >
      +
    </button>
  </div>
</template>
