import { reactive } from 'vue'

import * as api from '../lib/api'
import { runPluginCommand, type PluginContext } from '../lib/pluginRuntime'
import type { PluginCommand, PluginInfo, PluginRunResult } from '../types'

export const plugins = reactive({
  items: [] as PluginInfo[],
  loading: false,
  loaded: false,
  error: '',
})

let loadPromise: Promise<void> | null = null

export async function loadPlugins(force = false): Promise<void> {
  if (plugins.loaded && !force) return
  // Several note/citation/card components mount together on a route change.
  // Returning early while a load is in flight leaves those components with an
  // empty list; share one promise so every caller sees the final result.
  if (loadPromise) return loadPromise
  loadPromise = (async () => {
    plugins.loading = true
    plugins.error = ''
    try {
      plugins.items = await api.pluginsList()
      plugins.loaded = true
    } catch (cause) {
      plugins.error = api.errorText(cause)
    } finally {
      plugins.loading = false
    }
  })()
  try {
    await loadPromise
  } finally {
    loadPromise = null
  }
}

function replace(item: PluginInfo): void {
  const index = plugins.items.findIndex((entry) => entry.id === item.id)
  if (index >= 0) plugins.items.splice(index, 1, item)
  else plugins.items.push(item)
}

export async function setPluginEnabled(plugin: PluginInfo, enabled: boolean): Promise<PluginInfo> {
  const permissions = enabled ? plugin.permissions : plugin.grantedPermissions
  const next = await api.pluginSetEnabled(plugin.id, enabled, permissions)
  replace(next)
  return next
}

export async function savePluginSettings(
  plugin: PluginInfo,
  values: Record<string, unknown>,
): Promise<PluginInfo> {
  const next = await api.pluginSaveSettings(plugin.id, values)
  replace(next)
  return next
}

export async function installExamplePlugins(): Promise<void> {
  plugins.items = await api.pluginInstallExamples()
  plugins.loaded = true
}

export async function uninstallPlugin(id: string): Promise<void> {
  await api.pluginUninstall(id)
  plugins.items = plugins.items.filter((item) => item.id !== id)
}

export function runPlugin(
  plugin: PluginInfo,
  command: PluginCommand,
  context: PluginContext,
): Promise<PluginRunResult> {
  return runPluginCommand(plugin, command, context)
}

export function enabledPluginCommands(): Array<{ plugin: PluginInfo; command: PluginCommand }> {
  return plugins.items
    .filter((plugin) => plugin.enabled && !plugin.error && !plugin.needsPermission)
    .flatMap((plugin) => plugin.commands.map((command) => ({ plugin, command })))
}
