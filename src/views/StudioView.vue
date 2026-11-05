<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useRouter } from 'vue-router'

import * as api from '../lib/api'
import { loadStudio, selectedArtifact, studio } from '../stores/studio'
import type { NoteDetail } from '../types'

const router = useRouter()
const note = ref<NoteDetail | null>(null)
const noteError = ref('')
const opening = ref(false)
const openError = ref('')

// TODO(SF-002): live in-progress nodes come from ScribeFlow's /api/runs or a
// status sidecar. This view currently only knows artifacts that already exist.
const selected = computed(() => selectedArtifact.value)
const source = computed(() => selected.value?.source ?? null)
const run = computed(() => selected.value?.run ?? null)
const status = computed(() => selected.value?.status ?? '')
const statusLabel = computed(() => {
  if (status.value === 'published') return '已入库'
  if (status.value === 'running') return '进行中'
  if (status.value === 'pending') return '待入库'
  if (status.value === 'missing') return '笔记缺失'
  if (status.value === 'error') return '失败'
  return status.value || '已入库'
})
const preview = computed(() => {
  const body = note.value?.body ?? ''
  return body.replace(/^#{1,6}\s*/gm, '').replace(/\n{3,}/g, '\n\n').slice(0, 620)
})
const revisedAt = computed(() => formatDate(selected.value?.generatedAt ?? ''))
const folder = computed(() => studio.info?.folder ?? 'ScribeFlow')
const connected = computed(() => studio.info?.running === true)

watch(
  () => selected.value?.noteId,
  async (noteId) => {
    note.value = null
    noteError.value = ''
    if (!noteId || !selected.value?.noteExists) return
    try {
      const data = await api.getNote(noteId)
      if (data) note.value = data
      else noteError.value = '笔记文件存在，但还没有进入当前索引；刷新制作台或重建索引后重试。'
    } catch (cause) {
      noteError.value = api.errorText(cause)
    }
  },
  { immediate: true },
)

function formatDate(value: string): string {
  if (!value) return ''
  if (/^\d{4}-\d{2}-\d{2}$/.test(value)) return value.slice(5)
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) return value
  const mm = String(date.getMonth() + 1).padStart(2, '0')
  const dd = String(date.getDate()).padStart(2, '0')
  const hh = String(date.getHours()).padStart(2, '0')
  const min = String(date.getMinutes()).padStart(2, '0')
  return `${mm}-${dd} ${hh}:${min}`
}

function formatBytes(value: number): string {
  if (!value) return ''
  if (value < 1024) return `${value} B`
  if (value < 1024 * 1024) return `${Math.round(value / 1024)} KB`
  return `${(value / 1024 / 1024).toFixed(1)} MB`
}

function openNote(): void {
  if (!selected.value?.noteExists) return
  void router.push(`/note/${selected.value.noteId}`)
}

async function openCanvas(newProject = false): Promise<void> {
  opening.value = true
  openError.value = ''
  try {
    await api.openScribeFlow(newProject)
  } catch (cause) {
    openError.value = api.errorText(cause)
  } finally {
    opening.value = false
  }
}

async function refresh(): Promise<void> {
  await loadStudio()
}
</script>

<template>
  <div class="studio-view">
    <div v-if="selected" class="studio-main">
      <div class="studio-eyebrow">制作 / 最近产出</div>
      <div class="studio-title-row">
        <h1 class="studio-title">{{ selected.title }}</h1>
        <div class="studio-title-actions">
          <span class="studio-pill" :class="status === 'published' ? 'is-ok' : status === 'error' || status === 'missing' ? 'is-warn' : 'is-busy'">
            <i />{{ statusLabel }}
          </span>
          <button type="button" class="studio-btn studio-btn-sm studio-btn-primary" :disabled="!selected.noteExists" @click="openNote">
            打开笔记
          </button>
          <button type="button" class="studio-btn studio-btn-sm" :disabled="opening" @click="openCanvas(false)">
            在画布中查看
          </button>
        </div>
      </div>
      <div v-if="selected.error" class="studio-error">{{ selected.error }}</div>
      <div class="studio-meta-line">
        <template v-if="source">
          <b>{{ source.label || source.kind || '来源' }}</b>
          <span v-if="source.author"> · {{ source.author }}</span>
          <span v-if="selected.generatedAt"> · 生成于 {{ revisedAt }}</span>
        </template>
        <template v-else>由 ScribeFlow 生成 · {{ revisedAt || '生成时间未知' }}</template>
        <span v-if="selected.derived" class="studio-derived">从 front matter 降级读取</span>
      </div>

      <div class="studio-pipeline" aria-label="制作流程">
        <span class="studio-step is-done"><i>✓</i>来源</span>
        <span class="studio-connector" />
        <span class="studio-step" :class="{ 'is-done': selected.noteExists }"><i>{{ selected.noteExists ? '✓' : '·' }}</i>加工</span>
        <span class="studio-connector" />
        <span class="studio-step" :class="{ 'is-done': selected.noteExists }"><i>{{ selected.noteExists ? '✓' : '·' }}</i>入库</span>
        <span class="studio-pipeline-tail">{{ selected.revision > 0 ? `第 ${selected.revision} 版` : '' }}</span>
      </div>

      <div class="studio-preview">
        <div class="studio-preview-head">
          <span>笔记预览</span>
          <span class="studio-mono">{{ selected.noteRelPath }}</span>
        </div>
        <div v-if="note" class="studio-preview-body">
          <h2>{{ note.meta.title }}</h2>
          <p v-if="preview" class="studio-preview-text">{{ preview }}</p>
          <p v-else class="studio-preview-empty">这篇笔记还没有正文。</p>
          <div class="studio-chips">
            <span>id: {{ selected.noteId.slice(0, 10) }}…</span>
            <span v-if="note.meta.domain">{{ note.meta.domain }}</span>
            <span v-if="note.meta.people.length">人物: {{ note.meta.people.length }}</span>
            <span v-if="note.meta.events.length">事件: {{ note.meta.events.length }}</span>
            <span v-if="note.meta.periods.length">时期: {{ note.meta.periods.length }}</span>
          </div>
        </div>
        <div v-else-if="noteError" class="studio-preview-empty">{{ noteError }}</div>
        <div v-else class="studio-preview-empty">
          {{ selected.noteExists ? '正在打开笔记…' : '笔记文件还没有入库。' }}
        </div>
      </div>

      <div class="studio-log">
        <div class="studio-log-row is-done"><i /><time>{{ revisedAt || '—' }}</time><span>ScribeFlow 产出 {{ selected.noteRelPath }}</span></div>
        <div class="studio-log-row" :class="{ 'is-done': selected.noteExists }"><i /><time>{{ selected.noteExists ? '已入库' : '等待中' }}</time><span>{{ selected.noteExists ? '页边笔记索引已更新，标注锚点保留' : '等待 Markdown 落入 ScribeFlow/ 目录' }}</span></div>
        <div class="studio-log-row" :class="{ 'is-done': selected.noteExists }"><i /><time>{{ selected.annotationCount ? `${selected.annotationCount} 条` : '0 条' }}</time><span>标注仍只保存在页边笔记</span></div>
      </div>
      <div v-if="openError" class="studio-error">{{ openError }}</div>
    </div>

    <div v-else class="studio-empty">
      <div class="studio-empty-mark" aria-hidden="true">SF</div>
      <h1>还没有制作结果</h1>
      <p>在 ScribeFlow 里跑完一条画布，把输出目录设为这个库的 <code>{{ folder }}/</code>；结果会出现在这里，并自动进入页边笔记索引。</p>
      <div class="studio-empty-actions">
        <button type="button" class="studio-btn studio-btn-primary" :disabled="opening" @click="openCanvas(true)">打开 ScribeFlow 画布</button>
        <button type="button" class="studio-btn" @click="refresh">刷新</button>
      </div>
      <p class="studio-empty-foot">当前连接：{{ connected ? 'ScribeFlow 本机已启动' : '未检测到 127.0.0.1:8787' }} · 入库位置：{{ folder }}/</p>
    </div>

    <aside v-if="selected" class="studio-rail">
      <div class="studio-rail-head">
        <span>来源与运行</span>
        <span class="studio-rail-meta">{{ selected.derived ? 'front matter' : 'sidecar' }}</span>
      </div>

      <section class="studio-rail-block">
        <div class="studio-rail-label">来源</div>
        <div v-if="source" class="studio-source-card">
          <div class="studio-source-mark">{{ (source.label || source.kind || 'S').slice(0, 1) }}</div>
          <div class="studio-source-copy">
            <div class="studio-source-title">{{ source.label || source.kind }}</div>
            <div class="studio-source-sub">{{ source.author || '作者未提供' }}</div>
          </div>
        </div>
        <!-- TODO(SF-004): open the original URL after adding a restricted opener. -->
        <div v-if="source?.url" class="studio-source-url">{{ source.url }}</div>
        <p v-else class="studio-muted">没有原始链接。</p>
      </section>

      <section class="studio-rail-block">
        <div class="studio-rail-label">运行</div>
        <dl v-if="run" class="studio-kv">
          <div><dt>工程</dt><dd>{{ run.projectName || run.projectId || '—' }}</dd></div>
          <div><dt>流程</dt><dd>{{ run.prompt || '—' }}</dd></div>
          <div><dt>模型</dt><dd>{{ run.model || '—' }}</dd></div>
          <div><dt>运行</dt><dd class="studio-mono">{{ run.runId || '—' }}</dd></div>
          <div v-if="selected.usage"><dt>成本</dt><dd>{{ selected.usage.estimatedCost ? `${selected.usage.currency || 'CNY'} ${selected.usage.estimatedCost.toFixed(2)}` : '—' }}</dd></div>
        </dl>
        <p v-else class="studio-muted">ScribeFlow 没有写运行信息；这篇笔记只能按 front matter 显示。</p>
      </section>

      <section class="studio-rail-block">
        <div class="studio-rail-label">发布</div>
        <div class="studio-path">{{ folder }} / {{ selected.noteRelPath.split('/').pop() }}</div>
        <div class="studio-status-line">
          <i :class="{ 'is-on': selected.noteExists }" />
          {{ selected.noteExists ? '已写入并建立索引' : '笔记文件尚未出现' }}
        </div>
        <div class="studio-status-line">
          <i :class="{ 'is-on': selected.annotationCount > 0 }" />
          {{ selected.annotationCount }} 条标注
        </div>
      </section>

      <section v-if="selected.assets.length" class="studio-rail-block">
        <div class="studio-rail-label">本次产出</div>
        <div class="studio-file-list">
          <div v-for="asset in selected.assets" :key="asset.path || asset.name" class="studio-file">
            <span>{{ asset.name || asset.path }}</span>
            <span class="studio-mono">{{ formatBytes(asset.bytes) }}</span>
          </div>
        </div>
      </section>

      <section class="studio-rail-block">
        <div class="studio-rail-label">连接</div>
        <div class="studio-status-line">
          <i :class="{ 'is-on': connected }" />
          {{ connected ? 'ScribeFlow 本机服务已连接' : '未检测到 ScribeFlow（127.0.0.1:8787）' }}
        </div>
        <button type="button" class="studio-link" @click="openCanvas(false)">打开画布</button>
      </section>
    </aside>
  </div>
</template>
