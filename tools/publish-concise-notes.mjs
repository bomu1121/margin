// Publishes the conclusion+sections (arm C) notes into a separate vault folder.
// usage: node tools/publish-concise-notes.mjs <vault>

import fs from 'node:fs'
import path from 'node:path'

const vault = process.argv[2]
if (!vault || !fs.existsSync(vault)) {
  console.error('usage: node tools/publish-concise-notes.mjs <vault>')
  process.exit(2)
}

const lab = path.resolve('docs/experiments/template-lab')
const folder = path.join(vault, '新思路')
fs.mkdirSync(folder, { recursive: true })

const videos = [
  ['liubei', '刘备和刘邦互换，谁能统一天下？', '王山水读通鉴'],
  ['gongdou', '宫斗大戏再一次开场 美国刺破美国 美国收割美国', '发烧好了呢'],
  ['ide', '大厂都在开发自己的IDE，我也半年没打开Intelliji', '极海Channel'],
  ['byd', '荷兰禁比亚迪Ti7，路虎这招影响有多大呢？', '小丑朱迪'],
]

const rows = []
for (const [key, title, up] of videos) {
  const source = path.join(lab, key, 'C.md')
  if (!fs.existsSync(source)) throw new Error(`missing ${source}`)
  const body = fs.readFileSync(source, 'utf8').trim()
  const noteTitle = `[新思路 · 结论+小节] ${title}`
  const id = `lab-c-${key}`
  const front = `---\nid: "${id}"\n标题: "${noteTitle}"\n类型: "模板对照"\n来源: "实验 / ${up}"\n创建日期: "2026-11-23"\n---\n\n`
  const file = path.join(folder, `新思路-${key}.md`)
  fs.writeFileSync(file, `${front}${body}\n`, 'utf8')
  rows.push({ key, file })
}

const note = `---
id: "lab-c-readme"
标题: "新思路：只保留结论和论证小节"
类型: "模板对照"
来源: "实验"
创建日期: "2026-11-23"
---

# 新思路：只保留结论和论证小节

这 4 篇不再有「速览」「关键数字与专名」「不确定与未展开」三个独立块。结构只有：

1. 标题
2. 结论段：2–3 句（回答 + 最强理由 + 最大限定）
3. 4–5 个判断句小节

每个小节只回答一个问题：标题是判断句；第一段给判断和后果；第二段给素材依据（数字/原话就地出现）；需要时在节末加一句限定/反例；没有就不写。

同一批 4 个视频，和「模板对照」里的 A（成文版）/ B（结构版）可以直接对照。

本次 4 篇合计：13,085 字，模型成本 ¥0.1798，耗时 179.7s。相比之下，A 是 14,606 字 / ¥0.1372 / 165.1s，B 是 18,364 字 / ¥0.1575 / 187.1s。新思路更短，但比亚迪篇回合偏多，单篇成本偏高；这不是模板统一成本。

已知问题：比亚迪篇仍有少量推断需要核对；宫斗篇的作者归因、刘备篇的动机表述也仍需人工看。这里放的是候选稿，不是已验收的默认模板。
`
fs.writeFileSync(path.join(folder, '新思路说明.md'), note, 'utf8')

console.log(JSON.stringify({ folder, notes: rows.length + 1 }, null, 2))
for (const row of rows) console.log(row.file)
