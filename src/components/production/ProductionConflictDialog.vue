<script setup lang="ts">
import { computed } from 'vue'
import { AlertTriangle, Copy, FileWarning, ShieldCheck, X } from 'lucide-vue-next'

import type { ProductionConflict } from '../../types'

const props = defineProps<{
  open: boolean
  conflicts: ProductionConflict[]
}>()

const emit = defineEmits<{
  choose: [action: 'keep_both' | 'overwrite' | 'cancel']
}>()

const anyEdited = computed(() => props.conflicts.some((item) => item.edited))
const heading = computed(() => (anyEdited.value ? '这篇笔记在生成后被改过' : '这个来源已经有笔记'))
const eyebrow = computed(() => (anyEdited.value ? '制作 / 手改冲突' : '制作 / 已有笔记'))
const overwriteLabel = computed(() => (anyEdited.value ? '覆盖并备份' : '更新原笔记'))
</script>

<template>
  <div v-if="open" class="prod-overlay prod-conflict-overlay" role="dialog" aria-modal="true" aria-labelledby="prodConflictTitle">
    <div class="prod-dialog prod-conflict-dialog">
      <header class="prod-dialog-head">
        <div>
          <div class="prod-eyebrow">{{ eyebrow }}</div>
          <h2 id="prodConflictTitle">{{ heading }}</h2>
        </div>
        <button type="button" class="prod-icon-btn" aria-label="关闭" @click="emit('choose', 'cancel')"><X :size="15" /></button>
      </header>

      <div class="prod-conflict-body">
        <div class="prod-conflict-lead">
          <FileWarning :size="18" />
          <p v-if="anyEdited">如果直接重跑，会覆盖你后来手写的内容和标注。下面这些笔记的最后生成版本与当前文件不一致：</p>
          <p v-else>这个视频或文稿已经生成过一篇笔记。直接开始会更新原笔记；选「保留两份」会另存一篇新笔记，原笔记和标注不动。</p>
        </div>

        <div class="prod-conflict-list">
          <div v-for="conflict in conflicts" :key="conflict.noteId" class="prod-conflict-row">
            <b>{{ conflict.title }}</b>
            <span>{{ conflict.notePath }}</span>
            <small>当前 {{ conflict.currentChars }} 字 · 上次生成 {{ conflict.generatedChars }} 字 · revision {{ conflict.generatedRevision }} · {{ conflict.edited ? '生成后手改过' : '未手改' }}</small>
          </div>
        </div>

        <div class="prod-conflict-options">
          <div class="prod-conflict-option">
            <Copy :size="15" />
            <div><b>保留两份</b><span>原笔记和标注留在原处；新生成稿另存为一篇新笔记。</span></div>
          </div>
          <div class="prod-conflict-option">
            <ShieldCheck :size="15" />
            <div>
              <b>{{ overwriteLabel }}</b>
              <span v-if="anyEdited">先把当前手改稿存进 .yebian/versions，再覆盖原笔记；标注可能失效。</span>
              <span v-else>先把当前正文存进 .yebian/versions，再更新原笔记；标注可能失效。</span>
            </div>
          </div>
        </div>

        <p class="prod-warning-line"><AlertTriangle :size="12" /> 默认推荐「保留两份」。只有确认要用新稿替换原笔记，才选「{{ overwriteLabel }}」。</p>
      </div>

      <footer class="prod-wizard-foot">
        <span class="prod-wizard-hint">选择会应用到这次任务里的所有已有笔记。</span>
        <span class="prod-spacer" />
        <button type="button" class="prod-btn" @click="emit('choose', 'cancel')">取消</button>
        <button type="button" class="prod-btn is-danger" @click="emit('choose', 'overwrite')">{{ overwriteLabel }}</button>
        <button type="button" class="prod-btn is-primary" @click="emit('choose', 'keep_both')">保留两份</button>
      </footer>
    </div>
  </div>
</template>
