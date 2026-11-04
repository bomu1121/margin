const { DatabaseSync } = require('node:sqlite')
const path = require('node:path')
const os = require('node:os')

const dbPath =
  process.argv[2] ||
  path.join(os.homedir(), 'AppData', 'Roaming', 'com.yebian.notes', 'yebian.sqlite')

const db = new DatabaseSync(dbPath, { readOnly: true })
const rows = db
  .prepare(
    'select id, note_id, block_id, kind, created_at, quote, body from annotations order by created_at',
  )
  .all()
console.log(JSON.stringify({ dbPath, count: rows.length, rows }, null, 2))
const stats = db.prepare('select count(*) as notes from notes').get()
console.log(JSON.stringify({ notes: stats.notes }))
db.close()
