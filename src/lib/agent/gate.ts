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

/** The note audit reports "问题：无" only when every structural rule passed. */
export function noteClean(observation: string): boolean {
  return /问题：无/.test(observation)
}

/**
 * Minimal note gate: the author decides structure and length, so only a
 * non-empty draft with a title is required. This is the one-sentence variant;
 * the strict audit belongs to the spec-driven template.
 */
export function noteMinimalGate(draft: string): GateResult {
  const text = draft.trim()
  if (!text) return { ok: false, reason: '草稿为空，拒绝提交' }
  if (!text.startsWith('# ')) return { ok: false, reason: '草稿没有一级标题，拒绝提交' }
  const cjk = (text.match(/[一-鿿]/g) ?? []).length
  if (cjk < 200) return { ok: false, reason: `正文只有 ${cjk} 字，拒绝提交` }
  return { ok: true, reason: '' }
}

/**
 * Note-author gate: the last audit must come after the last write and report a
 * clean structure, and the draft must contain a title. The model's own
 * "finish" is not evidence.
 */
export function noteGate(turns: AgentTurn[], draft: string): GateResult {
  if (!draft.trim() || !draft.startsWith('# ')) {
    return { ok: false, reason: '草稿为空或没有一级标题，拒绝提交' }
  }
  const lastAudit = lastToolTurn(turns, ['audit_note'])
  const lastWrite = lastToolTurn(turns, ['write_note', 'patch_section'])
  if (!lastAudit) return { ok: false, reason: '没有结构审稿记录，拒绝提交' }
  if (lastAudit.index < (lastWrite?.index ?? -1)) {
    return { ok: false, reason: '审稿发生在最后一次修改之前，拒绝提交' }
  }
  if (!noteClean(lastAudit.observation)) {
    return { ok: false, reason: '结构问题没有清零，拒绝提交' }
  }
  return { ok: true, reason: '' }
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
