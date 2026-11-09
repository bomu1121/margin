<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { LoaderCircle, Plus, X } from 'lucide-vue-next'

import * as api from '../lib/api'
import type { Fact, FactSplit } from '../types'

const props = defineProps<{ fact: Fact | null }>()
const emit = defineEmits<{ close: []; split: [split: FactSplit] }>()

type Row = { id: number; text: string }

const rows = ref<Row[]>([])
const saving = ref(false)
const error = ref('')
let sequence = 0

function normalizeText(value: string): string {
  return value.replace(/\s+/g, '').trim()
}

function overlaps(left: string, right: string): boolean {
  return left.includes(right) || right.includes(left)
}

function bigrams(value: string): Set<string> {
  const chars = [...value]
  return new Set(chars.slice(0, -1).map((char, index) => char + chars[index + 1]))
}

function similar(left: string, right: string): boolean {
  const a = bigrams(left)
  const b = bigrams(right)
  if (!a.size || !b.size) return false
  let common = 0
  for (const item of a) if (b.has(item)) common += 1
  return common / Math.min(a.size, b.size) >= 0.62
}

function nearDuplicate(left: string, right: string): boolean {
  return overlaps(left, right) || similar(left, right)
}

function clauses(text: string): string[] {
  return text
    .split(/[，,；;]/)
    .map((item) => item.trim())
    .filter((item) => item.length >= 4)
}

function suggestions(fact: Fact): string[] {
  const related = fact.related
    .filter((item) => item.status !== 'split' && item.relation !== 'same_as' && item.relation !== 'contradicts')
    .map((item) => item.text.trim())
    .filter(Boolean)
  const output = [...related]
  for (const clause of clauses(fact.text)) {
    if (!output.some((item) => nearDuplicate(normalizeText(item), normalizeText(clause)))) output.push(clause)
  }
  if (output.length < 2) {
    for (const clause of fact.text.split(/[，,；;]/).map((item) => item.trim()).filter(Boolean)) {
      if (!output.some((item) => nearDuplicate(normalizeText(item), normalizeText(clause)))) output.push(clause)
    }
  }
  return output.slice(0, 6)
}

watch(
  () => props.fact,
  (fact) => {
    error.value = ''
    if (!fact) {
      rows.value = []
      return
    }
    const initial = suggestions(fact).map((text) => ({ id: ++sequence, text }))
    while (initial.length < 2) initial.push({ id: ++sequence, text: '' })
    rows.value = initial
  },
  { immediate: true },
)

const relatedTexts = computed(() => new Set((props.fact?.related ?? []).map((item) => normalizeText(item.text))))
const reuseCount = computed(() => rows.value.filter((row) => relatedTexts.value.has(normalizeText(row.text))).length)

function close(): void {
  if (saving.value) return
  emit('close')
}

function addRow(): void {
  if (rows.value.length >= 8) return
  rows.value.push({ id: ++sequence, text: '' })
}

function removeRow(index: number): void {
  if (rows.value.length <= 2) return
  rows.value.splice(index, 1)
}

async function save(): Promise<void> {
  const fact = props.fact
  if (!fact || saving.value) return
  const clean = rows.value.map((row) => row.text.trim()).filter(Boolean)
  if (clean.length < 2) {
    error.value = '至少需要两条子事实'
    return
  }
  if (clean.length > 8) {
    error.value = '一次最多拆成 8 条子事实'
    return
  }
  const seen = new Set<string>()
  for (const text of clean) {
    const value = normalizeText(text)
    if (value.length < 2) {
      error.value = '每条子事实至少要有两个字'
      return
    }
    if (value === normalizeText(fact.text)) {
      error.value = '子事实不能与原事实完全相同'
      return
    }
    if (seen.has(value)) {
      error.value = '子事实有重复'
      return
    }
    seen.add(value)
  }
  saving.value = true
  error.value = ''
  try {
    const split = await api.productionFactSplit(fact.id, clean)
    emit('split', split)
  } catch (cause) {
    error.value = api.errorText(cause)
  } finally {
    saving.value = false
  }
}
</script>

<template>
  <div v-if="fact" class="prod-overlay" @click.self="close">
    <div class="prod-dialog fact-split-dialog" role="dialog" aria-modal="true" aria-labelledby="factSplitTitle">
      <div class="prod-dialog-head">
        <div>
          <h2 id="factSplitTitle">拆分复合事实</h2>
          <p class="fact-split-sub">把一条事实拆成可以独立核验的原子事实。</p>
        </div>
        <button type="button" class="fact-merge-close" aria-label="关闭" :disabled="saving" @click="close">×</button>
      </div>
      <div class="prod-dialog-body">
        <blockquote class="fact-split-original">{{ fact.text }}</blockquote>
        <p class="fact-detail-hint">
          子事实会复用已存在的原子事实；网页证据和史料引用默认留在原事实，不复制到每一条。原事实会标记为「已拆分」，可以撤销。
        </p>
        <div class="fact-split-rows">
          <label v-for="(row, index) in rows" :key="row.id" class="fact-split-row">
            <span class="fact-split-index">{{ index + 1 }}</span>
            <textarea v-model="row.text" class="prod-input fact-split-input" rows="2" placeholder="写一条只包含一个断言的事实…" />
            <button type="button" class="fact-split-remove" :disabled="rows.length <= 2 || saving" aria-label="删除这一条" @click.prevent="removeRow(index)">
              <X :size="12" />
            </button>
          </label>
        </div>
        <div class="fact-split-foot">
          <button type="button" class="prod-btn is-small" :disabled="rows.length >= 8 || saving" @click="addRow">
            <Plus :size="12" />添加一条
          </button>
          <span v-if="reuseCount">其中 {{ reuseCount }} 条已有事实会直接复用</span>
        </div>
        <p v-if="error" class="fact-merge-error">{{ error }}</p>
      </div>
      <div class="prod-dialog-foot">
        <button type="button" class="prod-btn" :disabled="saving" @click="close">取消</button>
        <button type="button" class="prod-btn is-primary" :disabled="saving" @click="save">
          <LoaderCircle v-if="saving" :size="12" class="sub-spin" />{{ saving ? '拆分中…' : '确认拆分' }}
        </button>
      </div>
    </div>
  </div>
</template>
