import type { AgentTurn } from './types'

export interface GateResult {
  ok: boolean
  reason: string
}

export function lastToolTurn(
  turns: AgentTurn[],
  tools: string[],
): { index: number; observation: string } | null {
  for (let index = turns.length - 1; index >= 0; index -= 1) {
    const turn = turns[index]
    if (turn.tool && tools.includes(turn.tool)) {
      return { index, observation: turn.observation ?? '' }
    }
  }
  return null
}

/** The inspect summary must parse and every layout counter must be zero. */
export function layoutClean(observation: string): boolean {
  const summary = observation.split('问题：')[0]
  const values = [...summary.matchAll(/(?:页溢出|溢出|越界|小字|SVG 越界|空框|标签重叠|文字溢出)\s*(\d+)/g)].map(
    (match) => Number(match[1]),
  )
  return values.length >= 4 && values.every((value) => value === 0)
}

export function reviewPassed(observation: string): boolean {
  const text = observation.trim()
  return text.startsWith('通过') || text.startsWith('无')
}

/**
 * Deterministic gate: never trust the model's own "finish". The last inspect
 * and review must come after the last patch, and both must pass.
 */
export function agentGate(turns: AgentTurn[]): GateResult {
  const lastInspect = lastToolTurn(turns, ['inspect'])
  const lastReview = lastToolTurn(turns, ['review'])
  const lastPatch = lastToolTurn(turns, ['patch_figure', 'patch_section', 'edit_file', 'write_file'])
  const patchedAt = lastPatch?.index ?? -1
  const clean = Boolean(lastInspect && lastInspect.index > patchedAt && layoutClean(lastInspect.observation))
  const reviewed = Boolean(lastReview && lastReview.index > patchedAt && reviewPassed(lastReview.observation))
  if (!clean) return { ok: false, reason: '版面指标未清零，拒绝提交' }
  if (!reviewed) return { ok: false, reason: '审校未通过，或审校发生在最后一次修改之前，拒绝提交' }
  return { ok: true, reason: '' }
}
