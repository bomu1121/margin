// Converts the public-domain 清史稿 BKK/TLS YAML files into one UTF-8 text
// file per juan, so tools/import-local-corpus.mjs can ingest them.
// usage: node tools/convert-qingshigao.mjs <yaml-dir> <out-dir>

import fs from 'node:fs'
import path from 'node:path'

const source = process.argv[2]
const target = process.argv[3]
if (!source || !target) {
  console.error('usage: node tools/convert-qingshigao.mjs <yaml-dir> <out-dir>')
  process.exit(2)
}

const files = fs
  .readdirSync(source)
  .filter((name) => /^KR2a0039_\d+\.yaml$/i.test(name))
  .sort((left, right) => left.localeCompare(right, 'en', { numeric: true }))
fs.mkdirSync(target, { recursive: true })

let converted = 0
let chars = 0
for (const name of files) {
  const raw = fs.readFileSync(path.join(source, name), 'utf8')
  const body = raw.match(/body:\r?\n  text: (.*)\r?\n  hash:/)
  if (!body) continue
  const text = body[1].trim()
  if (!text) continue
  const seq = name.match(/_(\d+)\.yaml$/)?.[1] ?? ''
  const label = text
    .replace(/^清史稿/, '')
    .slice(0, 18)
    .replace(/[\\/:*?"<>|]/g, '')
  const out = path.join(target, `清史稿-${seq}-${label}.txt`)
  fs.writeFileSync(out, `${text}\n`, 'utf8')
  converted += 1
  chars += text.length
}

console.log(JSON.stringify({ files: files.length, converted, chars }, null, 2))
