import type { NoteMeta } from '../types'

/** A directory in the vault. The root node has an empty name and path. */
export interface FolderNode {
  name: string
  /** Vault-relative path with "/" separators, "" for the root. */
  path: string
  folders: FolderNode[]
  notes: NoteMeta[]
  /** Notes in this folder and everything below it. */
  count: number
}

export interface TreeRow {
  key: string
  depth: number
  folder?: FolderNode
  note?: NoteMeta
}

const collator = new Intl.Collator('zh-Hans-CN', { numeric: true, sensitivity: 'base' })

export type SortMode = 'name-asc' | 'name-desc' | 'time'

export const SORT_MODES: SortMode[] = ['name-asc', 'name-desc', 'time']

export const SORT_LABEL: Record<SortMode, string> = {
  'name-asc': '名称 ↑',
  'name-desc': '名称 ↓',
  time: '时间',
}

export const SORT_HINT: Record<SortMode, string> = {
  'name-asc': '按名称升序，点击切换',
  'name-desc': '按名称降序，点击切换',
  time: '按创建日期从新到旧，点击切换',
}

/**
 * Rebuilds the vault's directory tree from each note's relPath. Folders come
 * from the filesystem, not from front matter: moving a file in Explorer is
 * enough to move it in the sidebar.
 */
export function buildTree(
  notes: NoteMeta[],
  sort: SortMode = 'time',
  folders: string[] = [],
): FolderNode {
  const root: FolderNode = { name: '', path: '', folders: [], notes: [], count: 0 }
  const foldersByPath = new Map<string, FolderNode>([['', root]])

  // Creates the chain for a folder path and returns the leaf.
  const ensure = (fullPath: string): FolderNode => {
    let parent = root
    let path = ''
    for (const segment of fullPath.split('/').filter(Boolean)) {
      path = path ? `${path}/${segment}` : segment
      let folder = foldersByPath.get(path)
      if (!folder) {
        folder = { name: segment, path, folders: [], notes: [], count: 0 }
        foldersByPath.set(path, folder)
        parent.folders.push(folder)
      }
      parent = folder
    }
    return parent
  }

  // Empty folders are real structure the user created, so they come first.
  for (const folder of folders) ensure(folder)

  for (const note of notes) {
    const segments = note.relPath.split('/').filter(Boolean)
    // Last segment is the file itself; the rest is the folder chain.
    segments.pop()
    ensure(segments.join('/')).notes.push(note)
  }

  // Notes arrive sorted by date (db.rs ORDER BY date DESC, title ASC); "time"
  // keeps that. Folders are always alphabetical — they have no date of their
  // own, and a tree whose folder order jumps around is hard to re-find.
  const sortNotes = (folder: FolderNode): void => {
    if (sort !== 'time') {
      const direction = sort === 'name-asc' ? 1 : -1
      folder.notes.sort((a, b) => direction * collator.compare(a.title, b.title))
    }
    folder.folders.forEach(sortNotes)
  }
  sortNotes(root)

  const countNotes = (folder: FolderNode): number => {
    folder.count =
      folder.notes.length + folder.folders.reduce((sum, child) => sum + countNotes(child), 0)
    return folder.count
  }
  countNotes(root)
  return root
}

/**
 * Flattens the tree into render rows: folders first, then notes, skipping the
 * contents of collapsed folders. Flat rows keep re-renders cheap and let the
 * template stay recursive-free.
 */
export function flattenTree(root: FolderNode, collapsed: Set<string>): TreeRow[] {
  const rows: TreeRow[] = []
  const walk = (folder: FolderNode, depth: number): void => {
    for (const child of folder.folders) {
      rows.push({ key: `d:${child.path}`, depth, folder: child })
      if (!collapsed.has(child.path)) walk(child, depth + 1)
    }
    for (const note of folder.notes) {
      rows.push({ key: `n:${note.id}`, depth, note })
    }
  }
  walk(root, 0)
  return rows
}

/**
 * Filters the tree for the sidebar query. Notes match on title and path; a
 * folder whose own name matches keeps its whole subtree, which is what typing
 * a folder name should do. Returns the root with no children when nothing
 * matches, so callers can render one empty state instead of a null check.
 */
export function filterTree(root: FolderNode, query: string): FolderNode {
  const needle = query.trim().toLowerCase()
  if (!needle) return root

  const walk = (folder: FolderNode): FolderNode | null => {
    if (folder.name && folder.name.toLowerCase().includes(needle)) return folder
    const folders: FolderNode[] = []
    for (const child of folder.folders) {
      const kept = walk(child)
      if (kept) folders.push(kept)
    }
    const notes = folder.notes.filter(
      (note) =>
        note.title.toLowerCase().includes(needle) ||
        note.relPath.toLowerCase().includes(needle),
    )
    if (!folders.length && !notes.length) return null
    const count = notes.length + folders.reduce((sum, child) => sum + child.count, 0)
    return { ...folder, folders, notes, count }
  }

  return walk(root) ?? { ...root, folders: [], notes: [], count: 0 }
}

/** Every folder path in the tree, used by collapse-all. */
export function folderPaths(root: FolderNode): string[] {
  const paths: string[] = []
  const walk = (folder: FolderNode): void => {
    for (const child of folder.folders) {
      paths.push(child.path)
      walk(child)
    }
  }
  walk(root)
  return paths
}

/** Folder paths from the vault root down to a note's parent. */
export function ancestorPaths(relPath: string): string[] {
  const segments = relPath.split('/').filter(Boolean)
  segments.pop()
  const paths: string[] = []
  let path = ''
  for (const segment of segments) {
    path = path ? `${path}/${segment}` : segment
    paths.push(path)
  }
  return paths
}
