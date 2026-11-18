import type { AgentTurn } from './types'

export interface Section {
  name: string
  /** Higher priority survives trimming first. */
  priority: number
  content: string
}

export function estimateTokens(text: string): number {
  let cjk = 0
  let other = 0
  for (const ch of text) {
    if (/[\u3000-\u9fff\uff00-\uffef]/.test(ch)) cjk += 1
    else other += 1
  }
  return Math.ceil(cjk + other / 3.5)
}

/**
 * Assemble sections under a token budget. Low-priority sections are dropped
 * first and the dropped names are returned, so the runtime can log what it
 * could not afford to send.
 */
export function buildContext(sections: Section[], budget: number): { text: string; dropped: string[]; tokens: number } {
  const ordered = [...sections].sort((a, b) => b.priority - a.priority)
  const kept: Section[] = []
  let used = 0
  const dropped: string[] = []
  for (const section of ordered) {
    const cost = estimateTokens(section.content)
    if (used + cost > budget && kept.length > 0) {
      dropped.push(section.name)
      continue
    }
    kept.push(section)
    used += cost
  }
  const text = [...kept].reverse().map((section) => `## ${section.name}\n${section.content}`).join('\n\n')
  return { text, dropped, tokens: used }
}

/** Compact transcript for the model: tool calls and short observations only. */
export function renderTranscript(turns: AgentTurn[], limit = 12): string {
  return turns
    .slice(-limit)
    .map((turn) => {
      if (turn.tool) {
        const args = turn.args ? JSON.stringify(turn.args).slice(0, 240) : ''
        const observation = (turn.observation ?? '').replace(/\s+/g, ' ').slice(0, 320)
        return `- ${turn.tool}(${args}) → ${observation}`
      }
      return `- ${turn.note ?? turn.phase}`
    })
    .join('\n')
}
