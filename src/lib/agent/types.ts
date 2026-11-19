// Agent runtime types. The split mirrors the subsystems an agent harness needs:
// turn phases, context sections, tools with permissions, compaction, subagent.

export type Phase = 'idle' | 'model' | 'tool' | 'observe' | 'done' | 'failed'

export interface Usage {
  promptTokens: number
  completionTokens: number
  reasoningTokens: number
}

export interface AgentTurn {
  index: number
  phase: Phase
  tool?: string
  args?: Record<string, unknown>
  observation?: string
  note?: string
  /** Raw model output for this round (before action parsing). */
  content?: string
  /** Provider reasoning text, when the API exposes it. */
  reasoning?: string
  /** Tokens spent by the model call that produced this turn. */
  usage?: Usage
  /** Model latency for that call, separate from the tool duration. */
  modelMs?: number
  /** Tool execution time for an observe turn (Rust engine emits this). */
  toolMs?: number
  startedAt: number
  endedAt?: number
}

export interface TodoItem {
  text: string
  status: 'pending' | 'doing' | 'done'
}

export interface AgentRun {
  id: string
  noteId: string
  goal: string
  status: 'running' | 'done' | 'failed'
  phases: Phase[]
  turns: AgentTurn[]
  compacted: number
  todos: TodoItem[]
  startedAt: number
  endedAt?: number
  usage: Usage
  draft: string
}

export interface AgentOptions {
  noteId: string
  goal: string
  meta: { title: string; author?: string; source?: string; date?: string; paged?: boolean }
  allowMutations: boolean
  allowShell?: boolean
  /** Source material for an authoring run; exposed only through the read_input tool. */
  input?: string
  /** Restrict the tool set for this run (subagents use a read-only subset). */
  tools?: string[]
  /** Extra task instructions for a specialised run (e.g. HTML authoring). */
  instructions?: string
  maxRounds?: number
  contextBudget?: number
  onTurn?: (turn: AgentTurn) => void
  onPhase?: (phase: Phase) => void
}

export const emptyUsage = (): Usage => ({ promptTokens: 0, completionTokens: 0, reasoningTokens: 0 })
