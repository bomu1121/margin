<script setup lang="ts">
import { computed, nextTick, onMounted, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { Check, ExternalLink, Monitor, Moon, RotateCcw, Search, Sun } from 'lucide-vue-next'

import { THEMES, type ResolvedMode, type ThemeDefinition, type ThemeId, type ThemeMode } from '../lib/theme'
import { sectionById, SETTINGS_SECTIONS } from '../lib/settingsSections'
import { appearance, currentModeLabel, resetAppearance, resolvedMode, setTheme, setThemeMode } from '../stores/appearance'
import { loadProduction, productionUi } from '../stores/production'

const route = useRoute()
const router = useRouter()
const query = ref('')

const activeSection = computed(() => sectionById(String(route.params.section ?? '')) ?? SETTINGS_SECTIONS[0])
const currentTheme = computed(() => THEMES.find((theme) => theme.id === appearance.theme) ?? THEMES[0])
const currentMode = computed(() => currentModeLabel())
const modeOptions: { id: ThemeMode; label: string; icon: typeof Monitor }[] = [
  { id: 'system', label: '跟随系统', icon: Monitor },
  { id: 'light', label: '浅色', icon: Sun },
  { id: 'dark', label: '深色', icon: Moon },
]
const filteredSections = computed(() => {
  const value = query.value.trim().toLowerCase()
  if (!value) return SETTINGS_SECTIONS
  return SETTINGS_SECTIONS.filter((section) =>
    [section.label, section.description, ...section.keywords].some((item) => item.toLowerCase().includes(value)),
  )
})

function selectSection(id: string): void {
  void router.push({ name: 'settings', params: { section: id } })
}

function previewStyle(theme: ThemeDefinition, mode: ResolvedMode): Record<string, string> {
  const value = theme.preview[mode]
  return {
    '--pv-bg': value.bg,
    '--pv-surface': value.surface,
    '--pv-rail': value.rail,
    '--pv-ink': value.ink,
    '--pv-accent': value.accent,
    '--pv-warning': value.warning,
    '--pv-line': value.line,
  }
}

function chooseTheme(id: ThemeId): void {
  setTheme(id)
}

function onThemeKeydown(event: KeyboardEvent, index: number): void {
  if (!['ArrowRight', 'ArrowDown', 'ArrowLeft', 'ArrowUp'].includes(event.key)) return
  event.preventDefault()
  const step = event.key === 'ArrowRight' || event.key === 'ArrowDown' ? 1 : -1
  const next = (index + step + THEMES.length) % THEMES.length
  setTheme(THEMES[next].id)
  void nextTick(() => {
    const buttons = document.querySelectorAll<HTMLElement>('.settings-theme-card')
    buttons[next]?.focus()
  })
}

async function openProductionSettings(): Promise<void> {
  await loadProduction()
  productionUi.view = 'settings'
  await router.push('/studio')
}

watch(
  () => route.params.section,
  (section) => {
    if (section && !sectionById(String(section))) void router.replace({ name: 'settings', params: { section: 'appearance' } })
  },
)

onMounted(() => {
  if (!route.params.section) void router.replace({ name: 'settings', params: { section: 'appearance' } })
})
</script>

<template>
  <div class="settings-page">
    <header class="settings-head">
      <div>
        <div class="settings-kicker">SETTINGS</div>
        <h1>设置</h1>
        <p>外观、阅读、数据与制作。设置项会按类别持续补充，所有入口都在这里。</p>
      </div>
      <span class="settings-saved" aria-live="polite">主题：{{ currentTheme.label }} · {{ currentMode }}</span>
    </header>

    <div class="settings-layout">
      <aside class="settings-rail" aria-label="设置分类">
        <label class="settings-search">
          <Search :size="14" aria-hidden="true" />
          <input v-model="query" type="search" placeholder="搜索设置…" aria-label="搜索设置" />
        </label>

        <nav class="settings-nav" aria-label="设置分类">
          <button
            v-for="section in filteredSections"
            :key="section.id"
            type="button"
            class="settings-nav-item"
            :class="{ 'is-active': activeSection.id === section.id }"
            :aria-current="activeSection.id === section.id ? 'page' : undefined"
            @click="selectSection(section.id)"
          >
            <component :is="section.icon" :size="15" aria-hidden="true" />
            <span>
              <b>{{ section.label }}</b>
              <small>{{ section.description }}</small>
            </span>
            <i v-if="section.kind === 'planned'" class="settings-planned-pill">计划</i>
          </button>
          <p v-if="!filteredSections.length" class="settings-nav-empty">没有匹配的设置。</p>
        </nav>

        <p class="settings-rail-note">设置保存在本机；主题不随云同步。</p>
      </aside>

      <section class="settings-content" aria-live="polite">
        <article v-if="activeSection.id === 'appearance'" class="settings-panel">
          <header class="settings-panel-head">
            <div>
              <div class="settings-kicker">APPEARANCE</div>
              <h2>外观与主题</h2>
              <p>四套主题都遵守同一组语义：一个主色、三档表面、状态色只用于状态。</p>
            </div>
            <button type="button" class="settings-reset" :disabled="appearance.theme === 'neutral' && appearance.mode === 'system'" @click="resetAppearance">
              <RotateCcw :size="13" />恢复默认
            </button>
          </header>

          <section class="settings-block">
            <div class="settings-block-head">
              <h3>明暗模式</h3>
              <p>跟随系统会随操作系统的浅色/深色实时切换；也可以固定一个模式。</p>
            </div>
            <div class="settings-mode" role="group" aria-label="明暗模式">
              <button
                v-for="option in modeOptions"
                :key="option.id"
                type="button"
                class="settings-mode-button"
                :class="{ 'is-active': appearance.mode === option.id }"
                :aria-pressed="appearance.mode === option.id"
                @click="setThemeMode(option.id)"
              >
                <component :is="option.icon" :size="14" aria-hidden="true" />
                {{ option.label }}
              </button>
            </div>
            <p class="settings-current">当前：{{ currentTheme.label }} · {{ currentMode }}<template v-if="appearance.mode === 'system'">（系统当前为{{ resolvedMode === 'dark' ? '深色' : '浅色' }}）</template></p>
          </section>

          <section class="settings-block">
            <div class="settings-block-head">
              <h3>主题</h3>
              <p>预览按主题自己的固定色渲染，不会跟着当前主题变色；点击立即应用。</p>
            </div>
            <div class="settings-theme-grid" role="radiogroup" aria-label="主题">
              <button
                v-for="(theme, index) in THEMES"
                :key="theme.id"
                type="button"
                role="radio"
                class="settings-theme-card"
                :aria-checked="appearance.theme === theme.id"
                @click="chooseTheme(theme.id)"
                @keydown="onThemeKeydown($event, index)"
              >
                <span class="settings-theme-top">
                  <i class="settings-theme-index">{{ String.fromCharCode(65 + index) }}</i>
                  <b>{{ theme.label }}</b>
                  <Check v-if="appearance.theme === theme.id" :size="14" class="settings-theme-check" aria-hidden="true" />
                </span>
                <span class="settings-theme-preview" aria-hidden="true">
                  <span class="settings-theme-pane" :style="previewStyle(theme, 'light')">
                    <i class="settings-theme-pane-rail"></i>
                    <i class="settings-theme-pane-title"></i>
                    <i class="settings-theme-pane-line"></i>
                    <i class="settings-theme-pane-dot"></i>
                    <small>浅</small>
                  </span>
                  <span class="settings-theme-pane" :style="previewStyle(theme, 'dark')">
                    <i class="settings-theme-pane-rail"></i>
                    <i class="settings-theme-pane-title"></i>
                    <i class="settings-theme-pane-line"></i>
                    <i class="settings-theme-pane-dot"></i>
                    <small>深</small>
                  </span>
                </span>
                <span class="settings-theme-desc">{{ theme.tagline }}</span>
                <span class="settings-theme-ref">{{ theme.references }}</span>
              </button>
            </div>
          </section>

          <p class="settings-footnote">主题是设备本地偏好，不会写入笔记库，也不会覆盖其他设备的选择。</p>
        </article>

        <article v-else-if="activeSection.id === 'production'" class="settings-panel">
          <header class="settings-panel-head">
            <div>
              <div class="settings-kicker">PRODUCTION</div>
              <h2>制作与模型</h2>
              <p>AI 模型、语音识别、检索、订阅、云同步仍在制作台的设置里维护。</p>
            </div>
          </header>
          <div class="settings-external">
            <div>
              <b>打开制作设置</b>
              <p>这里不重复一套表单；制作侧设置保存后立即生效，订阅和同步也都在那边。</p>
            </div>
            <button type="button" class="settings-primary" @click="openProductionSettings">
              打开制作设置<ExternalLink :size="13" />
            </button>
          </div>
        </article>

        <article v-else-if="activeSection.id === 'about'" class="settings-panel">
          <header class="settings-panel-head">
            <div>
              <div class="settings-kicker">ABOUT</div>
              <h2>关于</h2>
              <p>页边笔记 · 本地优先的 Markdown 阅读与制作工具。</p>
            </div>
          </header>
          <dl class="settings-about">
            <div><dt>当前主题</dt><dd>{{ currentTheme.label }}（{{ currentMode }}）</dd></div>
            <div><dt>主题范围</dt><dd>本机；不参与云同步</dd></div>
            <div><dt>存储</dt><dd>笔记仍是普通 Markdown；索引与标注在本地 SQLite</dd></div>
          </dl>
        </article>

        <article v-else class="settings-panel">
          <header class="settings-panel-head">
            <div>
              <div class="settings-kicker">{{ activeSection.kind === 'planned' ? 'PLANNED' : 'SETTINGS' }}</div>
              <h2>{{ activeSection.label }}</h2>
              <p>{{ activeSection.description }}</p>
            </div>
          </header>
          <div class="settings-planned">
            <span class="settings-planned-pill">规划中</span>
            <b>这一节还没有接入</b>
            <p>设置页的导航、搜索和深链接已经就位；后续每个设置项只需登记到注册表并补一个面板，不会再散落在各个页面。</p>
          </div>
        </article>
      </section>
    </div>
  </div>
</template>
