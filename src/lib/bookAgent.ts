import type { BookRenderResult } from './api'
import { buildBookHtml, type BookMeta } from './booklet'
import { parseNote } from './markdown'

export interface AgentIO {
  ask(system: string, user: string): Promise<string>
  measure(html: string): Promise<BookRenderResult>
}

export interface AgentRound {
  round: number
  issues: string[]
  action: string
  note: string
  changed: boolean
  metrics: BookRenderResult['metrics']
}

type AgentAction =
  | { action: 'patchFigure'; figure: number; lines: string[]; note?: string }
  | { action: 'replaceSection'; index: number; markdown: string }
  | { action: 'finish'; reason?: string }

const SYSTEM = `你是小册子的视觉修复助手。输入是一份渲染检查结果、它的 figure 规格和章节列表。你只输出一个 JSON 对象，不要解释，不要 Markdown 围栏。

可选动作：
1. 改图：{"action":"patchFigure","figure":3,"lines":["A -> B"],"note":"一句判断"}
2. 改章节：{"action":"replaceSection","index":2,"markdown":"新的正文（Markdown，不要标题）"}
3. 结束：{"action":"finish","reason":"为什么"}

规则：
- 只修被报告的问题，不要重写无关内容。
- lines 必须符合 kind：flow 用 \`A -> B\`（可链式）；layers 用 \`层名：项1 | 项2\`；bar 用 \`标签 | 数值 | 说明\`；quote 用一句话。
- 空框通常是少了一个节点或标签；标签重叠通常是名字太长，缩短到 12 字以内。
- 不要新增图，不要改图号。检查结果里的「图 N」与下面列表的编号一一对应；只改被点名的图，不要改别的图。
- 如果问题还没修完，不要输出 finish。`

function issueList(metrics: BookRenderResult['metrics']): string[] {
  const issues: string[] = []
  const extra = metrics as unknown as {
    emptyBoxes?: Array<{ fig: number; title: string; x: number; y: number }>
    labelOverlaps?: Array<{ fig: number; title: string; a: string; b: string }>
    textOverflow?: Array<{ fig: number; title: string; text: string }>
  }
  for (const item of metrics.overflow ?? []) issues.push(`页面横向溢出：${item}`)
  for (const item of metrics.outOfBounds ?? []) issues.push(`元素越出正文列：${item}`)
  for (const item of metrics.under12 ?? []) issues.push(`字号小于 12px：${item}`)
  if ((metrics.svgClipped ?? 0) > 0) issues.push(`SVG 里有 ${metrics.svgClipped} 个文字超出画布`)
  for (const item of extra.emptyBoxes ?? []) issues.push(`空框：图 ${item.fig + 1}「${item.title}」@(${item.x},${item.y})`)
  for (const item of extra.labelOverlaps ?? []) issues.push(`标签重叠：图 ${item.fig + 1}「${item.a}」与「${item.b}」`)
  for (const item of extra.textOverflow ?? []) issues.push(`文字超出框：图 ${item.fig + 1}「${item.text}」`)
  return issues
}

export function figureBlocks(draft: string): Array<{ index: number; raw: string; start: number; end: number; kind: string; title: string; lines: string[]; note: string }> {
  const blocks: Array<{ index: number; raw: string; start: number; end: number; kind: string; title: string; lines: string[]; note: string }> = []
  const regex = /```figure\n([\s\S]*?)```/g
  let match: RegExpExecArray | null
  while ((match = regex.exec(draft))) {
    const inner = match[1]
    const kind = /^kind:\s*(\w+)/m.exec(inner)?.[1] ?? 'flow'
    const title = /^title:\s*(.+)$/m.exec(inner)?.[1] ?? ''
    const note = /^note:\s*(.+)$/m.exec(inner)?.[1] ?? ''
    const lines = inner
      .split('\n')
      .map((line) => line.trim())
      .filter((line) => line && !/^(kind|title|note):/.test(line))
    blocks.push({ index: blocks.length, raw: match[0], start: match.index, end: match.index + match[0].length, kind, title, lines, note })
  }
  return blocks
}

function sectionsOf(draft: string): string[] {
  return draft
    .split(/^## /m)
    .slice(1)
    .map((part) => part.split('\n')[0].trim())
}

export function applyAction(draft: string, action: AgentAction): string {
  if (action.action === 'patchFigure') {
    const blocks = figureBlocks(draft)
    const block = blocks[action.figure - 1]
    if (!block) return draft
    // Hand-authored SVG figures are not spec lines; rewriting them via
    // patchFigure would destroy the markup. Leave them to a human.
    if (block.kind === 'svg' || block.kind === 'compare') return draft
    const note = action.note ? `note: ${action.note}\n` : ''
    const next = `\`\`\`figure\nkind: ${block.kind}\ntitle: ${block.title}\n${action.lines.join('\n')}\n${note}\`\`\``
    return draft.slice(0, block.start) + next + draft.slice(block.end)
  }
  if (action.action === 'replaceSection') {
    const parts = draft.split(/^## /m)
    const target = Number(action.index)
    if (!Number.isInteger(target) || target < 1 || target >= parts.length) return draft
    const title = parts[target].split('\n')[0]
    const body = action.markdown.trim().replace(/^##\s+.*\n?/, '')
    parts[target] = `${title}\n\n${body}\n\n`
    return parts.join('## ')
  }
  return draft
}

function parseAction(text: string): AgentAction | null {
  const start = text.indexOf('{')
  const end = text.lastIndexOf('}')
  if (start < 0 || end <= start) return null
  try {
    const value = JSON.parse(text.slice(start, end + 1)) as AgentAction
    if (!value || typeof value.action !== 'string') return null
    return value
  } catch {
    return null
  }
}

function userPrompt(draft: string, issues: string[]): string {
  const figures = figureBlocks(draft)
    .map((block) => `${block.index + 1}) kind=${block.kind} title=${block.title}\n   ${block.lines.join('\n   ')}`)
    .join('\n')
  const sections = sectionsOf(draft)
    .map((title, index) => `${index + 1}. ${title}`)
    .join('\n')
  return `检查结果：\n${issues.map((item) => `- ${item}`).join('\n')}\n\n当前图（编号从 1 开始）：\n${figures || '（没有图）'}\n\n章节：\n${sections}\n\n只输出一个 JSON 动作。`
}

export async function runBookAgent(
  initialDraft: string,
  meta: BookMeta,
  io: AgentIO,
  onRound: (round: AgentRound) => void,
  maxRounds = 4,
): Promise<{ draft: string; rounds: AgentRound[] }> {
  let draft = initialDraft
  const rounds: AgentRound[] = []
  for (let round = 1; round <= maxRounds; round += 1) {
    const parsed = parseNote(draft)
    const html = buildBookHtml(parsed, meta)
    const result = await io.measure(html)
    const issues = issueList(result.metrics)
    if (!issues.length) {
      const done: AgentRound = { round, issues: [], action: 'finish', note: '测量通过，没有可修的问题', changed: false, metrics: result.metrics }
      rounds.push(done)
      onRound(done)
      return { draft, rounds }
    }
    const answer = await io.ask(SYSTEM, userPrompt(draft, issues))
    const action = parseAction(answer)
    if (action && action.action === 'finish' && issues.length > 0 && round < maxRounds) {
      const record: AgentRound = {
        round,
        issues,
        action: 'finish-rejected',
        note: '还有问题没修完，继续',
        changed: false,
        metrics: result.metrics,
      }
      rounds.push(record)
      onRound(record)
      continue
    }
    if (!action || action.action === 'finish') {
      const done: AgentRound = {
        round,
        issues,
        action: action?.action ?? 'unparsed',
        note: action?.action === 'finish' ? (action.reason ?? '模型结束') : '模型没有给出可执行动作',
        changed: false,
        metrics: result.metrics,
      }
      rounds.push(done)
      onRound(done)
      return { draft, rounds }
    }
    const next = applyAction(draft, action)
    const changed = next !== draft
    const record: AgentRound = {
      round,
      issues,
      action: action.action,
      note:
        action.action === 'patchFigure'
          ? `图 ${action.figure} 改成 ${action.lines.length} 行`
          : `章节 ${action.index} 重写`,
      changed,
      metrics: result.metrics,
    }
    rounds.push(record)
    onRound(record)
    if (!changed) return { draft, rounds }
    draft = next
  }
  return { draft, rounds }
}
