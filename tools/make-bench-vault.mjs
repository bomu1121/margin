// Generates a synthetic vault with a realistic attribute distribution.
// usage: node tools/make-bench-vault.mjs <out-dir> [count]
import fs from 'node:fs'
import path from 'node:path'

const out = process.argv[2]
const count = Number(process.argv[3] || 80)
if (!out || !count) {
  console.error('usage: node tools/make-bench-vault.mjs <out-dir> [count]')
  process.exit(2)
}

function mulberry32(seed) {
  return () => {
    seed |= 0
    seed = (seed + 0x6d2b79f5) | 0
    let t = Math.imul(seed ^ (seed >>> 15), 1 | seed)
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296
  }
}

const random = mulberry32(20261104)
const pick = (list) => list[Math.floor(random() * list.length)]
const domains = ['财经', '历史', 'AI', '社会']
const types = ['视频笔记', '学习笔记', '文章']
const sources = ['B站', '得到', '公众号']
const ups = Array.from({ length: 12 }, (_, i) => `UP主${String.fromCharCode(65 + i)}`)
const people = Array.from({ length: 28 }, (_, i) => `人物${i + 1}`)
const periods = ['当代', '明清', '民国', '宋代', '唐代', '魏晋', '先秦', '近现代']
const topics = Array.from({ length: 42 }, (_, i) => `主题${String(i + 1).padStart(2, '0')}`)
const body =
  '## 速览\n\n' +
  '- 养老金账户余额不是社会养老能力，真正养老的是商品和服务供给。\n' +
  '- 中国的社保是累退的，体制内外差距大，财政补贴分配不均。\n\n' +
  '## 正文\n\n### 一\n\n养老金与财政的关系，不能只看账户余额。\n'

fs.rmSync(out, { recursive: true, force: true })
fs.mkdirSync(out, { recursive: true })
for (let i = 1; i <= count; i += 1) {
  const t1 = pick(topics)
  let t2 = pick(topics)
  if (t2 === t1) t2 = topics[(topics.indexOf(t1) + 1) % topics.length]
  const p1 = pick(people)
  let p2 = pick(people)
  if (p2 === p1) p2 = people[(people.indexOf(p1) + 1) % people.length]
  const title = `${t1}与${t2}：第 ${i} 篇观察`
  const date = `2026-${String((i % 12) + 1).padStart(2, '0')}-${String((i % 27) + 1).padStart(2, '0')}`
  const content = `---
标题: ${title}
作者: ${pick(ups)}
创建日期: ${date}
tags:
- 来源/${pick(sources)}
- 领域/${pick(domains)}
- 类型/${pick(types)}
- 主题/${t1}
- 主题/${t2}
人物:
- ${p1}
- ${p2}
事件:
- 事件${i % 9}
时期:
- ${pick(periods)}
---

# ${title}

${body}`
  fs.writeFileSync(path.join(out, `bench-${String(i).padStart(3, '0')}.md`), content, 'utf8')
}
console.log(JSON.stringify({ out, count }))
