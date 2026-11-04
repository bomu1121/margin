import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { open } from '@tauri-apps/plugin-dialog'

import type {
  Annotation,
  AppStats,
  IndexStats,
  NoteDetail,
  NoteMeta,
} from '../types'

export function getVault(): Promise<string | null> {
  return invoke<string | null>('get_vault')
}

export function setVault(path: string): Promise<IndexStats> {
  return invoke<IndexStats>('set_vault', { path })
}

export function reindex(): Promise<IndexStats> {
  return invoke<IndexStats>('reindex')
}

export function listNotes(): Promise<NoteMeta[]> {
  return invoke<NoteMeta[]>('list_notes')
}

export function getNote(id: string): Promise<NoteDetail | null> {
  return invoke<NoteDetail | null>('get_note', { id })
}

export function searchNotes(query: string): Promise<NoteMeta[]> {
  return invoke<NoteMeta[]>('search_notes', { query })
}

export function listAnnotations(noteId: string): Promise<Annotation[]> {
  return invoke<Annotation[]>('list_annotations', { noteId })
}

export function addAnnotation(input: {
  noteId: string
  blockId: string
  quote: string
  body: string
  kind: string
}): Promise<Annotation> {
  return invoke<Annotation>('add_annotation', input)
}

export function deleteAnnotation(id: string): Promise<void> {
  return invoke<void>('delete_annotation', { id })
}

export function getStats(): Promise<AppStats> {
  return invoke<AppStats>('get_stats')
}

export function appReady(): Promise<void> {
  return invoke<void>('app_ready')
}

export function onVaultChanged(
  handler: (stats: IndexStats) => void,
): Promise<UnlistenFn> {
  return listen<IndexStats>('vault-changed', (event) => handler(event.payload))
}

export async function chooseVaultFolder(): Promise<string | null> {
  const selected = await open({
    directory: true,
    multiple: false,
    title: '选择笔记库',
  })
  return typeof selected === 'string' ? selected : null
}

export function errorText(error: unknown): string {
  if (typeof error === 'string') return error
  if (error instanceof Error) return error.message
  return String(error)
}
