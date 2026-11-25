<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, ref, watch } from 'vue'
import { ChevronDown, LoaderCircle, Puzzle, X } from 'lucide-vue-next'

import { renderRichText } from '../lib/richText'
import { enabledPluginCommands, loadPlugins, plugins, runPlugin } from '../stores/plugins'
import type { PluginCommand, PluginInfo, PluginRunResult } from '../types'

const props = defineProps<{ noteId: string; noteTitle: string }>()

const open = ref(false)
const menuUp = ref(false)
const menuStyle = ref<Record<string, string>>({})
const runningKey = ref('')
const error = ref('')
const result = ref<{ plugin: string; command: string; data: PluginRunResult } | null>(null)
const root = ref<HTMLElement | null>(null)
const menu = ref<HTMLElement | null>(null)

const commands = computed(() => enabledPluginCommands())
const hasPlugins = computed(() => plugins.items.length > 0)

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
    error.value = ''
  },
)

document.addEventListener('pointerdown', onDocumentPointerDown, true)
window.addEventListener('resize', positionMenu)
document.addEventListener('scroll', positionMenu, true)
onBeforeUnmount(() => {
  document.removeEventListener('pointerdown', onDocumentPointerDown, true)
  window.removeEventListener('resize', positionMenu)
  document.removeEventListener('scroll', positionMenu, true)
})
</script>

<template>
  <span class="plugin-command-root">
    <button
      ref="root"
      type="button"
      class="plugin-command-trigger"
      :aria-expanded="open"
      aria-haspopup="menu"
      @click.stop="toggle"
    >
      <Puzzle :size="12" /> 插件 <ChevronDown :size="10" />
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
        <div v-if="plugins.loading" class="plugin-command-empty"><LoaderCircle :size="12" class="plugin-spin" /> 正在读取插件…</div>
        <div v-else-if="plugins.error" class="plugin-command-empty">{{ plugins.error }}</div>
        <div v-else-if="!commands.length" class="plugin-command-empty">
          <template v-if="!hasPlugins">还没有安装插件。</template>
          <template v-else>插件都已禁用，或缺少权限。</template>
          <router-link to="/plugins" class="plugin-command-link" @click="open = false">去插件页安装 / 启用</router-link>
        </div>
        <template v-else>
          <div v-for="item in commands" :key="`${item.plugin.id}:${item.command.id}`" class="plugin-command-group">
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
                v-if="runningKey === `${item.plugin.id}:${item.command.id}`"
                :size="12"
                class="plugin-spin"
              />
            </button>
          </div>
          <router-link to="/plugins" class="plugin-command-link" @click="open = false">管理插件</router-link>
        </template>
        <p v-if="error" class="plugin-command-error">{{ error }}</p>
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
  </span>
</template>
