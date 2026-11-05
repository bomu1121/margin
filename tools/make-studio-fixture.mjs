// Builds a small vault for the ScribeFlow studio/provenance acceptance test.
// usage: node tools/make-studio-fixture.mjs <out-dir>
import fs from 'node:fs'
import path from 'node:path'

const out = process.argv[2]
if (!out) {
  console.error('usage: node tools/make-studio-fixture.mjs <out-dir>')
  process.exit(2)
}
const resolved = path.resolve(out)
if (!path.basename(resolved).includes('fixture')) {
  console.error('refusing to delete a non-fixture directory; name it *fixture*')
  process.exit(2)
}
fs.rmSync(resolved, { recursive: true, force: true })
fs.mkdirSync(path.join(out, 'ScribeFlow'), { recursive: true })
fs.mkdirSync(path.join(out, '研究'), { recursive: true })

const body = `---
id: sf-demo-001
标题: 中国社保的累退、体制内外差距与“法币时代”养老生产力论
类型: 视频笔记
来源: B站
作者: 内部看美国
原始链接: https://www.bilibili.com/video/BV1DTWw6UE1y
发布日期: 2026-10-14
创建日期: 2026-11-05
人物:
  - 人社部
  - 美国国税局
事件:
  - 人社部提出“用数据找人”
时期:
  - 法币时代
tags:
  - 类型/视频笔记
  - 领域/财经
---

# 中国社保的累退、体制内外差距与“法币时代”养老生产力论

人社部提出“用数据找人”动员参保，但作者认为真问题不在动员，而在制度本身。中国社保是累退的、体制内外差距大，且在他看来是改革开放后盲目学习美国的产物。

## 速览

- 人社部“用数据找人”只能动员，因为职工养老强制、灵活就业人员自愿。
- 养老金会不会崩，关键不是账户余额和年轻人数量，而是法币时代的生产力与商品服务供给。
- 中国低通胀、失业率 5.2%，有闲置劳动力和财政空间，问题在分配。

## 01 · 人社部能“用数据找人”，但法律留给灵活就业人员的是自愿选择

《中国社会保险法》第十条规定，职工应当参加基本养老保险，由单位和个人共同缴费；属于法律规定范围的灵活就业人员可以选择参加。所以“用数据找人”只能动员，不能强制。
`
fs.writeFileSync(path.join(out, 'ScribeFlow', '中国社保的累退.md'), body)
fs.writeFileSync(
  path.join(out, 'ScribeFlow', '中国社保的累退.md.artifact.json'),
  JSON.stringify(
    {
      schema: 'yebian.artifact.v1',
      artifactId: 'artifact-001',
      noteId: 'sf-demo-001',
      revision: 2,
      status: 'published',
      note: 'ScribeFlow/中国社保的累退.md',
      title: '中国社保的累退、体制内外差距与“法币时代”养老生产力论',
      source: {
        kind: 'bilibili',
        label: 'B站',
        url: 'https://www.bilibili.com/video/BV1DTWw6UE1y',
        author: '内部看美国',
        publishedAt: '2026-10-14',
      },
      run: {
        projectId: 'project-social',
        projectName: '社保专题',
        runId: 'run_128',
        nodeId: 'node-ob',
        prompt: '观点提炼 · 成文版',
        model: 'DeepSeek V3.2',
      },
      generatedAt: '2026-11-05T08:42:00+08:00',
      usage: { promptTokens: 8200, completionTokens: 1400, estimatedCost: 0.42, currency: 'CNY' },
      assets: [
        { name: 'transcript.md', path: 'ScribeFlow/assets/transcript.md', kind: 'transcript', bytes: 42000 },
        { name: 'cover.jpg', path: 'ScribeFlow/assets/cover.jpg', kind: 'cover', bytes: 18000 },
      ],
    },
    null,
    2,
  ),
)

fs.writeFileSync(
  path.join(out, 'ScribeFlow', 'AI 翻译为何翻不好口语.md'),
  `---
id: sf-demo-002
标题: AI 翻译为何翻不好口语
类型: 视频笔记
来源: B站
作者: 某翻译 UP
原始链接: https://www.bilibili.com/video/BVdemo2
创建日期: 2026-11-04
tags:
  - 类型/视频笔记
  - 领域/AI
---

# AI 翻译为何翻不好口语

## 速览

- 口语翻译的难点是上下文和语气，不是词表。
`,
)

fs.writeFileSync(
  path.join(out, 'ScribeFlow', '等待入库的笔记.md.artifact.json'),
  JSON.stringify(
    {
      schema: 'yebian.artifact.v1',
      artifactId: 'artifact-pending',
      noteId: 'sf-demo-pending',
      revision: 1,
      status: 'pending',
      note: 'ScribeFlow/等待入库的笔记.md',
      title: '等待入库的笔记',
      source: { kind: 'upload', label: '本地音视频', author: '' },
    },
    null,
    2,
  ),
)

fs.writeFileSync(
  path.join(out, '研究', '普通笔记.md'),
  `---
标题: 普通笔记
tags:
  - 领域/研究
---

# 普通笔记

这里没有 ScribeFlow 元数据，制作台不应该把它算成制作结果。
`,
)

console.log(JSON.stringify({ out, artifacts: 3, notes: 2 }))
