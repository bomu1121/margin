<script setup lang="ts">
import type { Block } from '../lib/markdown'

defineProps<{
  block: Block
}>()
</script>

<template>
  <div class="unit" :data-block="block.id">
    <div class="prose" :class="{ quote: block.kind === 'quote' }" v-html="block.html" />
  </div>
</template>
