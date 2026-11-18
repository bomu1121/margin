import { buildBookHtml } from '../booklet'
import { parseNote } from '../markdown'
import type { BookRenderResult } from '../api'
import { compactTranscript } from './compact'
import { buildContext, renderTranscript, type Section } from './context'
import { TOOL_MAP, metricsSummary, toolsPrompt, type ToolContext } from './tools'
import { TurnMachine } from './turn'
import { emptyUsage, type AgentOptions, type AgentRun, type AgentTurn, type Usage } from './types'

const CONSTRAINTS = `规则：
- 完成本次目标；目标没要求的事不要顺手做。
- 每一回合只输出一个 JSON 工具调用，不要输出解释或 Markdown 围栏。
- 信息不足时先用只读工具确认，不要凭想象改。
- 一次只改一处；改完再检查。
- 目标要求的检查没通过时不要 finish；结束的 reason 要写清依据。`

const BOOKLET_NOTES = `领域知识（仅当目标与小册子或版面相关时使用）：
- 先 inspect，再决定改什么；版面指标不为 0 时不要 finish。
- 图注必须是一句判断；不要新增没有依据的数字或原话。
- 手写 svg 图不能通过 patch_figure 修改。
- 最近一次 inspect 全部为 0 时调用 review；review 通过后 finish。`

function parseDsml(text: string): { tool: string; args: Record<string, unknown> } | null {
  const invoke = text.match(/<[^>]*invoke\s+name="([^"]+)"[^>]*>([\s\S]*?)<\/[^>]*invoke>/i)
  if (!invoke) return null
  const tool = invoke[1].trim()
  if (!tool) return null
  const args: Record<string, unknown> = {}
  const re = /<[^>]*parameter\s+name="([^"]+)"[^>]*>([\s\S]*?)<\/[^>]*parameter>/gi
  let match: RegExpExecArray | null
  while ((match = re.exec(invoke[2]))) {
    args[match[1].trim()] = match[2].trim()
  }
  return { tool, args }
}

function parseAction(text: string): { tool: string; args: Record<string, unknown> } | null {
  const start = text.indexOf('{')
  const end = text.lastIndexOf('}')
  if (start >= 0 && end > start) {
    try {
      const value = JSON.parse(text.slice(start, end + 1)) as { tool?: unknown; args?: unknown }
      if (typeof value.tool === 'string' && value.tool.trim()) {
        return { tool: value.tool.trim(), args: (value.args ?? {}) as Record<string, unknown> }
      }
    } catch {
      /* fall through to DSML */
    }
  }
  return parseDsml(text)
}

function draftState(draft: string): string {
  const headings = draft
    .split(/^## /m)
    .slice(1)
    .map((part) => part.split('\n')[0])
  const figures = draft.match(/^kind: (.+)$/gm) ?? []
  return `章节（${headings.length}）：${headings.join(' / ')}\n图（${figures.length}）：${figures.map((line) => line.replace('kind: ', '')).join(',')}`
}

function userMessage(context: string, transcript: string, draft: string): string {
  return `${context}\n\n## 记录\n${transcript || '（还没有动作）'}\n\n## 草稿状态\n${draftState(draft)}\n\n输出一个工具调用 JSON。`
}

export async function runAgent(options: AgentOptions, initialDraft: string): Promise<AgentRun> {
  const api = await import('../api')
  const run: AgentRun = {
    id: `agent_${Date.now().toString(36)}`,
    noteId: options.noteId,
    goal: options.goal,
    status: 'running',
    phases: ['idle'],
    turns: [],
    compacted: 0,
    todos: [],
    startedAt: Date.now(),
    usage: emptyUsage(),
    draft: initialDraft,
  }
  const machine = new TurnMachine()
  const budget = options.contextBudget ?? 6000
  const maxRounds = options.maxRounds ?? 6
  let draft = initialDraft
  let metrics: BookRenderResult['metrics'] | null = null
  let slices: string[] = []
  let summary: string | null = null
  let consecutiveErrors = 0
  let lastActionKey = ''
  let duplicateActions = 0

  const addUsage = (usage: Usage): void => {
    run.usage.promptTokens += usage.promptTokens
    run.usage.completionTokens += usage.completionTokens
    run.usage.reasoningTokens += usage.reasoningTokens
  }

  const ctx: ToolContext = {
    noteId: options.noteId,
    meta: options.meta,
    draft,
    metrics,
    slices,
    allowMutations: options.allowMutations,
    setDraft(next) {
      draft = next
      ctx.draft = next
      run.draft = next
    },
    setTodos(todos) {
      run.todos = todos
    },
    addUsage,
  }

  const phase = (next: Parameters<TurnMachine['to']>[0]): void => {
    machine.to(next)
    run.phases.push(next)
    options.onPhase?.(next)
  }

  const record = (
    turn: Omit<AgentTurn, 'index' | 'startedAt' | 'endedAt'>,
    startedAt = Date.now(),
  ): void => {
    const entry: AgentTurn = { ...turn, index: run.turns.length, startedAt, endedAt: Date.now() }
    run.turns.push(entry)
    options.onTurn?.(entry)
  }

  const identity = `你是页边笔记里的 agent。本次目标：${options.goal}
每一回合只输出一个 JSON 工具调用：{"tool":"<名字>","args":{...}}，不要解释；禁止 DSML、XML 标签或 Markdown 围栏。`
  const needsBooklet = /小册子|版面|排版|图|审校/.test(options.goal)
  try {
    phase('model')
    for (let round = 0; round < maxRounds; round += 1) {
      const previous = renderTranscript(run.turns)
      const compactStart = Date.now()
      const compacted = await compactTranscript(previous, Math.floor(budget / 2))
      if (compacted.compacted) {
        run.compacted += 1
        addUsage(compacted.usage)
        summary = compacted.text
        record(
          { phase: 'model', note: `压缩上下文（第 ${run.compacted} 次）`, observation: compacted.text.slice(0, 120) },
          compactStart,
        )
      }
      const transcript = summary ?? previous

      const sections: Section[] = [
        { name: '身份', priority: 10, content: identity },
        { name: '约束', priority: 9, content: CONSTRAINTS },
        { name: '工具', priority: 8, content: toolsPrompt(options.tools) },
        ...(options.instructions
          ? [{ name: '任务说明', priority: 6, content: options.instructions }]
          : needsBooklet
            ? [{ name: '领域知识', priority: 5, content: BOOKLET_NOTES }]
            : []),
        {
          name: '权限',
          priority: 7,
          content: options.allowMutations ? '本次运行允许修改草稿/文件（受工具白名单限制）。' : '本次运行只读。',
        },
        {
          name: '版面状态',
          priority: 6,
          content: metrics ? metricsSummary(metrics) : '还没有检查过版面。',
        },
      ]
      const context = buildContext(sections, budget)
      const modelStart = Date.now()
      const answer = await api.agentChat(identity, userMessage(context.text, transcript, draft))
      addUsage({
        promptTokens: answer.promptTokens,
        completionTokens: answer.completionTokens,
        reasoningTokens: answer.reasoningTokens,
      })

      const action = parseAction(answer.content)
      if (!action) {
        consecutiveErrors += 1
        record(
          { phase: 'model', note: '模型没有给出可执行动作', observation: answer.content.slice(0, 200) },
          modelStart,
        )
        if (consecutiveErrors >= 2) throw new Error('连续两次没有给出工具调用')
        continue
      }
      consecutiveErrors = 0
      const allowedTools = options.tools?.length ? new Set(options.tools) : null
      const tool = allowedTools && !allowedTools.has(action.tool) ? null : TOOL_MAP.get(action.tool)
      phase('tool')
      const actionStart = Date.now()
      let observation = ''
      let done = false
      const actionKey = `${action.tool}:${JSON.stringify(action.args ?? {})}`
      if (actionKey === lastActionKey) {
        duplicateActions += 1
        observation = '重复动作：同一个调用已经执行过，结果不会变。请换工具或修改参数。'
        if (duplicateActions >= 2) throw new Error('连续重复动作，停止运行')
      } else if (!tool) {
        duplicateActions = 0
        observation = `未知工具：${action.tool}`
      } else if (tool.requiresShell && !options.allowShell) {
        duplicateActions = 0
        observation = '权限拒绝：本次运行未开启 shell'
      } else if (tool.mutating && !options.allowMutations) {
        duplicateActions = 0
        observation = '权限拒绝：本次运行未开启修改'
      } else {
        duplicateActions = 0
        try {
          const result = await tool.run(action.args, ctx, options)
          observation = result.observation
          done = Boolean(result.done)
        } catch (error) {
          observation = `工具失败：${error instanceof Error ? error.message : String(error)}`
        }
      }
      lastActionKey = actionKey
      phase('observe')
      metrics = ctx.metrics
      slices = ctx.slices
      record({ phase: 'observe', tool: action.tool, args: action.args, observation }, actionStart)
      if (done) {
        phase('done')
        run.status = 'done'
        break
      }
      phase('model')
    }
    if (run.status === 'running') {
      // Running out of rounds is not success: the goal was never confirmed.
      // Mark it failed so callers cannot publish an unverified draft.
      run.status = 'failed'
      phase('failed')
      record({ phase: 'failed', note: `达到最大回合 ${maxRounds}，未调用 finish` })
    }
  } catch (error) {
    if (machine.phase !== 'failed') phase('failed')
    run.status = 'failed'
    record({ phase: 'failed', note: error instanceof Error ? error.message : String(error) })
  } finally {
    run.endedAt = Date.now()
    run.draft = draft
  }
  return run
}

// Kept for callers that need the built HTML without running the agent.
export function buildDraftHtml(draft: string, meta: AgentOptions['meta']): string {
  return buildBookHtml(parseNote(draft), meta)
}
