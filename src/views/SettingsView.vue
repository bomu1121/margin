<script setup lang="ts">
import { computed, nextTick, onMounted, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { Check, Copy, ExternalLink, FlaskConical, Monitor, Moon, RotateCcw, Search, ShieldAlert, Sun } from 'lucide-vue-next'

import { mcpConfigRead, mcpConfigSave, mcpConfigTest } from '../lib/api'
import { THEMES, type ResolvedMode, type ThemeDefinition, type ThemeId, type ThemeMode } from '../lib/theme'
import { sectionById, SETTINGS_SECTIONS } from '../lib/settingsSections'
import { appearance, currentModeLabel, resetAppearance, resolvedMode, setTheme, setThemeMode } from '../stores/appearance'
import { loadProduction, productionUi } from '../stores/production'
import type { McpConfigView, McpTestResult } from '../types'

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

const mcpConfig = ref<McpConfigView | null>(null)
const mcpText = ref('')
const mcpTests = ref<McpTestResult[]>([])
const mcpBusy = ref(false)
const mcpMessage = ref('')
const mcpError = ref('')
const mcpDirty = computed(() => Boolean(mcpConfig.value) && mcpText.value !== mcpConfig.value?.text)
const mcpCanSave = computed(
  () => mcpDirty.value || Boolean(mcpConfig.value && !mcpConfig.value.exists && mcpText.value.trim()),
)
const mcpDiskState = computed(() => {
  if (mcpDirty.value) return '有未保存的修改'
  if (mcpConfig.value && !mcpConfig.value.exists) return '尚未保存（当前为示例）'
  return '已与磁盘一致'
})

async function loadMcp(): Promise<void> {
  mcpError.value = ''
  try {
    const view = await mcpConfigRead()
    mcpConfig.value = view
    mcpText.value = view.text
    mcpTests.value = []
    mcpMessage.value = ''
  } catch (error) {
    mcpError.value = String(error)
  }
}

async function saveMcp(): Promise<void> {
  if (mcpBusy.value) return
  mcpBusy.value = true
  mcpError.value = ''
  mcpMessage.value = ''
  try {
    const view = await mcpConfigSave(mcpText.value)
    mcpConfig.value = view
    mcpText.value = view.text
    mcpTests.value = []
    mcpMessage.value = '已保存；下一次任务开始时会读取新配置。'
  } catch (error) {
    mcpError.value = String(error)
  } finally {
    mcpBusy.value = false
  }
}

async function testMcp(): Promise<void> {
  if (mcpBusy.value) return
  mcpBusy.value = true
  mcpError.value = ''
  mcpMessage.value = ''
  mcpTests.value = []
  try {
    mcpTests.value = await mcpConfigTest(mcpText.value)
    const passed = mcpTests.value.filter((item) => item.ok).length
    mcpMessage.value = mcpTests.value.length
      ? `测试完成：${passed}/${mcpTests.value.length} 个 server 连接成功。`
      : '没有启用的 server。'
  } catch (error) {
    mcpError.value = String(error)
  } finally {
    mcpBusy.value = false
  }
}

function resetMcp(): void {
  mcpText.value = mcpConfig.value?.text ?? ''
  mcpTests.value = []
  mcpError.value = ''
  mcpMessage.value = ''
}

async function copyMcpPath(): Promise<void> {
  if (!mcpConfig.value) return
  try {
    await navigator.clipboard.writeText(mcpConfig.value.path)
    mcpMessage.value = '配置路径已复制。'
  } catch {
    mcpError.value = '复制失败，请手动选中路径。'
  }
}

watch(mcpText, () => {
  if (!mcpTests.value.length && !mcpError.value && !mcpMessage.value) return
  mcpTests.value = []
  mcpError.value = ''
  mcpMessage.value = ''
})

watch(
  () => activeSection.value.id,
  (id) => {
    if (id === 'mcp' && !mcpConfig.value) void loadMcp()
  },
  { immediate: true },
)

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

        <article v-else-if="activeSection.id === 'mcp'" class="settings-panel">
          <header class="settings-panel-head">
            <div>
              <div class="settings-kicker">MCP · EXPERIMENTAL</div>
              <h2>MCP 连接</h2>
              <p>让页边的 Agent 调用外部 MCP server 的工具，例如浏览器、PDF、OCR。默认关闭，配置只在任务开始时读取。</p>
            </div>
            <span v-if="mcpConfig" class="mcp-count">{{ mcpConfig.enabledCount }}/{{ mcpConfig.serverCount }} 启用</span>
          </header>

          <section class="settings-block">
            <div class="settings-block-head">
              <h3>配置文件</h3>
              <p>写入应用数据目录下的 <code>mcp.json</code>，不会同步到笔记库。没有文件时显示一份禁用示例，保存后才会生效。</p>
            </div>
            <div class="mcp-path">
              <code>{{ mcpConfig?.path || '读取中…' }}</code>
              <button type="button" class="mcp-icon-button" title="复制路径" aria-label="复制配置路径" :disabled="!mcpConfig" @click="copyMcpPath">
                <Copy :size="14" />
              </button>
            </div>
            <textarea
              v-model="mcpText"
              class="mcp-editor"
              spellcheck="false"
              autocomplete="off"
              aria-label="MCP 配置 JSON"
              placeholder="加载配置中…"
            />
            <p v-if="mcpConfig?.error" class="mcp-error">{{ mcpConfig.error }}</p>
            <div class="mcp-actions">
              <span class="mcp-status" :class="{ 'is-dirty': mcpDirty || (mcpConfig && !mcpConfig.exists) }">{{ mcpDiskState }}</span>
              <button type="button" class="settings-reset" :disabled="!mcpDirty || mcpBusy" @click="resetMcp">
                <RotateCcw :size="13" />撤销
              </button>
              <button type="button" class="settings-reset" :disabled="mcpBusy" @click="testMcp">
                <FlaskConical :size="13" />测试连接
              </button>
              <button type="button" class="settings-primary" :disabled="!mcpCanSave || mcpBusy" @click="saveMcp">
                <Check :size="13" />保存
              </button>
            </div>
            <p v-if="mcpError" class="mcp-error" role="alert">{{ mcpError }}</p>
            <p v-else-if="mcpMessage" class="mcp-message" role="status">{{ mcpMessage }}</p>
          </section>

          <section v-if="mcpTests.length" class="settings-block">
            <div class="settings-block-head">
              <h3>测试结果</h3>
              <p>测试会真实启动 server；stdio server 用完立即关闭。失败只影响这一项，不会阻断笔记任务。</p>
            </div>
            <div class="mcp-results">
              <div v-for="test in mcpTests" :key="test.id" class="mcp-result" :class="{ 'is-ok': test.ok }">
                <div class="mcp-result-head">
                  <b>{{ test.id }}</b>
                  <span>{{ test.transport || 'config' }} · {{ test.elapsedMs }} ms</span>
                  <i class="mcp-result-state">{{ test.ok ? '通过' : '失败' }}</i>
                </div>
                <p v-if="test.ok && test.tools.length" class="mcp-result-tools">
                  <code v-for="tool in test.tools" :key="tool">{{ tool }}</code>
                </p>
                <p v-else-if="test.ok" class="mcp-result-empty">连接成功，但没有发现工具。</p>
                <p v-else class="mcp-result-error">{{ test.error }}</p>
              </div>
            </div>
          </section>

          <section class="settings-block mcp-security">
            <div class="settings-block-head">
              <h3><ShieldAlert :size="15" />边界与风险</h3>
              <p>MCP server 不是沙箱：stdio server 以当前用户权限运行任意程序，HTTP server 也会访问网络。只添加你信任的来源。</p>
            </div>
            <ul>
              <li><code>tools.allow</code> 为空时不会暴露任何工具；要放行全部，显式写 <code>"*"</code>。</li>
              <li>每个 server 最多暴露 20 个工具，描述截断到 500 字符，单次结果截断到 20,000 字符。</li>
              <li>工具输出会进入模型上下文；不要把密钥写进 server 的返回值。</li>
              <li>本版只有页边作为客户端。页边自己的只读 MCP server 尚未实现。</li>
            </ul>
          </section>
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
          <div class="settings-external">
            <div>
              <b>旧事实档案</b>
              <p>事实/溯源页已停用，只保留为历史数据档案；日常出处请用笔记右栏的「出处」。</p>
            </div>
            <router-link to="/facts" class="settings-primary">打开事实库<ExternalLink :size="13" /></router-link>
          </div>
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
