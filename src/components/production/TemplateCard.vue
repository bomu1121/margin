<script setup lang="ts">
import type { ProductionTemplate } from '../../types'

defineProps<{ template: ProductionTemplate; icon: unknown; badge?: string; prompt?: string }>()
const emit = defineEmits<{ select: [] }>()

const SOURCE_LABELS: Record<string, string> = {
  bili: 'B站',
  favorites: '收藏',
  file: '本地文件',
  text: '文稿',
}

// The text template's pipeline still uses the backend stage name "文本";
// the source row already calls it 文稿, so keep one term on the card.
const PIPELINE_LABELS: Record<string, string> = { 文本: '文稿' }

function sourcesLabel(template: ProductionTemplate): string {
  return template.sources.map((source) => SOURCE_LABELS[source] ?? source).join(' · ')
}

function pipelineLabel(item: string): string {
  return PIPELINE_LABELS[item] ?? item
}
</script>

<template>
  <button type="button" class="prod-template" :aria-label="`${template.name}，使用此模板`" @click="emit('select')">
    <span class="prod-template-top">
      <span class="prod-template-icon"><component :is="icon" :size="16" /></span>
      <span class="prod-template-name">{{ template.name }}</span>
      <span v-if="badge" class="prod-template-badge">{{ badge }}</span>
      <span class="prod-template-go" aria-hidden="true">使用 <em>→</em></span>
    </span>
    <span class="prod-template-desc">{{ template.description }}</span>
    <span class="prod-template-meta">
      <span v-if="prompt" class="prod-template-row">
        <span class="prod-template-label">提示词</span>
        <span class="prod-template-value">{{ prompt }}</span>
      </span>
      <span class="prod-template-row">
        <span class="prod-template-label">来源</span>
        <span class="prod-template-value">{{ sourcesLabel(template) }}</span>
      </span>
      <span class="prod-template-row">
        <span class="prod-template-label">流程</span>
        <span class="prod-template-value prod-template-flow">
          <template v-for="(item, index) in template.pipeline" :key="item">
            <span>{{ pipelineLabel(item) }}</span>
            <em v-if="index < template.pipeline.length - 1">→</em>
          </template>
        </span>
      </span>
    </span>
  </button>
</template>
