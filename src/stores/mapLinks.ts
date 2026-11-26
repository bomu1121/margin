import { reactive } from 'vue'

import type { MapPoint } from '../lib/mapText'

const PREFIX = 'yebian.map.links.'

export const mapLinks = reactive({
  enabled: false,
  noteId: '',
  points: [] as MapPoint[],
})

type OpenHandler = (point: MapPoint) => void

let openHandler: OpenHandler | null = null

function storageKey(noteId: string): string {
  return PREFIX + noteId
}

export function setMapLinks(noteId: string, points: MapPoint[]): void {
  mapLinks.noteId = noteId
  mapLinks.points = points
  try {
    mapLinks.enabled = localStorage.getItem(storageKey(noteId)) === '1'
  } catch {
    mapLinks.enabled = false
  }
}

export function setMapLinksEnabled(enabled: boolean): void {
  mapLinks.enabled = enabled
  if (!mapLinks.noteId) return
  try {
    localStorage.setItem(storageKey(mapLinks.noteId), enabled ? '1' : '0')
  } catch {
    // Storage may be unavailable; the switch still works for this session.
  }
}

export function registerMapOpen(handler: OpenHandler): () => void {
  openHandler = handler
  return () => {
    if (openHandler === handler) openHandler = null
  }
}

export function openMapPoint(point: MapPoint): void {
  openHandler?.(point)
}
