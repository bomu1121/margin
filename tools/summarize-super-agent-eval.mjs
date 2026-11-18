// Aggregate the Super Agent note experiment.
// App runs: <TEMP>/super-agent-eval/{minimal,plan,audit,free}-N/summary.json
// ZCode runs: <TEMP>/super-agent-eval/zcode-N.md + child sessions in db.sqlite
import fs from 'node:fs'
import path from 'node:path'
import { DatabaseSync } from 'node:sqlite'

const root = path.join(process.env.TEMP || '.', 'super-agent-eval')
const variants = ['minimal', 'plan', 'audit', 'free']

const CHECKPOINTS = [
  { name: '会话制局限', keys: ['会话', 'react loop', '单任务', '多任务'], min: 3 },
  { name: '注意力 3–5', keys: ['三到五', '三到五个', '3 到 5', '3~5', '3–5', '3-5', '注意力'], min: 2 },
  { name: '三个问题', keys: ['卡住', '上下文', '休眠'], min: 3 },
  { name: '四个特征', keys: ['连续', '高效', '主动', '持久'], min: 3 },
  { name: '两层架构', keys: ['router', 'worker', '项目经理', '工程师'], min: 3 },
  { name: '事件+两级过滤', keys: ['trigger', '触发', '门卫', 'fast path', 'slow path', '快慢路径'], min: 3 },
  { name: '工单/订阅', keys: ['thread', '工单', '订阅', '抄送'], min: 2 },
  { name: 'inline/spill/召回', keys: ['inline', 'spill', '召回', '编号'], min: 2 },
  { name: '全双工/异步', keys: ['双工', '异步', '对讲机', '电话'], min: 2 },
  { name: '存算分离/BYOM', keys: ['存算', 'BYOM', 'Daemon', '容器'], min: 2 },
  { name: 'Watchdog/进展', keys: ['watchdog', '看门狗', '进展'], min: 2 },
  { name: '成本/开放问题', keys: ['两倍', '四倍', '缓存', '注入', '开放问题'], min: 2 },
]

function stats(text) {
  return {
    bytes: Buffer.byteLength(text),
    cjk: (text.match(/[\u4e00-\u9fff]/g) ?? []).length,
    h2: (text.match(/^## /gm) ?? []).length,
    tables: (text.match(/^\|/gm) ?? []).length,
    figures: (text.match(/^```figure/gm) ?? []).length,
  }
}

function coverage(text) {
  const lower = text.toLowerCase()
  return CHECKPOINTS.map((check) => {
    const hits = check.keys.filter((key) => lower.includes(key.toLowerCase())).length
    return hits >= check.min
  })
}

function mean(values) {
  return values.length ? values.reduce((sum, value) => sum + value, 0) / values.length : 0
}

function fmt(value, digits = 0) {
  return Number(value).toFixed(digits)
}

const rows = []
const notes = []

for (const variant of variants) {
  const dirs = fs
    .readdirSync(root)
    .filter((name) => name.startsWith(`${variant}-`) && fs.statSync(path.join(root, name)).isDirectory())
    .sort()
  for (const dir of dirs) {
    const summaryPath = path.join(root, dir, 'summary.json')
    if (!fs.existsSync(summaryPath)) continue
    const summary = JSON.parse(fs.readFileSync(summaryPath, 'utf8'))
    const notePath = path.join(root, dir, 'note.md')
    const note = fs.existsSync(notePath) ? fs.readFileSync(notePath, 'utf8') : ''
    rows.push({ variant, run: dir, summary, note: note ? stats(note) : null, coverage: note ? coverage(note) : null })
    if (note) notes.push({ variant, run: dir, file: notePath })
  }
}

// ZCode child sessions (newest three), in launch order.
const db = new DatabaseSync(path.join(process.env.USERPROFILE || '.', '.zcode/cli/db/db.sqlite'), { readOnly: true })
const parent = 'sess_73ef6a5c-c03f-4f01-bfa6-76ec8f1ed26c'
const children = db
  .prepare('SELECT id FROM session WHERE parent_id = ? ORDER BY time_created ASC')
  .all(parent)
  .slice(-3)
children.forEach((session, index) => {
  const notePath = path.join(root, `zcode-${index + 1}.md`)
  if (!fs.existsSync(notePath)) return
  const noteText = fs.readFileSync(notePath, 'utf8')
  const usage = db
    .prepare(
      'SELECT COUNT(*) n, SUM(model_request_count) requests, SUM(tool_call_count) tools, SUM(input_tokens) input, SUM(output_tokens) output, SUM(reasoning_tokens) reasoning, SUM(duration_ms) duration FROM turn_usage WHERE session_id = ?',
    )
    .get(session.id)
  const tools = db
    .prepare('SELECT tool_name, COUNT(*) n FROM tool_usage WHERE session_id = ? GROUP BY tool_name')
    .all(session.id)
  rows.push({
    variant: 'zcode',
    run: `zcode-${index + 1}`,
    summary: {
      status: 'success',
      durationMs: usage.duration ?? 0,
      usage: {
        promptTokens: usage.input ?? 0,
        completionTokens: usage.output ?? 0,
        reasoningTokens: usage.reasoning ?? 0,
        estimatedCost: null,
      },
      run: { turns: Array.from({ length: usage.requests ?? 0 }, () => ({})) },
      models: usage.requests,
      toolHistogram: tools,
    },
    note: stats(noteText),
    coverage: coverage(noteText),
  })
  notes.push({ variant: 'zcode', run: `zcode-${index + 1}`, file: notePath })
})

function aggregate(variant) {
  const group = rows.filter((row) => row.variant === variant)
  const ok = group.filter((row) => row.summary.status === 'success' && row.note)
  return {
    runs: group.length,
    ok: ok.length,
    cjk: [Math.min(...ok.map((row) => row.note.cjk)), Math.max(...ok.map((row) => row.note.cjk))],
    cjkMean: mean(ok.map((row) => row.note.cjk)),
    h2: mean(ok.map((row) => row.note.h2)),
    tables: mean(ok.map((row) => row.note.tables)),
    figures: mean(ok.map((row) => row.note.figures)),
    turns: mean(ok.map((row) => row.summary.run?.turns?.length ?? 0)),
    input: mean(ok.map((row) => row.summary.usage?.promptTokens ?? 0)),
    output: mean(ok.map((row) => row.summary.usage?.completionTokens ?? 0)),
    cost: mean(ok.map((row) => row.summary.usage?.estimatedCost ?? 0)),
    duration: mean(ok.map((row) => row.summary.durationMs ?? 0)) / 1000,
    coverage: mean(ok.map((row) => row.coverage.filter(Boolean).length)),
  }
}

const lines = ['# Super Agent 笔记对照实验', '']
lines.push('| 组 | 成功/总数 | CJK 均值 (范围) | H2 | 表格行 | 图 | 回合 | 输入 tok | 输出 tok | 耗时 | 成本 | 12 要点 |')
lines.push('| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |')
for (const variant of [...variants, 'zcode']) {
  const agg = aggregate(variant)
  lines.push(
    `| ${variant} | ${agg.ok}/${agg.runs} | ${fmt(agg.cjkMean)} (${agg.cjk[0]}–${agg.cjk[1]}) | ${fmt(agg.h2, 1)} | ${fmt(agg.tables, 1)} | ${fmt(agg.figures, 1)} | ${fmt(agg.turns, 1)} | ${fmt(agg.input)} | ${fmt(agg.output)} | ${fmt(agg.duration, 1)}s | ${agg.cost ? `¥${fmt(agg.cost, 4)}` : '—'} | ${fmt(agg.coverage, 1)}/12 |`,
  )
}
lines.push('')
lines.push('## 逐次运行')
lines.push('| 组 | 运行 | 状态 | CJK | H2 | 表格 | 图 | 回合 | 输入/输出 | 成本 | 12 要点 |')
lines.push('| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |')
for (const row of rows) {
  const ok = row.summary.status === 'success' && row.note
  const usage = row.summary.usage ?? {}
  lines.push(
    `| ${row.variant} | ${row.run} | ${ok ? 'ok' : row.summary.status} | ${row.note?.cjk ?? '—'} | ${row.note?.h2 ?? '—'} | ${row.note?.tables ?? '—'} | ${row.note?.figures ?? '—'} | ${row.summary.run?.turns?.length ?? '—'} | ${usage.promptTokens ?? '—'}/${usage.completionTokens ?? '—'} | ${usage.estimatedCost ? `¥${fmt(usage.estimatedCost, 4)}` : '—'} | ${row.coverage ? `${row.coverage.filter(Boolean).length}/12` : '—'} |`,
  )
}
lines.push('')
lines.push('## 12 要点覆盖矩阵')
lines.push(`| 组 | ${CHECKPOINTS.map((item) => item.name).join(' | ')} |`)
lines.push(`| --- | ${CHECKPOINTS.map(() => '---').join(' | ')} |`)
for (const variant of [...variants, 'zcode']) {
  const group = rows.filter((row) => row.variant === variant && row.coverage)
  if (!group.length) continue
  const cells = CHECKPOINTS.map((_, index) => {
    const hit = group.filter((row) => row.coverage[index]).length
    return `${hit}/${group.length}`
  })
  lines.push(`| ${variant} | ${cells.join(' | ')} |`)
}
lines.push('')
lines.push('## 文件')
for (const note of notes) lines.push(`- ${note.variant}/${note.run}: ${note.file}`)

const report = lines.join('\n')
fs.writeFileSync(path.join(root, 'REPORT.md'), report)
console.log(report)
