#!/usr/bin/env node
// One-off migration: use the original Bilibili video title as the note title,
// update its H1, and rewrite [[wiki links]] that point at old generated titles.
//
//   node --no-warnings tools/retitle-video-notes.mjs --dry-run
//   node --no-warnings tools/retitle-video-notes.mjs --apply
//
// The app must be running with CDP on 9223. --apply backs up every visible note
// first, edits content, then renames files through the app so the registry and
// index stay consistent.

import fs from 'node:fs'
import os from 'node:os'
import path from 'node:path'
import { DatabaseSync } from 'node:sqlite'

import { connectCdp, invoke } from './lib/cdp.mjs'

const args = process.argv.slice(2)
const apply = args.includes('--apply')
const port = Number(process.env.YEBian_RERUN_PORT || 9223)
const dataDir = path.join(process.env.APPDATA || '', 'com.yebian.notes')
const dbPath = path.join(dataDir, 'yebian.sqlite')

const db = new DatabaseSync(dbPath, { readOnly: true })
const jobs = db
  .prepare(
    "SELECT note_id, note_rel_path, source_json, created_at FROM production_jobs WHERE note_id != '' ORDER BY created_at DESC",
  )
  .all()
db.close()

const videoByNote = new Map()
for (const row of jobs) {
  if (videoByNote.has(row.note_id)) continue
  let source
  try {
    source = JSON.parse(row.source_json)
  } catch {
    continue
  }
  const video = source?.videos?.find((item) => String(item?.title || '').trim())
  if (!video) continue
  videoByNote.set(row.note_id, {
    title: String(video.title).trim(),
    rel: row.note_rel_path || '',
  })
}

function wikiKey(value) {
  return String(value || '')
    .replace(/\\"/g, '"')
    .replace(/\\'/g, "'")
    .replace(/[“”"‘’']/g, '')
    .replace(/\s+/g, '')
    .toLowerCase()
}

function yamlTitle(value) {
  return value.replace(/\\/g, '\\\\').replace(/"/g, '\\"')
}

// Keep in sync with Rust outbox::safe_file_stem: Windows-illegal characters
// become _, ASCII quotes become paired Chinese quotes, and the stem is capped.
function safeStem(value) {
  let open = true
  const cleaned = [...value]
    .map((ch) => {
      if (ch === '"') {
        const quote = open ? '“' : '”'
        open = !open
        return quote
      }
      return /[\\/:*?<>|]/.test(ch) ? '_' : ch
    })
    .join('')
    .trim()
    .replace(/^\.+|\.+$/g, '')
    .trim()
  return cleaned ? [...cleaned].slice(0, 80).join('') : '笔记'
}

const cdp = await connectCdp(port)
const notes = await invoke(cdp.evaluate, 'list_notes', {})
const byId = new Map(notes.map((note) => [note.id, note]))
const changes = []

for (const note of notes) {
  const source = videoByNote.get(note.id)
  if (!source?.title || source.title === note.title) continue
  changes.push({ note, newTitle: source.title })
}

const titleMap = new Map(changes.map(({ note, newTitle }) => [wikiKey(note.title), newTitle]))
const titleByNote = new Map(changes.map(({ note, newTitle }) => [note.id, newTitle]))

function rewriteWikiLinks(text) {
  return text.replace(/\[\[([^\[\]]+)\]\]/g, (whole, raw) => {
    const separator = raw.indexOf('|')
    const target = separator >= 0 ? raw.slice(0, separator) : raw
    const alias = separator >= 0 ? raw.slice(separator + 1) : ''
    const replacement =
      titleMap.get(wikiKey(target)) ?? titleMap.get(wikiKey(target.split('/').pop() ?? target))
    if (!replacement) return whole
    return `[[${replacement}${separator >= 0 ? '|' + alias : ''}]]`
  })
}

function rewriteNote(text, newTitle) {
  let next = text.replace(/^(标题:\s*).*$/m, (_line, prefix) => `${prefix}"${yamlTitle(newTitle)}"`)
  next = next.replace(/^# .*$/m, `# ${newTitle}`)
  next = rewriteWikiLinks(next)
  return next
}

console.log(`visible=${notes.length} retitle=${changes.length} apply=${apply}`)
for (const { note, newTitle } of changes) {
  console.log(`- ${note.title}\n  -> ${newTitle}`)
}

if (!apply) {
  const dangling = []
  for (const note of notes) {
    const text = fs.readFileSync(note.path, 'utf8')
    for (const match of text.matchAll(/\[\[([^\[\]]+)\]\]/g)) {
      const target = match[1].split('|')[0]
      if (!byId.has(note.id)) continue
      if (!titleMap.has(wikiKey(target)) && !titleMap.has(wikiKey(target.split('/').pop() ?? target)) && !notes.some((item) => wikiKey(item.title) === wikiKey(target) || wikiKey(item.title) === wikiKey(target.split('/').pop() ?? target))) {
        dangling.push({ note: note.relPath, target })
      }
    }
  }
  console.log(`dry-run: dangling wiki targets=${dangling.length}`)
  for (const item of dangling.slice(0, 20)) console.log(`  ${item.note} -> ${item.target}`)
  cdp.close()
  process.exit(0)
}

const stamp = new Date().toISOString().replace(/[:.]/g, '-')
const backup = path.join(os.tmpdir(), `yebian-retitle-${stamp}`)
fs.mkdirSync(backup, { recursive: true })
for (const note of notes) {
  const target = path.join(backup, note.relPath)
  fs.mkdirSync(path.dirname(target), { recursive: true })
  fs.copyFileSync(note.path, target)
}
console.log(`backup=${backup}`)

for (const note of notes) {
  const text = fs.readFileSync(note.path, 'utf8')
  const newTitle = titleByNote.get(note.id)
  const next = newTitle ? rewriteNote(text, newTitle) : rewriteWikiLinks(text)
  if (next !== text) {
    const temp = `${note.path}.title-tmp`
    fs.writeFileSync(temp, next, 'utf8')
    fs.renameSync(temp, note.path)
  }
}

for (const { note, newTitle } of changes) {
  const current = note.relPath
  const stem = safeStem(newTitle)
  if (path.basename(current, '.md') === stem) continue
  try {
    const report = await invoke(cdp.evaluate, 'rename_entry', {
      relPath: current,
      newName: `${stem}.md`,
    })
    console.log(`renamed ${current} -> ${report.relPath}`)
  } catch (error) {
    console.error(`rename failed for ${current}: ${String(error)}`)
  }
}

const stats = await invoke(cdp.evaluate, 'reindex', {})
console.log(`reindex=${JSON.stringify(stats)}`)
cdp.close()
