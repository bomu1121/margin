// Seeds the local full-text corpus from an EPUB/HTML directory.
// usage: node tools/import-local-corpus.mjs <dir> <work-title> [author] [era]
//
// Calls production_local_corpus_import once per file. The app must be running
// with CDP on 9222. Text is stripped of tags/scripts; original characters are
// kept (traditional variants are matched by 4-character overlap).

import fs from 'node:fs'
import path from 'node:path'

const root = process.argv[2]
const workTitle = (process.argv[3] || '').trim()
const author = (process.argv[4] || '').trim()
const era = (process.argv[5] || '').trim()
if (!root || !workTitle) {
  console.error('usage: node tools/import-local-corpus.mjs <dir> <work-title> [author] [era]')
  process.exit(2)
}

function decodeEntities(text) {
  return text
    .replace(/&nbsp;/g, ' ')
    .replace(/&amp;/g, '&')
    .replace(/&lt;/g, '<')
    .replace(/&gt;/g, '>')
    .replace(/&quot;/g, '"')
    .replace(/&#(\d+);/g, (_, value) => String.fromCodePoint(Number(value)))
    .replace(/&#x([0-9a-f]+);/gi, (_, value) => String.fromCodePoint(parseInt(value, 16)))
}

function htmlToText(raw) {
  return decodeEntities(
    raw
      .replace(/<script[\s\S]*?<\/script>/gi, ' ')
      .replace(/<style[\s\S]*?<\/style>/gi, ' ')
      .replace(/<span[^>]*class="[^"]*yinwen[^"]*"[^>]*>[\s\S]*?<\/span>/gi, '')
      .replace(/<br\s*\/?>/gi, '\n')
      .replace(/<\/(p|div|h[1-6]|li)>/gi, '\n')
      .replace(/<[^>]+>/g, ' '),
  )
    .replace(/[ \t]+/g, ' ')
    .replace(/\n{3,}/g, '\n\n')
    .trim()
}

function collect(dir) {
  const output = []
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name)
    if (entry.isDirectory()) output.push(...collect(full))
    else if (/\.(html?|txt|md)$/i.test(entry.name)) output.push(full)
  }
  return output.sort((left, right) => left.localeCompare(right, 'zh-CN', { numeric: true }))
}

const files = collect(root)
if (!files.length) throw new Error(`no html/txt files under ${root}`)
console.log(`files ${files.length}`)

const list = await (await fetch('http://127.0.0.1:9222/json/list')).json()
const page = list.find((item) => item.type === 'page' && item.url.startsWith('http'))
if (!page) throw new Error('no CDP page target')
const ws = new WebSocket(page.webSocketDebuggerUrl)
await new Promise((resolve, reject) => {
  ws.addEventListener('open', resolve, { once: true })
  ws.addEventListener('error', reject, { once: true })
})
let id = 0
const pending = new Map()
ws.addEventListener('message', (event) => {
  const message = JSON.parse(String(event.data))
  if (!message.id) return
  const entry = pending.get(message.id)
  if (!entry) return
  pending.delete(message.id)
  entry(message)
})
function send(method, params = {}) {
  const callId = ++id
  ws.send(JSON.stringify({ id: callId, method, params }))
  return new Promise((resolve, reject) => {
    pending.set(callId, (message) => {
      if (message.error) reject(new Error(JSON.stringify(message.error)))
      else resolve(message.result)
    })
  })
}
async function invoke(command, args = {}) {
  const result = await send('Runtime.evaluate', {
    expression: `window.__TAURI_INTERNALS__.invoke(${JSON.stringify(command)}, ${JSON.stringify(args)})`,
    awaitPromise: true,
    returnByValue: true,
  })
  if (result.exceptionDetails) throw new Error(result.exceptionDetails.text || 'page error')
  return result.result?.value
}

let chars = 0
let passages = 0
for (const file of files) {
  const raw = fs.readFileSync(file, 'utf8')
  const titleMatch = raw.match(/<title[^>]*>([\s\S]*?)<\/title>/i)
  const title = titleMatch
    ? decodeEntities(titleMatch[1]).replace(/\s+/g, ' ').trim()
    : path.basename(file, path.extname(file))
  const text = /\.(html?)$/i.test(file) ? htmlToText(raw) : raw.trim()
  if (text.length < 100) continue
  const report = await invoke('production_local_corpus_import', {
    input: {
      workTitle,
      sectionTitle: '全卷',
      chapter: title,
      author,
      era,
      text,
    },
  })
  chars += report.chars
  passages += report.passages
  console.log(`${path.basename(file)}  ${report.chars} 字符 / ${report.passages} 段`)
}
console.log(JSON.stringify({ workTitle, files: files.length, chars, passages }, null, 2))
ws.close()
