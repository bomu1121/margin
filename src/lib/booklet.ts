import type { ParsedNote } from './markdown'

export interface BookMeta {
  title: string
  author?: string
  source?: string
  date?: string
  lede?: string
  paged?: boolean
}

const PRINT_CSS = `
:root {
  --paper: #f7f2e8;
  --surface: #ffffff;
  --rail: #f1eadb;
  --ink: #1f1c17;
  --ink-2: #4b463d;
  --ink-3: #7d7466;
  --line: #d9cebc;
  --line-2: #c3b7a3;
  --blue: #36596a;
  --gold: #8a6a3c;
  --red: #a93b2b;
  --green: #3d6b4f;
  --mono: "Cascadia Code", Consolas, monospace;
  --serif: "Noto Serif SC", "Source Han Serif SC", "Songti SC", SimSun, serif;
  --sans: "Noto Sans SC", "Microsoft YaHei", system-ui, sans-serif;
}
* { box-sizing: border-box; }
html, body { margin: 0; padding: 0; background: #fff; color: var(--ink); }
body { font-family: var(--serif); font-size: 11pt; line-height: 1.9; }
.doc { max-width: 680px; margin: 0 auto; padding: 0 0 40px; }
.book-cover { min-height: 235mm; display: flex; flex-direction: column; justify-content: center; page-break-after: always; }
.book-kicker { font-family: var(--sans); font-size: 9.5pt; letter-spacing: .18em; color: var(--blue); margin-bottom: 12mm; }
.book-cover h1 { font-size: 30pt; line-height: 1.3; font-weight: 500; margin: 0 0 8mm; }
.book-lede { font-size: 12pt; line-height: 1.9; color: var(--ink-2); margin: 0 0 10mm; }
.book-meta { font-family: var(--sans); font-size: 9.5pt; color: var(--ink-3); }
.book-toc { page-break-after: always; }
.book-toc h2 { font-family: var(--sans); font-size: 10pt; letter-spacing: .14em; color: var(--ink-3); margin: 0 0 6mm; }
.book-toc ol { list-style: none; padding: 0; margin: 0; }
.book-toc li { display: flex; gap: 8px; align-items: baseline; font-size: 11.5pt; padding: 2.4mm 0; border-bottom: 1px solid var(--line); }
.book-toc .toc-n { font-family: var(--mono); font-size: 9.5pt; color: var(--ink-3); }
.sec { margin: 0 0 10mm; }
body.is-paged .sec { page-break-before: always; }
body.is-paged .sec:first-of-type { page-break-before: auto; }
.sec-no { font-family: var(--mono); font-size: 9.5pt; letter-spacing: .08em; color: var(--ink-3); display: block; margin-bottom: 2mm; }
.sec h2 { font-size: 19pt; line-height: 1.5; font-weight: 500; margin: 0 0 6mm; }
.sec h3 { font-size: 12pt; margin: 5mm 0 2mm; }
.unit { margin: 0 0 3.4mm; }
.prose { font-size: 10.6pt; line-height: 1.95; }
.prose p { margin: 0 0 3.2mm; }
.prose ul, .prose ol { margin: 0 0 3.2mm; padding-left: 6mm; }
.prose li { margin: 0 0 1.6mm; }
.prose strong { font-weight: 600; }
.prose table { width: 100%; border-collapse: collapse; font-size: 9pt; margin: 0 0 4mm; }
.prose th, .prose td { border-bottom: 1px solid var(--line); text-align: left; vertical-align: top; padding: 4px 6px; }
.prose blockquote { margin: 0 0 3.2mm; padding-left: 4mm; border-left: 2px solid var(--line-2); color: var(--ink-2); }
.prose code { font-family: var(--mono); font-size: .9em; background: var(--rail); padding: 1px 3px; border-radius: 2px; }
.prose pre { background: var(--rail); padding: 3mm; font-size: 8.5pt; overflow: auto; }
.fig { margin: 6mm 0 7mm; page-break-inside: avoid; }
.fig-title { font-family: var(--sans); font-size: 9.5pt; color: var(--ink-3); margin-bottom: 2mm; }
.fig-frame { border-top: 1px solid var(--line); border-bottom: 1px solid var(--line); padding: 3mm 0; }
.fig svg { display: block; width: 100%; height: auto; }
.fig svg text { font-family: var(--sans); font-size: 12.5px; fill: var(--ink); }
.fig svg .label { font-weight: 500; }
.fig svg .band-label { font-weight: 500; fill: var(--ink-2); }
.fig svg .chip-label { fill: var(--ink-2); }
.fig svg .muted { fill: var(--ink-3); }
.fig svg .value { font-family: var(--mono); fill: var(--ink-2); }
.fig svg .quote { font-size: 15px; fill: var(--ink); }
.fig svg .box { fill: var(--surface); stroke: var(--line-2); stroke-width: 1; }
.fig svg .band { fill: var(--paper); stroke: var(--line); stroke-width: 1; }
.fig svg .chip { fill: var(--surface); stroke: var(--line-2); stroke-width: 1; }
.fig svg .edge { fill: none; stroke: var(--ink-3); stroke-width: 1.2; }
.fig svg .arrow-head { fill: var(--ink-3); }
.fig svg .bar { fill: var(--blue); }
.fig svg .bar-bg { fill: var(--line); }
.fig-cap { font-family: var(--sans); font-size: 9.5pt; line-height: 1.6; color: var(--ink-3); margin: 2mm 0 0; }
.sec.is-onepage { display: grid; grid-template-columns: 1fr 1fr; gap: 0 8mm; }
.sec.is-onepage > .sec-no, .sec.is-onepage > h2 { grid-column: 1 / -1; }
.sec.is-onepage .li-mark { display: none; }
.sec.is-onepage .unit { border-top: 1px solid var(--line); padding: 2mm 0; margin: 0; }
.sec.is-cheat { counter-reset: cheat; }
.sec.is-cheat .unit { counter-increment: cheat; border-top: 1px solid var(--line); padding: 2.4mm 0; }
.sec.is-cheat .li-mark { display: none; }
.sec.is-cheat .unit .prose::before { content: counter(cheat, decimal-leading-zero); font-family: var(--mono); font-size: 9.5pt; color: var(--blue); margin-right: 3mm; }
.gloss-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 0 8mm; }
.gloss-item { border-top: 1px solid var(--line); padding: 1.8mm 0; font-size: 9pt; line-height: 1.6; }
.gloss-term { font-weight: 600; }
.gloss-desc { color: var(--ink-2); }
@page { size: A4; margin: 16mm 15mm; }
`

function esc(value: string): string {
  return value
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
}

function sectionClass(heading: string): string {
  if (heading === '一页看懂') return ' is-onepage'
  if (heading === '三句话' || heading === '自检问题') return ' is-cheat'
  return ''
}

export function buildBookHtml(parsed: ParsedNote, meta: BookMeta): string {
  const sections = parsed.sections.filter((section) => section.heading || section.blocks.length)
  const toc = sections
    .map((section, index) => {
      const number = String(index + 1).padStart(2, '0')
      return `<li><span class="toc-n">${number}</span><span>${esc(section.heading || '（无标题）')}</span></li>`
    })
    .join('')
  const body = sections
    .map((section, index) => {
      const number = String(index + 1).padStart(2, '0')
      const heading = section.heading ? `<h2>${esc(section.heading)}</h2>` : ''
      const units = section.blocks
        .map((block) => `<div class="unit"><div class="prose">${block.html}</div></div>`)
        .join('')
      return `<section class="sec${sectionClass(section.heading)}" data-sec="${index}"><span class="sec-no">§ ${number}</span>${heading}${units}</section>`
    })
    .join('')
  const glossary = parsed.glossary.length
    ? `<section class="sec"><span class="sec-no">词汇</span><h2>术语表</h2><div class="gloss-grid">${parsed.glossary
        .map((item) => `<div class="gloss-item"><span class="gloss-term">${esc(item.term)}</span>　<span class="gloss-desc">${esc(item.desc)}</span></div>`)
        .join('')}</div></section>`
    : ''
  const metaLine = [meta.author, meta.source, meta.date].filter(Boolean).join(' · ')

  return `<!doctype html>
<html lang="zh-CN">
<head>
<meta charset="utf-8">
<title>${esc(meta.title)}</title>
<style>${PRINT_CSS}</style>
</head>
<body class="${meta.paged ? 'is-paged' : ''}">
<article class="doc">
  <header class="book-cover">
    <div class="book-kicker">技术小册子</div>
    <h1>${esc(meta.title)}</h1>
    ${meta.lede ? `<p class="book-lede">${esc(meta.lede)}</p>` : ''}
    ${metaLine ? `<div class="book-meta">${esc(metaLine)}</div>` : ''}
  </header>
  <nav class="book-toc"><h2>目录</h2><ol>${toc}</ol></nav>
  ${body}
  ${glossary}
</article>
</body>
</html>`
}
