<script setup lang="ts">
import { AlertTriangle, Copy, FileWarning, ShieldCheck, X } from 'lucide-vue-next'

import type { ProductionConflict } from '../../types'

defineProps<{
  open: boolean
  conflicts: ProductionConflict[]
}>()

const emit = defineEmits<{
  choose: [action: 'keep_both' | 'overwrite' | 'cancel']
}>()
</script>

<template>
  <div v-if="open" class="prod-overlay prod-conflict-overlay" role="dialog" aria-modal="true" aria-labelledby="prodConflictTitle">
    <div class="prod-dialog prod-conflict-dialog">
      <header class="prod-dialog-head">
        <div>
          <div class="prod-eyebrow">制作 / 手改冲突</div>
          <h2 id="prodConflictTitle">这篇笔记在生成后被改过</h2>
        </div>
        <button type="button" class="prod-icon-btn" aria-label="关闭" @click="emit('choose', 'cancel')"><X :size="15" /></button>
      </header>

      <div class="prod-conflict-body">
        <div class="prod-conflict-lead">
          <FileWarning :size="18" />
          <p>如果直接重跑，会覆盖你后来手写的内容和标注。下面这些笔记的最后生成版本与当前文件不一致：</p>
        </div>

        <div class="prod-conflict-list">
          <div v-for="conflict in conflicts" :key="conflict.noteId" class="prod-conflict-row">
            <b>{{ conflict.title }}</b>
            <span>{{ conflict.notePath }}</span>
            <small>当前 {{ conflict.currentChars }} 字 · 上次生成 {{ conflict.generatedChars }} 字 · revision {{ conflict.generatedRevision }}</small>
          </div>
        </div>

        <div class="prod-conflict-options">
          <div class="prod-conflict-option">
            <Copy :size="15" />
            <div><b>保留两份</b><span>原笔记和标注留在原处；新生成稿另存为一篇新笔记。</span></div>
          </div>
          <div class="prod-conflict-option">
            <ShieldCheck :size="15" />
            <div><b>覆盖并备份</b><span>先把当前手改稿存进 .yebian/versions，再覆盖原笔记；标注可能失效。</span></div>
          </div>
        </div>

        <p class="prod-warning-line"><AlertTriangle :size="12" /> 默认推荐「保留两份」。只有确认当前手改内容不要了，才选「覆盖并备份」。</p>
      </div>

      <footer class="prod-wizard-foot">
        <span class="prod-wizard-hint">选择会应用到这次任务里的所有冲突笔记。</span>
        <span class="prod-spacer" />
        <button type="button" class="prod-btn" @click="emit('choose', 'cancel')">取消</button>
        <button type="button" class="prod-btn is-danger" @click="emit('choose', 'overwrite')">覆盖并备份</button>
        <button type="button" class="prod-btn is-primary" @click="emit('choose', 'keep_both')">保留两份</button>
      </footer>
    </div>
  </div>
</template>
