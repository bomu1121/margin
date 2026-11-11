<script setup lang="ts">
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
      <span class="anno-colors">
        <button
          v-for="item in colors"
          :key="item.value"
          type="button"
          class="anno-color"
          :class="[`is-${item.value}`, { 'is-picked': color === item.value }]"
          :aria-label="item.label"
          :title="item.label"
          @click="$emit('color', item.value)"
        />
      </span>
      <span class="anno-hint">Enter 保存 · Esc 取消</span>
      <button type="button" class="save" @click="$emit('save')">保存</button>
    </div>
  </div>
</template>
