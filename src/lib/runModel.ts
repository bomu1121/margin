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

export function isAgentNode(key: string | undefined): boolean {
  return key === 'agent_note'
}

/** Publish is the same step in every template; keep one readable label even
 * for jobs created before the template label was renamed. */
export function stepLabel(key: string, label: string): string {
  return key === 'publish' ? '保存笔记' : label
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
      label: stepLabel(step.key, step.label),
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
      label: stepLabel(run.key, run.label || run.key),
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
