<script setup lang="ts">
import { computed, nextTick, ref } from 'vue'
import { X } from 'lucide-vue-next'

const props = defineProps<{
  modelValue: string[]
  suggestions?: string[]
  placeholder?: string
}>()
const emit = defineEmits<{ 'update:modelValue': [value: string[]] }>()

const draft = ref('')
const focused = ref(false)
const composing = ref(false)
const active = ref(-1)
const input = ref<HTMLInputElement | null>(null)

const filtered = computed(() => {
  const query = draft.value.trim().toLowerCase()
  if (!query) return []
  const selected = new Set(props.modelValue)
  return (props.suggestions ?? [])
    .filter((tag) => !selected.has(tag) && tag.toLowerCase().includes(query))
    .slice(0, 8)
})

const placeholderText = computed(() => {
  if (props.modelValue.length) return props.placeholder ?? '继续添加…'
  return props.placeholder ?? '输入标签，回车添加'
})

function commit(value: string): boolean {
  const parts = value
    .split(/[,，、;；\n]+/)
    .map((item) => item.trim())
    .filter(Boolean)
  if (!parts.length) return false
  const next = [...props.modelValue]
  for (const part of parts) {
    if (!next.includes(part)) next.push(part)
  }
  if (next.length !== props.modelValue.length) emit('update:modelValue', next)
  draft.value = ''
  active.value = -1
  return true
}

function remove(index: number): void {
  const next = [...props.modelValue]
  next.splice(index, 1)
  emit('update:modelValue', next)
  void nextTick(() => input.value?.focus())
}

function focusInput(): void {
  input.value?.focus()
}

function onInput(): void {
  active.value = -1
  if (composing.value) return
  // A Chinese comma typed after an IME composition can arrive through input
  // instead of keydown; commit it without requiring a second key press.
  const match = /^(.*)[,，、;；]\s*$/.exec(draft.value)
  if (match && match[1].trim()) commit(match[1])
}

function onKeydown(event: KeyboardEvent): void {
  if (composing.value || event.isComposing) return
  if (event.key === 'Enter') {
    event.preventDefault()
    if (active.value >= 0 && filtered.value[active.value]) commit(filtered.value[active.value])
    else commit(draft.value)
    return
  }
  if (event.key === ',' || event.key === '，' || event.key === '、' || event.key === ';' || event.key === '；') {
    event.preventDefault()
    if (draft.value.trim()) commit(draft.value)
    return
  }
  if (event.key === 'Backspace' && !draft.value && props.modelValue.length) {
    event.preventDefault()
    remove(props.modelValue.length - 1)
    return
  }
  if (event.key === 'ArrowDown' && filtered.value.length) {
    event.preventDefault()
    active.value = Math.min(filtered.value.length - 1, active.value + 1)
    return
  }
  if (event.key === 'ArrowUp' && filtered.value.length) {
    event.preventDefault()
    active.value = Math.max(-1, active.value - 1)
    return
  }
  if (event.key === 'Escape') {
    draft.value = ''
    active.value = -1
    input.value?.blur()
  }
}

function onPaste(event: ClipboardEvent): void {
  const pasted = event.clipboardData?.getData('text') ?? ''
  if (!/[,，、;；\n]/.test(pasted)) return
  event.preventDefault()
  commit(pasted)
}

function onBlur(): void {
  window.setTimeout(() => {
    focused.value = false
    active.value = -1
  }, 120)
}
</script>

<template>
  <div class="tag-input" @click="focusInput">
    <div class="tag-input-shell" :class="{ 'is-focused': focused }">
      <span v-for="(tag, index) in modelValue" :key="tag" class="tag-chip">
        {{ tag }}
        <button type="button" :aria-label="`移除标签 ${tag}`" @click.stop="remove(index)">
          <X :size="10" />
        </button>
      </span>
      <input
        ref="input"
        v-model="draft"
        class="tag-input-field"
        :placeholder="placeholderText"
        aria-label="添加标签"
        @focus="focused = true"
        @blur="onBlur"
        @input="onInput"
        @keydown="onKeydown"
        @paste="onPaste"
        @compositionstart="composing = true"
        @compositionend="composing = false"
      />
    </div>
    <div v-if="focused && filtered.length" class="tag-suggest" role="listbox" aria-label="已有标签">
      <button
        v-for="(tag, index) in filtered"
        :key="tag"
        type="button"
        class="tag-suggest-row"
        :class="{ 'is-active': index === active }"
        role="option"
        :aria-selected="index === active"
        @mousedown.prevent="commit(tag)"
      >
        {{ tag }}
      </button>
    </div>
  </div>
</template>

<style scoped>
.tag-input {
  position: relative;
  width: 100%;
}

.tag-input-shell {
  display: flex;
  min-height: 34px;
  flex-wrap: wrap;
  align-items: center;
  gap: 5px;
  padding: 4px 6px;
  border: 1px solid var(--line);
  border-radius: 4px;
  background: var(--surface);
  cursor: text;
  transition: border-color .14s ease, box-shadow .14s ease;
}

.tag-input-shell.is-focused {
  border-color: var(--blue-line);
  box-shadow: 0 0 0 2px var(--blue-soft);
}

.tag-chip {
  display: inline-flex;
  max-width: 100%;
  align-items: center;
  gap: 2px;
  padding: 2px 3px 2px 7px;
  border: 1px solid var(--blue-line);
  border-radius: 4px;
  background: var(--blue-soft);
  color: var(--blue);
  font-family: var(--sans);
  font-size: 12px;
  line-height: 1.4;
}

.tag-chip > button {
  display: grid;
  width: 16px;
  height: 16px;
  flex: none;
  place-items: center;
  padding: 0;
  border: 0;
  border-radius: 3px;
  background: transparent;
  color: var(--blue);
  cursor: pointer;
}

.tag-chip > button:hover {
  background: color-mix(in srgb, var(--blue) 12%, transparent);
  color: var(--ink);
}

.tag-input-field {
  min-width: 96px;
  height: 24px;
  flex: 1 1 96px;
  padding: 0 2px;
  border: 0;
  outline: none;
  background: transparent;
  color: var(--ink);
  font-family: var(--sans);
  font-size: 13px;
}

.tag-input-field::placeholder {
  color: var(--ink-3);
}

.tag-suggest {
  position: absolute;
  top: calc(100% + 4px);
  right: 0;
  left: 0;
  z-index: 30;
  display: grid;
  gap: 1px;
  max-height: 208px;
  padding: 4px;
  overflow: auto;
  border: 1px solid var(--line-2);
  border-radius: 5px;
  background: var(--surface);
  box-shadow: 0 10px 28px color-mix(in srgb, var(--ink) 12%, transparent);
}

.tag-suggest-row {
  width: 100%;
  padding: 5px 7px;
  border: 0;
  border-radius: 3px;
  background: transparent;
  color: var(--ink-2);
  font-family: var(--sans);
  font-size: 12px;
  text-align: left;
  cursor: pointer;
}

.tag-suggest-row:hover,
.tag-suggest-row.is-active {
  background: var(--blue-soft);
  color: var(--blue);
}
</style>
