<script setup lang="ts">
import { computed, onMounted, reactive, ref } from 'vue'
import { FolderOpen, LoaderCircle, PackagePlus, Puzzle, RefreshCw, ShieldCheck, Trash2 } from 'lucide-vue-next'

import * as api from '../lib/api'
import {
  installExamplePlugins,
  loadPlugins,
  plugins,
  savePluginSettings,
  setPluginEnabled,
  uninstallPlugin,
} from '../stores/plugins'
import type { PluginInfo, PluginSetting } from '../types'

const notice = ref('')
const actionError = ref('')
const busy = ref('')
const drafts = reactive<Record<string, Record<string, unknown>>>({})
const uninstallArmed = ref('')
let uninstallTimer = 0

const permissionLabels: Record<string, string> = {
  'notes:read': '读取笔记正文',
}

function permissionLabel(permission: string): string {
  return permissionLabels[permission] ?? permission
}

function panelDomain(url: string): string {
  try {
    return new URL(url.replace(/\{[^}]+\}/g, 'x')).host
  } catch {
    return '地址无效'
  }
}

function settingsDraft(plugin: PluginInfo): Record<string, unknown> {
  if (!drafts[plugin.id]) drafts[plugin.id] = { ...(plugin.settingsValues || {}) }
  return drafts[plugin.id]
}

function resetDraft(plugin: PluginInfo): void {
  drafts[plugin.id] = { ...(plugin.settingsValues || {}) }
}

function flash(message: string): void {
  notice.value = message
  window.setTimeout(() => {
    if (notice.value === message) notice.value = ''
  }, 2600)
}

async function refresh(): Promise<void> {
  busy.value = 'refresh'
  actionError.value = ''
  try {
    await loadPlugins(true)
    for (const plugin of plugins.items) resetDraft(plugin)
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
    flash('示例插件已写入插件目录；默认未启用。')
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
    await savePluginSettings(plugin, { ...settingsDraft(plugin) })
    flash(`已保存「${plugin.name}」的设置`)
  } catch (cause) {
    actionError.value = api.errorText(cause)
  } finally {
    busy.value = ''
  }
}

function armUninstall(plugin: PluginInfo): void {
  if (uninstallArmed.value !== plugin.id) {
    uninstallArmed.value = plugin.id
    window.clearTimeout(uninstallTimer)
    uninstallTimer = window.setTimeout(() => {
      if (uninstallArmed.value === plugin.id) uninstallArmed.value = ''
    }, 4000)
    return
  }
  void remove(plugin)
}

async function remove(plugin: PluginInfo): Promise<void> {
  busy.value = `remove:${plugin.id}`
  actionError.value = ''
  try {
    await uninstallPlugin(plugin.id)
    uninstallArmed.value = ''
    flash(`已卸载「${plugin.name}」`)
  } catch (cause) {
    actionError.value = api.errorText(cause)
  } finally {
    busy.value = ''
  }
}

function setValue(plugin: PluginInfo, setting: PluginSetting, value: unknown): void {
  settingsDraft(plugin)[setting.key] = value
}

const enabledCount = computed(() => plugins.items.filter((plugin) => plugin.enabled && !plugin.needsPermission).length)
const panelCount = computed(() => plugins.items.reduce((sum, plugin) => sum + plugin.panels.length, 0))

onMounted(() => {
  void refresh()
})
</script>

<template>
  <div class="plugins-page">
    <header class="plugins-head">
      <div>
        <div class="plugins-eyebrow">扩展 / 本地插件</div>
        <h1>插件</h1>
        <p>命令插件在独立 Worker 里运行；面板插件在跨域 sandbox iframe 里加载。两者默认只读，启用前需要授权。</p>
      </div>
      <div class="plugins-actions">
        <button type="button" class="prod-btn" :disabled="busy === 'install'" @click="installExamples">
          <LoaderCircle v-if="busy === 'install'" :size="13" class="plugin-spin" />
          <PackagePlus v-else :size="13" /> 安装内置插件
        </button>
        <button type="button" class="prod-btn" @click="openFolder"><FolderOpen :size="13" /> 打开插件目录</button>
        <button type="button" class="prod-btn" :disabled="busy === 'refresh'" @click="refresh">
          <LoaderCircle v-if="busy === 'refresh'" :size="13" class="plugin-spin" />
          <RefreshCw v-else :size="13" /> 重新扫描
        </button>
      </div>
    </header>

    <p v-if="notice" class="plugins-notice">{{ notice }}</p>
    <p v-if="actionError" class="plugins-error">{{ actionError }}</p>
    <p v-else-if="plugins.error" class="plugins-error">{{ plugins.error }}</p>

    <div class="plugins-summary">
      <span><b>{{ plugins.items.length }}</b> 个插件</span>
      <span><b>{{ enabledCount }}</b> 个已启用</span>
      <span><b>{{ plugins.items.reduce((sum, plugin) => sum + plugin.commands.length, 0) }}</b> 个命令</span>
      <span><b>{{ panelCount }}</b> 个面板</span>
      <span class="plugins-summary-hint">运行入口在笔记页标题下方的「插件」菜单</span>
    </div>

    <div v-if="plugins.loading && !plugins.items.length" class="plugins-empty">
      <LoaderCircle :size="16" class="plugin-spin" /> 正在读取插件…
    </div>
    <div v-else-if="!plugins.items.length" class="plugins-empty">
      <Puzzle :size="22" />
      <b>还没有插件</b>
      <p>安装内置插件，或把插件目录放进插件文件夹后点「重新扫描」。</p>
      <button type="button" class="prod-btn is-primary" :disabled="busy === 'install'" @click="installExamples">
        <PackagePlus :size="13" /> 安装内置插件
      </button>
    </div>

    <div v-else class="plugins-list">
      <article v-for="plugin in plugins.items" :key="plugin.id" class="plugin-card" :class="{ 'is-error': plugin.error, 'is-enabled': plugin.enabled }">
        <header class="plugin-card-head">
          <div class="plugin-card-title">
            <span class="plugin-card-icon"><Puzzle :size="16" /></span>
            <div>
              <h2>{{ plugin.name }} <em>v{{ plugin.version || '?' }}</em></h2>
              <small>{{ plugin.id }}<template v-if="plugin.author"> · {{ plugin.author }}</template></small>
            </div>
          </div>
          <div class="plugin-card-state">
            <span v-if="plugin.error" class="plugin-state is-bad">配置错误</span>
            <span v-else-if="plugin.enabled" class="plugin-state is-on">{{ plugin.needsPermission ? '缺少权限' : '已启用' }}</span>
            <span v-else class="plugin-state">未启用</span>
          </div>
        </header>

        <p v-if="plugin.error" class="plugin-card-error">{{ plugin.error }}</p>
        <p v-else class="plugin-card-desc">{{ plugin.description || '这个插件没有写描述。' }}</p>

        <template v-if="!plugin.error">
          <div class="plugin-card-row">
            <span class="plugin-card-label">权限</span>
            <span class="plugin-perm-list">
              <span v-for="permission in plugin.permissions" :key="permission" class="plugin-perm">
                <ShieldCheck :size="11" /> {{ permissionLabel(permission) }}
              </span>
              <span v-if="!plugin.permissions.length" class="plugin-perm is-none">不需要权限</span>
            </span>
          </div>

          <div class="plugin-card-row">
            <span class="plugin-card-label">命令</span>
            <span class="plugin-command-list">
              <span v-for="command in plugin.commands" :key="command.id" class="plugin-command-chip">
                <b>{{ command.title }} <em v-if="command.primary" class="plugin-command-primary">主操作</em></b>
                <small v-if="command.description">{{ command.description }}</small>
              </span>
              <span v-if="!plugin.commands.length" class="plugin-perm is-none">没有声明命令</span>
            </span>
          </div>

          <div v-if="plugin.panels.length" class="plugin-card-row">
            <span class="plugin-card-label">面板</span>
            <span class="plugin-command-list">
              <span v-for="panel in plugin.panels" :key="panel.id" class="plugin-command-chip">
                <b>{{ panel.title }}</b>
                <small>{{ panelDomain(panel.url) }} · 跨域沙箱</small>
              </span>
            </span>
          </div>

          <div v-if="plugin.settings.length" class="plugin-settings">
            <div class="plugin-settings-head">
              <span>设置</span>
              <button type="button" class="plugin-settings-save" :disabled="busy === `settings:${plugin.id}`" @click="saveSettings(plugin)">保存设置</button>
            </div>
            <label v-for="setting in plugin.settings" :key="setting.key" class="plugin-field">
              <span>{{ setting.label }}</span>
              <input
                v-if="setting.type === 'boolean'"
                type="checkbox"
                :checked="Boolean(settingsDraft(plugin)[setting.key])"
                @change="setValue(plugin, setting, ($event.target as HTMLInputElement).checked)"
              />
              <select
                v-else-if="setting.type === 'select'"
                class="prod-select"
                :value="String(settingsDraft(plugin)[setting.key] ?? '')"
                @change="setValue(plugin, setting, ($event.target as HTMLSelectElement).value)"
              >
                <option v-for="option in setting.options" :key="option" :value="option">
                  {{ setting.optionLabels?.[option] ?? option }}
                </option>
              </select>
              <input
                v-else-if="setting.type === 'number'"
                type="number"
                class="plugin-input"
                :value="Number(settingsDraft(plugin)[setting.key] ?? 0)"
                @input="setValue(plugin, setting, Number(($event.target as HTMLInputElement).value))"
              />
              <input
                v-else
                type="text"
                class="plugin-input"
                :placeholder="setting.placeholder || ''"
                :value="String(settingsDraft(plugin)[setting.key] ?? '')"
                @input="setValue(plugin, setting, ($event.target as HTMLInputElement).value)"
              />
            </label>
          </div>
        </template>

        <footer class="plugin-card-foot">
          <span class="plugin-card-path" :title="plugin.path">{{ plugin.path }}</span>
          <div class="plugin-card-actions">
            <button
              v-if="!plugin.error"
              type="button"
              class="prod-btn is-small"
              :class="{ 'is-primary': !plugin.enabled || plugin.needsPermission }"
              :disabled="busy === plugin.id"
              @click="toggle(plugin)"
            >
              <LoaderCircle v-if="busy === plugin.id" :size="12" class="plugin-spin" />
              {{ plugin.enabled && !plugin.needsPermission ? '禁用' : plugin.needsPermission ? '重新授权' : '启用' }}
            </button>
            <button type="button" class="plugin-remove" :disabled="busy === `remove:${plugin.id}`" @click="armUninstall(plugin)">
              <Trash2 :size="12" /> {{ uninstallArmed === plugin.id ? '确认卸载' : '卸载' }}
            </button>
          </div>
        </footer>
      </article>
    </div>

    <footer class="plugins-foot">
      <b>边界</b>
      <span>命令插件不能写笔记、不能联网、不能访问 Tauri IPC；面板插件只拿 URL 参数，在跨域 sandbox iframe 里加载，不共享登录态。启用即信任它读取整个笔记库；插件状态只存本机，不参与同步。</span>
    </footer>
  </div>
</template>
