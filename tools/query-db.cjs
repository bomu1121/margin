const { DatabaseSync } = require('node:sqlite')
const path = require('node:path')
const os = require('node:os')
const fs = require('node:fs')

const root = path.join(os.homedir(), 'AppData', 'Roaming', 'com.yebian.notes')
const userDbPath = process.argv[2] || path.join(root, 'yebian.sqlite')
const indexDbPath = process.argv[3] || path.join(root, 'yebian-index.sqlite')

// Annotations/settings live in the user DB; notes live in the index DB.
const user = new DatabaseSync(userDbPath, { readOnly: true })
const rows = user
  .prepare(
    'select id, note_id, block_id, kind, created_at, quote, body from annotations order by created_at',
  )
  .all()
console.log(JSON.stringify({ userDbPath, count: rows.length, rows }, null, 2))
user.close()

const notesPath = fs.existsSync(indexDbPath) ? indexDbPath : userDbPath
const index = new DatabaseSync(notesPath, { readOnly: true })
const stats = index.prepare('select count(*) as notes from notes').get()
console.log(JSON.stringify({ notesDbPath: notesPath, notes: stats.notes }))
index.close()
