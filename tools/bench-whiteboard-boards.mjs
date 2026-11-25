#!/usr/bin/env node
// SQL-layer benchmark for the whiteboard switcher: proves that opening the
// switcher reads board metadata only, and that a board load reads that board's
// cards rather than every card in the database.
//
// This measures SQLite/Rust-equivalent SQL, not UI paint time.
//
//   node --no-warnings tools/bench-whiteboard-boards.mjs
//   node --no-warnings tools/bench-whiteboard-boards.mjs --boards 200 --items 200

import fs from 'node:fs'
import os from 'node:os'
import path from 'node:path'
import { performance } from 'node:perf_hooks'
import { DatabaseSync } from 'node:sqlite'

const args = process.argv.slice(2)
const arg = (name, fallback) => {
  const index = args.indexOf(name)
  return index >= 0 ? Number(args[index + 1]) : fallback
}
const boardCount = arg('--boards', 100)
const itemsPerBoard = arg('--items', 200)
const rounds = arg('--rounds', 60)
const dbPath = path.join(os.tmpdir(), `yebian-whiteboard-bench-${process.pid}.sqlite`)
const db = new DatabaseSync(dbPath)

db.exec(`
  PRAGMA journal_mode = WAL;
  PRAGMA synchronous = NORMAL;
  CREATE TABLE whiteboard_boards (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL DEFAULT '我的白板',
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
  );
  CREATE TABLE whiteboard_items (
    id TEXT PRIMARY KEY,
    board_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    x REAL NOT NULL DEFAULT 0,
    y REAL NOT NULL DEFAULT 0,
    w REAL NOT NULL DEFAULT 220,
    h REAL NOT NULL DEFAULT 160,
    z INTEGER NOT NULL DEFAULT 0,
    text TEXT NOT NULL DEFAULT '',
    title TEXT NOT NULL DEFAULT '',
    url TEXT NOT NULL DEFAULT '',
    asset_path TEXT NOT NULL DEFAULT '',
    color TEXT NOT NULL DEFAULT '',
    checked INTEGER NOT NULL DEFAULT 0,
    note_id TEXT NOT NULL DEFAULT '',
    job_id TEXT NOT NULL DEFAULT '',
    data_json TEXT NOT NULL DEFAULT '{}',
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
  );
  CREATE INDEX idx_whiteboard_items_board
    ON whiteboard_items(board_id, z, created_at);
`)

const now = Date.now()
const insertBoard = db.prepare(
  'INSERT INTO whiteboard_boards(id, name, created_at, updated_at) VALUES (?, ?, ?, ?)',
)
const insertItem = db.prepare(
  `INSERT INTO whiteboard_items(
     id, board_id, kind, x, y, w, h, z, text, title, url, asset_path,
     color, checked, note_id, job_id, data_json, created_at, updated_at
   ) VALUES (?, ?, 'sticky', ?, ?, 270, 190, ?, ?, '', '', '', 'plain', 0, '', '', '{}', ?, ?)`,
)

db.exec('BEGIN')
for (let board = 0; board < boardCount; board += 1) {
  const boardId = `board_${String(board).padStart(3, '0')}`
  insertBoard.run(boardId, `工作区 ${board + 1}`, now + board, now + board)
  for (let item = 0; item < itemsPerBoard; item += 1) {
    insertItem.run(
      `wi_${board}_${item}`,
      boardId,
      item * 24,
      item * 18,
      item,
      `卡片 ${item + 1}：把资料、结论和下一步放在同一块可拖动的工作面上。`,
      now + item,
      now + item,
    )
  }
}
db.exec('COMMIT')
db.exec('PRAGMA wal_checkpoint(TRUNCATE)')

const listBoards = db.prepare(
  'SELECT id, name, created_at, updated_at FROM whiteboard_boards ORDER BY updated_at DESC, created_at DESC',
)
const listBoardItems = db.prepare(
  `SELECT id, board_id, kind, x, y, w, h, z, text, title, url, asset_path,
          color, checked, note_id, job_id, data_json, created_at, updated_at
   FROM whiteboard_items WHERE board_id = ? ORDER BY z ASC, created_at ASC`,
)
const listAllItems = db.prepare(
  `SELECT id, board_id, kind, x, y, w, h, z, text, title, url, asset_path,
          color, checked, note_id, job_id, data_json, created_at, updated_at
   FROM whiteboard_items`,
)

function pct(values, ratio) {
  const sorted = [...values].sort((a, b) => a - b)
  return sorted[Math.min(sorted.length - 1, Math.floor(sorted.length * ratio))]
}

function measure(label, statement, params = []) {
  const timings = []
  let rows = 0
  for (let index = 0; index < rounds + 5; index += 1) {
    const started = performance.now()
    rows = statement.all(...params).length
    const elapsed = performance.now() - started
    if (index >= 5) timings.push(elapsed)
  }
  const total = timings.reduce((sum, value) => sum + value, 0)
  console.log(
    `${label.padEnd(30)} rows=${String(rows).padStart(6)}  ` +
      `p50=${pct(timings, 0.5).toFixed(2)}ms  ` +
      `p95=${pct(timings, 0.95).toFixed(2)}ms  ` +
      `max=${Math.max(...timings).toFixed(2)}ms  ` +
      `avg=${(total / timings.length).toFixed(2)}ms`,
  )
}

const targetBoard = `board_${String(Math.floor(boardCount / 2)).padStart(3, '0')}`
console.log(
  `boards=${boardCount} items/board=${itemsPerBoard} total_items=${boardCount * itemsPerBoard} rounds=${rounds}`,
)
console.log(`db=${(fs.statSync(dbPath).size / 1024 / 1024).toFixed(2)} MB (WAL checkpointed)`)
measure('switcher: board metadata', listBoards)
measure('open one board: its cards', listBoardItems, [targetBoard])
measure('comparison: every card', listAllItems)

db.close()
for (const suffix of ['', '-wal', '-shm']) {
  fs.rmSync(dbPath + suffix, { force: true })
}
