<script setup lang="ts">
import { ref, watch } from 'vue'
import { LoaderCircle } from 'lucide-vue-next'

import * as api from '../lib/api'
import type { HistoryCandidate } from '../types'

const props = defineProps<{ candidate: HistoryCandidate | null }>()
const emit = defineEmits<{ close: []; imported: [] }>()

const text = ref('')
const saving = ref(false)
const error = ref('')

watch(() => props.candidate, () => {
  text.value = ''
  error.value = ''
})

function close(): void {
  if (saving.value) return
  emit('close')
}

function openSource(): void {
  if (props.candidate?.url) void api.openExternalUrl(props.candidate.url)
}

async function save(): Promise<void> {
  const candidate = props.candidate
  if (!candidate || saving.value) return
  const value = text.value.trim()
  if (value.length < 8) {
    error.value = '至少粘贴一段原文'
    return
  }
  saving.value = true
  error.value = ''
  try {
    await api.productionFactHistoryCandidateImport(candidate.id, value)
    emit('imported')
  } catch (cause) {
    error.value = api.errorText(cause)
  } finally {
    saving.value = false
  }
}
</script>

<template>
  <div v-if="candidate" class="prod-overlay" @click.self="close">
    <div class="prod-dialog source-dialog fact-paste-dialog" role="dialog" aria-modal="true" aria-labelledby="candidatePasteTitle">
      <div class="prod-dialog-head">
        <div>
          <h2 id="candidatePasteTitle">补录候选原文</h2>
          <p class="source-dialog-sub">{{ candidate.provider }} · {{ candidate.title }}</p>
        </div>
        <button type="button" class="fact-merge-close" aria-label="关闭" :disabled="saving" @click="close">×</button>
      </div>
      <div class="prod-dialog-body">
        <p class="fact-detail-hint">从打开的页面复制正文或相关段落，粘贴到这里。程序会自己切分、找逐字原文并校验；不会让你重填书名/版本。</p>
        <button v-if="candidate.url" type="button" class="source-link-button fact-paste-open" @click="openSource">打开来源页面</button>
        <textarea v-model="text" class="prod-input fact-paste-text" placeholder="粘贴页面正文/相关段落…" autofocus />
        <p v-if="error" class="fact-merge-error">{{ error }}</p>
      </div>
      <div class="prod-dialog-foot">
        <button type="button" class="prod-btn" :disabled="saving" @click="close">取消</button>
        <button type="button" class="prod-btn is-primary" :disabled="saving || text.trim().length < 8" @click="save">
          <LoaderCircle v-if="saving" :size="12" class="sub-spin" />{{ saving ? '对齐中…' : '对齐并写入引用' }}
        </button>
      </div>
    </div>
  </div>
</template>
