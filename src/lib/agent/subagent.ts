import type { BookRenderResult } from '../api'
import { emptyUsage, type Usage } from './types'

export interface ReviewInput {
  title: string
  metrics: BookRenderResult['metrics'] | null
  slices: string[]
}

export interface ReviewVerdict {
  text: string
  usage: Usage
}

/**
 * Independent reviewer. It runs in its own conversation with its own system
 * prompt and only receives the rendered slices plus metrics — never the main
 * agent's reasoning or tool log. This is the "second pair of eyes" role.
 */
export async function reviewDraft(input: ReviewInput): Promise<ReviewVerdict> {
  const api = await import('../api')
  const metricsText = input.metrics
    ? JSON.stringify({
        sections: input.metrics.sections,
        figures: input.metrics.figures,
        overflow: input.metrics.overflow.length,
        outOfBounds: input.metrics.outOfBounds.length,
        under12: input.metrics.under12.length,
        svgClipped: input.metrics.svgClipped,
      })
    : '（没有指标）'
  const answer = await api.agentVision(
    [
      '独立审校：只依据切片和指标，不要猜作者意图。',
      '只输出「通过」，或最多 2 行确凿问题（空框/压线/文字被裁/留白异常）。不要解释，不要推理过程。',
      '切片上下边缘的裁切不算问题。',
    ].join('\n'),
    `标题：${input.title}\n版面指标：${metricsText}\n请审校这些切片。`,
    input.slices,
  )
  return {
    text: answer.content.trim(),
    usage: {
      promptTokens: answer.promptTokens,
      completionTokens: answer.completionTokens,
      reasoningTokens: answer.reasoningTokens,
    },
  }
}

export function reviewUsage(): Usage {
  return emptyUsage()
}
