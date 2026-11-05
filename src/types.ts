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

export interface ArtifactSource {
  kind: string
  label: string
  url: string
  author: string
  publishedAt: string
}

export interface ArtifactRun {
  projectId: string
  projectName: string
  runId: string
  nodeId: string
  prompt: string
  model: string
}

export interface ArtifactUsage {
  promptTokens: number
  completionTokens: number
  estimatedCost: number
  currency: string
}

export interface ArtifactAsset {
  name: string
  path: string
  kind: string
  bytes: number
}

export interface StudioArtifact {
  artifactId: string
  noteId: string
  noteRelPath: string
  title: string
  status: string
  revision: number
  generatedAt: string
  source: ArtifactSource | null
  run: ArtifactRun | null
  usage: ArtifactUsage | null
  assets: ArtifactAsset[]
  noteExists: boolean
  annotationCount: number
  derived: boolean
  error: string | null
}

export interface StudioInfo {
  folder: string
  url: string
  running: boolean
}

export interface NoteDetail {
  meta: NoteMeta
  body: string
  artifact?: StudioArtifact | null
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

export interface IndexStatus {
  state: 'idle' | 'scanning' | 'indexing' | 'writing' | 'committing' | 'error'
  done: number
  total: number
  current: string
  generation: number
  error: string | null
  root: string | null
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
