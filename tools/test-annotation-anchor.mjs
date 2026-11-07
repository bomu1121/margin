import { planAnchorRelocations } from '../src/lib/annotationAnchor.ts'

function block(id, text) {
  return { id, text, html: '', kind: 'p' }
}
const cases = []
function check(name, actual, expected) {
  const ok = JSON.stringify(actual) === JSON.stringify(expected)
  cases.push(ok)
  console.log(`${ok ? 'PASS' : 'FAIL'}  ${name}  ${JSON.stringify(actual)}`)
}

const blocks = [
  block('b-exact', '作者认为，补贴退坡会让地方财政压力迅速上升。'),
  block('b-close', '补贴持续到 2030 年，地方财政才有缓冲。'),
  block('b-near-a', '补贴会持续到2031年。'),
  block('b-near-b', '补贴会持续到2032年。'),
  block('b-other', '这是一段完全无关的内容，讨论的是芯片制程。'),
]

check(
  'exact with punctuation changes',
  planAnchorRelocations([{ id: 'a1', quote: '作者认为 补贴退坡会让地方财政压力迅速上升' }], blocks).matches.map((m) => m.blockId),
  ['b-exact'],
)
check(
  'ambiguous near-duplicate blocks stay unresolved',
  planAnchorRelocations([{ id: 'a2', quote: '补贴会持续到2030年。' }], blocks).matches,
  [],
)
check(
  'short quote is not fuzzy-matched',
  planAnchorRelocations([{ id: 'a-short', quote: '是的' }], [block('b-short', '是的，不过还有后续。')]).matches,
  [],
)
check(
  'unrelated text stays unresolved',
  planAnchorRelocations([{ id: 'a3', quote: '这是一段完全无关的内容，讨论的是芯片制程。' }], [block('b-new', '全新的正文')]).matches,
  [],
)
check(
  'one unchanged block is recovered',
  planAnchorRelocations([{ id: 'a4', quote: '补贴持续到 2030 年，地方财政才有缓冲。' }], blocks).matches.map((m) => m.blockId),
  ['b-close'],
)
const failed = cases.filter((ok) => !ok).length
console.log(`\n${cases.length - failed}/${cases.length} checks passed`)
if (failed) process.exitCode = 1
