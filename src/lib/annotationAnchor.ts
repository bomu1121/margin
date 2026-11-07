import type { Block } from './markdown'

export interface AnchorAnnotation {
  id: string
  quote: string
}

export interface AnchorMatch {
  id: string
  blockId: string
  score: number
  margin: number
}

export interface AnchorPlan {
  matches: AnchorMatch[]
  unresolved: number
}

const MIN_SCORE = 0.86
const MIN_MARGIN = 0.08
const MIN_FUZZY_LENGTH = 8

export function normalizeAnchor(text: string): string {
  return text
    .replace(/!\[([^\]]*)\]\([^)]*\)/g, '$1')
    .replace(/\[([^\]]*)\]\([^)]*\)/g, '$1')
    .replace(/<[^>]+>/g, '')
    .replace(/[`*_>#~|]/g, '')
    .replace(/[\s\p{P}\p{S}]+/gu, '')
    .toLowerCase()
}

function bigrams(value: string): string[] {
  if (value.length < 2) return value ? [value] : []
  const items: string[] = []
  for (let index = 0; index < value.length - 1; index += 1) {
    items.push(value.slice(index, index + 2))
  }
  return items
}

function dice(a: string, b: string): number {
  if (!a || !b) return 0
  if (a === b) return 1
  const left = bigrams(a)
  const right = bigrams(b)
  if (!left.length || !right.length) return 0
  const counts = new Map<string, number>()
  for (const item of left) counts.set(item, (counts.get(item) ?? 0) + 1)
  let overlap = 0
  for (const item of right) {
    const count = counts.get(item) ?? 0
    if (count > 0) {
      overlap += 1
      counts.set(item, count - 1)
    }
  }
  return (2 * overlap) / (left.length + right.length)
}

function score(quote: string, blockText: string): number {
  const left = normalizeAnchor(quote)
  const right = normalizeAnchor(blockText)
  if (!left || !right) return 0
  if (left === right) return 1
  if (Math.min(left.length, right.length) < MIN_FUZZY_LENGTH) return 0
  // A quote embedded in a longer block is a strong signal, but still leaves
  // room for a second candidate to win the margin check.
  if (right.includes(left) || left.includes(right)) return 0.95
  return dice(left, right)
}

export function planAnchorRelocations(
  annotations: AnchorAnnotation[],
  blocks: Block[],
): AnchorPlan {
  const matches: AnchorMatch[] = []
  let unresolved = 0
  for (const annotation of annotations) {
    const ranked = blocks
      .map((block) => ({ blockId: block.id, score: score(annotation.quote, block.text) }))
      .filter((item) => item.blockId && item.score > 0)
      .sort((a, b) => b.score - a.score || a.blockId.localeCompare(b.blockId))
    const best = ranked[0]
    const runnerUp = ranked[1]
    const margin = best ? best.score - (runnerUp?.score ?? 0) : 0
    if (!best || best.score < MIN_SCORE || (runnerUp && margin < MIN_MARGIN)) {
      unresolved += 1
      continue
    }
    matches.push({ id: annotation.id, blockId: best.blockId, score: best.score, margin })
  }
  return { matches, unresolved }
}
