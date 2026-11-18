// Probe the experimental HTML-author booklet chain on the Super Agent
// transcript. Requires the dev app on CDP 9222 and a saved AI key.
// usage: node tools/probe-html-booklet.mjs
import fs from 'node:fs'
import path from 'node:path'

const transcriptPath = path.resolve('docs/research/super-agent/TRANSCRIPT.md')
if (!fs.existsSync(transcriptPath)) throw new Error(`missing ${transcriptPath}`)
const text = fs.readFileSync(transcriptPath, 'utf8')

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
async function evaluate(expression) {
  const result = await send('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true })
  if (result.exceptionDetails) throw new Error(result.exceptionDetails.text || 'page error')
  return result.result?.value
}
const invoke = (command, args = {}) =>
  evaluate(`window.__TAURI_INTERNALS__.invoke(${JSON.stringify(command)}, ${JSON.stringify(args)})`)

const request = {
  templateId: 'html-booklet',
  title: 'Super Agent · HTML 作者版',
  source: { kind: 'text', text, path: '', url: '', videos: [], pages: [] },
  options: {
    prompt: '成文版',
    tags: [],
    fields: [],
    split: true,
    exportMarkdown: false,
    chapter: 'medium',
    maxChapters: 20,
    traceProvider: 'deepseek',
    traceMax: 5,
    traceClaims: 10,
    drillCount: 6,
    mindDepth: 3,
    styles: ['成文版', '速览版'],
    destination: 'folder',
    folder: '',
    agent: false,
  },
}
const resumeId = process.argv[2]
const job = resumeId
  ? await invoke('production_resume', { id: resumeId })
  : await invoke('production_start', { request })
console.log('job', job.id, 'steps', job.steps.map((step) => step.key).join(' > '))
const deadline = Date.now() + 30 * 60 * 1000
let lastLine = ''
let lastLogAt = 0
while (Date.now() < deadline) {
  const detail = await invoke('production_get', { id: job.id })
  if (!detail) throw new Error('job disappeared')
  const current = detail.job
  const steps = current.steps.map((step) => `${step.key}:${step.status}`).join(' ')
  const line = `${current.status} ${current.progress}% ${current.currentStep} | ${steps}`
  if (line !== lastLine) {
    console.log(new Date().toISOString().slice(11, 19), line)
    lastLine = line
  }
  if (current.currentStep.includes('HTML 作者') && Date.now() - lastLogAt > 15000) {
    const recent = detail.logs.filter((log) => log.message.startsWith('#')).slice(-3)
    if (recent.length) console.log('  ', recent.map((log) => log.message.slice(0, 180)).join('\n   '))
    lastLogAt = Date.now()
  }
  if (['success', 'error', 'cancelled', 'interrupted'].includes(current.status)) {
    console.log('note', current.noteRelPath)
    console.log('usage', JSON.stringify(current.usage))
    console.log('error', current.error)
    for (const step of current.steps) console.log('step', step.key, step.status, step.detail)
    const graph = await invoke('production_run_graph', { id: job.id })
    console.log('nodes', graph.nodes.map((node) => `${node.key}:${node.status}:${node.tokens}tok`).join(' | '))
    console.log('artifacts', graph.artifacts.map((item) => `${item.nodeKey}/${item.name}`).join(' | '))
    break
  }
  await new Promise((resolve) => setTimeout(resolve, 3000))
}
ws.close()
