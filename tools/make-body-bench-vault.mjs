// Creates a large vault whose notes all have the real sample body size.
// `make-bench-vault` writes short synthetic notes; this one exists to measure
// indexing with bodies closer to the user's real notes (~1.5k Chinese chars).
//
// usage: node tools/make-body-bench-vault.mjs <out-dir> [count] [source-note]
//   node tools/make-body-bench-vault.mjs D:/tmp/v10000 10000
//
// 10,000 copies of sample-vault/shebao.md are about 81MB. All copies have the
// same title/body; that is intentional (we measure bytes/tokens, not diversity).

import fs from 'node:fs'
import path from 'node:path'

const out = process.argv[2]
const count = Number(process.argv[3] || 10000)
const source = process.argv[4] || path.resolve('sample-vault/shebao.md')

if (!out) {
  console.error('usage: node tools/make-body-bench-vault.mjs <out-dir> [count] [source-note]')
  process.exit(2)
}
if (!fs.existsSync(source)) {
  console.error(`source note not found: ${source}`)
  process.exit(2)
}

const body = fs.readFileSync(source)
fs.rmSync(out, { recursive: true, force: true })
fs.mkdirSync(out, { recursive: true })

for (let i = 0; i < count; i += 1) {
  fs.writeFileSync(path.join(out, `note-${i}.md`), body)
}

const bytes = count * body.length
console.log(JSON.stringify({ out, count, source, sourceBytes: body.length, approxBytes: bytes }))
