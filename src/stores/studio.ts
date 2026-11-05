import { computed, reactive } from 'vue'

import * as api from '../lib/api'
import type { StudioArtifact, StudioInfo } from '../types'

export const studio = reactive({
  artifacts: [] as StudioArtifact[],
  selectedId: '',
  info: null as StudioInfo | null,
  loading: false,
  error: '',
})

let inFlight: Promise<void> | null = null

export async function loadStudio(): Promise<void> {
  if (inFlight) return inFlight
  studio.loading = true
  studio.error = ''
  inFlight = (async () => {
    try {
      const [artifacts, info] = await Promise.all([api.listArtifacts(), api.getStudioInfo()])
      studio.artifacts = artifacts
      studio.info = info
      if (!studio.selectedId || !artifacts.some((item) => item.artifactId === studio.selectedId)) {
        studio.selectedId = artifacts[0]?.artifactId ?? ''
      }
    } catch (error) {
      studio.error = api.errorText(error)
    } finally {
      studio.loading = false
      inFlight = null
    }
  })()
  return inFlight
}

export function selectArtifact(artifactId: string): void {
  studio.selectedId = artifactId
}

export const selectedArtifact = computed(
  () => studio.artifacts.find((item) => item.artifactId === studio.selectedId) ?? null,
)

export function artifactForNote(noteId: string): StudioArtifact | null {
  return studio.artifacts.find((item) => item.noteId === noteId) ?? null
}
