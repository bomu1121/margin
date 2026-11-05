import { reactive } from 'vue'

import * as api from '../lib/api'
import type { AppStats, IndexStatus, NoteMeta } from '../types'

export const library = reactive({
  notes: [] as NoteMeta[],
  vault: null as string | null,
  stats: null as AppStats | null,
  index: null as IndexStatus | null,
  loading: true,
  error: '',
})

export async function loadLibrary(): Promise<void> {
  library.loading = true
  library.error = ''
  try {
    const [vault, notes, stats, index] = await Promise.all([
      api.getVault(),
      api.listNotes(),
      api.getStats(),
      api.getIndexStatus(),
    ])
    library.vault = vault
    library.notes = notes
    library.stats = stats
    library.index = index
  } catch (error) {
    library.error = api.errorText(error)
  } finally {
    library.loading = false
  }
}

export async function refreshNotes(): Promise<void> {
  try {
    const [notes, stats] = await Promise.all([api.listNotes(), api.getStats()])
    library.notes = notes
    library.stats = stats
  } catch (error) {
    library.error = api.errorText(error)
  }
}

export async function chooseVault(): Promise<boolean> {
  try {
    const path = await api.chooseVaultFolder()
    if (!path) return false
    library.loading = true
    const stats = await api.setVault(path)
    library.vault = path
    library.stats = {
      notes: stats.notes,
      annotations: library.stats?.annotations ?? 0,
      vault: path,
    }
    await refreshNotes()
    return true
  } catch (error) {
    library.error = api.errorText(error)
    return false
  } finally {
    library.loading = false
  }
}

export async function reindexLibrary(): Promise<boolean> {
  try {
    library.loading = true
    await api.reindex()
    await refreshNotes()
    return true
  } catch (error) {
    library.error = api.errorText(error)
    return false
  } finally {
    library.loading = false
  }
}

export function setIndexStatus(status: IndexStatus): void {
  library.index = status
}

export function noteById(id: string | undefined): NoteMeta | undefined {
  if (!id) return undefined
  return library.notes.find((note) => note.id === id)
}
