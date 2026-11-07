// Verifies annotation relocation after a note body changes.
// usage: node tools/verify-anchor.mjs (app with CDP on 9222)
import fs from 'node:fs'
import os from 'node:os'
import path from 'node:path'
import { execFileSync } from 'node:child_process'
import { connectCdp, invoke, sleep } from './lib/cdp.mjs'

const results = []
function check(name, ok, detail = '') {
  results.push({ name, ok, detail })
  console.log(`${ok ? 'PASS' : 'FAIL'}  ${name}${detail ? `  ${detail}` : ''}`)
}

const cdp = await connectCdp()
const evaluate = cdp.evaluate
const shot = async (name) => {
  const image = await cdp.send('Page.captureScreenshot', { format: 'png' })
  fs.writeFileSync(path.join('shots', name), Buffer.from(image.data, 'base64'))
}
const originalVault = await invoke(evaluate, 'get_vault')
const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'yebian-anchor-'))
const notePath = path.join(dir, 'anchor-note.md')
fs.writeFileSync(notePath, `---\n标题: 标注重定位 UI\n创建日期: 2026-11-07\n---\n\n# 标注重定位 UI\n\n## 正文\n\n这是一段需要重新定位的正文。\n`)
let annotationId = ''
try {
  await invoke(evaluate, 'set_vault', { path: dir.replaceAll('\\', '/') })
  let note = null
  for (let i = 0; i < 30 && !note; i += 1) {
    const notes = await invoke(evaluate, 'list_notes')
    note = notes.find((item) => item.title === '标注重定位 UI')
    if (!note) await sleep(200)
  }
  if (!note) throw new Error('test note was not indexed')
  const annotation = await invoke(evaluate, 'add_annotation', {
    noteId: note.id,
    blockId: 'old-block',
    quote: '这是一段需要重新定位的正文。',
    body: '这是一条测试标注。',
    kind: '标记',
  })
  annotationId = annotation.id
  await evaluate(`location.hash = '#/studio'`)
  await sleep(300)
  await evaluate(`location.hash = '#/note/${note.id}'`)
  await sleep(1200)
  const before = await evaluate(`(() => ({
    orphan: document.querySelectorAll('.anno.is-orphan').length,
    valid: document.querySelectorAll('.anno:not(.is-orphan)').length,
    head: document.querySelector('.anno-orphan-head')?.textContent?.replace(/\s+/g,' ').trim() || '',
  }))()`)
  check('orphan is visible', before.orphan === 1 && before.valid === 0, JSON.stringify(before))
  await shot('production-22-annotation-orphan.png')
  for (const mode of ['light', 'dark']) {
    const args = ['tools/audit-text.mjs']
    if (mode === 'dark') args.push('--dark')
    try {
      const output = execFileSync('node', args, { cwd: path.resolve('.'), encoding: 'utf8' })
      console.log(JSON.stringify({ audit: mode, ok: true, out: output.trim().split(String.fromCharCode(10)).slice(-3) }))
    } catch (error) {
      console.log(JSON.stringify({ audit: mode, ok: false, out: String(error.stdout || error.message).trim().split(String.fromCharCode(10)).slice(-8) }))
    }
  }
  await evaluate(`document.querySelector('.anno-orphan-head button')?.click()`)
  await sleep(900)
  const after = await evaluate(`(() => ({
    orphan: document.querySelectorAll('.anno.is-orphan').length,
    valid: document.querySelectorAll('.anno:not(.is-orphan)').length,
    notice: document.querySelector('.anno-relocate-note')?.textContent?.replace(/\s+/g,' ').trim() || '',
  }))()`)
  const stored = await invoke(evaluate, 'list_annotations', { noteId: note.id })
  check('annotation was re-anchored', after.orphan === 0 && after.valid === 1 && stored[0]?.blockId !== 'old-block', JSON.stringify({ after, stored }))
  await shot('production-23-annotation-relocated.png')
  check('relocation result is reported', after.notice.includes('已重新定位 1'), after.notice)
} catch (error) {
  console.error('verification aborted:', error)
  process.exitCode = 1
} finally {
  if (annotationId) await invoke(evaluate, 'delete_annotation', { id: annotationId }).catch(() => {})
  if (originalVault) await invoke(evaluate, 'set_vault', { path: originalVault }).catch(() => {})
  fs.rmSync(dir, { recursive: true, force: true })
  cdp.close()
  const failed = results.filter((item) => !item.ok)
  console.log(`\n${results.length - failed.length}/${results.length} checks passed`)
  if (failed.length) process.exitCode = 1
}
