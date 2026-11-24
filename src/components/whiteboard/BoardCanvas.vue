<script setup lang="ts">
import { computed, ref } from 'vue'

import BoardCard from './BoardCard.vue'
import type { AgentState } from '../../stores/whiteboard'
import type { WhiteboardItem } from '../../lib/api'
import { saveView, view } from '../../stores/whiteboard'

const props = defineProps<{
  items: WhiteboardItem[]
  selectedIds: string[]
  assetUrls: Record<string, string>
  agentStates: Record<string, AgentState>
  linkLoading: Record<string, boolean>
}>()

const emit = defineEmits<{
  select: [id: string, additive: boolean]
  clearSelection: []
  createSticky: [x: number, y: number]
  patch: [item: WhiteboardItem, values: Partial<WhiteboardItem>]
  commit: [item: WhiteboardItem]
  remove: [item: WhiteboardItem]
  openLink: [item: WhiteboardItem, url?: string]
  openNote: [item: WhiteboardItem]
  openResearch: [item: WhiteboardItem]
  lookupImage: [item: WhiteboardItem, mode: 'source' | 'similar']
  cycleColor: [item: WhiteboardItem]
}>()

const canvas = ref<HTMLElement | null>(null)
const panning = ref(false)
let startX = 0
let startY = 0
let originX = 0
let originY = 0
let moved = false

const stageStyle = computed(() => ({
  transform: `translate(${view.x}px, ${view.y}px) scale(${view.scale})`,
}))

const gridStyle = computed(() => ({
  backgroundSize: `${28 * view.scale}px ${28 * view.scale}px`,
  backgroundPosition: `${view.x}px ${view.y}px`,
}))

function isCard(target: EventTarget | null): boolean {
  return !!((target as HTMLElement | null)?.closest('.wb-card'))
}

function onPointerDown(event: PointerEvent): void {
  if (event.button !== 0 || isCard(event.target)) return
  panning.value = true
  moved = false
  startX = event.clientX
  startY = event.clientY
  originX = view.x
  originY = view.y
  canvas.value?.setPointerCapture(event.pointerId)
}

function onPointerMove(event: PointerEvent): void {
  if (!panning.value) return
  const dx = event.clientX - startX
  const dy = event.clientY - startY
  if (Math.abs(dx) + Math.abs(dy) > 3) moved = true
  view.x = originX + dx
  view.y = originY + dy
}

function onPointerUp(event: PointerEvent): void {
  if (!panning.value) return
  panning.value = false
  try {
    canvas.value?.releasePointerCapture(event.pointerId)
  } catch {
    // Pointer capture can already be gone.
  }
  if (!moved) emit('clearSelection')
  saveView()
}

function onWheel(event: WheelEvent): void {
  const element = canvas.value
  if (!element) return
  const rect = element.getBoundingClientRect()
  const px = event.clientX - rect.left
  const py = event.clientY - rect.top
  const worldX = (px - view.x) / view.scale
  const worldY = (py - view.y) / view.scale
  const next = Math.min(2, Math.max(0.4, view.scale * Math.exp(-event.deltaY * 0.0015)))
  view.x = px - worldX * next
  view.y = py - worldY * next
  view.scale = next
  saveView()
}

function onDoubleClick(event: MouseEvent): void {
  if (isCard(event.target)) return
  const element = canvas.value
  if (!element) return
  const rect = element.getBoundingClientRect()
  const x = (event.clientX - rect.left - view.x) / view.scale
  const y = (event.clientY - rect.top - view.y) / view.scale
  emit('createSticky', x - 135, y - 95)
}
</script>

<template>
  <div
    ref="canvas"
    class="wb-canvas"
    :class="{ 'is-panning': panning }"
    :style="gridStyle"
    @pointerdown="onPointerDown"
    @pointermove="onPointerMove"
    @pointerup="onPointerUp"
    @pointercancel="onPointerUp"
    @wheel.prevent="onWheel"
    @dblclick="onDoubleClick"
  >
    <div class="wb-stage" :style="stageStyle">
      <BoardCard
        v-for="item in props.items"
        :key="item.id"
        :item="item"
        :selected="props.selectedIds.includes(item.id)"
        :scale="view.scale"
        :asset-url="props.assetUrls[item.assetPath] || ''"
        :agent-state="props.agentStates[item.jobId]"
        :link-loading="props.linkLoading[item.id]"
        @select="(additive) => emit('select', item.id, additive)"
        @patch="(values) => emit('patch', item, values)"
        @commit="emit('commit', item)"
        @remove="emit('remove', item)"
        @open-link="(url) => emit('openLink', item, url)"
        @open-note="emit('openNote', item)"
        @open-research="emit('openResearch', item)"
        @lookup-image="(mode) => emit('lookupImage', item, mode)"
        @cycle-color="emit('cycleColor', item)"
      />
    </div>
  </div>
</template>
