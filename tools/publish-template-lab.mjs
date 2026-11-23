// Publishes the template-lab A/B outputs into the vault as visible notes.
// usage: node tools/publish-template-lab.mjs <vault>

import fs from 'node:fs'
import path from 'node:path'

const vault = process.argv[2]
if (!vault || !fs.existsSync(vault)) {
  console.error('usage: node tools/publish-template-lab.mjs <vault>')
  process.exit(2)
}

const lab = path.resolve('docs/experiments/template-lab')
const folder = path.join(vault, '模板对照')
fs.mkdirSync(folder, { recursive: true })

const videos = [
  ['liubei', '刘备和刘邦互换，谁能统一天下？', '王山水读通鉴'],
  ['gongdou', '宫斗大戏再一次开场 美国刺破美国 美国收割美国', '发烧好了呢'],
  ['ide', '大厂都在开发自己的IDE，我也半年没打开Intelliji', '极海Channel'],
  ['byd', '荷兰禁比亚迪Ti7，路虎这招影响有多大呢？', '小丑朱迪'],
]
const arms = [
  ['A', '对照A · 成文版'],
  ['B', '对照B · 结构版'],
]

const rows = []
for (const [key, title, up] of videos) {
  for (const [arm, label] of arms) {
    const source = path.join(lab, key, `${arm}.md`)
    if (!fs.existsSync(source)) throw new Error(`missing ${source}`)
    const body = fs.readFileSync(source, 'utf8').trim()
    const noteTitle = `[${label}] ${title}`
    const id = `lab-${key}-${arm.toLowerCase()}`
    const front = `---\nid: "${id}"\n标题: "${noteTitle}"\n类型: "模板对照"\n来源: "实验 / ${up}"\n创建日期: "2026-11-23"\n---\n\n`
    const file = path.join(folder, `${label.replace(/\s+/g, '')}-${key}.md`)
    fs.writeFileSync(file, `${front}${body}\n`, 'utf8')
    rows.push({ key, arm, file })
  }
}

const note = `---
id: "lab-readme"
标题: "模板对照实验说明"
类型: "模板对照"
来源: "实验"
创建日期: "2026-11-23"
---

# 模板对照实验说明

这里放的是同一批视频、同一模型、同一份 refined transcript 的两版笔记：

- **对照A · 成文版**：改动前的默认样式，连贯文章、判断-led 小节。
- **对照B · 结构版**：新候选模板，固定「一句话结论 / 速览 / 判断句小节 / 关键数字与专名 / 不确定与未展开」，并区分作者判断与素材事实。

4 个视频：王山水读通鉴（刘备）、发烧好了呢（宫斗）、极海Channel（IDE）、小丑朱迪（比亚迪）。每个视频左右两篇对照看即可。

已知差异：结构版四篇结构完整度 4/4（成文版 0/4），盲评 provenance 更高；但总字数 +25.7%、成本 +14.8%、耗时 +13.3%，刘备/宫斗篇有重复，宫斗篇仍有“民主党逼美联储”这类素材没有的因果，比亚迪篇漏“宋 38 万辆”。

完整实验报告：\`docs/experiments/template-lab/REPORT.md\`。
`
fs.writeFileSync(path.join(folder, '模板对照实验说明.md'), note, 'utf8')

console.log(JSON.stringify({ folder, notes: rows.length + 1 }, null, 2))
for (const row of rows) console.log(`${row.arm} ${row.file}`)
