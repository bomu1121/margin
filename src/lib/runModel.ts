import type {
  ProductionJob,
  ProductionLog,
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

export const NODE_W = 156
export const NODE_H = 84
export const BAND_H = 112
export const GAP_X = 28
export const GAP_Y = 18
export const PAD = 20
export const BAND_LABEL_H = 24

export function phaseFor(key: string): RunPhase {
  if (['source', 'transcribe', 'refine'].includes(key)) {
    return { key: 'source', label: '素材', index: 0 }
  }
  if (['outline', 'chapter', 'assemble', 'design', 'plan', 'author', 'write'].includes(key)) {
    return { key: 'generate', label: '生成', index: 1 }
  }
  if (key === 'agent') return { key: 'quality', label: '质检', index: 2 }
  if (['render', 'fix', 'print'].includes(key)) return { key: 'render', label: '渲染', index: 3 }
  if (['pagecheck', 'judge'].includes(key)) return { key: 'verify', label: '验收', index: 4 }
  if (key === 'publish') return { key: 'publish', label: '入库', index: 5 }
  if (['book_build', 'book_measure', 'book_print'].includes(key)) {
    return { key: 'export', label: '导出', index: 6 }
  }
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

export function buildRunNodes(
  job: ProductionJob,
  graph: ProductionRunGraph | null,
  logs: ProductionLog[] = [],
): RunNode[] {
  const runs = graph?.nodes ?? []
  const artifacts = graph?.artifacts ?? []
  const runMap = new Map(runs.map((node) => [node.key, node]))
  const terminal = ['success', 'error', 'interrupted', 'cancelled'].includes(job.status)
  const jobEnd = job.finishedAt || job.updatedAt || Date.now()
  const stepEnd = (stepIndex: number): number => {
    if (stepIndex < 0) return jobEnd
    const entries = logs.filter((log) => log.stepIndex === stepIndex)
    return entries.length ? entries[entries.length - 1].createdAt : jobEnd
  }
  const list: RunNode[] = job.steps.map((step) => {
    const run = runMap.get(step.key)
    const raw = stepStatus(step, run)
    return {
      key: step.key,
      label: step.label,
      status: normalizeStatus(job, raw),
      startedAt: run?.startedAt ?? 0,
      endedAt: run?.endedAt || (terminal && raw === 'running' ? stepEnd(step.index) : 0),
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
 * Layered layout for a mostly-linear workflow: one row per phase, and a
 * single-node phase is pulled under its predecessor so the vertical handoff
 * stays short. Multiple nodes in a phase spread to the right.
 */
export function layoutRun(nodes: RunNode[]): RunLayout {
  // A long workflow (the HTML-author chain) reads better as one horizontal
  // strip with phase groups than as one row per phase: five phase rows would
  // shrink the cards to an unreadable size.
  if (nodes.length > 6) {
    const y = PAD + BAND_LABEL_H;
    const positioned = nodes.map((node, index) => ({
      ...node,
      x: PAD + index * (NODE_W + GAP_X),
      y,
    }));
    const edges: RunEdge[] = [];
    for (let index = 0; index < positioned.length - 1; index += 1) {
      const from = positioned[index];
      const to = positioned[index + 1];
      edges.push({ from, to, d: edgePath(from, to), active: to.status === 'running' && from.status !== 'queued' });
    }
    const bands: PhaseBand[] = [];
    let start = 0;
    while (start < nodes.length) {
      let end = start;
      while (end + 1 < nodes.length && nodes[end + 1].phase.key === nodes[start].phase.key) end += 1;
      const first = positioned[start];
      const last = positioned[end];
      bands.push({
        key: nodes[start].phase.key,
        label: nodes[start].phase.label,
        y: PAD,
        height: BAND_LABEL_H + NODE_H + 16,
        x: first.x - 10,
        width: last.x + NODE_W - first.x + 20,
      });
      start = end + 1;
    }
    return {
      nodes: positioned,
      edges,
      bands,
      width: PAD * 2 + (nodes.length - 1) * (NODE_W + GAP_X) + NODE_W,
      height: PAD * 2 + BAND_LABEL_H + NODE_H + 16,
    };
  }

  const phases = new Map<number, RunNode[]>()
  for (const node of nodes) {
    const list = phases.get(node.phase.index) ?? []
    list.push(node)
    phases.set(node.phase.index, list)
  }
  const phaseIndexes = [...phases.keys()].sort((a, b) => a - b)
  const positioned = new Map<string, PositionedNode>()
  const edges: RunEdge[] = []
  const bands: PhaseBand[] = []
  let previousLastX = PAD
  let maxX = PAD

  phaseIndexes.forEach((phaseIndex, row) => {
    const rowNodes = phases.get(phaseIndex) ?? []
    const y = PAD + row * (BAND_H + GAP_Y) + BAND_LABEL_H
    let x = row > 0 ? previousLastX : PAD
    let rowRight = x
    for (const node of rowNodes) {
      const item: PositionedNode = { ...node, x, y }
      positioned.set(node.key, item)
      rowRight = Math.max(rowRight, x + NODE_W)
      x += NODE_W + GAP_X
    }
    if (rowNodes.length) {
      const last = positioned.get(rowNodes[rowNodes.length - 1].key)
      if (last) previousLastX = last.x
    }
    maxX = Math.max(maxX, rowRight)
    const first = positioned.get(rowNodes[0].key)
    bands.push({
      key: rowNodes[0].phase.key,
      label: rowNodes[0].phase.label,
      y: first ? first.y - BAND_LABEL_H - 4 : PAD,
      height: BAND_H,
      x: first ? first.x - 10 : PAD - 10,
      width: first ? rowRight - first.x + 20 : NODE_W + 20,
    })
  })

  for (let index = 0; index < nodes.length - 1; index += 1) {
    const from = positioned.get(nodes[index].key)
    const to = positioned.get(nodes[index + 1].key)
    if (!from || !to) continue
    edges.push({
      from,
      to,
      d: edgePath(from, to),
      active: to.status === 'running' && from.status !== 'queued',
    })
  }

  return {
    nodes: [...positioned.values()],
    edges,
    bands,
    width: maxX + PAD,
    height: PAD * 2 + phaseIndexes.length * (BAND_H + GAP_Y) - GAP_Y,
  }
}

export function edgePath(from: PositionedNode, to: PositionedNode): string {
  const fx = from.x + NODE_W / 2
  const fy = from.y + NODE_H
  const tx = to.x + NODE_W / 2
  const ty = to.y
  if (Math.abs(fx - tx) < 1) return `M${fx} ${fy} V${ty - 4}`
  if (from.y === to.y) {
    const leftToRight = to.x > from.x
    const x1 = leftToRight ? from.x + NODE_W : from.x
    const x2 = leftToRight ? to.x - 4 : to.x + NODE_W + 4
    return `M${x1} ${from.y + NODE_H / 2} H${x2}`
  }
  const midY = fy + (ty - fy) / 2
  return `M${fx} ${fy} V${midY} H${tx} V${ty - 4}`
}

export function nodeDuration(node: { startedAt: number; endedAt: number }): number {
  if (!node.startedAt) return 0
  const end = node.endedAt || Date.now()
  return Math.max(0, Math.round((end - node.startedAt) / 1000))
}
