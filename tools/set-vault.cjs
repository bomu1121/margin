const { DatabaseSync } = require('node:sqlite')
const path = require('node:path')
const os = require('node:os')

const value = process.argv[2]
if (!value) {
  console.error('usage: node tools/set-vault.cjs <absolute-or-empty-path>')
  process.exit(2)
}

const dbPath =
  process.argv[3] ||
  path.join(os.homedir(), 'AppData', 'Roaming', 'com.yebian.notes', 'yebian.sqlite')
const db = new DatabaseSync(dbPath)
const before = db
  .prepare("select value from settings where key = 'vault_path'")
  .get()
if (value === '-') {
  db.prepare("delete from settings where key = 'vault_path'").run()
} else {
  db.prepare(
    "insert into settings (key, value) values ('vault_path', ?1) on conflict(key) do update set value = excluded.value",
  ).run(value)
}
const after = db
  .prepare("select value from settings where key = 'vault_path'")
  .get()
console.log(JSON.stringify({ dbPath, before: before?.value ?? null, after: after?.value ?? null }))
db.close()
