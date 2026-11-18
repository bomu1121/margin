// Structural invariants for an agent-authored note. Genre rules are not
// checked here: the agent chooses the sections from the material, so this is
// the minimum that must hold for the note to be safe to publish.

export const FIGURE_KINDS = ['flow', 'layers', 'bar', 'quote', 'compare']

interface AuditState {
  h1: number
  h2: number
  cjk: number
  figureIssues: string[]
  issues: string[]
  outline: string
}

function auditState(draft: string): AuditState {
  const newline = String.fromCharCode(10)
  const carriage = String.fromCharCode(13)
  const lines = draft.split(newline).map((line) => (line.endsWith(carriage) ? line.slice(0, -1) : line))
  const cjk = (draft.match(/\p{Script=Han}/gu) ?? []).length
  const h1 = lines.filter((line) => line.startsWith('# ')).length
  const h2Lines = lines.filter((line) => line.startsWith('## '))
  let inFence = false
  let figure = false
  let kind = ''
  const figureIssues: string[] = []
  for (const line of lines) {
    const trimmed = line.trimEnd()
    if (inFence) {
      if (trimmed.trim() === '```') {
        inFence = false
        if (figure) {
          if (!FIGURE_KINDS.includes(kind)) {
            figureIssues.push(`图 kind「${kind || '缺失'}」`)
          }
          figure = false
          kind = ''
        }
      } else if (figure && trimmed.trim().startsWith('kind:')) {
        kind = trimmed.trim().slice(5).trim()
      }
      continue
    }
    if (trimmed.trimStart().startsWith('```')) {
      inFence = true
      figure = trimmed.trimStart().startsWith('```figure')
      continue
    }
  }
  if (inFence) figureIssues.push('代码块没闭合')
  const issues: string[] = []
  if (h1 !== 1) issues.push(`H1 ${h1} 个`)
  if (h2Lines.length < 4) issues.push(`二级小节只有 ${h2Lines.length} 个`)
  if (h2Lines.length > 20) issues.push(`二级小节 ${h2Lines.length} 个，超过 20`)
  if (cjk < 1000 || cjk > 6000) issues.push(`正文 ${cjk} 字`)
  issues.push(...figureIssues)
  const outline = h2Lines
    .map((line, index) => `${index + 1} ${line.slice(3).trim().slice(0, 18)}`)
    .join(' / ')
  return { h1, h2: h2Lines.length, cjk, figureIssues, issues, outline }
}

/** Short state line appended to every mutation, so the author sees whether
 * the note is structurally done without spending a tool call. */
export function auditSummary(draft: string): string {
  const state = auditState(draft)
  const problems = state.issues.length ? state.issues.join('；') : '无'
  return `结构：H1 ${state.h1} · H2 ${state.h2} · ${state.cjk} 字 · 图问题 ${state.figureIssues.length}；问题：${problems}`
}

export function auditDraft(draft: string): string {
  const state = auditState(draft)
  const outline = state.outline ? `；小节：${state.outline}` : ''
  return `${auditSummary(draft)}${outline}`
}
