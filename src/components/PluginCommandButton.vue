<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { ChevronDown, LoaderCircle, Map as MapIcon, Puzzle, X } from 'lucide-vue-next'

import AtlasPanel from './AtlasPanel.vue'
import PluginPanelHost from './PluginPanelHost.vue'
import { renderRichText } from '../lib/richText'
import type { MapPoint } from '../lib/mapText'
import { registerMapOpen } from '../stores/mapLinks'
import { enabledPluginCommands, loadPlugins, plugins, runPlugin } from '../stores/plugins'
import type {
  NoteMeta,
  PluginCommand,
  PluginInfo,
  PluginPanel,
  PluginRunResult,
} from '../types'

const props = defineProps<{ noteId: string; noteTitle: string; noteMeta?: NoteMeta | null }>()

const open = ref(false)
const menuUp = ref(false)
const menuStyle = ref<Record<string, string>>({})
const runningKey = ref('')
const error = ref('')
const result = ref<{ plugin: string; command: string; data: PluginRunResult } | null>(null)
const panelState = ref<{ plugin: PluginInfo; panel: PluginPanel; params: Record<string, string | number | boolean>; noteId: string } | null>(null)
const root = ref<HTMLElement | null>(null)
const menu = ref<HTMLElement | null>(null)

function openMapAt(point: MapPoint): void {
  const plugin = plugins.items.find((item) => item.id === 'jiangshan.atlas')
  const panel = plugin?.panels.find((item) => item.id === 'atlas')
  if (!plugin || !panel) return
  panelState.value = {
    plugin,
    panel,
    params: {
      year: point.year,
      place: point.place,
      event: point.event,
    },
    noteId: props.noteId,
  }
}

let unregisterMapOpen: (() => void) | null = null

function hasHistoricalPeriods(periods: string[]): boolean {
  return periods.some((period) => !/^\d{4}(?:-\d{2}){0,2}$/.test(period.trim()))
}

function commandVisible(command: PluginCommand): boolean {
  if (command.when !== 'history') return true
  const meta = props.noteMeta
  if (!meta) return true
  return meta.tags.includes('领域/历史') || hasHistoricalPeriods(meta.periods)
}

const commands = computed(() => enabledPluginCommands().filter((item) => commandVisible(item.command)))
const primaryCommands = computed(() => commands.value.filter((item) => item.command.primary))
const secondaryCommands = computed(() => commands.value.filter((item) => !item.command.primary))

function iconFor(command: PluginCommand) {
  return command.icon === 'map' ? MapIcon : Puzzle
}

function isBusy(plugin: PluginInfo, command: PluginCommand): boolean {
  return runningKey.value === `${plugin.id}:${command.id}`
}

function pointInside(target: EventTarget | null): boolean {
  return (
    target instanceof Node &&
    (Boolean(root.value?.contains(target)) || Boolean(menu.value?.contains(target)))
  )
}

function onDocumentPointerDown(event: PointerEvent): void {
  if (!open.value || pointInside(event.target)) return
  open.value = false
}

/** The trigger lives inside the scrolling document column; a fixed, teleported
 * menu avoids being clipped by `.doc-scroll` or the right rail. */
function positionMenu(): void {
  if (!open.value) return
  const rect = root.value?.getBoundingClientRect()
  if (!rect) return
  const width = 310
  const left = Math.max(8, Math.min(rect.left, window.innerWidth - width - 8))
  const up = rect.bottom + 300 > window.innerHeight && rect.top > 280
  menuUp.value = up
  menuStyle.value = {
    left: `${Math.round(left)}px`,
    top: `${Math.round(up ? rect.top - 6 : rect.bottom + 6)}px`,
  }
}

async function toggle(): Promise<void> {
  open.value = !open.value
  if (!open.value) return
  await nextTick()
  positionMenu()
  await loadPlugins()
  await nextTick()
  positionMenu()
}

async function run(plugin: PluginInfo, command: PluginCommand): Promise<void> {
  if (runningKey.value) return
  runningKey.value = `${plugin.id}:${command.id}`
  error.value = ''
  try {
    const data = await runPlugin(plugin, command, {
      noteId: props.noteId,
      noteTitle: props.noteTitle,
    })
    if (data.panel) {
      const target = plugin.panels.find((entry) => entry.id === data.panel?.id)
      if (!target) throw new Error(`插件没有声明面板：${data.panel.id}`)
      panelState.value = { plugin, panel: target, params: data.panel.params, noteId: props.noteId }
      open.value = false
      return
    }
    result.value = { plugin: plugin.name, command: command.title, data }
    open.value = false
  } catch (cause) {
    error.value = String(cause instanceof Error ? cause.message : cause)
  } finally {
    runningKey.value = ''
  }
}

function closeResult(): void {
  result.value = null
}

watch(
  () => props.noteId,
  () => {
    open.value = false
    result.value = null
    panelState.value = null
    error.value = ''
  },
)

onMounted(() => {
  void loadPlugins()
  unregisterMapOpen = registerMapOpen(openMapAt)
})
document.addEventListener('pointerdown', onDocumentPointerDown, true)
window.addEventListener('resize', positionMenu)
document.addEventListener('scroll', positionMenu, true)
onBeforeUnmount(() => {
  unregisterMapOpen?.()
  document.removeEventListener('pointerdown', onDocumentPointerDown, true)
  window.removeEventListener('resize', positionMenu)
  document.removeEventListener('scroll', positionMenu, true)
})
</script>

<template>
  <span v-if="commands.length" class="plugin-command-root">
    <button
      v-for="item in primaryCommands"
      :key="`${item.plugin.id}:${item.command.id}`"
      type="button"
      class="plugin-action-btn"
      :title="item.command.description || item.command.title"
      :disabled="Boolean(runningKey)"
      @click.stop="run(item.plugin, item.command)"
    >
      <LoaderCircle v-if="isBusy(item.plugin, item.command)" :size="13" class="plugin-spin" />
      <component :is="iconFor(item.command)" v-else :size="13" />
      <span>{{ item.command.title }}</span>
    </button>

    <button
      v-if="secondaryCommands.length"
      ref="root"
      type="button"
      class="plugin-command-trigger"
      :aria-expanded="open"
      aria-haspopup="menu"
      @click.stop="toggle"
    >
      <Puzzle :size="12" /> 更多 <ChevronDown :size="10" />
    </button>

    <Teleport to="body">
      <div
        v-if="open"
        ref="menu"
        class="plugin-command-menu"
        :class="{ 'is-up': menuUp }"
        :style="menuStyle"
        role="menu"
      >
        <div v-for="item in secondaryCommands" :key="`${item.plugin.id}:${item.command.id}`" class="plugin-command-group">
          <div class="plugin-command-plugin">{{ item.plugin.name }}</div>
          <button
            type="button"
            class="plugin-command-item"
            role="menuitem"
            :disabled="Boolean(runningKey)"
            @click.stop="run(item.plugin, item.command)"
          >
            <span>
              <b>{{ item.command.title }}</b>
              <small v-if="item.command.description">{{ item.command.description }}</small>
            </span>
            <LoaderCircle
              v-if="isBusy(item.plugin, item.command)"
              :size="12"
              class="plugin-spin"
            />
          </button>
        </div>
        <router-link to="/plugins" class="plugin-command-link" @click="open = false">管理插件</router-link>
        <p v-if="error" class="plugin-command-error">{{ error }}</p>
      </div>
    </Teleport>

    <Teleport to="body">
      <div v-if="error && !open" class="plugin-action-toast" role="alert">
        <span>{{ error }}</span>
        <button type="button" aria-label="关闭" @click="error = ''"><X :size="12" /></button>
      </div>
    </Teleport>

    <Teleport to="body">
      <div v-if="result" class="plugin-result-overlay" @click.self="closeResult">
        <section class="plugin-result" role="dialog" aria-modal="true" aria-labelledby="pluginResultTitle">
          <header class="plugin-result-head">
            <div>
              <div class="plugin-result-kicker">{{ result.plugin }}</div>
              <h2 id="pluginResultTitle">{{ result.command }}</h2>
            </div>
            <button type="button" class="plugin-result-close" aria-label="关闭" @click="closeResult"><X :size="15" /></button>
          </header>
          <div v-if="result.data.markdown" class="prose plugin-result-body" v-html="renderRichText(result.data.markdown)" />
          <p v-else class="plugin-result-text">{{ result.data.message || '插件没有返回内容。' }}</p>
        </section>
      </div>
    </Teleport>

    <AtlasPanel
      v-if="panelState && panelState.plugin.id === 'jiangshan.atlas'"
      :plugin="panelState.plugin"
      :panel="panelState.panel"
      :note-id="panelState.noteId"
      :initial="panelState.params"
      @close="panelState = null"
    />
    <PluginPanelHost
      v-else-if="panelState"
      :plugin="panelState.plugin"
      :panel="panelState.panel"
      :params="panelState.params"
      :note-id="panelState.noteId"
      @close="panelState = null"
    />
  </span>
</template>
