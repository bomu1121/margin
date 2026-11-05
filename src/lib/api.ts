import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { open } from '@tauri-apps/plugin-dialog'

import type {
  Annotation,
  AppStats,
  CreatedNote,
  DeletedEntry,
  IndexStats,
  IndexStatus,
  MoveReport,
  NoteDetail,
  NoteMeta,
  StudioArtifact,
  StudioInfo,
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

export function listFolders(): Promise<string[]> {
  return invoke<string[]>('list_folders')
}

export function createNote(parent: string, title: string): Promise<CreatedNote> {
  return invoke<CreatedNote>('create_note', { parent, title })
}

export function createFolder(parent: string, name: string): Promise<string> {
  return invoke<string>('create_folder', { parent, name })
}

export function renameEntry(relPath: string, newName: string): Promise<MoveReport> {
  return invoke<MoveReport>('rename_entry', { relPath, newName })
}

export function moveEntry(relPath: string, newParent: string): Promise<MoveReport> {
  return invoke<MoveReport>('move_entry', { relPath, newParent })
}

export function deleteEntry(relPath: string): Promise<DeletedEntry> {
  return invoke<DeletedEntry>('delete_entry', { relPath })
}

export function restoreEntry(trashPath: string, relPath: string): Promise<string> {
  return invoke<string>('restore_entry', { trashPath, relPath })
}

export function listArtifacts(): Promise<StudioArtifact[]> {
  return invoke<StudioArtifact[]>('list_artifacts')
}

export function getStudioInfo(): Promise<StudioInfo> {
  return invoke<StudioInfo>('studio_info')
}

export function openScribeFlow(): Promise<void> {
  return invoke<void>('open_scribeflow')
}

export function getStats(): Promise<AppStats> {
  return invoke<AppStats>('get_stats')
}

export function getIndexStatus(): Promise<IndexStatus> {
  return invoke<IndexStatus>('get_index_status')
}

export function onIndexStatus(
  handler: (status: IndexStatus) => void,
): Promise<UnlistenFn> {
  return listen<IndexStatus>('index-status', (event) => handler(event.payload))
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
