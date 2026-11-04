import type { GraphNode, LocalGraph } from './graph'
import type { NoteMeta } from '../types'

export interface PlacedAttr {
  node: GraphNode
  x: number
  y: number
  weight: number
}

export interface PlacedRelated {
  note: NoteMeta
  node: GraphNode
  common: GraphNode[]
  x: number
  y: number
  weight: number
}

export interface LayoutEdge {
  source: string
  target: string
  kind: string
}

export interface LocalGraphLayout {
  current: GraphNode
  attrs: PlacedAttr[]
  related: PlacedRelated[]
  edges: LayoutEdge[]
}

interface Body {
  x: number
  y: number
  vx: number
  vy: number
}

const ATTR_RADIUS = 0.58
const RELATED_RADIUS = 0.94
const TICKS = 280

function clampRadius(body: Body, max: number): void {
  const distance = Math.hypot(body.x, body.y)
  if (distance > max && distance > 1e-6) {
    body.x = (body.x / distance) * max
    body.y = (body.y / distance) * max
  }
}

/**
 * Deterministic local force layout in a unit disc.
 *
 * The graph is tiny (center + <=6 attributes + <=3 notes), so a direct O(n^2)
 * pass is cheaper and more predictable than pulling in a physics dependency.
 * The result is computed once per note; the renderer only animates the reveal
 * and hover, never a permanent simulation loop.
 */
export function layoutLocalGraph(graph: LocalGraph): LocalGraphLayout {
  const count = Math.max(1, graph.attrs.length)
  const attrs: Array<PlacedAttr & Body> = graph.attrs.map((node, index) => {
    const angle = -Math.PI / 2 + (Math.PI * 2 * index) / count
    const radius = ATTR_RADIUS + (index % 2 === 0 ? 0 : 0.018)
    const shared = graph.related.filter((entry) =>
      entry.common.some((item) => item.id === node.id),
    ).length
    return {
      node,
      x: Math.cos(angle) * radius,
      y: Math.sin(angle) * radius,
      vx: 0,
      vy: 0,
      weight: shared,
    }
  })

  const attrById = new Map(attrs.map((attr) => [attr.node.id, attr]))
  const related: Array<PlacedRelated & Body> = graph.related.map((entry, index) => {
    const anchor = entry.common
      .map((item) => attrById.get(item.id))
      .find((item): item is PlacedAttr & Body => Boolean(item))
    const baseAngle = anchor
      ? Math.atan2(anchor.y, anchor.x)
      : -Math.PI / 2 + (Math.PI * 2 * index) / Math.max(1, graph.related.length)
    const spread = graph.related.length > 1 ? (index - (graph.related.length - 1) / 2) * 0.34 : 0
    const angle = baseAngle + spread
    return {
      note: entry.note,
      node: entry.node,
      common: entry.common,
      x: Math.cos(angle) * RELATED_RADIUS,
      y: Math.sin(angle) * RELATED_RADIUS,
      vx: 0,
      vy: 0,
      weight: entry.common.length,
    }
  })

  for (let tick = 0; tick < TICKS; tick += 1) {
    for (const attr of attrs) {
      const distance = Math.hypot(attr.x, attr.y) || 1e-6
      const radial = (ATTR_RADIUS - distance) * 0.06
      attr.vx += (attr.x / distance) * radial
      attr.vy += (attr.y / distance) * radial

      for (const other of attrs) {
        if (other === attr) continue
        const dx = attr.x - other.x
        const dy = attr.y - other.y
        const d2 = dx * dx + dy * dy + 0.004
        const force = 0.004 / d2
        attr.vx += dx * force
        attr.vy += dy * force
      }

      for (const note of related) {
        const dx = attr.x - note.x
        const dy = attr.y - note.y
        const d2 = dx * dx + dy * dy + 0.01
        const force = 0.0016 / d2
        attr.vx += dx * force
        attr.vy += dy * force
      }

      attr.x += attr.vx
      attr.y += attr.vy
      attr.vx *= 0.8
      attr.vy *= 0.8
      clampRadius(attr, 0.72)
    }

    for (const note of related) {
      // Weak pull to the outer ring keeps the satellites from drifting away.
      const distance = Math.hypot(note.x, note.y) || 1e-6
      const radial = (RELATED_RADIUS - distance) * 0.01
      note.vx += (note.x / distance) * radial
      note.vy += (note.y / distance) * radial

      for (const common of note.common) {
        const attr = attrById.get(common.id)
        if (!attr) continue
        const dx = attr.x - note.x
        const dy = attr.y - note.y
        const dist = Math.hypot(dx, dy) || 1e-6
        const spring = (dist - 0.34) * 0.035
        note.vx += (dx / dist) * spring
        note.vy += (dy / dist) * spring
      }

      for (const other of related) {
        if (other === note) continue
        const dx = note.x - other.x
        const dy = note.y - other.y
        const d2 = dx * dx + dy * dy + 0.01
        const force = 0.0022 / d2
        note.vx += dx * force
        note.vy += dy * force
      }

      for (const attr of attrs) {
        const dx = note.x - attr.x
        const dy = note.y - attr.y
        const d2 = dx * dx + dy * dy + 0.012
        const force = 0.0014 / d2
        note.vx += dx * force
        note.vy += dy * force
      }

      note.x += note.vx
      note.y += note.vy
      note.vx *= 0.82
      note.vy *= 0.82
      clampRadius(note, 1)
    }
  }

  const edges: LayoutEdge[] = []
  for (const attr of attrs) {
    edges.push({ source: graph.current.id, target: attr.node.id, kind: attr.node.kind })
  }
  for (const note of related) {
    for (const common of note.common) {
      if (!attrById.has(common.id)) continue
      edges.push({ source: common.id, target: note.node.id, kind: common.kind })
    }
  }

  return {
    current: graph.current,
    attrs,
    related,
    edges,
  }
}
