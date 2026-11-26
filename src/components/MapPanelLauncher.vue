// Reusable entry point for the Jiangshan atlas outside the note header:
// citation cards, whiteboard note cards and future search rows. It opens the
// same PluginPanelHost with the note's saved/cached context when available.
<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { LoaderCircle, Map as MapIcon } from 'lucide-vue-next'

import * as api from '../lib/api'
import { inferMapText } from '../lib/mapText'
import { loadPlugins, plugins } from '../stores/plugins'
import type { MapContext, PluginInfo, PluginPanel } from '../types'
import AtlasPanel from './AtlasPanel.vue'
import PluginPanelHost from './PluginPanelHost.vue'

defineOptions({ inheritAttrs: false })

const props = defineProps<{
  noteId?: string
  label?: string
  title?: string
  compact?: boolean
  historicalOnly?: boolean
  text?: string
  requireYear?: boolean
}>()

const plugin = ref<PluginInfo | null>(null)
const noteAllowed = ref(true)
const panel = ref<PluginPanel | null>(null)
const open = ref(false)
const busy = ref(false)
const params = ref<Record<string, string | number | boolean>>({})
const panelNoteId = ref<string | undefined>(undefined)
const textContext = computed(() => (props.text ? inferMapText(props.text) : null))

const available = computed(
  () =>
    noteAllowed.value &&
    Boolean(plugin.value?.enabled && !plugin.value.needsPermission && panel.value),
)

function contextParams(context: MapContext | null): Record<string, string | number | boolean> {
  return {
    year: context?.yearText || context?.year || '',
    place: context?.place || '',
    person: context?.person || '',
    event: context?.event || '',
  }
}

async function launch(): Promise<void> {
  if (!available.value || busy.value) return
  busy.value = true
  let context: MapContext | null = null
  if (props.noteId) {
    try {
      context = await api.mapContext(props.noteId, true)
    } catch {
      // No saved context/cache: open with empty fields instead of hiding the
      // entry; the panel can still use AI or manual input.
    }
  }
  params.value = context
    ? contextParams(context)
    : textContext.value
      ? {
          year: textContext.value.year,
          place: textContext.value.place,
          person: textContext.value.person,
          event: textContext.value.event,
        }
      : {}
  panelNoteId.value = props.noteId
  open.value = true
  busy.value = false
}

onMounted(async () => {
  try {
    if (props.historicalOnly && props.noteId) {
      const note = await api.getNote(props.noteId)
      const meta = note?.meta
      const historical = Boolean(
        meta &&
          (meta.tags.includes('领域/历史') ||
            meta.periods.some((period) => !/^\d{4}(?:-\d{2}){0,2}$/.test(period.trim()))),
      )
      if (!historical) {
        noteAllowed.value = false
        return
      }
    }
    if (props.requireYear && textContext.value && !textContext.value.year) {
      noteAllowed.value = false
      return
    }
    await loadPlugins()
    const found = plugins.items.find((item) => item.id === 'jiangshan.atlas')
    if (found) {
      plugin.value = found
      panel.value = found.panels.find((item) => item.id === 'atlas') || null
    }
  } catch {
    // The entry stays hidden when plugins/note metadata cannot be read.
  }
})
</script>

<template>
  <button
    v-if="available"
    type="button"
    v-bind="$attrs"
    class="map-launcher-btn"
    :class="{ 'is-compact': compact }"
    :title="title || '在江山时序中查看'"
    :disabled="busy"
    @pointerdown.stop
    @click.stop="launch"
  >
    <LoaderCircle v-if="busy" :size="12" class="plugin-spin" />
    <MapIcon v-else :size="12" />
    <span v-if="!compact">{{ label || '地图' }}</span>
  </button>
  <AtlasPanel
    v-if="open && plugin && panel && plugin.id === 'jiangshan.atlas'"
    :plugin="plugin"
    :panel="panel"
    :note-id="panelNoteId"
    :text="text"
    :initial="params"
    @close="open = false"
  />
  <PluginPanelHost
    v-else-if="open && plugin && panel"
    :plugin="plugin"
    :panel="panel"
    :params="params"
    :note-id="panelNoteId"
    @close="open = false"
  />
</template>
