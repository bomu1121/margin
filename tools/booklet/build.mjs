// Build a self-contained booklet HTML from a note: front matter + markdown +
// ```figure specs. CSS is inlined (fonts point at local files) so a headless
// browser can render it without a dev server.
//
// usage: node tools/booklet/build.mjs <note.md> <out.html>
import { readFile, writeFile } from 'node:fs/promises'
import { resolve, dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { marked } from 'marked'
import { parseFigure, renderFigure } from '../../src/lib/figures.mjs'

const here = dirname(fileURLToPath(import.meta.url))
const root = resolve(here, '../..')
const [notePath, outPath] = process.argv.slice(2)
if (!notePath || !outPath) {
  console.error('usage: node tools/booklet/build.mjs <note.md> <out.html>')
  process.exit(1)
}

const CSS_FILES = [
  'fonts.css',
  'app.css',
  'sidebar.css',
  'force.css',
  'readability.css',
  'facts.css',
  'history.css',
  'studio.css',
  'production.css',
  'panels.css',
  'book.css',
  'themes.css',
]

const fontBase = resolve(root, 'src/assets/fonts').replace(/\\/g, '/')
let css = ''
for (const name of CSS_FILES) {
  const file = resolve(root, 'src/styles', name)
  let text = await readFile(file, 'utf8')
  if (name === 'fonts.css') {
    text = text.replace(/url\("\.\.\/assets\/fonts\/([^"]+)"\)/g, `url("file:///${fontBase}/$1")`)
  }
  css += `\n/* ${name} */\n${text}`
}

const esc = (value) =>
  String(value)
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
const plain = (value) =>
  String(value)
    .replace(/!\[([^\]]*)\]\([^)]*\)/g, '$1')
    .replace(/\[([^\]]*)\]\([^)]*\)/g, '$1')
    .replace(/[*_`>#~]/g, '')
    .replace(/<[^>]+>/g, '')
    .trim()

const source = await readFile(notePath, 'utf8')
const frontMatch = source.match(/^---\n([\s\S]*?)\n---\n/)
const front = {}
if (frontMatch) {
  for (const line of frontMatch[1].split('\n')) {
    const m = line.match(/^([^\s:][^:]*):\s*(.*)$/)
    if (m) front[m[1].trim()] = m[2].trim().replace(/^["']|["']$/g, '')
  }
}
const body = frontMatch ? source.slice(frontMatch[0].length) : source

const tokens = marked.lexer(body)
const sections = []
let current = null
let pendingH3 = null

function ensureSection() {
  if (!current) {
    current = { heading: '', blocks: [] }
    sections.push(current)
  }
  return current
}

const ITEM_SECTIONS = new Set(['一页看懂', '三句话', '自检问题'])

for (const token of tokens) {
  const type = token.type
  if (type === 'space') continue
  if (type === 'heading') {
    const depth = Number(token.depth)
    const text = plain(token.text)
    if (depth === 1) continue
    if (depth === 2) {
      current = { heading: text, blocks: [] }
      sections.push(current)
      continue
    }
    if (depth >= 3) {
      ensureSection().blocks.push(`<h3>${esc(text)}</h3>`)
      continue
    }
  }
  if (type === 'code' && String(token.lang || '').trim().toLowerCase() === 'figure') {
    const spec = parseFigure(String(token.text || ''))
    const figure = renderFigure(spec, 724)
    if (figure.html) ensureSection().blocks.push(figure.html)
    continue
  }
  if (type === 'list' && ITEM_SECTIONS.has(current?.heading || '')) {
    for (const item of token.items || []) {
      const html = marked.parseInline(item.text || '')
      if (String(html).trim()) ensureSection().blocks.push('<span class="li-mark">·</span> ' + html)
    }
    continue
  }
  const html = marked.parse(token.raw || '')
  if (String(html).trim()) ensureSection().blocks.push(html)
}

const title = front['标题'] || sections.find((s) => s.heading)?.heading || '未命名'
const metaLine = [front['作者'], front['来源'], front['发布日期'], front['创建日期']]
  .filter(Boolean)
  .join(' · ')

const sectionClass = (heading) =>
  heading === '一页看懂'
    ? ' is-onepage'
    : heading === '三句话' || heading === '自检问题'
      ? ' is-cheat'
      : heading === '术语表'
        ? ' is-gloss'
        : ''

const sectionsHtml = sections
  .map((section, index) => {
    const number = String(index + 1).padStart(2, '0')
    const heading = section.heading ? `<h2>${esc(section.heading)}</h2>` : ''
    return `<section class="sec${sectionClass(section.heading)}" id="s${number}" data-sec="${index}">
<span class="sec-no">§ ${number}</span>${heading}
${section.blocks.map((html) => `<div class="unit"><div class="prose">${html}</div></div>`).join('\n')}
</section>`
  })
  .join('\n')

const toc = sections
  .map((section, index) => {
    const number = String(index + 1).padStart(2, '0')
    return `<li><span class="toc-n">${number}</span><span>${esc(section.heading || '（无标题）')}</span></li>`
  })
  .join('\n')

const lede = front['导语'] || ''

const html = `<!doctype html>
<html lang="zh-CN" data-theme="neutral" data-mode="light">
<head>
<meta charset="utf-8">
<title>${esc(title)}</title>
<style>${css}</style>
<style>
  html, body { height: auto; overflow: visible; background: var(--surface); }
  body { padding: 0; }
  .book-wrap { max-width: 1000px; margin: 0 auto; padding: 36px 48px 80px; }
  .doc.book { max-width: 820px; }
  .book-wrap .book-cover { padding-top: 8vh; }
</style>
</head>
<body>
<article class="doc book">
  <header class="book-cover">
    <div class="book-kicker">技术小册子</div>
    <h1>${esc(title)}</h1>
    ${lede ? `<p class="book-lede">${esc(lede)}</p>` : ''}
    ${metaLine ? `<div class="book-meta">${esc(metaLine)}</div>` : ''}
  </header>
  <nav class="book-toc">
    <h2>目录</h2>
    <ol>${toc}</ol>
  </nav>
  ${sectionsHtml}
</article>
</body>
</html>`

await writeFile(outPath, html, 'utf8')
const figureCount = (html.match(/class="fig"/g) || []).length
console.log(JSON.stringify({ out: outPath, bytes: Buffer.byteLength(html), sections: sections.length, figures: figureCount }))
