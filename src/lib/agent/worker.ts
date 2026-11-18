// Production-side worker for the opt-in agent step.
//
// The Rust pipeline cannot build the booklet HTML, so it parks a pending task
// in SQLite and waits. This module picks that task up in the webview, runs the
// same runtime as the manual button, and submits the patched draft back.
import { reactive } from 'vue'

import * as api from '../api'
import type { ProductionAgentTask, ProductionJob } from '../../types'
import { agentGate } from './gate'
import { runAgent as runAgentRuntime } from './runtime'
import type { AgentTurn, TodoItem } from './types'

const GOAL = '把这份小册子草稿修到版面干净、结构完整，并让独立审校通过。'
const TOOLS = [
  'read_draft',
  'inspect',
  'look',
  'patch_figure',
  'patch_section',
  'review',
  'todo_write',
  'finish',
]

const HTML_TOOLS = [
  'write_file',
  'edit_file',
  'inspect_file',
  'look',
  'review',
  'todo_write',
  'finish',
]

const HTML_NOTES = `HTML 小册子任务：
- 目标文件已存在，是一个自包含 HTML 骨架；保留 <main class="doc"> 与 .page 分页，可改内容与 CSS。
- 作者简报和设计 tokens 已附在任务说明末尾；不要调用 read_file/list_dir，不要读 book/brief/plan/material。
- 每回合只输出一行 JSON：{"tool":"...","args":{...}}；禁止 DSML/XML/Markdown 围栏。
- 封面/目录/一页看懂/三句话/自检/术语表已由骨架预填；你只写章节占位符。不要新增计划外的章节。
- 图用 <figure class="fig"><svg>…</svg></figure>；文字不能超出画布；不要引用外链、不要 <script>、不要 iframe。
- 每个 .page 是固定 A4 高度，内容不能溢出；优先拆页，不要缩小字号到 12px 以下。
- 直接 edit_file 替换对应的 <!-- CHAPTER-XX --> 占位符；不要用 write_file 覆盖整份文件。
- 每轮改完调用 inspect_file；指标有非 0（尤其页溢出/文字越界/空框）就继续改；最后 review 通过再 finish。`

export const productionAgent = reactive({
  jobId: '',
  title: '',
  status: 'idle' as 'idle' | 'running' | 'done' | 'failed',
  phase: '',
  turns: [] as AgentTurn[],
  todos: [] as TodoItem[],
  tokens: 0,
  error: '',
})

let busy = false
let rerun = false

/** Called from the production store whenever a job update arrives. */
export function maybeRunProductionAgent(job: ProductionJob): void {
  const step = job.steps.find((item) => item.key === 'agent' || item.key === 'author')
  if (!step || step.status !== 'running') return
  void drain()
}

/** On startup or after a manual refresh: one pending task at a time. */
export async function drainProductionAgent(): Promise<void> {
  await drain()
}

async function drain(): Promise<void> {
  if (busy) {
    rerun = true
    return
  }
  busy = true
  try {
    for (;;) {
      const task = await api.productionAgentPending()
      if (!task) break
      await runTask(task)
    }
  } catch (cause) {
    productionAgent.status = 'failed'
    productionAgent.error = api.errorText(cause)
  } finally {
    busy = false
    if (rerun) {
      rerun = false
      window.setTimeout(() => void drain(), 50)
    }
  }
}

async function runTask(task: ProductionAgentTask): Promise<void> {
  const mode = task.mode || 'layout'
  const htmlMode = mode === 'html-author'
  let payload: Record<string, unknown> = {}
  try {
    payload = JSON.parse(task.payload || '{}') as Record<string, unknown>
  } catch {
    payload = {}
  }
  const text = (value: unknown): string => (typeof value === 'string' ? value : '')
  const bookPath = text(payload.bookPath)
  const brief = text(payload.brief)
  const design = text(payload.design)
  const goal = htmlMode
    ? `把技术小册子写成可打印的 HTML：${bookPath}。作者简报和设计 tokens 已附在任务说明里；不要读外部文件。直接 edit_file 替换 <!-- CHAPTER-XX --> 占位符，写完用 inspect_file 检查，修到指标全 0，review 通过后 finish。`
    : GOAL
  const tools = htmlMode ? HTML_TOOLS : TOOLS
  const maxRounds = htmlMode ? 24 : 8
  const contextBudget = htmlMode ? 10000 : 4000
  const initialDraft = htmlMode ? '' : task.draft

  productionAgent.jobId = task.jobId
  productionAgent.title = task.title
  productionAgent.status = 'running'
  productionAgent.phase = 'model'
  productionAgent.turns = []
  productionAgent.todos = []
  productionAgent.tokens = 0
  productionAgent.error = ''

  void api
    .productionAgentLog(
      task.jobId,
      task.stepIndex,
      `智能体开始 · ${task.title || '小册子'}${htmlMode ? ' · HTML 作者' : ''}`,
    )
    .catch(() => {})

  try {
    const run = await runAgentRuntime(
      {
        noteId: task.noteId || task.jobId,
        goal,
        meta: { title: task.title || '小册子' },
        allowMutations: true,
        allowShell: false,
        tools,
        instructions: htmlMode ? `${HTML_NOTES}

## 作者简报
${brief}

## 设计 tokens
${design}` : undefined,
        maxRounds,
        contextBudget,
        onTurn: (turn) => {
          productionAgent.turns.push(turn)
          productionAgent.phase = turn.phase
          const label = turn.tool ? `${turn.tool}()` : turn.phase
          const detail = (turn.observation || turn.note || '').replace(/\s+/g, ' ').slice(0, 240)
          void api
            .productionAgentLog(
              task.jobId,
              task.stepIndex,
              `#${turn.index + 1} ${label}${detail ? ` · ${detail}` : ''}`,
            )
            .catch(() => {})
        },
        onPhase: (phase) => {
          productionAgent.phase = phase
        },
      },
      initialDraft,
    )
    productionAgent.turns = run.turns
    productionAgent.todos = run.todos
    productionAgent.tokens = run.usage.promptTokens + run.usage.completionTokens

    if (run.status === 'failed') {
      const failed = run.turns.filter((turn) => turn.phase === 'failed').slice(-1)[0]
      productionAgent.status = 'failed'
      productionAgent.error = failed?.note ?? '运行失败'
      void api
        .productionAgentLog(task.jobId, task.stepIndex, `智能体失败 · ${productionAgent.error}`)
        .catch(() => {})
      await api.productionAgentFail(task.id, productionAgent.error)
      return
    }

    // Deterministic gate: never trust the model's own "finish".
    const gate = agentGate(run.turns)
    if (!gate.ok) {
      productionAgent.status = 'failed'
      productionAgent.error = gate.reason
      void api
        .productionAgentLog(task.jobId, task.stepIndex, `智能体失败 · ${productionAgent.error}`)
        .catch(() => {})
      await api.productionAgentFail(task.id, productionAgent.error)
      return
    }

    if (htmlMode && !bookPath) {
      productionAgent.status = 'failed'
      productionAgent.error = '没有拿到 bookPath'
      await api.productionAgentFail(task.id, productionAgent.error)
      return
    }

    await api.productionAgentSubmit(task.id, htmlMode ? bookPath : run.draft, JSON.stringify(run))
    productionAgent.status = 'done'
    void api
      .productionAgentLog(
        task.jobId,
        task.stepIndex,
        `智能体完成 · ${run.turns.length} 回合 · ${productionAgent.tokens} tokens`,
      )
      .catch(() => {})
    void api.agentRunSave(run.id, JSON.stringify(run)).catch(() => {})
  } catch (cause) {
    productionAgent.status = 'failed'
    productionAgent.error = api.errorText(cause)
    void api
      .productionAgentLog(task.jobId, task.stepIndex, `智能体失败 · ${productionAgent.error}`)
      .catch(() => {})
    await api.productionAgentFail(task.id, productionAgent.error).catch(() => {})
  }
}
