import assert from 'node:assert/strict'

import { agentGate, layoutClean, reviewPassed } from '../src/lib/agent/gate.ts'

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

console.log('agent gate: 11 assertions passed')
