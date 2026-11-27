// Pure checks for the quick-production path: clipboard extraction and the
// request recipe. Run with:
//   node --experimental-strip-types --no-warnings tools/test-quick-request.mjs
import assert from 'node:assert/strict'
import { buildQuickBiliRequest, extractBiliLink, looksLikeBiliLink } from '../src/lib/production.ts'

const links = [
  ['看 BV1Yc411E7Ab 这个', 'https://www.bilibili.com/video/BV1Yc411E7Ab'],
  ['【标题】 https://www.bilibili.com/video/BV1Yc411E7Ab?p=1&share_source=copy_web 好看。', 'https://www.bilibili.com/video/BV1Yc411E7Ab?p=1&share_source=copy_web'],
  ['复制打开 b23.tv/AbCd12，看视频', 'https://b23.tv/AbCd12'],
  ['av12345 这个老视频', 'https://www.bilibili.com/video/av12345'],
  ['https://example.com/video/BV1Yc411E7Ab', ''],
  ['今天没有链接', ''],
]
for (const [input, expected] of links) {
  assert.equal(extractBiliLink(input), expected, input)
}
assert.equal(looksLikeBiliLink('https://www.bilibili.com/video/BV1Yc411E7Ab'), true)
assert.equal(looksLikeBiliLink('今天没有链接'), false)

const preview = {
  bvid: 'BV1Yc411E7Ab',
  title: '一个视频标题',
  author: 'UP 主',
  ownerMid: 1,
  duration: 600,
  pubdate: 0,
  cover: '',
  pages: [
    { page: 1, cid: 2, title: 'P1', duration: 600 },
    { page: 2, cid: 3, title: 'P2', duration: 300 },
  ],
}
const request = buildQuickBiliRequest(
  preview,
  ' https://www.bilibili.com/video/BV1Yc411E7Ab ',
  { prompt: '成稿 Agent', noteTask: '系统提示正文', searchProvider: 'deepseek' },
)
assert.equal(request.templateId, 'ai-note')
assert.equal(request.title, preview.title)
assert.equal(request.source.kind, 'bili')
assert.equal(request.source.url, 'https://www.bilibili.com/video/BV1Yc411E7Ab')
assert.deepEqual(request.source.pages, [1, 2])
assert.equal(request.options.prompt, '成稿 Agent')
assert.equal(request.options.noteTask, '系统提示正文')
assert.equal(request.options.destination, 'folder')
assert.equal(request.options.folder, '待整理')
assert.equal(request.options.traceProvider, 'deepseek')
assert.deepEqual(request.options.tags, [])
assert.deepEqual(request.options.fields, [])
assert.equal(request.options.split, true)

console.log(JSON.stringify({
  linkCases: links.length,
  request: {
    templateId: request.templateId,
    destination: request.options.destination,
    folder: request.options.folder,
    pages: request.source.pages,
    noteTaskChars: request.options.noteTask.length,
  },
}, null, 2))
console.log('PASS quick-production request checks')
