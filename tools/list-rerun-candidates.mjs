// Lists hidden legacy notes that still have a Bilibili source and are not in
// docs/rerun-queue.json yet. Read-only; prints `rel | title | url`.
import fs from 'node:fs'
import path from 'node:path'

const root = path.resolve(process.argv[2] || 'src-tauri/target/debug/sample-vault')
const queue = JSON.parse(fs.readFileSync(path.resolve('docs/rerun-queue.json'), 'utf8'))
const entries = [...(queue.pending || []), ...(queue.completed || [])]
const usedRel = new Set(entries.filter((item) => typeof item === 'object').map((item) => item.oldRel))
const doneTitle = new Set(
  entries
    .map((item) => (typeof item === 'string' ? item : item.title))
    .map((title) => title.split(' → ')[0].trim()),
)

const rows = []
function walk(dir) {
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    if (entry.name.startsWith('.')) continue
    const full = path.join(dir, entry.name)
    if (entry.isDirectory()) {
      walk(full)
      continue
    }
    if (!entry.name.endsWith('.md')) continue
    const text = fs.readFileSync(full, 'utf8')
    const front = text.match(/^---\r?\n([\s\S]*?)\r?\n---/)?.[1] || ''
    if (!/^隐藏:\s*true/m.test(front)) continue
    const title = front.match(/^标题:\s*"?(.*?)"?\s*$/m)?.[1] || entry.name.replace(/\.md$/, '')
    const url = front.match(/^原始链接:\s*"?(.*?)"?\s*$/m)?.[1] || ''
    const source = front.match(/^来源:\s*"?(.*?)"?\s*$/m)?.[1] || ''
    rows.push({
      rel: path.relative(root, full).split(path.sep).join('/'),
      title,
      url,
      source,
    })
  }
}
walk(root)

const candidates = rows.filter(
  (row) => /bilibili\.com/.test(row.url) && !usedRel.has(row.rel) && !doneTitle.has(row.title),
)
console.log(`hidden=${rows.length} candidates=${candidates.length}`)
for (const row of candidates) console.log(`${row.rel} | ${row.title} | ${row.url}`)
