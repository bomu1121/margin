// 插件管理器：左栏只负责“找插件”，右栏负责介绍、用法、设置和技术信息。
// 默认选中江山时序，避免地图插件只有一个技术描述、看不到功能和来源。
<script setup lang="ts">
import { computed, nextTick, onMounted, reactive, ref, watch } from 'vue'
import {
  Check,
  FolderOpen,
  Github,
  Globe,
  Heading1,
  Link2,
  LoaderCircle,
  Map as MapIcon,
  PackagePlus,
  PanelTop,
  Puzzle,
  RefreshCw,
  Scale,
  Search,
  ShieldCheck,
  Trash2,
  Type,
} from 'lucide-vue-next'

import * as api from '../lib/api'
import {
  installExamplePlugins,
  loadPlugins,
  plugins,
  savePluginSettings,
  setPluginEnabled,
  uninstallPlugin,
} from '../stores/plugins'
import type { PluginInfo, PluginPanel, PluginSetting } from '../types'

type FilterKey = 'all' | 'enabled' | 'panels' | 'examples'
type Tone = 'on' | 'warn' | 'bad' | 'off'

const notice = ref('')
const actionError = ref('')
const busy = ref('')
const drafts = reactive<Record<string, Record<string, unknown>>>({})
const uninstallArmed = ref('')
const query = ref('')
const filter = ref<FilterKey>('all')
const selectedId = ref('')
const assets = reactive<Record<string, { icon: string; screenshots: string[] }>>({})

const permissionLabels: Record<string, string> = {
  'notes:read': '读取笔记正文',
}

function permissionLabel(permission: string): string {
  return permissionLabels[permission] ?? permission
}

function panelHost(panel: PluginPanel): string {
  try {
    return new URL(panel.url.replace(/\{[^}]+\}/g, 'x')).host
  } catch {
    return '地址无效'
  }
}

function panelUsage(panel: PluginPanel): string {
  if (panel.context === 'historical-map') {
    return '在历史笔记标题右侧点「打开地图」；面板顶部可开关正文地名。'
  }
  return '由插件命令打开。'
}

function settingsDraft(plugin: PluginInfo): Record<string, unknown> {
  if (!drafts[plugin.id]) drafts[plugin.id] = { ...(plugin.settingsValues || {}) }
  return drafts[plugin.id]
}

function resetDraft(plugin: PluginInfo): void {
  drafts[plugin.id] = { ...(plugin.settingsValues || {}) }
}

function isDirty(plugin: PluginInfo): boolean {
  return JSON.stringify(settingsDraft(plugin)) !== JSON.stringify(plugin.settingsValues || {})
}

function flash(message: string): void {
  notice.value = message
  window.setTimeout(() => {
    if (notice.value === message) notice.value = ''
  }, 2600)
}

function pluginStatus(plugin: PluginInfo): { label: string; tone: Tone } {
  if (plugin.error) return { label: '配置错误', tone: 'bad' }
  if (plugin.needsPermission) return { label: '缺少权限', tone: 'warn' }
  if (plugin.enabled) return { label: '已启用', tone: 'on' }
  return { label: '未启用', tone: 'off' }
}

function toggleLabel(plugin: PluginInfo): string {
  if (plugin.error) return '配置错误'
  if (plugin.enabled && !plugin.needsPermission) return '禁用'
  return plugin.needsPermission ? '重新授权' : '启用'
}

function isExample(plugin: PluginInfo): boolean {
  return plugin.id.startsWith('example.')
}

function pluginGlyph(plugin: PluginInfo) {
  if (plugin.id === 'jiangshan.atlas') return MapIcon
  if (plugin.id.includes('word-count')) return Type
  if (plugin.id.includes('heading')) return Heading1
  if (plugin.id.includes('link')) return Link2
  return Puzzle
}

async function resolveAsset(plugin: PluginInfo, value: string): Promise<string> {
  try {
    return (await api.pluginSiteUrl(plugin.id, value)) || ''
  } catch {
    return ''
  }
}

async function resolveAssets(): Promise<void> {
  for (const plugin of plugins.items) {
    const icon = plugin.icon ? await resolveAsset(plugin, plugin.icon) : ''
    const screenshots: string[] = []
    for (const shot of plugin.screenshots) {
      const url = await resolveAsset(plugin, shot)
      if (url) screenshots.push(url)
    }
    assets[plugin.id] = { icon, screenshots }
  }
}

const selected = computed(() => plugins.items.find((plugin) => plugin.id === selectedId.value) || null)
const selectedShot = computed(() => {
  const plugin = selected.value
  if (!plugin) return ''
  return assets[plugin.id]?.screenshots[0] || ''
})

const activePlugins = computed(() =>
  plugins.items.filter((plugin) => plugin.enabled && !plugin.error && !plugin.needsPermission),
)
const enabledCount = computed(() => activePlugins.value.length)
const commandCount = computed(() => activePlugins.value.reduce((sum, plugin) => sum + plugin.commands.length, 0))
const panelCount = computed(() => activePlugins.value.reduce((sum, plugin) => sum + plugin.panels.length, 0))

const filterOptions = computed<Array<{ key: FilterKey; label: string; count: number }>>(() => [
  { key: 'all', label: '全部', count: plugins.items.length },
  {
    key: 'enabled',
    label: '已启用',
    count: plugins.items.filter((plugin) => plugin.enabled && !plugin.error && !plugin.needsPermission).length,
  },
  { key: 'panels', label: '有面板', count: plugins.items.filter((plugin) => plugin.panels.length > 0).length },
  { key: 'examples', label: '示例', count: plugins.items.filter((plugin) => isExample(plugin)).length },
])

const filteredPlugins = computed(() => {
  const keyword = query.value.trim().toLowerCase()
  return plugins.items.filter((plugin) => {
    if (filter.value === 'enabled' && !(plugin.enabled && !plugin.error && !plugin.needsPermission)) return false
    if (filter.value === 'panels' && !plugin.panels.length) return false
    if (filter.value === 'examples' && !isExample(plugin)) return false
    if (!keyword) return true
    return [plugin.name, plugin.id, plugin.author, plugin.description, ...plugin.features]
      .join(' ')
      .toLowerCase()
      .includes(keyword)
  })
})

function ensureSelection(): void {
  const visible = filteredPlugins.value
  if (visible.some((plugin) => plugin.id === selectedId.value)) return
  const atlas = visible.find((plugin) => plugin.id === 'jiangshan.atlas')
  selectedId.value = (atlas || visible[0])?.id || ''
}

function select(id: string): void {
  selectedId.value = id
  uninstallArmed.value = ''
}

watch([query, filter], () => {
  ensureSelection()
})

// 窄窗时左栏变成横向列表；默认选中的江山时序在第三位，需要滚进可视区。
watch(selectedId, () => {
  void nextTick(() => {
    document
      .querySelector<HTMLElement>('.plugin-row.is-active')
      ?.scrollIntoView({ block: 'nearest', inline: 'center' })
  })
})

async function refresh(): Promise<void> {
  busy.value = 'refresh'
  actionError.value = ''
  try {
    await loadPlugins(true)
    for (const plugin of plugins.items) resetDraft(plugin)
    await resolveAssets()
    ensureSelection()
  } catch (cause) {
    actionError.value = api.errorText(cause)
  } finally {
    busy.value = ''
  }
}

async function openFolder(): Promise<void> {
  try {
    const path = await api.pluginOpenDir()
    flash(`插件目录：${path}`)
  } catch (cause) {
    actionError.value = api.errorText(cause)
  }
}

async function installExamples(): Promise<void> {
  busy.value = 'install'
  actionError.value = ''
  try {
    await installExamplePlugins()
    for (const plugin of plugins.items) resetDraft(plugin)
    await resolveAssets()
    ensureSelection()
    flash('内置插件已写入；重装会保留启用状态和已有设置。')
  } catch (cause) {
    actionError.value = api.errorText(cause)
  } finally {
    busy.value = ''
  }
}

async function toggle(plugin: PluginInfo): Promise<void> {
  busy.value = plugin.id
  actionError.value = ''
  try {
    const next = await setPluginEnabled(plugin, !plugin.enabled || plugin.needsPermission)
    flash(next.enabled ? `已启用「${next.name}」` : `已禁用「${next.name}」`)
  } catch (cause) {
    actionError.value = api.errorText(cause)
  } finally {
    busy.value = ''
  }
}

async function saveSettings(plugin: PluginInfo): Promise<void> {
  busy.value = `settings:${plugin.id}`
  actionError.value = ''
  try {
    const next = await savePluginSettings(plugin, { ...settingsDraft(plugin) })
    resetDraft(next)
    flash(`已保存「${next.name}」的设置`)
  } catch (cause) {
    actionError.value = api.errorText(cause)
  } finally {
    busy.value = ''
  }
}

async function remove(plugin: PluginInfo): Promise<void> {
  busy.value = `remove:${plugin.id}`
  actionError.value = ''
  try {
    await uninstallPlugin(plugin.id)
    uninstallArmed.value = ''
    flash(`已卸载「${plugin.name}」`)
    ensureSelection()
  } catch (cause) {
    actionError.value = api.errorText(cause)
  } finally {
    busy.value = ''
  }
}

async function openLink(url: string): Promise<void> {
  if (!url) return
  try {
    await api.openExternalUrl(url)
  } catch (cause) {
    actionError.value = api.errorText(cause)
  }
}

function setValue(plugin: PluginInfo, setting: PluginSetting, value: unknown): void {
  settingsDraft(plugin)[setting.key] = value
}

onMounted(() => {
  void refresh()
})
</script>

<template>
  <div class="plugins-page">
    <header class="plugins-head">
      <div class="plugins-heading">
        <h1>插件</h1>
        <p>
          {{ plugins.items.length }} 个已安装 · {{ enabledCount }} 个已启用 ·
          {{ commandCount }} 个命令 · {{ panelCount }} 个面板
        </p>
      </div>
      <div class="plugins-actions">
        <button type="button" class="prod-btn" :disabled="busy === 'install'" @click="installExamples">
          <LoaderCircle v-if="busy === 'install'" :size="13" class="plugin-spin" />
          <PackagePlus v-else :size="13" /> 安装内置插件
        </button>
        <button type="button" class="prod-btn is-icon" title="打开插件目录" @click="openFolder">
          <FolderOpen :size="14" />
        </button>
        <button type="button" class="prod-btn is-icon" title="重新扫描" :disabled="busy === 'refresh'" @click="refresh">
          <LoaderCircle v-if="busy === 'refresh'" :size="13" class="plugin-spin" />
          <RefreshCw v-else :size="14" />
        </button>
      </div>
    </header>

    <p v-if="notice" class="plugins-notice">{{ notice }}</p>
    <p v-if="actionError" class="plugins-error">{{ actionError }}</p>
    <p v-else-if="plugins.error" class="plugins-error">{{ plugins.error }}</p>

    <div v-if="plugins.loading && !plugins.items.length" class="plugins-empty">
      <LoaderCircle :size="16" class="plugin-spin" /> 正在读取插件…
    </div>
    <div v-else-if="!plugins.items.length" class="plugins-empty">
      <Puzzle :size="22" />
      <b>还没有插件</b>
      <p>安装内置插件，或把插件目录放进插件文件夹后点右上角的「重新扫描」。</p>
      <button type="button" class="prod-btn is-primary" :disabled="busy === 'install'" @click="installExamples">
        <PackagePlus :size="13" /> 安装内置插件
      </button>
    </div>

    <div v-else class="plugins-workbench">
      <aside class="plugin-rail">
        <div class="plugin-rail-search">
          <Search :size="13" />
          <input v-model="query" type="search" placeholder="搜索插件、作者或功能" />
        </div>
        <div class="plugin-rail-filters">
          <button
            v-for="option in filterOptions"
            :key="option.key"
            type="button"
            :class="{ 'is-active': filter === option.key }"
            @click="filter = option.key"
          >
            {{ option.label }} <b>{{ option.count }}</b>
          </button>
        </div>
        <div class="plugin-rail-list">
          <button
            v-for="plugin in filteredPlugins"
            :key="plugin.id"
            type="button"
            class="plugin-row"
            :class="{ 'is-active': plugin.id === selectedId }"
            @click="select(plugin.id)"
          >
            <span class="plugin-avatar is-small">
              <img v-if="assets[plugin.id]?.icon" :src="assets[plugin.id].icon" alt="" />
              <component :is="pluginGlyph(plugin)" v-else :size="15" />
            </span>
            <span class="plugin-row-body">
              <span class="plugin-row-title">{{ plugin.name }} <em>v{{ plugin.version || '?' }}</em></span>
              <span class="plugin-row-desc">{{ plugin.description || '没有描述' }}</span>
              <span v-if="isExample(plugin)" class="plugin-tag">示例</span>
            </span>
            <span class="plugin-status-dot" :class="`is-${pluginStatus(plugin).tone}`" :title="pluginStatus(plugin).label" />
          </button>
          <p v-if="!filteredPlugins.length" class="plugin-rail-empty">没有匹配的插件。</p>
        </div>
        <p class="plugin-rail-hint">运行入口在笔记标题下方的「插件」菜单。</p>
      </aside>

      <section v-if="selected" class="plugin-detail">
        <header class="plugin-detail-head">
          <span class="plugin-avatar is-large">
            <img v-if="assets[selected.id]?.icon" :src="assets[selected.id].icon" alt="" />
            <component :is="pluginGlyph(selected)" v-else :size="24" />
          </span>
          <div class="plugin-detail-title">
            <h2>{{ selected.name }} <em>v{{ selected.version || '?' }}</em></h2>
            <p>
              <span>{{ selected.author || selected.id }}</span>
              <span class="plugin-state" :class="`is-${pluginStatus(selected).tone}`">
                {{ pluginStatus(selected).label }}
              </span>
              <span v-if="isExample(selected)" class="plugin-tag">示例</span>
            </p>
          </div>
          <div class="plugin-detail-actions">
            <button
              v-if="!selected.error"
              type="button"
              class="prod-btn is-small"
              :class="{ 'is-primary': !selected.enabled || selected.needsPermission }"
              :disabled="busy === selected.id"
              @click="toggle(selected)"
            >
              <LoaderCircle v-if="busy === selected.id" :size="12" class="plugin-spin" />
              {{ toggleLabel(selected) }}
            </button>
            <template v-if="uninstallArmed === selected.id">
              <button
                type="button"
                class="prod-btn is-small is-danger"
                :disabled="busy === `remove:${selected.id}`"
                @click="remove(selected)"
              >
                确认卸载
              </button>
              <button type="button" class="prod-btn is-small" @click="uninstallArmed = ''">取消</button>
            </template>
            <button
              v-else
              type="button"
              class="plugin-icon-btn"
              title="卸载这个插件"
              @click="uninstallArmed = selected.id"
            >
              <Trash2 :size="14" />
            </button>
          </div>
        </header>

        <p class="plugin-detail-desc">{{ selected.description || '这个插件没有写描述。' }}</p>
        <p v-if="isExample(selected)" class="plugin-example-note">
          内置示例：用来演示插件 Worker 的只读命令接口，可以关掉，也可以照着写自己的插件。
        </p>

        <div v-if="selected.homepage || selected.repository || selected.license" class="plugin-links">
          <button v-if="selected.homepage" type="button" @click="openLink(selected.homepage)">
            <Globe :size="13" /> 官网
          </button>
          <button v-if="selected.repository" type="button" @click="openLink(selected.repository)">
            <Github :size="13" /> 开源仓库
          </button>
          <span v-if="selected.license" class="plugin-license"><Scale :size="12" /> {{ selected.license }}</span>
        </div>
        <p v-if="selected.license" class="plugin-license-note">
          许可证以仓库 LICENSE 为准；站点内第三方数据可能另有限制。
        </p>

        <figure v-if="selectedShot" class="plugin-shot">
          <img :src="selectedShot" :alt="`${selected.name} 截图`" loading="lazy" />
        </figure>

        <p v-if="selected.error" class="plugin-detail-error">{{ selected.error }}</p>

        <template v-else>
          <section v-if="selected.features.length" class="plugin-section">
            <h3>能做什么</h3>
            <ul class="plugin-features">
              <li v-for="feature in selected.features" :key="feature">
                <Check :size="13" />
                <span>{{ feature }}</span>
              </li>
            </ul>
          </section>

          <section class="plugin-section">
            <h3>使用方式</h3>
            <div class="plugin-usage-list">
              <div v-for="command in selected.commands" :key="command.id" class="plugin-usage-row">
                <span class="plugin-usage-icon"><Puzzle :size="14" /></span>
                <div>
                  <b>
                    {{ command.title }}
                    <em v-if="command.primary" class="plugin-badge">主操作</em>
                    <em v-if="command.when === 'history'" class="plugin-badge is-muted">仅历史笔记</em>
                  </b>
                  <small v-if="command.description">{{ command.description }}</small>
                </div>
              </div>
              <div v-for="panel in selected.panels" :key="panel.id" class="plugin-usage-row">
                <span class="plugin-usage-icon"><PanelTop :size="14" /></span>
                <div>
                  <b>{{ panel.title }} <em class="plugin-badge">面板</em></b>
                  <small>{{ panel.width }}×{{ panel.height }} · {{ panel.local ? '本地站点' : panelHost(panel) }}</small>
                  <small class="is-hint">{{ panelUsage(panel) }}</small>
                </div>
              </div>
              <p v-if="!selected.commands.length && !selected.panels.length" class="plugin-muted">
                这个插件没有声明命令或面板。
              </p>
            </div>
          </section>

          <section v-if="selected.settings.length" class="plugin-section">
            <h3>
              设置
              <span v-if="isDirty(selected)" class="plugin-dirty">未保存</span>
            </h3>
            <div class="plugin-setting-list">
              <label v-for="setting in selected.settings" :key="setting.key" class="plugin-setting">
                <span>{{ setting.label }}</span>
                <input
                  v-if="setting.type === 'boolean'"
                  type="checkbox"
                  :checked="Boolean(settingsDraft(selected)[setting.key])"
                  @change="setValue(selected, setting, ($event.target as HTMLInputElement).checked)"
                />
                <select
                  v-else-if="setting.type === 'select'"
                  class="prod-select"
                  :value="String(settingsDraft(selected)[setting.key] ?? '')"
                  @change="setValue(selected, setting, ($event.target as HTMLSelectElement).value)"
                >
                  <option v-for="option in setting.options" :key="option" :value="option">
                    {{ setting.optionLabels?.[option] ?? option }}
                  </option>
                </select>
                <input
                  v-else-if="setting.type === 'number'"
                  type="number"
                  class="plugin-input"
                  :value="Number(settingsDraft(selected)[setting.key] ?? 0)"
                  @input="setValue(selected, setting, Number(($event.target as HTMLInputElement).value))"
                />
                <input
                  v-else
                  type="text"
                  class="plugin-input"
                  :placeholder="setting.placeholder || ''"
                  :value="String(settingsDraft(selected)[setting.key] ?? '')"
                  @input="setValue(selected, setting, ($event.target as HTMLInputElement).value)"
                />
              </label>
            </div>
            <div class="plugin-setting-foot">
              <button
                type="button"
                class="prod-btn is-small"
                :disabled="!isDirty(selected) || busy === `settings:${selected.id}`"
                @click="saveSettings(selected)"
              >
                <LoaderCircle v-if="busy === `settings:${selected.id}`" :size="12" class="plugin-spin" />
                保存设置
              </button>
            </div>
          </section>

          <section class="plugin-section">
            <h3>权限</h3>
            <div v-for="permission in selected.permissions" :key="permission" class="plugin-permission">
              <ShieldCheck :size="14" />
              <div>
                <b>{{ permissionLabel(permission) }}</b>
                <small>读取整个笔记库；插件不能写笔记、联网或访问 Tauri IPC。</small>
              </div>
            </div>
            <p v-if="!selected.permissions.length" class="plugin-muted">不需要权限。</p>
          </section>

          <details class="plugin-tech">
            <summary>技术信息</summary>
            <dl>
              <div><dt>插件 ID</dt><dd>{{ selected.id }}</dd></div>
              <div><dt>API 版本</dt><dd>{{ selected.apiVersion }}</dd></div>
              <div><dt>入口文件</dt><dd>{{ selected.main }}</dd></div>
              <div><dt>安装路径</dt><dd class="is-path">{{ selected.path }}</dd></div>
            </dl>
          </details>
        </template>
      </section>
    </div>

    <details class="plugins-security">
      <summary>安全与边界</summary>
      <p>
        命令插件不能写笔记、不能联网、不能访问 Tauri IPC；面板插件只拿 URL 参数，在跨域 sandbox iframe
        里加载，不共享登录态。启用即信任它读取整个笔记库；插件状态只存本机，不参与同步。
      </p>
    </details>
  </div>
</template>
