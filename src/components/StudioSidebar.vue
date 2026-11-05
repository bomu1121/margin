<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'

import * as api from '../lib/api'
import { studio, loadStudio, selectArtifact } from '../stores/studio'
import { refreshNotes } from '../stores/library'

const opening = ref(false)
const error = ref('')

const active = computed(() =>
  studio.artifacts.filter((item) => item.status === 'running' || item.status === 'pending'),
)
const folder = computed(() => studio.info?.folder ?? 'ScribeFlow')
const connected = computed(() => studio.info?.running === true)

function statusLabel(status: string): string {
  if (status === 'published') return '已入库'
  if (status === 'running') return '进行中'
  if (status === 'pending') return '待入库'
  if (status === 'missing') return '笔记缺失'
  if (status === 'error') return '失败'
  return status || '已入库'
}

function statusClass(status: string): string {
  if (status === 'published') return 'is-ok'
  if (status === 'error' || status === 'missing') return 'is-warn'
  return 'is-busy'
}

// TODO(SF-001/SF-002): a native ScribeFlow publish target should expose
// project/run status here; for now the canvas opens externally.
async function openCanvas(): Promise<void> {
  opening.value = true
  error.value = ''
  try {
    await api.openScribeFlow()
  } catch (cause) {
    error.value = api.errorText(cause)
  } finally {
    opening.value = false
  }
}

async function refresh(): Promise<void> {
  try {
    const report = await api.drainOutbox()
    if (report.published > 0) {
      await new Promise((resolve) => window.setTimeout(resolve, 200))
      await refreshNotes()
    }
  } catch (cause) {
    error.value = api.errorText(cause)
  }
  await loadStudio()
}

let drainTimer: number | undefined

onMounted(() => {
  void refresh()
  // ScribeFlow can publish while the desktop app is open. A lightweight
  // outbox poll is enough until the local ACK/event channel exists (SF-002).
  drainTimer = window.setInterval(() => void refresh(), 5000)
})

onBeforeUnmount(() => {
  window.clearInterval(drainTimer)
})
</script>

<template>
  <div class="studio-side">
    <div class="studio-side-head">
      <span class="studio-side-title">制作</span>
      <span class="studio-connection" :class="{ 'is-off': !connected }">
        <i />
        {{ connected ? 'ScribeFlow 本机' : '未连接' }}
      </span>
    </div>

    <div class="studio-side-actions">
      <button type="button" class="studio-btn studio-btn-primary" :disabled="opening" @click="openCanvas">
        <span aria-hidden="true">＋</span>新建制作
      </button>
      <button type="button" class="studio-btn studio-btn-ghost" :disabled="opening" @click="openCanvas">
        打开画布
      </button>
    </div>

    <div class="studio-label">从什么开始</div>
    <div class="studio-sources">
      <button type="button" class="studio-source" @click="openCanvas">B站链接</button>
      <button type="button" class="studio-source" @click="openCanvas">本地音视频</button>
      <button type="button" class="studio-source" @click="openCanvas">已有文稿</button>
    </div>
    <p class="studio-hint">画布仍在 ScribeFlow 中打开；这里只接收入库结果。</p>

    <div class="studio-label">
      <b>进行中</b>
      <span>{{ active.length }}</span>
    </div>
    <div v-if="active.length" class="studio-active-list">
      <button
        v-for="item in active"
        :key="item.artifactId"
        type="button"
        class="studio-active"
        :class="{ 'is-selected': studio.selectedId === item.artifactId }"
        @click="selectArtifact(item.artifactId)"
      >
        <span class="studio-active-top">
          <span class="studio-active-title">{{ item.title }}</span>
          <span class="studio-active-state">{{ statusLabel(item.status) }}</span>
        </span>
        <span class="studio-active-sub">{{ item.run?.projectName || item.source?.label || 'ScribeFlow' }}</span>
      </button>
    </div>
    <p v-else class="studio-empty-line">
      {{ connected ? '本机没有进行中的制作' : '启动 ScribeFlow 后，这里会显示进行中的制作' }}
    </p>

    <div class="studio-label">
      <b>最近产出</b>
      <span>{{ studio.artifacts.length }}</span>
    </div>
    <div v-if="studio.artifacts.length" class="studio-recent">
      <button
        v-for="item in studio.artifacts"
        :key="item.artifactId"
        type="button"
        class="studio-recent-item"
        :class="{ 'is-selected': studio.selectedId === item.artifactId }"
        @click="selectArtifact(item.artifactId)"
      >
        <span class="studio-recent-dot" :class="statusClass(item.status)" aria-hidden="true" />
        <span class="studio-recent-title">{{ item.title }}</span>
        <span class="studio-recent-state" :class="statusClass(item.status)">{{ statusLabel(item.status) }}</span>
      </button>
    </div>
    <p v-else-if="studio.loading" class="studio-empty-line">正在读取制作结果…</p>
    <p v-else class="studio-empty-line">还没有 ScribeFlow 产物。</p>

    <div v-if="studio.error || error" class="studio-error">{{ studio.error || error }}</div>

    <div class="studio-target">
      <div class="studio-label" style="padding: 0 0 4px">入库位置</div>
      <div class="studio-target-path">
        <span aria-hidden="true">▱</span>
        <span>笔记库 / {{ folder }}</span>
      </div>
      <div class="studio-target-actions">
        <button type="button" class="studio-link" @click="refresh">刷新</button>
        <button type="button" class="studio-link" @click="openCanvas">在画布中打开</button>
      </div>
    </div>
  </div>
</template>
