export interface NoteMeta {
  id: string
  path: string
  relPath: string
  title: string
  up: string
  date: string
  domain: string
  tags: string[]
  people: string[]
  events: string[]
  periods: string[]
  runId: string
  mtime: number
  chars: number
  annotationCount: number
}

export interface NoteDetail {
  meta: NoteMeta
  body: string
}

export interface Annotation {
  id: string
  noteId: string
  blockId: string
  quote: string
  body: string
  kind: string
  createdAt: number
}

export interface IndexStats {
  notes: number
  added: number
  updated: number
  removed: number
  skipped: number
  unchanged?: number
  elapsedMs: number
}

export interface AppStats {
  notes: number
  annotations: number
  vault: string | null
}

export interface CreatedNote {
  relPath: string
  id: string
}

export interface MoveReport {
  relPath: string
  linksRewritten: number
  filesScanned: number
}

export interface DeletedEntry {
  relPath: string
  trashPath: string
}
