// Print a built booklet to A4 PDF through Edge/Chrome headless, then report
// page count and file size. No extra dependencies.
//
// usage: node tools/booklet/print.mjs <file.html> <out.pdf>
import { spawn } from 'node:child_process'
import { existsSync, mkdtempSync, readFileSync, statSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'
import { pathToFileURL } from 'node:url'

const [file, out] = process.argv.slice(2)
if (!file || !out) {
  console.error('usage: node tools/booklet/print.mjs <file.html> <out.pdf>')
  process.exit(1)
}

const candidates = [
  process.env.BOOK_BROWSER,
  'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe',
  'C:/Program Files/Google/Chrome/Application/chrome.exe',
  'C:/Program Files (x86)/Google/Chrome/Application/chrome.exe',
].filter(Boolean)
const browser = candidates.find((path) => existsSync(path))
if (!browser) {
  console.error('no Edge/Chrome found; set BOOK_BROWSER')
  process.exit(1)
}

const profile = mkdtempSync(join(tmpdir(), 'yebian-print-'))
const target = resolve(out)
const code = await new Promise((resolveCode, reject) => {
  const child = spawn(
    browser,
    [
      '--headless=new',
      '--disable-gpu',
      '--allow-file-access-from-files',
      '--no-pdf-header-footer',
      '--print-to-pdf-no-header',
      '--virtual-time-budget=8000',
      `--user-data-dir=${profile}`,
      `--print-to-pdf=${target}`,
      pathToFileURL(file).href,
    ],
    { stdio: ['ignore', 'ignore', 'pipe'] },
  )
  let stderr = ''
  child.stderr.on('data', (chunk) => {
    stderr += String(chunk)
  })
  child.on('error', reject)
  child.on('exit', (exitCode) => {
    if (exitCode !== 0) console.error(stderr.slice(0, 400))
    resolveCode(exitCode)
  })
})
if (code !== 0) process.exit(code)

const bytes = readFileSync(target)
const pages = (bytes.toString('latin1').match(/\/Type\s*\/Page[^s]/g) || []).length
console.log(
  JSON.stringify({ pdf: target, bytes: statSync(target).size, pages }, null, 1),
)
