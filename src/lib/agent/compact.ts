import { estimateTokens } from './context'
import { emptyUsage, type Usage } from './types'

export interface CompactResult {
  text: string
  usage: Usage
  compacted: boolean
}

/**
 * Replace a long transcript with a short summary when it exceeds the budget.
 * This is what lets the loop run past the model window instead of truncating
 * silently.
 */
export async function compactTranscript(transcript: string, budget: number): Promise<CompactResult> {
  if (estimateTokens(transcript) <= budget) {
    return { text: transcript, usage: emptyUsage(), compacted: false }
  }
  const api = await import('../api')
  const answer = await api.agentChat(
    '你是上下文压缩器。把记录压成不超过 300 字的摘要，只保留：已做的修改、未解决的问题、关键指标。不要保留工具调用的完整参数。',
    transcript,
  )
  return {
    text: answer.content.trim(),
    usage: {
      promptTokens: answer.promptTokens,
      completionTokens: answer.completionTokens,
      reasoningTokens: answer.reasoningTokens,
    },
    compacted: true,
  }
}
