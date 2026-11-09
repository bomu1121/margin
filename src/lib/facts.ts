import type { Fact } from '../types'

type FactLike = Pick<Fact, 'status'> & { domain?: string; citationCount?: number }

export function factIsHistory(fact: { domain?: string }): boolean {
  return fact.domain === 'history'
}

/** Same composite heuristic used by the curation workbench. */
export function isCompositeFact(fact: Pick<Fact, 'text' | 'related'>): boolean {
  const clauses = (fact.text.match(/[，,；;]/g) ?? []).length
  return clauses >= 2 && fact.text.length >= 30 && fact.related.length > 0
}

export function factStatusLabel(fact: FactLike): string {
  if (fact.status === 'split') return '已拆分'
  if (fact.status === 'disputed') return '存在争议'
  if (factIsHistory(fact) && (fact.citationCount ?? 0) > 0) return '有史料'
  if (fact.status === 'unsupported') return '未找到证据'
  if (fact.status === 'verified') return '有证据支持'
  return '未核验'
}

export function factStatusClass(fact: FactLike): string {
  if (fact.status === 'split') return 'is-split'
  if (fact.status === 'disputed') return 'is-disputed'
  if (factIsHistory(fact) && (fact.citationCount ?? 0) > 0) return 'is-verified'
  if (fact.status === 'unsupported') return 'is-unsupported'
  if ((fact.citationCount ?? 0) > 0 || fact.status === 'verified') return 'is-verified'
  return 'is-candidate'
}

export function factStatusKey(fact: FactLike): string {
  if (fact.status === 'split') return 'split'
  if (fact.status === 'disputed') return 'disputed'
  if (factIsHistory(fact) && (fact.citationCount ?? 0) > 0) return 'verified'
  if (fact.status === 'unsupported') return 'unsupported'
  if ((fact.citationCount ?? 0) > 0 || fact.status === 'verified') return 'verified'
  return 'candidate'
}

export function factHasEvidence(fact: FactLike): boolean {
  if (factIsHistory(fact)) return (fact.citationCount ?? 0) > 0 || fact.status === 'verified'
  return fact.status === 'verified'
}

export function factSourceSummary(fact: Pick<Fact, 'domain' | 'citationCount' | 'evidence'>): string {
  const parts: string[] = []
  if (factIsHistory(fact) && fact.citationCount > 0) parts.push(`${fact.citationCount} 条史料`)
  if (fact.evidence.length > 0) parts.push(`${fact.evidence.length} 条网页`)
  if (!parts.length && factIsHistory(fact)) return '待史料'
  return parts.join(' · ') || '0 条来源'
}
