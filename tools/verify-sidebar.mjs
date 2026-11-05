// Drives the sidebar's file operations through the real Tauri window via CDP.
// usage: node --no-warnings tools/verify-sidebar.mjs [vault-dir]
//
// Prereq: launch the app with
//   WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222
// The script rebuilds [vault-dir] from sample-vault, switches the app to it,
// reloads, runs the checks, and switches the app back to the previous vault.
//
// Every step asserts against the DOM, the filesystem, or SQLite; it exits
// non-zero if any check fails. Screenshots go to shots/accept-*.png.

import fs from 'node:fs'
import path from 'node:path'
import os from 'node:os'
import { DatabaseSync } from 'node:sqlite'

const vault = process.argv[2] || 'D:\\Develop\\yebian-accept-20261105\\库'
const userDbPath = path.join(os.homedir(), 'AppData', 'Roaming', 'com.yebian.notes', 'yebian.sqlite')
const dbPath = path.join(os.homedir(), 'AppData', 'Roaming', 'com.yebian.notes', 'yebian-index.sqlite')
const shotsDir = path.resolve('shots')
fs.mkdirSync(shotsDir, { recursive: true })

const results = []
let failed = false

function check(name, ok, detail = '') {
  results.push({ name, ok, detail })
  if (!ok) failed = true
  console.log(`${ok ? 'PASS' : 'FAIL'}  ${name}${detail ? `  ${detail}` : ''}`)
}

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms))

// ---------- fixture ----------

const targetNote = `---
标题: 目标笔记
类型: 测试
创建日期: 2026-11-05
领域: 测试
tags:
- 领域/测试
---

# 目标笔记

## 速览

- 这是移动与重命名的验收目标。

## 正文

### 第一节

这段文字用来验证标注在移动之后是否仍然锚定，所以它必须是一段独立的、足够长的正文。

### 第二节

另一段正文，用来让上面那段有明确的边界。
`

const linkSource = `---
标题: 链接源
类型: 测试
创建日期: 2026-11-05
领域: 测试
tags:
- 领域/测试
---

# 链接源

## 正文

### 链接

指向 [[目标笔记]] 的 wikilink。

以及一个 [相对链接](研究/目标笔记.md)。
`

const sampleVault = path.resolve('sample-vault')
fs.rmSync(vault, { recursive: true, force: true })
fs.mkdirSync(path.join(vault, '研究', '空目录'), { recursive: true })
fs.mkdirSync(path.join(vault, '归档'), { recursive: true })
for (const name of ['nanbei.md', 'shebao.md', 'yingxue.md', 'zhuanai.md']) {
  const source = path.join(sampleVault, name)
  if (fs.existsSync(source)) fs.copyFileSync(source, path.join(vault, name))
}
fs.writeFileSync(path.join(vault, '研究', '目标笔记.md'), targetNote)
fs.writeFileSync(path.join(vault, '链接源.md'), linkSource)
console.log(`fixture rebuilt at ${vault}`)

// ---------- CDP ----------

const targets = await (await fetch('http://127.0.0.1:9222/json/list')).json()
const page = targets.find((item) => item.type === 'page')
if (!page) throw new Error('no CDP page target')

const ws = new WebSocket(page.webSocketDebuggerUrl)
await new Promise((resolve, reject) => {
  ws.addEventListener('open', resolve, { once: true })
  ws.addEventListener('error', reject, { once: true })
})

let nextId = 0
const pending = new Map()
ws.addEventListener('message', (event) => {
  const message = JSON.parse(String(event.data))
  const entry = pending.get(message.id)
  if (!entry) return
  pending.delete(message.id)
  entry(message)
})

function send(method, params = {}) {
  const id = ++nextId
  ws.send(JSON.stringify({ id, method, params }))
  return new Promise((resolve, reject) => {
    pending.set(id, (message) => {
      if (message.error) reject(new Error(JSON.stringify(message.error)))
      else resolve(message.result)
    })
  })
}

async function evaluate(expression) {
  const result = await send('Runtime.evaluate', {
    expression,
    awaitPromise: true,
    returnByValue: true,
  })
  if (result.exceptionDetails) {
    throw new Error(`page error: ${JSON.stringify(result.exceptionDetails)}`)
  }
  return result.result?.value
}

async function waitFor(expression, label, timeout = 10000) {
  const start = Date.now()
  while (Date.now() - start < timeout) {
    try {
      if (await evaluate(expression)) return true
    } catch {
      // The execution context is briefly gone while the page reloads.
    }
    await sleep(120)
  }
  throw new Error(`timeout waiting for ${label}`)
}

async function screenshot(name) {
  const shot = await send('Page.captureScreenshot', { format: 'png' })
  fs.writeFileSync(path.join(shotsDir, `accept-${name}.png`), Buffer.from(shot.data, 'base64'))
}

const previousVault = await evaluate(`window.__TAURI_INTERNALS__.invoke('get_vault')`)
await evaluate(`window.__TAURI_INTERNALS__.invoke('set_vault', { path: ${JSON.stringify(vault)} })`)
await evaluate(`location.hash = '#/note'`)
await send('Page.reload', { ignoreCache: true })
await waitFor(`document.readyState === 'complete'`, 'page reload', 15000)
console.log(`vault switched to ${vault} (previous: ${previousVault ?? 'none'})`)

// Page-side helpers live on window.__v so they survive across evaluations.
await evaluate(`window.__v = {
  noteRow(title) {
    return [...document.querySelectorAll('.tree-body .note-row')]
      .find((row) => row.querySelector('.tree-title')?.textContent.trim() === title) || null
  },
  folder(path) {
    return document.querySelector('[data-folder="' + path + '"]')
  },
  menuOn(el, x = 320, y = 260) {
    el.dispatchEvent(new MouseEvent('contextmenu', {
      bubbles: true, cancelable: true, clientX: x, clientY: y,
    }))
  },
  menuItem(text) {
    return [...document.querySelectorAll('.ctx button')]
      .find((b) => b.textContent.trim().startsWith(text)) || null
  },
  setInput(el, value) {
    el.focus()
    el.value = value
    el.dispatchEvent(new Event('input', { bubbles: true }))
  },
  pressEnter(el) {
    el.dispatchEvent(new KeyboardEvent('keydown', {
      key: 'Enter', code: 'Enter', keyCode: 13, which: 13,
      bubbles: true, cancelable: true,
    }))
  },
  dragTo(fromRow, folderPath) {
    const dt = new DataTransfer()
    fromRow.dispatchEvent(new DragEvent('dragstart', {
      bubbles: true, cancelable: true, dataTransfer: dt,
    }))
    const dst = this.folder(folderPath)
    if (!dst) return false
    dst.dispatchEvent(new DragEvent('dragover', {
      bubbles: true, cancelable: true, dataTransfer: dt,
    }))
    dst.dispatchEvent(new DragEvent('drop', {
      bubbles: true, cancelable: true, dataTransfer: dt,
    }))
    return true
  },
  toastText() {
    return document.querySelector('.toast')?.textContent.trim() || ''
  },
}`)

// ---------- filesystem / db helpers ----------

const abs = (rel) => path.join(vault, ...rel.split('/'))
const exists = (rel) => fs.existsSync(abs(rel))
const read = (rel) => fs.readFileSync(abs(rel), 'utf8')

function frontMatterId(rel) {
  const match = read(rel).match(/^---\r?\n([\s\S]*?)\r?\n---/)
  return match?.[1].match(/^id:\s*(\S+)\s*$/m)?.[1] ?? null
}

function trashEntries() {
  const root = path.join(vault, '.trash')
  if (!fs.existsSync(root)) return []
  return fs
    .readdirSync(root, { withFileTypes: true })
    .filter((entry) => entry.isDirectory())
    .map((entry) => path.join(root, entry.name))
}

function userRows(sql, ...params) {
  const db = new DatabaseSync(userDbPath, { readOnly: true })
  try {
    return db.prepare(sql).all(...params)
  } finally {
    db.close()
  }
}

function dbRows(sql, ...params) {
  const db = new DatabaseSync(dbPath, { readOnly: true })
  try {
    return db.prepare(sql).all(...params)
  } finally {
    db.close()
  }
}

async function waitForDb(expression, label) {
  const start = Date.now()
  while (Date.now() - start < 10000) {
    const row = dbRows(expression)
    if (row.length) return row
    await sleep(150)
  }
  throw new Error(`timeout waiting for db: ${label}`)
}

// The fixture note keeps a path-derived id across runs, so an annotation from
// a previous run would still be attached. Remove only the fixture paragraph's
// rows for that note before measuring; nothing else is touched.
async function clearStaleAnnotations(noteId) {
  for (let attempt = 1; attempt <= 5; attempt += 1) {
    try {
      const db = new DatabaseSync(userDbPath)
      try {
        const result = db
          .prepare("delete from annotations where note_id = ? and quote like '这段文字用来验证%'")
          .run(noteId)
        if (result.changes) console.log(`cleared ${result.changes} stale annotation(s) for ${noteId}`)
      } finally {
        db.close()
      }
      return
    } catch (error) {
      if (attempt === 5) throw error
      await sleep(200)
    }
  }
}

// ---------- steps ----------

await waitFor(`!!document.querySelector('.side-nav .tree-title')`, 'sidebar notes')
check('sidebar-rendered', true)
await waitForDb("select id from notes where rel_path = '研究/目标笔记.md'", 'fixture indexed')
const fixtureRow = dbRows('select id from notes where rel_path = ?', '研究/目标笔记.md')[0]
await clearStaleAnnotations(fixtureRow.id)
await screenshot('01-start')

// Empty folders are real structure, not derived from .md files.
const expandResearch = await evaluate(`(() => {
  const btn = window.__v.folder('研究')
  if (!btn) return false
  if (btn.getAttribute('aria-expanded') === 'false') btn.click()
  return true
})()`)
await sleep(250)
const foldersVisible = await evaluate(`({
  archive: !!window.__v.folder('归档'),
  empty: !!window.__v.folder('研究/空目录'),
})`)
check('empty-folders-visible', expandResearch && foldersVisible.archive && foldersVisible.empty,
  JSON.stringify(foldersVisible))

// Open the target note and add one annotation to it.
const opened = await evaluate(`(() => {
  const row = window.__v.noteRow('目标笔记')
  if (!row) return false
  row.click()
  return true
})()`)
await waitFor(`document.querySelector('.doc h1')?.textContent.includes('目标笔记')`, 'target note')
const annotated = await evaluate(`(() => {
  const prose = [...document.querySelectorAll('.doc .prose')]
    .find((node) => node.textContent.includes('这段文字用来验证'))
  if (!prose) return false
  prose.closest('.unit').querySelector('button.add').click()
  return true
})()`)
await waitFor(`!!document.querySelector('.composer textarea')`, 'annotation composer')
await evaluate(`(() => {
  const input = document.querySelector('.composer textarea')
  window.__v.setInput(input, '移动验收：标注应跟随笔记')
  document.querySelector('.composer button.save').click()
})()`)
await waitFor(`!!document.querySelector('.rail .anno:not(.is-orphan)')`, 'annotation card')
const quote = await evaluate(`document.querySelector('.rail .anno .anno-q')?.textContent.trim()`)
check('annotation-added', opened && annotated && !!quote, quote ?? '')

const beforeId = dbRows('select id, rel_path from notes where rel_path = ?', '研究/目标笔记.md')[0] ?? null
const beforeAnno = userRows('select note_id from annotations where quote = ?', quote)[0] ?? null
check('baseline-identity', !!beforeId && !!beforeAnno && beforeId.id === beforeAnno.note_id,
  JSON.stringify({ beforeId, beforeAnno }))
await screenshot('02-annotated')

// Rename the target from the context menu.
await evaluate(`(() => {
  const row = window.__v.noteRow('目标笔记')
  window.__v.menuOn(row)
})()`)
await waitFor(`!!window.__v.menuItem('重命名')`, 'rename menu item')
await evaluate(`window.__v.menuItem('重命名').click()`)
await waitFor(`!!document.querySelector('.rename-input')`, 'rename input')
await evaluate(`(() => {
  const input = document.querySelector('.rename-input')
  window.__v.setInput(input, '目标笔记-改名')
  window.__v.pressEnter(input)
})()`)
await waitFor(`!!window.__v.noteRow('目标笔记-改名')`, 'renamed tree row', 15000)
check('rename-file', exists('研究/目标笔记-改名.md') && !exists('研究/目标笔记.md'),
  exists('研究/目标笔记-改名.md') ? '研究/目标笔记-改名.md' : 'missing')

const afterRenameId = dbRows('select id, rel_path from notes where rel_path = ?', '研究/目标笔记-改名.md')[0] ?? null
const afterRenameAnno = userRows('select note_id from annotations where quote = ?', quote)[0] ?? null
check('rename-keeps-id',
  !!afterRenameId && !!afterRenameAnno &&
    afterRenameId.id === afterRenameAnno.note_id &&
    afterRenameAnno.note_id === beforeAnno.note_id,
  JSON.stringify({ afterRenameId, afterRenameAnno }))
check('rename-writes-front-matter-id', !!frontMatterId('研究/目标笔记-改名.md'),
  frontMatterId('研究/目标笔记-改名.md') ?? 'none')

const linkAfterRename = read('链接源.md')
check('rename-rewrites-wikilink', linkAfterRename.includes('[[目标笔记-改名]]'),
  linkAfterRename.split('\n').find((line) => line.includes('[[')) ?? '')

// Move the renamed note into 归档 with a synthetic HTML5 drag.
const draggedTarget = await evaluate(`(() => {
  const row = window.__v.noteRow('目标笔记-改名')
  return row ? window.__v.dragTo(row, '归档') : false
})()`)
await waitFor(`!!window.__v.noteRow('目标笔记-改名')`, 'moved row still listed', 15000)
await waitForDb(`select id from notes where rel_path = '归档/目标笔记-改名.md'`, 'moved note indexed')
const movedId = dbRows('select id from notes where rel_path = ?', '归档/目标笔记-改名.md')[0]
const movedAnno = userRows('select note_id from annotations where quote = ?', quote)[0]
check('drag-move-file', draggedTarget && exists('归档/目标笔记-改名.md') && !exists('研究/目标笔记-改名.md'),
  draggedTarget ? '归档/目标笔记-改名.md' : 'drag failed')
check('move-keeps-id-and-annotation',
  movedId?.id === beforeAnno.note_id && movedAnno?.note_id === beforeAnno.note_id,
  JSON.stringify({ movedId, movedAnno }))
const moveToast = await evaluate(`window.__v.toastText()`)
check('move-reports-link-rewrites', /更新了 \d+ 处链接/.test(moveToast), moveToast)
await screenshot('03-moved')

// Moving a link *source* must re-relativize its Markdown links.
await waitFor(`!!window.__v.noteRow('链接源')`, 'link source row')
await evaluate(`(() => {
  const row = window.__v.noteRow('链接源')
  window.__v.dragTo(row, '研究')
})()`)
await waitForDb(`select id from notes where rel_path = '研究/链接源.md'`, 'moved link source indexed')
const movedLink = exists('研究/链接源.md') ? read('研究/链接源.md') : ''
check('move-rewrites-relative-link',
  movedLink.includes('[相对链接](../归档/目标笔记-改名.md)') &&
    movedLink.includes('[[目标笔记-改名]]'),
  movedLink.split('\n').find((line) => line.includes('相对链接')) ?? 'missing')

// Create a note at the root through the root context menu.
await evaluate(`window.__v.menuOn(document.querySelector('.tree-scroll'), 420, 320)`)
await waitFor(`!!window.__v.menuItem('新建笔记')`, 'create-note menu item')
await evaluate(`window.__v.menuItem('新建笔记').click()`)
await waitFor(`!!document.querySelector('.rename-input')`, 'new note rename input')
await evaluate(`(() => {
  const input = document.querySelector('.rename-input')
  window.__v.setInput(input, '验收新笔记')
  window.__v.pressEnter(input)
})()`)
await waitFor(`!!window.__v.noteRow('验收新笔记')`, 'new note row', 15000)
const newNoteId = exists('验收新笔记.md') ? frontMatterId('验收新笔记.md') : null
check('create-note', !!newNoteId && exists('验收新笔记.md'), newNoteId ?? 'missing')

// Create a folder at the root, then drag the new note into it.
await evaluate(`window.__v.menuOn(document.querySelector('.tree-scroll'), 420, 320)`)
await waitFor(`!!window.__v.menuItem('新建文件夹')`, 'create-folder menu item')
await evaluate(`window.__v.menuItem('新建文件夹').click()`)
await waitFor(`!!document.querySelector('.rename-input')`, 'new folder rename input')
await evaluate(`(() => {
  const input = document.querySelector('.rename-input')
  window.__v.setInput(input, '验收目录')
  window.__v.pressEnter(input)
})()`)
await waitFor(`!!window.__v.folder('验收目录')`, 'new folder row', 15000)
check('create-folder', fs.existsSync(abs('验收目录')) && fs.statSync(abs('验收目录')).isDirectory(),
  exists('验收目录') ? '验收目录' : 'missing')

const draggedNew = await evaluate(`(() => {
  const row = window.__v.noteRow('验收新笔记')
  return row ? window.__v.dragTo(row, '验收目录') : false
})()`)
await waitFor(`!!window.__v.noteRow('验收新笔记')`, 'note row after folder move', 15000)
await waitForDb(`select id from notes where rel_path = '验收目录/验收新笔记.md'`, 'new note indexed in folder')
check('drag-note-into-new-folder',
  draggedNew && exists('验收目录/验收新笔记.md') && !exists('验收新笔记.md'),
  draggedNew ? '验收目录/验收新笔记.md' : 'drag failed')
check('moved-note-keeps-front-matter-id',
  frontMatterId('验收目录/验收新笔记.md') === newNoteId,
  `${newNoteId} -> ${frontMatterId('验收目录/验收新笔记.md')}`)

// Delete it, verify the trash entry, then undo within the 8s window.
await evaluate(`(() => {
  const row = window.__v.noteRow('验收新笔记')
  window.__v.menuOn(row)
})()`)
await waitFor(`!!window.__v.menuItem('删除')`, 'delete menu item')
await evaluate(`window.__v.menuItem('删除').click()`)
await waitFor(`!!document.querySelector('.confirm')`, 'delete confirm')
await evaluate(`document.querySelector('.confirm button.danger').click()`)
await waitFor(`!document.querySelector('.confirm')`, 'confirm closed')
try {
  await waitFor(
    `![...document.querySelectorAll('.side-nav .tree-title')].some((t) => t.textContent.includes('验收新笔记'))`,
    'row removed',
    15000,
  )
} catch (error) {
  const state = await evaluate(`({
    toast: document.querySelector('.toast')?.textContent?.trim() || '',
    titles: [...document.querySelectorAll('.side-nav .tree-title')].map((t) => t.textContent.trim()),
    confirm: !!document.querySelector('.confirm'),
  })`)
  console.log('delete debug', JSON.stringify(state))
  throw error
}
const trashed = trashEntries().some((dir) => fs.existsSync(path.join(dir, '验收目录', '验收新笔记.md')))
check('delete-moves-to-trash', trashed && !exists('验收目录/验收新笔记.md'),
  trashed ? 'trash entry found' : 'no trash entry')
await waitFor(`window.__v.toastText().includes('已删除')`, 'delete toast', 5000)
const deleteToast = await evaluate(`window.__v.toastText()`)
check('delete-offers-undo', deleteToast.includes('已删除') && deleteToast.includes('撤销'), deleteToast)
await screenshot('04-deleted')

await evaluate(`document.querySelector('.toast button').click()`)
await waitFor(`!!window.__v.noteRow('验收新笔记')`, 'row restored', 15000)
check('undo-restores-note', exists('验收目录/验收新笔记.md') && !trashEntries().some((dir) => fs.existsSync(path.join(dir, '验收目录', '验收新笔记.md'))),
  exists('验收目录/验收新笔记.md') ? 'restored' : 'missing')

// Final identity check: open the moved note and read the annotation card.
const openedMoved = await evaluate(`(() => {
  const btn = window.__v.folder('归档')
  if (btn && btn.getAttribute('aria-expanded') === 'false') btn.click()
  return true
})()`)
await sleep(300)
await evaluate(`(() => {
  const row = window.__v.noteRow('目标笔记-改名')
  if (!row) return false
  row.click()
  return true
})()`)
await waitFor(`document.querySelector('.doc h1')?.textContent.includes('目标笔记-改名')`, 'moved note page')
const cards = await evaluate(`({
  normal: document.querySelectorAll('.rail .anno:not(.is-orphan)').length,
  orphan: document.querySelectorAll('.rail .anno.is-orphan').length,
  text: document.querySelector('.rail .anno .anno-q')?.textContent.trim() || '',
})`)
check('annotation-still-anchored', openedMoved && cards.normal >= 1 && cards.orphan === 0 && cards.text === quote,
  JSON.stringify(cards))
await screenshot('05-final')

if (previousVault) {
  await evaluate(`window.__TAURI_INTERNALS__.invoke('set_vault', { path: ${JSON.stringify(previousVault)} })`)
  console.log(`vault restored to ${previousVault}`)
}

ws.close()

const passed = results.filter((item) => item.ok).length
console.log(`\n${passed}/${results.length} checks passed`)
console.log(JSON.stringify({ vault, results }, null, 2))
process.exit(failed ? 1 : 0)
