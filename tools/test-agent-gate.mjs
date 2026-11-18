import assert from 'node:assert/strict'

import { auditDraft } from '../src/lib/agent/audit.ts'
import { agentGate, layoutClean, noteClean, noteGate, noteMinimalGate, reviewPassed } from '../src/lib/agent/gate.ts'

const clean =
  '节 8 · 图 3；溢出 0 · 越界 0 · 小字 0；SVG 越界 0 · 空框 0 · 标签重叠 0 · 文字溢出 0；切片 3 张'
const dirty =
  '节 8 · 图 3；溢出 0 · 越界 0 · 小字 0；SVG 越界 1 · 空框 0 · 标签重叠 0 · 文字溢出 0；问题：图2「图 2」文字越出画布：再测量；切片 3 张'

const turn = (index, tool, observation) => ({
  index,
  phase: 'observe',
  tool,
  observation,
  startedAt: 0,
  endedAt: 0,
})

assert.equal(layoutClean(clean), true)
assert.equal(layoutClean(dirty), false)
assert.equal(reviewPassed('通过'), true)
assert.equal(reviewPassed('无'), true)
assert.equal(reviewPassed('图1 文字压线'), false)

assert.equal(agentGate([turn(0, 'inspect', clean), turn(1, 'review', '通过')]).ok, true)
assert.equal(agentGate([turn(0, 'inspect', dirty), turn(1, 'review', '通过')]).ok, false)
assert.equal(
  agentGate([turn(0, 'inspect', clean), turn(1, 'review', '通过'), turn(2, 'patch_figure', '已改图 1')]).ok,
  false,
)
assert.equal(
  agentGate([turn(0, 'patch_figure', '已改'), turn(1, 'inspect', clean), turn(2, 'review', '通过')]).ok,
  true,
)
assert.equal(
  agentGate([
    turn(0, 'patch_figure', '已改'),
    turn(1, 'inspect', clean),
    turn(2, 'review', '通过'),
    turn(3, 'patch_section', '已重写'),
  ]).ok,
  false,
)
assert.equal(agentGate([turn(0, 'inspect', clean)]).ok, false)

// Note-author gate: the audit must come after the last write and report no issues.
const section = (index) => `## ${index}. 小节\n\n${'这一段写机制、限定条件和反例。'.repeat(16)}`
const note = `# 值得抄的是进展判定\n\n${Array.from({ length: 10 }, (_, index) => section(index + 1)).join('\n\n')}`
const cleanAudit = auditDraft(note)
// The agent chooses the structure, so a table and a free section name are allowed.
const freeAudit = auditDraft(`${note}\n\n## 速查\n\n| 项 | 值 |\n| --- | --- |\n| A | 1 |`)
const badFigureAudit = auditDraft(`${note}\n\n\`\`\`figure\nkind: timeline\n\`\`\``)
assert.match(cleanAudit, /问题：无/)
assert.equal(noteClean(cleanAudit), true)
assert.equal(noteClean(freeAudit), true)
assert.equal(noteClean(badFigureAudit), false)
assert.match(badFigureAudit, /timeline/)
assert.equal(noteGate([turn(0, 'write_note', '已写入'), turn(1, 'audit_note', cleanAudit)], note).ok, true)
assert.equal(noteGate([turn(0, 'write_note', '已写入')], note).ok, false)
assert.equal(
  noteGate(
    [turn(0, 'write_note', '已写入'), turn(1, 'audit_note', cleanAudit), turn(2, 'patch_section', '已改')],
    note,
  ).ok,
  false,
)
assert.equal(
  noteGate([turn(0, 'write_note', '已写入'), turn(1, 'audit_note', '问题：H1 0 个')], note).ok,
  false,
)
assert.equal(
  noteGate([turn(0, 'write_note', '已写入'), turn(1, 'audit_note', auditDraft('# 短'))], '# 短').ok,
  false,
)
assert.equal(noteGate([turn(0, 'write_note', '已写入'), turn(1, 'audit_note', cleanAudit)], '').ok, false)

// Minimal one-sentence gate: no audit requirement, only a titled draft.
assert.equal(noteMinimalGate(note).ok, true)
assert.equal(noteMinimalGate('').ok, false)
assert.equal(noteMinimalGate('# 短').ok, false)
assert.equal(noteMinimalGate(`# 标题

${'字'.repeat(300)}`).ok, true)
assert.equal(noteMinimalGate(`没有标题

${'字'.repeat(300)}`).ok, false)

// A ZCode-thick note (14 sections) must pass; the old 12-section cap was too tight.
const nl = String.fromCharCode(10)
const thick = ['# 厚笔记', '', ...Array.from({ length: 14 }, (_, index) => section(index + 1))].join(nl + nl)
assert.equal(noteClean(auditDraft(thick)), true)

console.log('agent gate: 25 assertions passed')
