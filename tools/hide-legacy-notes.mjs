// Marks legacy notes with `隐藏: true` in front matter so the indexer skips
// them. This is reversible and never deletes a note.
// usage:
//   node tools/hide-legacy-notes.mjs <vault> list
//   node tools/hide-legacy-notes.mjs <vault> hide-all
//   node tools/hide-legacy-notes.mjs <vault> unhide <id|relative-path>
//   node tools/hide-legacy-notes.mjs <vault> unhide-all

import fs from 'node:fs'
import path from 'node:path'

const vault = process.argv[2]
const mode = process.argv[3] || 'list'
const target = process.argv[4] || ''
if (!vault || !fs.existsSync(vault)) {
  console.error('usage: node tools/hide-legacy-notes.mjs <vault> <list|hide-all|unhide|unhide-all> [id|path]')
  process.exit(2)
}

const SKIP = new Set(['.git', '.trash', '.yebian', 'node_modules', 'target', 'Attachments'])
const HIDDEN = /^(隐藏|hidden)\s*:/i

function walk(dir) {
  const out = []
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    if (entry.name.startsWith('.') && entry.name !== '.') continue
    if (SKIP.has(entry.name)) continue
    const full = path.join(dir, entry.name)
    if (entry.isDirectory()) out.push(...walk(full))
    else if (/\.md$/i.test(entry.name)) out.push(full)
  }
  return out
}

function frontMatter(text) {
  const lines = text.split(/\r?\n/)
  if (lines[0]?.replace(/^\uFEFF/, '') !== '---') return null
  const end = lines.findIndex((line, index) => index > 0 && line.trim() === '---')
  if (end < 0) return null
  return { lines, end }
}

function noteId(lines) {
  const line = lines.find((item) => /^id\s*:/i.test(item))
  return line ? line.replace(/^id\s*:/i, '').trim().replace(/^["']|["']$/g, '') : ''
}

function isHidden(lines) {
  return lines.some((line) => HIDDEN.test(line))
}

const files = walk(vault)
const rows = []
let changed = 0
let skipped = 0

for (const file of files) {
  const raw = fs.readFileSync(file, 'utf8')
  const text = raw.replace(/^\uFEFF/, '')
  const fm = frontMatter(text)
  const rel = path.relative(vault, file).replace(/\\/g, '/')
  if (!fm) {
    skipped += 1
    continue
  }
  const id = noteId(fm.lines)
  const hidden = isHidden(fm.lines)
  const row = { file, rel, id, hidden }
  rows.push(row)
  if (mode === 'list') continue
  if (mode === 'hide-all') {
    if (hidden) continue
    const eol = raw.includes('\r\n') ? '\r\n' : '\n'
    const lines = text.split(/\r?\n/)
    lines.splice(1, 0, '隐藏: true')
    const bom = raw.startsWith('\uFEFF') ? '\uFEFF' : ''
    fs.writeFileSync(file, `${bom}${lines.join(eol)}`, 'utf8')
    row.hidden = true
    changed += 1
  } else if (mode === 'unhide-all') {
    if (!hidden) continue
    const eol = raw.includes('\r\n') ? '\r\n' : '\n'
    const lines = text.split(/\r?\n/).filter((line) => !HIDDEN.test(line))
    const bom = raw.startsWith('\uFEFF') ? '\uFEFF' : ''
    fs.writeFileSync(file, `${bom}${lines.join(eol)}`, 'utf8')
    row.hidden = false
    changed += 1
  } else if (mode === 'unhide') {
    if (!target) throw new Error('unhide needs an id or relative path')
    if (!hidden) continue
    if (id !== target && rel !== target) continue
    const eol = raw.includes('\r\n') ? '\r\n' : '\n'
    const lines = text.split(/\r?\n/).filter((line) => !HIDDEN.test(line))
    const bom = raw.startsWith('\uFEFF') ? '\uFEFF' : ''
    fs.writeFileSync(file, `${bom}${lines.join(eol)}`, 'utf8')
    row.hidden = false
    changed += 1
  }
}

if (mode === 'list') {
  console.log(JSON.stringify({
    notes: rows.length,
    hidden: rows.filter((row) => row.hidden).length,
    skipped,
    items: rows.filter((row) => row.hidden).map(({ id, rel }) => ({ id, rel })),
  }, null, 2))
} else {
  console.log(JSON.stringify({
    mode,
    notes: rows.length,
    hidden: rows.filter((row) => row.hidden).length,
    changed,
    skipped,
  }, null, 2))
}
