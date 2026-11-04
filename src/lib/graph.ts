import type { NoteMeta } from '../types'

export type NodeKind =
  | 'note'
  | 'tag'
  | 'person'
  | 'period'
  | 'source'
  | 'up'
  | 'domain'
  | 'type'
  | 'topic'

export interface GraphNode {
  id: string
  kind: NodeKind
  label: string
  noteId?: string
  meta?: string
  weight: number
}

export interface GraphEdge {
  source: string
  target: string
  kind: NodeKind | 'tag'
  label: string
}

export interface NoteLink {
  a: string
  b: string
  common: GraphNode[]
}

export interface Graph {
  nodes: GraphNode[]
  nodeMap: Record<string, GraphNode>
  edges: GraphEdge[]
  notes: NoteMeta[]
  noteLinks: NoteLink[]
}

export function tagKind(label: string): string {
  if (label.startsWith('来源/')) return 'source'
  if (label.startsWith('UP主/') || label.startsWith('UP主名称/')) return 'up'
  if (label.startsWith('领域/')) return 'domain'
  if (label.startsWith('类型/')) return 'type'
  if (label.startsWith('主题/')) return 'topic'
  return 'tag'
}

export function tagValue(label: string): string {
  return label.includes('/') ? label.split('/').pop() ?? label : label
}

export function noteShort(title: string, max = 7): string {
  const cleaned = title.replace(/^【[^】]*】/, '').trim()
  const cut = cleaned.split(/[：:，,。！!？?]/)[0] ?? cleaned
  return cut.length > max ? `${cut.slice(0, max)}…` : cut
}

export function attrList(note: NoteMeta, includeSource = false): GraphNode[] {
  const out: GraphNode[] = []
  for (const tag of note.tags ?? []) {
    if (!includeSource && tag.startsWith('来源/')) continue
    out.push({
      id: `t:${tag}`,
      kind: tagKind(tag) as NodeKind,
      label: tag,
      weight: 0,
    })
  }
  for (const person of note.people ?? []) {
    out.push({ id: `p:${person}`, kind: 'person', label: person, weight: 0 })
  }
  for (const period of note.periods ?? []) {
    out.push({ id: `d:${period}`, kind: 'period', label: period, weight: 0 })
  }
  return out
}

export function buildGraph(notes: NoteMeta[], includeSource = false): Graph {
  const nodeMap: Record<string, GraphNode> = {}
  const nodes: GraphNode[] = []
  const edges: GraphEdge[] = []

  const addNode = (node: GraphNode) => {
    if (nodeMap[node.id]) return nodeMap[node.id]
    node.weight = 0
    nodeMap[node.id] = node
    nodes.push(node)
    return node
  }

  for (const note of notes) {
    addNode({
      id: `n:${note.id}`,
      kind: 'note',
      label: noteShort(note.title, 9),
      noteId: note.id,
      meta: `${note.up} · ${note.date}`,
      weight: 0,
    })
  }

  const noteAttrs: Record<string, GraphNode[]> = {}
  for (const note of notes) {
    const attrs = attrList(note, includeSource)
    noteAttrs[note.id] = attrs
    for (const attr of attrs) {
      addNode({ ...attr })
      edges.push({
        source: `n:${note.id}`,
        target: attr.id,
        kind: attr.kind,
        label: attr.label,
      })
    }
  }

  for (const edge of edges) {
    if (nodeMap[edge.source]) nodeMap[edge.source].weight += 1
    if (nodeMap[edge.target]) nodeMap[edge.target].weight += 1
  }

  const noteLinks: NoteLink[] = []
  for (let i = 0; i < notes.length; i += 1) {
    for (let j = i + 1; j < notes.length; j += 1) {
      const a = notes[i]
      const b = notes[j]
      const A = noteAttrs[a.id] ?? []
      const B = noteAttrs[b.id] ?? []
      const common = A.filter((item) => B.some((other) => other.id === item.id))
      if (common.length) noteLinks.push({ a: a.id, b: b.id, common })
    }
  }

  return { nodes, nodeMap, edges, notes, noteLinks }
}

export function sharedBetween(a: NoteMeta, b: NoteMeta, includeSource = false): GraphNode[] {
  const A = attrList(a, includeSource)
  const B = attrList(b, includeSource)
  return A.filter((item) => B.some((other) => other.id === item.id))
}

export interface LocalGraph {
  current: GraphNode
  attrs: GraphNode[]
  related: Array<{ note: NoteMeta; node: GraphNode; common: GraphNode[] }>
}

export function localGraph(notes: NoteMeta[], noteId: string): LocalGraph | null {
  const currentNote = notes.find((note) => note.id === noteId)
  if (!currentNote) return null
  const current: GraphNode = {
    id: `n:${currentNote.id}`,
    kind: 'note',
    label: noteShort(currentNote.title, 6),
    noteId: currentNote.id,
    weight: 0,
  }

  const related = notes
    .filter((note) => note.id !== noteId)
    .map((note) => {
      const common = sharedBetween(currentNote, note, false)
      return {
        note,
        node: {
          id: `n:${note.id}`,
          kind: 'note' as const,
          label: noteShort(note.title, 6),
          noteId: note.id,
          weight: 0,
        },
        common,
      }
    })
    .filter((entry) => entry.common.length > 0)
    .sort((a, b) => b.common.length - a.common.length)
    .slice(0, 3)

  const commonIds = new Set(related.flatMap((entry) => entry.common.map((item) => item.id)))
  const rank: Record<string, number> = {
    tag: 0,
    domain: 0,
    type: 1,
    topic: 2,
    up: 3,
    person: 4,
    period: 5,
    source: 6,
  }
  const sortedAttrs = attrList(currentNote, true).sort((a, b) => {
    const aCommon = commonIds.has(a.id) ? -1 : 0
    const bCommon = commonIds.has(b.id) ? -1 : 0
    if (aCommon !== bCommon) return aCommon - bCommon
    const aRank = rank[a.kind] ?? 9
    const bRank = rank[b.kind] ?? 9
    return aRank - bRank
  })
  // Six slots, but do not let one kind eat the whole map: at most two
  // people and one period. An attribute shared with a related note is always
  // kept, otherwise its edge would disappear.
  const caps: Record<string, number> = {
    person: 2,
    period: 1,
    up: 1,
    domain: 1,
    type: 1,
    topic: 1,
    source: 1,
    tag: 2,
  }
  const counts: Record<string, number> = {}
  const attrs: GraphNode[] = []
  for (const attr of sortedAttrs) {
    if (attrs.length >= 6) break
    const shared = commonIds.has(attr.id)
    if (!shared && (counts[attr.kind] ?? 0) >= (caps[attr.kind] ?? 2)) continue
    attrs.push(attr)
    counts[attr.kind] = (counts[attr.kind] ?? 0) + 1
  }

  return { current, attrs, related }
}

export function kindLabel(kind: string): string {
  const map: Record<string, string> = {
    note: '笔记',
    tag: '标签',
    source: '来源',
    up: 'UP 主',
    domain: '领域',
    type: '类型',
    topic: '主题',
    person: '人物',
    period: '时期',
  }
  return map[kind] ?? kind
}
