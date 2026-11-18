import type { BookRenderResult } from '../api'
import { applyAction, figureBlocks } from '../bookAgent'
import type { BookMeta } from '../booklet'
import type { AgentOptions, TodoItem, Usage } from './types'
import { reviewDraft } from './subagent'
import { auditDraft, auditSummary } from './audit'

export interface ToolContext {
  noteId: string
  meta: BookMeta
  draft: string
  input?: string
  metrics: BookRenderResult['metrics'] | null
  slices: string[]
  allowMutations: boolean
  setDraft: (draft: string) => void
  setTodos: (todos: TodoItem[]) => void
  addUsage: (usage: Usage) => void
}

export interface ToolResult {
  observation: string
  done?: boolean
}

export interface ToolDef {
  name: string
  description: string
  args: string
  mutating: boolean
  requiresShell?: boolean
  run: (args: Record<string, unknown>, ctx: ToolContext, options: AgentOptions) => Promise<ToolResult>
}

const num = (value: unknown, fallback = 0): number => (typeof value === 'number' && Number.isFinite(value) ? value : fallback)
const str = (value: unknown, fallback = ''): string => (typeof value === 'string' ? value : fallback)

export function metricsSummary(metrics: BookRenderResult['metrics']): string {
  const extra = metrics as unknown as {
    emptyBoxes?: Array<{ fig: number; title: string }>
    labelOverlaps?: Array<{ fig: number; title: string; a: string; b: string }>
    textOverflow?: Array<{ fig: number; title: string; text: string }>
    clippedFigures?: Array<{ fig: number; title: string; texts: string[] }>
    pages?: number
    pageOverflow?: number[]
  }
  const details: string[] = []
  for (const item of extra.emptyBoxes ?? []) details.push(`图${item.fig + 1}「${item.title}」空框`)
  for (const item of extra.labelOverlaps ?? []) details.push(`图${item.fig + 1}「${item.title}」标签重叠：${item.a}/${item.b}`)
  for (const item of extra.textOverflow ?? []) details.push(`图${item.fig + 1}「${item.title}」文字溢出：${item.text}`)
  for (const item of extra.clippedFigures ?? []) details.push(`图${item.fig + 1}「${item.title}」文字越出画布：${item.texts.join('/')}`)
  for (const page of extra.pageOverflow ?? []) details.push(`页${page + 1}内容超出 A4 高度`)
  return [
    `节 ${metrics.sections} · 图 ${metrics.figures}${extra.pages ? ` · 页 ${extra.pages}` : ''}`,
    `溢出 ${metrics.overflow.length} · 越界 ${metrics.outOfBounds.length} · 小字 ${metrics.under12.length} · 页溢出 ${extra.pageOverflow?.length ?? 0}`,
    `SVG 越界 ${metrics.svgClipped} · 空框 ${extra.emptyBoxes?.length ?? 0} · 标签重叠 ${extra.labelOverlaps?.length ?? 0} · 文字溢出 ${extra.textOverflow?.length ?? 0}`,
    ...(details.length ? [`问题：${details.join('；')}`] : []),
  ].join('；')
}

export const TOOLS: ToolDef[] = [
  {
    name: 'read_input',
    description: '读取本次任务的素材全文。只读；只在需要回看原文时调用。',
    args: '{}',
    mutating: false,
    async run(_args, ctx) {
      const input = (ctx.input ?? '').trim()
      if (!input) return { observation: '本次任务没有素材' }
      return { observation: input.length > 48000 ? `${input.slice(0, 48000)}\n…（素材已截断）` : input }
    },
  },
  {
    name: 'write_note',
    description: '把整篇笔记写入草稿（第一次成稿用；之后用 patch_section 只改局部）。会修改草稿。',
    args: '{"markdown":"# 标题\n\n## 结论..."}',
    mutating: true,
    async run(args, ctx) {
      const markdown = str(args.markdown)
      if (!markdown.trim()) return { observation: 'markdown 为空' }
      ctx.setDraft(markdown)
      return { observation: `已写入 ${markdown.length} 字符；${auditSummary(ctx.draft)}` }
    },
  },
  {
    name: 'audit_note',
    description: '检查草稿：标题、小节数、字数、图 kind、代码块闭合。只读；问题为「无」才能 finish。',
    args: '{}',
    mutating: false,
    async run(_args, ctx) {
      return { observation: auditDraft(ctx.draft) }
    },
  },
  {
    name: 'read_draft',
    description: '读取当前草稿的章节标题与字数，用于确认状态。只读。',
    args: '{}',
    mutating: false,
    async run(_args, ctx) {
      const blocks = figureBlocks(ctx.draft)
      const headings = ctx.draft
        .split(/^## /m)
        .slice(1)
        .map((part) => part.split('\n')[0])
      return {
        observation: `字数 ${ctx.draft.length}；章节：${headings.join(' / ')}；图 ${blocks.length} 张（${blocks.map((block) => block.kind).join(',')}）`,
      }
    },
  },
  {
    name: 'inspect',
    description: '把草稿构建成 HTML 并渲染切片，返回版面指标。这是唯一的「看版面」入口。只读。',
    args: '{}',
    mutating: false,
    async run(_args, ctx) {
      const { buildBookHtml } = await import('../booklet')
      const { parseNote } = await import('../markdown')
      const api = await import('../api')
      const html = buildBookHtml(parseNote(ctx.draft), ctx.meta)
      const result = await api.agentInspect(html, ctx.meta.title || 'agent')
      ctx.metrics = result.metrics
      ctx.slices = result.slices
      return { observation: `${metricsSummary(result.metrics)}；切片 ${result.slices.length} 张` }
    },
  },
  {
    name: 'inspect_file',
    description: '把笔记库里的 HTML 文件交给应用内渲染器测量并生成切片；指标进上下文，供 look/review 使用。只读。',
    args: '{"path":"book.html","slices":6}',
    mutating: false,
    async run(args, ctx) {
      const path = str(args.path)
      if (!path) return { observation: '缺少 path' }
      const api = await import('../api')
      const file = await api.agentReadFile(path)
      if (file.bytes > 512 * 1024) return { observation: `文件 ${file.bytes} B，超过 512 KB` }
      const count = Math.min(12, Math.max(3, Math.round(num(args.slices, 6))))
      const result = await api.agentInspect(file.text ?? '', ctx.meta.title || 'booklet', count)
      ctx.metrics = result.metrics
      ctx.slices = result.slices
      return { observation: `${metricsSummary(result.metrics)}；切片 ${result.slices.length} 张` }
    },
  },
  {
    name: 'look',
    description: '把切片交给视觉模型，返回它认为的确凿视觉问题。只读。',
    args: '{}',
    mutating: false,
    async run(_args, ctx) {
      const api = await import('../api')
      if (!ctx.slices.length) return { observation: '还没有切片，先调用 inspect' }
      const answer = await api.agentVision(
        '你是排版审校。只回答「无」，或最多两个确凿问题（空框 / 文字压线 / 文字被裁 / 留白异常），每行一个，不解释。切片上下边缘是裁切，不算问题。',
        `草稿：${ctx.meta.title}。检查这些切片。`,
        ctx.slices,
      )
      ctx.addUsage({
        promptTokens: answer.promptTokens,
        completionTokens: answer.completionTokens,
        reasoningTokens: answer.reasoningTokens,
      })
      return { observation: answer.content.trim() || '（视觉模型没有返回内容）' }
    },
  },
  {
    name: 'patch_figure',
    description: '改写某一张图的内容行（flow/layers/bar/quote/compare）。会修改草稿。',
    args: '{"figure":1,"lines":["A -> B"],"note":"一句判断"}',
    mutating: true,
    async run(args, ctx) {
      const figure = num(args.figure)
      const lines = Array.isArray(args.lines) ? args.lines.map((line) => String(line)) : []
      if (!figure || !lines.length) return { observation: '参数不完整：需要 figure 和 lines' }
      const next = applyAction(ctx.draft, { action: 'patchFigure', figure, lines, note: str(args.note) })
      if (next === ctx.draft) return { observation: `没有改动：图 ${figure} 不存在，或它是手写 svg/compare` }
      ctx.setDraft(next)
      return { observation: `已改图 ${figure}（${lines.length} 行）` }
    },
  },
  {
    name: 'patch_section',
    description: '整节重写某一个小节（index 用 audit_note 列出的小节序号）。会修改草稿。',
    args: '{"index":2,"markdown":"新的正文"}',
    mutating: true,
    async run(args, ctx) {
      const index = num(args.index)
      const markdown = str(args.markdown)
      if (!index || !markdown.trim()) return { observation: '参数不完整：需要 index 和 markdown' }
      const next = applyAction(ctx.draft, { action: 'replaceSection', index, markdown })
      if (next === ctx.draft) return { observation: `没有改动：章节 ${index} 不存在` }
      ctx.setDraft(next)
      return { observation: `已重写章节 ${index}；${auditSummary(ctx.draft)}` }
    },
  },
  {
    name: 'review',
    description: '启动一个独立审校子 agent（只看到切片和指标，看不到你的推理），返回它的裁决。只读。',
    args: '{}',
    mutating: false,
    async run(_args, ctx) {
      if (!ctx.slices.length) return { observation: '还没有切片，先调用 inspect' }
      const verdict = await reviewDraft({
        title: ctx.meta.title,
        metrics: ctx.metrics,
        slices: ctx.slices,
      })
      ctx.addUsage(verdict.usage)
      return { observation: verdict.text || '（审校没有返回内容）' }
    },
  },
  {
    name: 'read_file',
    description: '读取笔记库里的文本文件（md/json/mjs/js/txt/html，≤512KB）。可用 offset/limit 分块读。只读。',
    args: '{"path":"相对路径","offset":0,"limit":3000}',
    mutating: false,
    async run(args) {
      const api = await import('../api')
      const result = await api.agentReadFile(str(args.path))
      const full = result.text ?? ''
      const offset = Math.max(0, Math.round(num(args.offset)))
      const limit = Math.min(6000, Math.max(500, Math.round(num(args.limit, 3000))))
      const text = full.slice(offset, offset + limit)
      const end = offset + text.length
      const suffix = end < full.length ? `\n…（${end}/${full.length}，继续用 offset=${end} 读）` : ''
      return { observation: `${result.path} · ${result.bytes} B\n${text}${suffix}` }
    },
  },
  {
    name: 'write_file',
    description: '在笔记库里新建或覆盖文本文件（md/json/mjs/js/txt/csv/html/svg），会先备份。',
    args: '{"path":"qa/check.mjs","content":"..."}',
    mutating: true,
    async run(args) {
      const api = await import('../api')
      const result = await api.agentWriteFile(str(args.path), str(args.content))
      return { observation: `已写 ${result.path}（${result.bytes} B）` }
    },
  },
  {
    name: 'edit_file',
    description: '按唯一片段替换文件内容（old 必须只出现一次），会先备份。',
    args: '{"path":"note.md","old":"旧片段","newText":"新片段"}',
    mutating: true,
    async run(args) {
      const api = await import('../api')
      const result = await api.agentEditFile(str(args.path), str(args.old), str(args.newText))
      return { observation: `已改 ${result.path}（${result.bytes} B）` }
    },
  },
  {
    name: 'list_dir',
    description: '列出笔记库某个目录的条目。只读。',
    args: '{"path":"待整理"}',
    mutating: false,
    async run(args) {
      const api = await import('../api')
      const result = await api.agentListDir(str(args.path))
      const items = result.items
        .slice(0, 60)
        .map((item) => `${item.dir ? '[d]' : '   '} ${item.name}`)
        .join('\n')
      return { observation: `${result.path}\n${items}` }
    },
  },
  {
    name: 'bash',
    description:
      '在笔记库内运行白名单命令（node 脚本 / ls / cat / rg / grep / wc / head / tail / git status|diff|log|show）。不能串联、重定向或管道。',
    args: '{"cwd":"","command":"node qa/check.mjs"}',
    mutating: true,
    requiresShell: true,
    async run(args, _ctx, options) {
      if (!options.allowShell) return { observation: '权限拒绝：本次运行未开启 shell' }
      const api = await import('../api')
      const result = await api.agentBash(str(args.cwd), str(args.command))
      const out = result.stdout.slice(0, 1500)
      const err = result.stderr.slice(0, 600)
      return { observation: `exit ${result.code}\nstdout: ${out || '（空）'}${err ? `\nstderr: ${err}` : ''}` }
    },
  },
  {
    name: 'todo_write',
    description: '维护本次任务的待办清单（覆盖式写入），用于规划和自查。不改文件。',
    args: '{"items":[{"text":"检查版面","status":"doing"}]}',
    mutating: false,
    async run(args, ctx) {
      const raw = Array.isArray(args.items) ? args.items : []
      const items: TodoItem[] = raw
        .map((item) => {
          const value = item as { text?: unknown; status?: unknown }
          const status: TodoItem['status'] =
            value.status === 'doing' || value.status === 'done' ? value.status : 'pending'
          return { text: str(value.text), status }
        })
        .filter((item) => item.text)
      if (!items.length) return { observation: '参数不完整：需要 items 数组' }
      ctx.setTodos(items)
      return { observation: items.map((item) => `[${item.status}] ${item.text}`).join(' · ') }
    },
  },
  {
    name: 'skill',
    description: '加载一份指令包（`.yebian/skills/<name>.md`），返回正文。只读。',
    args: '{"name":"booklet"}',
    mutating: false,
    async run(args) {
      const api = await import('../api')
      const name = str(args.name).replace(/[^A-Za-z0-9_-]/g, '')
      if (!name) return { observation: '缺少 name' }
      try {
        const result = await api.agentReadFile(`.yebian/skills/${name}.md`)
        return { observation: (result.text ?? '').slice(0, 4000) || '（skill 是空的）' }
      } catch {
        return { observation: `没有这份 skill：${name}（应放在 .yebian/skills/${name}.md）` }
      }
    },
  },
  {
    name: 'agent',
    description: '派一个只读子 agent 完成一个子任务：独立上下文、最多 3 回合、不能改文件。',
    args: '{"goal":"统计每节字数并排序"}',
    mutating: false,
    async run(args, ctx) {
      const goal = str(args.goal)
      if (!goal) return { observation: '缺少 goal' }
      const { runAgent } = await import('./runtime')
      const sub = await runAgent(
        {
          noteId: ctx.noteId,
          goal,
          meta: ctx.meta,
          allowMutations: false,
          allowShell: false,
          maxRounds: 3,
          contextBudget: 2500,
          tools: ['read_draft', 'inspect', 'look', 'read_file', 'list_dir', 'todo_write', 'finish'],
        },
        ctx.draft,
      )
      const tail = sub.turns
        .slice(-4)
        .map((turn) => `${turn.tool ?? turn.phase}: ${(turn.observation ?? turn.note ?? '').replace(/\s+/g, ' ').slice(0, 110)}`)
        .join(' | ')
      return { observation: `子 agent ${sub.status}（${sub.turns.length} 回合）：${tail}` }
    },
  },
  {
    name: 'finish',
    description: '结束。只有在结构检查通过时使用。',
    args: '{"reason":"为什么结束"}',
    mutating: false,
    async run(args) {
      return { observation: `结束：${str(args.reason, '完成')}`, done: true }
    },
  },
]

export const TOOL_MAP = new Map(TOOLS.map((tool) => [tool.name, tool]))

export function toolsPrompt(allowed?: string[]): string {
  const list = allowed?.length ? TOOLS.filter((tool) => allowed.includes(tool.name)) : TOOLS
  return list
    .map(
      (tool) =>
        `- ${tool.name}: ${tool.description} 参数：${tool.args}${tool.mutating ? '（会改文件/草稿，需授权）' : ''}${tool.requiresShell ? '（需要 shell 授权）' : ''}`,
    )
    .join('\n')
}
