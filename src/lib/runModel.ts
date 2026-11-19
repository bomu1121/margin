import type {
  ProductionJob,
  ProductionRunArtifact,
  ProductionRunGraph,
  ProductionStep,
} from '../types'

export type RunPhaseKey = 'source' | 'generate' | 'quality' | 'render' | 'verify' | 'publish' | 'export' | 'other'

export interface RunPhase {
  key: RunPhaseKey
  label: string
  index: number
}

export interface RunNode {
  key: string
  label: string
  status: string
  startedAt: number
  endedAt: number
  tokens: number
  cost: number
  summary: string
  error: string
  artifacts: ProductionRunArtifact[]
  stepIndex: number
  stepDetail: string
  phase: RunPhase
}

export interface PositionedNode extends RunNode {
  x: number
  y: number
}

export interface RunEdge {
  from: PositionedNode
  to: PositionedNode
  d: string
  active: boolean
}

export interface PhaseBand {
  key: RunPhaseKey
  label: string
  y: number
  height: number
  x: number
  width: number
}

export const NODE_W = 180
export const NODE_H = 92
const COL_GAP = 48
const STACK_GAP = 16
const BAND_LABEL_H = 30
const PAD = 24

export function isAgentNode(key: string | undefined): boolean {
  return key === 'agent_note'
}

export function phaseFor(key: string): RunPhase {
  if (['source', 'transcribe', 'refine'].includes(key)) {
    return { key: 'source', label: '素材', index: 0 }
  }
  if (['prompt', 'write', 'agent_note'].includes(key)) {
    return { key: 'generate', label: '生成', index: 1 }
  }
  if (key === 'trace') return { key: 'quality', label: '质检', index: 2 }
  if (key === 'publish') return { key: 'publish', label: '入库', index: 5 }
  return { key: 'other', label: '其他', index: 6 }
}

function stepStatus(step: ProductionStep, run?: { status: string }): string {
  if (run) return run.status
  return step.status === 'pending' ? 'queued' : step.status
}

/**
 * A killed run leaves its last node marked "running" forever. Once the job is
 * terminal, normalize that stale status and freeze its end at the last log for
 * that step (or the job's end), so the page never shows a 5-hour "running" node.
 */
function normalizeStatus(job: ProductionJob, raw: string): string {
  const terminal = ['success', 'error', 'interrupted', 'cancelled'].includes(job.status)
  if (!terminal) return raw === 'pending' ? 'queued' : raw
  if (raw === 'running') {
    if (job.status === 'success') return 'done'
    if (job.status === 'error') return 'error'
    if (job.status === 'interrupted') return 'interrupted'
    return 'cancelled'
  }
  if (raw === 'pending' || raw === 'queued') return 'skipped'
  return raw
}

export function buildRunNodes(job: ProductionJob, graph: ProductionRunGraph | null): RunNode[] {
  const runs = graph?.nodes ?? []
  const artifacts = graph?.artifacts ?? []
  const runMap = new Map(runs.map((node) => [node.key, node]))
  const terminal = ['success', 'error', 'interrupted', 'cancelled'].includes(job.status)
  const jobEnd = job.finishedAt || job.updatedAt || Date.now()
  const list: RunNode[] = job.steps.map((step) => {
    const run = runMap.get(step.key)
    const raw = stepStatus(step, run)
    return {
      key: step.key,
      label: step.label,
      status: normalizeStatus(job, raw),
      startedAt: run?.startedAt ?? 0,
      endedAt: run?.endedAt || (terminal && run?.startedAt ? jobEnd : 0),
      tokens: run?.tokens ?? 0,
      cost: run?.cost ?? 0,
      summary: run?.summary || step.detail || '',
      error: run?.error ?? '',
      artifacts: artifacts.filter((item) => item.nodeKey === step.key),
      stepIndex: step.index,
      stepDetail: step.detail,
      phase: phaseFor(step.key),
    }
  })
  for (const run of runs) {
    if (job.steps.some((step) => step.key === run.key)) continue
    list.push({
      key: run.key,
      label: run.label || run.key,
      status: normalizeStatus(job, run.status),
      startedAt: run.startedAt,
      endedAt: run.endedAt || (terminal && run.status === 'running' ? jobEnd : 0),
      tokens: run.tokens,
      cost: run.cost,
      summary: run.summary,
      error: run.error,
      artifacts: artifacts.filter((item) => item.nodeKey === run.key),
      stepIndex: -1,
      stepDetail: '',
      phase: phaseFor(run.key),
    })
  }
  return list.sort((a, b) => {
    if (a.phase.index !== b.phase.index) return a.phase.index - b.phase.index
    const ai = a.stepIndex < 0 ? 1000 : a.stepIndex
    const bi = b.stepIndex < 0 ? 1000 : b.stepIndex
    return ai - bi || a.key.localeCompare(b.key)
  })
}

export interface RunLayout {
  nodes: PositionedNode[]
  edges: RunEdge[]
  bands: PhaseBand[]
  width: number
  height: number
}

/**
 * Stage-column layout: one column per phase, nodes stacked inside the column.
 * A linear pipeline (source → agent → publish) becomes three readable cards
 * across the canvas instead of a narrow vertical strip with clipped ends.
 */
export function layoutRun(nodes: RunNode[]): RunLayout {
  if (!nodes.length) {
    return {
      nodes: [],
      edges: [],
      bands: [],
      width: NODE_W + PAD * 2,
      height: NODE_H + BAND_LABEL_H + PAD * 2,
    }
  }
  const columns = new Map<number, RunNode[]>()
  for (const node of nodes) {
    const list = columns.get(node.phase.index) ?? []
    list.push(node)
    columns.set(node.phase.index, list)
  }
  const phaseIndexes = [...columns.keys()].sort((a, b) => a - b)
  const positioned: PositionedNode[] = []
  const byKey = new Map<string, PositionedNode>()
  const bands: PhaseBand[] = []
  let maxColumnHeight = NODE_H

  phaseIndexes.forEach((phaseIndex, columnIndex) => {
    const columnNodes = columns.get(phaseIndex) ?? []
    const x = PAD + columnIndex * (NODE_W + COL_GAP)
    columnNodes.forEach((node, rowIndex) => {
      const item: PositionedNode = {
        ...node,
        x,
        y: PAD + BAND_LABEL_H + rowIndex * (NODE_H + STACK_GAP),
      }
      positioned.push(item)
      byKey.set(node.key, item)
    })
    const columnHeight = columnNodes.length * NODE_H + (columnNodes.length - 1) * STACK_GAP
    maxColumnHeight = Math.max(maxColumnHeight, columnHeight)
    bands.push({
      key: columnNodes[0].phase.key,
      label: columnNodes[0].phase.label,
      x: x - 12,
      y: PAD,
      width: NODE_W + 24,
      height: BAND_LABEL_H + columnHeight + 16,
    })
  })

  const edges: RunEdge[] = []
  for (let index = 0; index < nodes.length - 1; index += 1) {
    const from = byKey.get(nodes[index].key)
    const to = byKey.get(nodes[index + 1].key)
    if (!from || !to) continue
    edges.push({
      from,
      to,
      d: edgePath(from, to),
      active: to.status === 'running' && from.status !== 'queued',
    })
  }

  return {
    nodes: positioned,
    edges,
    bands,
    width: PAD * 2 + phaseIndexes.length * NODE_W + (phaseIndexes.length - 1) * COL_GAP,
    height: PAD * 2 + BAND_LABEL_H + maxColumnHeight + 8,
  }
}

export function edgePath(from: PositionedNode, to: PositionedNode): string {
  if (Math.abs(from.x - to.x) < 1) {
    const downward = to.y >= from.y
    const startY = from.y + (downward ? NODE_H : 0)
    const endY = to.y + (downward ? 0 : NODE_H)
    return `M${from.x + NODE_W / 2} ${startY} V${endY + (downward ? -4 : 4)}`
  }
  const rightward = to.x > from.x
  const startX = from.x + (rightward ? NODE_W : 0)
  const endX = to.x + (rightward ? 0 : NODE_W)
  const startY = from.y + NODE_H / 2
  const endY = to.y + NODE_H / 2
  const midX = (startX + endX) / 2
  return `M${startX} ${startY} C${midX} ${startY} ${midX} ${endY} ${endX + (rightward ? -4 : 4)} ${endY}`
}

export function nodeDuration(node: { startedAt: number; endedAt: number }): number {
  if (!node.startedAt) return 0
  const end = node.endedAt || Date.now()
  return Math.max(0, Math.round((end - node.startedAt) / 1000))
}
