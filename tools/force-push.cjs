#!/usr/bin/env node
// Force this device's local state to win the next sync.
//
// Every synced table row is compared against `sync_shadow.row_hash`, and rows
// already marked deleted are skipped. This tool marks every shadow as stale and
// clears its `deleted` flag; the next sync re-pushes all local rows with a fresh
// HLC, and re-sends deletes for rows/files that exist in the shadow but not on
// disk. That is the "this machine is the source of truth" operation: it also
// removes content another device resurrected while offline.
//
// Run it on the device whose content should win, with the app closed:
//   npm run sync:force-push
//   npm run sync:force-push -- --dry-run
//   npm run sync:force-push -- --db <path-to-yebian.sqlite>

const fs = require('node:fs')
const path = require('node:path')
const { execSync } = require('node:child_process')
const { DatabaseSync } = require('node:sqlite')

const args = process.argv.slice(2)
const dryRun = args.includes('--dry-run')
const dbFlag = args.indexOf('--db')
const defaultDb = path.join(process.env.APPDATA || '', 'com.yebian.notes', 'yebian.sqlite')
const dbPath = dbFlag >= 0 ? args[dbFlag + 1] : defaultDb
const sentinel = 'force-push'

function fail(message) {
  console.error(`✗ ${message}`)
  process.exit(1)
}

if (!dbPath || !fs.existsSync(dbPath)) {
  fail(`找不到数据库：${dbPath || '(空)'}`)
}

if (!dryRun && dbFlag < 0) {
  try {
    const output = execSync('tasklist /FI "IMAGENAME eq yebian.exe"', { encoding: 'utf8' })
    if (/yebian\.exe/i.test(output)) {
      fail('页边笔记正在运行。先完全退出应用，再运行这个命令。')
    }
  } catch {
    // tasklist can fail on non-Windows; the DB write below will surface it.
  }
}

const db = new DatabaseSync(dbPath)
const before = db.prepare('SELECT COUNT(*) AS n FROM sync_shadow').get().n
const pending = db
  .prepare('SELECT COUNT(*) AS n FROM sync_shadow WHERE row_hash <> ? OR deleted <> 0')
  .get(sentinel).n
const deleted = db.prepare('SELECT COUNT(*) AS n FROM sync_shadow WHERE deleted <> 0').get().n

if (!dryRun) {
  const stamp = new Date().toISOString().replace(/[:.]/g, '-')
  const backup = dbPath.replace(/\.sqlite$/i, `.pre-force-push-${stamp}.sqlite`).replace(/\\/g, '/')
  db.prepare('VACUUM INTO ?').run(backup)
  console.log(`备份：${backup}`)
}

const changed = dryRun
  ? pending
  : db
      .prepare('UPDATE sync_shadow SET row_hash = ?, deleted = 0 WHERE row_hash <> ? OR deleted <> 0')
      .run(sentinel, sentinel).changes

db.close()
console.log(`${dryRun ? '将标记' : '已标记'} ${changed} / ${before} 条同步影子，其中 ${deleted} 条本地已删除、会重发 delete`)
console.log(
  dryRun
    ? '这是 dry-run，没有修改数据库。'
    : '现在打开页边笔记，等它完成一次同步；这台机器的本地内容会以新的 HLC 重推，另一台随后拉取，另一台多出来的旧路径也会被删除。',
)
