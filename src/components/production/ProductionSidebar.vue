<script setup lang="ts">
import { computed, onMounted } from 'vue'
import {
  Cpu,
  LayoutDashboard,
  ListVideo,
  Plus,
  Rss,
  Settings2,
  Shapes,
  Sparkles,
} from 'lucide-vue-next'

import { formatTokens, statusLabel, templateById } from '../../lib/production'
import {
  activeJobs,
  finishedJobs,
  loadProduction,
  production,
  productionUi,
} from '../../stores/production'
import type { ProductionJob } from '../../types'

const jobs = computed(() => production.jobs)

function setView(view: typeof productionUi.view): void {
  productionUi.view = view
}

function openWizard(source = ''): void {
  productionUi.wizardSource = source
  productionUi.wizardTemplate = ''
  productionUi.wizardOpen = true
}

function openTask(job: ProductionJob): void {
  productionUi.taskId = job.id
}

function jobTemplateName(job: ProductionJob): string {
  return templateById(production.templates, job.templateId)?.name ?? job.templateId
}

function engineLabel(): string {
  if (!production.settings) return '读取中'
  if (!production.settings.hasAiKey) return 'AI 未配置'
  return '引擎就绪'
}

onMounted(() => {
  if (!production.loaded) void loadProduction()
})
</script>

<template>
  <div class="prod-side">
    <div class="prod-side-head">
      <span class="prod-side-title">制作</span>
      <span class="prod-engine-pill">
        <i class="prod-dot" :class="production.settings?.hasAiKey ? 'is-green' : 'is-gold'" />
        {{ engineLabel() }}
      </span>
    </div>

    <button type="button" class="prod-new" @click="openWizard()">
      <Plus :size="14" />
      新建制作
    </button>

    <nav class="prod-nav" aria-label="制作导航">
      <button type="button" :class="{ 'is-active': productionUi.view === 'home' }" @click="setView('home')">
        <LayoutDashboard :size="15" />
        工作台
      </button>
      <button type="button" :class="{ 'is-active': productionUi.view === 'tasks' }" @click="setView('tasks')">
        <ListVideo :size="15" />
        任务
        <span class="prod-nav-count">{{ jobs.length }}</span>
      </button>
      <button type="button" :class="{ 'is-active': productionUi.view === 'subscriptions' }" @click="setView('subscriptions')">
        <Rss :size="15" />
        订阅
      </button>
      <button type="button" :class="{ 'is-active': productionUi.view === 'templates' }" @click="setView('templates')">
        <Shapes :size="15" />
        模板
      </button>
      <button type="button" :class="{ 'is-active': productionUi.view === 'prompts' }" @click="setView('prompts')">
        <Sparkles :size="15" />
        提示词
      </button>
      <button type="button" :class="{ 'is-active': productionUi.view === 'settings' }" @click="setView('settings')">
        <Settings2 :size="15" />
        设置
      </button>
    </nav>

    <div class="prod-side-section">
      <div class="prod-side-label"><span>进行中</span><span>{{ activeJobs.length }}</span></div>
      <button
        v-for="job in activeJobs"
        :key="job.id"
        type="button"
        class="prod-mini-task"
        @click="openTask(job)"
      >
        <span class="prod-mini-title">{{ job.title }}</span>
        <span class="prod-mini-meta">
          <i class="prod-dot is-blue" />
          {{ statusLabel(job.status) }} · {{ job.progress }}%
        </span>
      </button>
      <div v-if="!activeJobs.length" class="prod-side-empty">没有进行中的制作</div>
    </div>

    <div class="prod-side-section">
      <div class="prod-side-label"><span>最近完成</span><span>{{ finishedJobs.length }}</span></div>
      <button
        v-for="job in finishedJobs.slice(0, 3)"
        :key="job.id"
        type="button"
        class="prod-mini-task"
        @click="openTask(job)"
      >
        <span class="prod-mini-title">{{ job.title }}</span>
        <span class="prod-mini-meta">
          <i class="prod-dot is-green" />
          {{ jobTemplateName(job) }} · {{ formatTokens(job.usage.promptTokens + job.usage.completionTokens) }}
        </span>
      </button>
      <div v-if="!finishedJobs.length" class="prod-side-empty">还没有完成的任务</div>
    </div>

    <div class="prod-side-foot">
      <div class="prod-side-target">
        <span aria-hidden="true">▱</span>
        <span>笔记库 / ScribeFlow</span>
      </div>
      <div style="display: flex; align-items: center; gap: 5px; margin-top: 6px">
        <Cpu :size="12" />
        本地 Rust 引擎 · 不占端口
      </div>
      <div class="prod-side-links">
        <button type="button" @click="setView('settings')">制作设置</button>
        <button type="button" @click="openWizard()">新建</button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.prod-mini-task :deep(svg) { flex: none; }
.prod-side-foot :deep(svg) { vertical-align: -2px; }
</style>
