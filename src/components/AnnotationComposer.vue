<script setup lang="ts">
import { X } from 'lucide-vue-next'

defineProps<{
  quote: string
  color: string
  body: string
}>()

defineEmits<{
  color: [value: string]
  body: [value: string]
  save: []
  cancel: []
}>()

const colors = [
  { value: 'gold', label: '金色' },
  { value: 'blue', label: '蓝色' },
  { value: 'rose', label: '红色' },
]
</script>

<template>
  <div class="anno-composer">
    <header class="anno-composer-head">
      <span class="anno-composer-label">标注</span>
      <button type="button" class="anno-composer-close" title="取消标注（Esc）" aria-label="取消标注" @click="$emit('cancel')">
        <X :size="12" />
      </button>
    </header>
    <div class="anno-composer-q">“{{ quote }}”</div>
    <input
      class="anno-input"
      :value="body"
      placeholder="写一句为什么，可留空"
      aria-label="标注内容"
      @input="$emit('body', ($event.target as HTMLInputElement).value)"
      @keydown.enter.prevent="$emit('save')"
      @keydown.esc.prevent="$emit('cancel')"
    />
    <div class="anno-composer-foot">
      <span class="anno-colors" role="group" aria-label="标注颜色">
        <span class="anno-colors-label">颜色</span>
        <button
          v-for="item in colors"
          :key="item.value"
          type="button"
          class="anno-color"
          :class="[`is-${item.value}`, { 'is-picked': color === item.value }]"
          :aria-label="item.label"
          :aria-pressed="color === item.value"
          :title="item.label"
          @click="$emit('color', item.value)"
        />
      </span>
      <span class="anno-composer-actions">
        <button type="button" class="anno-cancel" @click="$emit('cancel')">取消</button>
        <button type="button" class="save" title="保存标注（Enter）" @click="$emit('save')">保存</button>
      </span>
    </div>
  </div>
</template>
