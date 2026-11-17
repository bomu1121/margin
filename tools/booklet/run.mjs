// One entry point for the booklet loop: build → measure (screen + print) →
// export PDF. Any gate failure exits non-zero so a caller (human or agent) can
// patch the note and rerun.
//
// usage: node tools/booklet/run.mjs <note.md> <outDir>
import { execFileSync } from 'node:child_process'
import { mkdirSync } from 'node:fs'
import { join, resolve } from 'node:path'

const [note, outDir] = process.argv.slice(2)
if (!note || !outDir) {
  console.error('usage: node tools/booklet/run.mjs <note.md> <outDir>')
  process.exit(1)
}
const dir = resolve(outDir)
mkdirSync(dir, { recursive: true })
const html = join(dir, 'book.html')
const pdf = join(dir, 'book.pdf')
const here = new URL('.', import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, '$1')

const run = (label, script, args) => {
  const out = execFileSync(process.execPath, [join(here, script), ...args], { encoding: 'utf8' }).trim()
  let parsed = null
  try {
    parsed = JSON.parse(out)
  } catch {
    const line = out.split('\n').filter((item) => item.startsWith('{')).pop()
    if (line) parsed = JSON.parse(line)
  }
  console.log(`[${label}] ok`)
  return parsed
}

const built = run('build', 'build.mjs', [note, html])
const screen = run('measure', 'measure.mjs', [html, '1440', '1100'])
const print = run('measure-print', 'measure.mjs', [html, '--print', '680', '1123'])
const pages = run('pdf', 'print.mjs', [html, pdf])

const summary = {
  note,
  html,
  pdf,
  bytes: built?.bytes,
  sections: built?.sections,
  figures: built?.figures,
  screen: {
    docWidth: screen?.doc?.w,
    overflow: screen?.overflow?.length,
    outOfBounds: screen?.outOfBounds?.length,
    under12: screen?.under12?.length,
    svgClipped: (screen?.svgClip || []).reduce((sum, item) => sum + item.clipped, 0),
  },
  print: {
    pages: pages?.pages,
    estimatedPages: print?.pageFit?.estimatedPages,
    sectionsOverOnePage: print?.pageFit?.overOne?.length,
    overflow: print?.overflow?.length,
    svgClipped: (print?.svgClip || []).reduce((sum, item) => sum + item.clipped, 0),
  },
}
console.log(JSON.stringify(summary, null, 1))
