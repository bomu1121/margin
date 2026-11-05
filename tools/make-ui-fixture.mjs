// Builds a small, nested vault for sidebar visual/interaction review.
// usage: node tools/make-ui-fixture.mjs <out-dir>
//
// The fixture is deliberately awkward: long Chinese titles, deep nesting,
// an empty folder, a folder with one note, and a mix of dates. It is not a
// benchmark corpus; it exists so before/after screenshots show the same rows.

import fs from 'node:fs'
import path from 'node:path'

const out = process.argv[2]
if (!out) {
  console.error('usage: node tools/make-ui-fixture.mjs <out-dir>')
  process.exit(2)
}

const notes = [
  ['01 阅读/为什么 AI 回答越长页面越卡.md', '为什么 AI 回答越长页面越卡', '2026-10-28', '内部看美国', 'AI'],
  ['01 阅读/智能体工具的沙箱边界.md', '智能体工具的沙箱边界', '2026-10-22', 'Anthropic', 'AI'],
  ['01 阅读/上下文预算：窗口里该丢什么.md', '上下文预算：窗口里该丢什么', '2026-10-18', 'Simon Willison', 'AI'],
  ['01 阅读/深一层/记忆系统不是向量库.md', '记忆系统不是向量库', '2026-10-12', 'Lilian Weng', 'AI'],
  ['01 阅读/深一层/评估不能只看 pass@k.md', '评估不能只看 pass@k', '2026-10-09', 'Hamel Husain', 'AI'],
  ['02 研究/中国社保的累退、体制内外差距与法币时代养老生产力论.md', '中国社保的累退、体制内外差距与法币时代养老生产力论', '2026-10-14', '内部看美国', '财经'],
  ['02 研究/南北朝各选十人最强阵容，哪边更强.md', '南北朝各选十人最强阵容，哪边更强', '2026-10-02', '王山水读通鉴', '历史'],
  ['02 研究/冰雪经济的供给侧到底缺什么.md', '冰雪经济的供给侧到底缺什么', '2026-09-27', '央视财经', '社会'],
  ['03 归档/一个很长很长的标题用来检查单行省略号和悬停提示是否真的可用.md', '一个很长很长的标题用来检查单行省略号和悬停提示是否真的可用', '2026-09-21', 'ScribeFlow', '未分类'],
  ['03 归档/旧笔记：浏览器里跑抠图模型.md', '旧笔记：浏览器里跑抠图模型', '2026-09-15', '自研', 'AI'],
  ['收件箱：还没想好放哪里.md', '收件箱：还没想好放哪里', '2026-11-03', '随手记', '未分类'],
  ['待整理.md', '待整理', '2026-11-01', '随手记', '未分类'],
]

fs.rmSync(out, { recursive: true, force: true })
fs.mkdirSync(path.join(out, '02 研究', '空文件夹'), { recursive: true })
fs.mkdirSync(path.join(out, '03 归档'), { recursive: true })

for (const [rel, title, date, up, domain] of notes) {
  const abs = path.join(out, ...rel.split('/'))
  fs.mkdirSync(path.dirname(abs), { recursive: true })
  const body = [
    '---',
    `标题: ${title}`,
    '类型: 视频笔记',
    `创建日期: ${date}`,
    `作者: ${up}`,
    '领域: ' + domain,
    'tags:',
    `- 领域/${domain}`,
    '---',
    '',
    `# ${title}`,
    '',
    '## 速览',
    '',
    '- 这是视觉夹具，不是真实笔记。',
    '',
    '## 正文',
    '',
    '### 第一节',
    '',
    '这一段文字用来撑出正文，方便检查侧栏选中态与阅读页的排版是否互相干扰。',
    '',
  ].join('\n')
  fs.writeFileSync(abs, body)
}

console.log(JSON.stringify({ out, notes: notes.length, emptyFolder: '02 研究/空文件夹' }))
