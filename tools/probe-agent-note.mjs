// Experiment probe for agent-note: starts one job with a given behavior
// variant, captures the note / agent trajectory / usage into an output dir.
//
// usage: node tools/probe-agent-note.mjs <material> <variant> <outdir>
import fs from 'node:fs'
import path from 'node:path'

const materialPath = process.argv[2] || path.join(process.env.TEMP || '.', 'super-agent-eval', 'material.md')
const variant = process.argv[3] || 'minimal'
const outDir = process.argv[4] || path.join(process.env.TEMP || '.', 'super-agent-eval', `${variant}-${Date.now()}`)
const material = fs.readFileSync(materialPath, 'utf8')
fs.mkdirSync(outDir, { recursive: true })

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
  templateId: 'agent-note',
  title: `超智能体对照 · ${variant}`,
  source: { kind: 'text', text: material, path: '', url: '', videos: [], pages: [] },
  options: {
    prompt: variant,
    tags: [],
    fields: [],
    split: false,
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
    folder: '待整理',
    agent: false,
  },
}
const started = Date.now()
const job = await invoke('production_start', { request })
console.log('job', job.id, 'variant', variant)

const deadline = Date.now() + 20 * 60 * 1000
let cursor = 0
let lastLine = ''
while (Date.now() < deadline) {
  const detail = await invoke('production_get', { id: job.id })
  if (!detail) throw new Error('job disappeared')
  const current = detail.job
  const steps = current.steps.map((step) => `${step.key}:${step.status}`).join(' ')
  const line = `${current.status} ${current.progress}% ${current.currentStep || ''} | ${steps}`
  if (line !== lastLine) {
    console.log(new Date().toISOString().slice(11, 19), line)
    lastLine = line
  }
  try {
    const snapshot = await invoke('production_run_snapshot', { id: job.id, afterLogId: cursor })
    for (const log of snapshot.logs || []) {
      if (log.message.startsWith('#') || log.message.includes('智能体')) {
        console.log(`  [${log.stepIndex ?? '-'}] ${log.message.slice(0, 160)}`)
      }
    }
    if (snapshot.lastLogId) cursor = snapshot.lastLogId
  } catch {
    /* best-effort */
  }
  if (['success', 'error', 'cancelled', 'interrupted'].includes(current.status)) {
    const graph = await invoke('production_run_graph', { id: job.id })
    const noteArtifact =
      graph.artifacts.find((item) => item.name === 'note.md' && item.nodeKey === 'publish') ||
      graph.artifacts.find((item) => item.name === 'note.md')
    const runArtifact = graph.artifacts.find((item) => item.name === 'agent-run.json')
    let note = ''
    let run = null
    if (noteArtifact) {
      note = await invoke('production_artifact_text', { id: noteArtifact.id })
      fs.writeFileSync(path.join(outDir, 'note.md'), note)
    }
    if (runArtifact) {
      const raw = await invoke('production_artifact_text', { id: runArtifact.id })
      run = JSON.parse(raw)
      fs.writeFileSync(path.join(outDir, 'agent-run.json'), raw)
    }
    const summary = {
      variant,
      jobId: job.id,
      status: current.status,
      error: current.error,
      durationMs: Date.now() - started,
      notePath: current.noteRelPath,
      usage: current.usage,
      steps: current.steps.map((step) => ({ key: step.key, status: step.status, detail: step.detail })),
      nodes: graph.nodes.map((node) => ({ key: node.key, status: node.status, tokens: node.tokens, cost: node.cost })),
      artifacts: graph.artifacts.map((item) => ({ node: item.nodeKey, name: item.name, bytes: item.bytes })),
      note: note
        ? {
            bytes: Buffer.byteLength(note),
            cjk: (note.match(/[\u4e00-\u9fff]/g) ?? []).length,
            h1: (note.match(/^# /gm) ?? []).length,
            h2: (note.match(/^## /gm) ?? []).length,
            tables: (note.match(/^\|/gm) ?? []).length,
            figures: (note.match(/^```figure/gm) ?? []).length,
            headings: (note.match(/^## .*/gm) ?? []).map((line) => line.slice(3).trim()),
          }
        : null,
      run: run
        ? {
            status: run.status,
            usage: run.usage,
            draftChars: (run.draft || '').length,
            todos: run.todos ?? [],
            turns: (run.turns ?? []).map((turn) => ({
              index: turn.index,
              tool: turn.tool ?? turn.phase,
              modelMs: turn.modelMs ?? 0,
              usage: turn.usage ?? null,
              observation: (turn.observation ?? turn.note ?? '').slice(0, 300),
              reasoningChars: (turn.reasoning ?? '').length,
            })),
          }
        : null,
    }
    fs.writeFileSync(path.join(outDir, 'summary.json'), JSON.stringify(summary, null, 2))
    console.log('saved', outDir, JSON.stringify(summary.note))
    break
  }
  await new Promise((resolve) => setTimeout(resolve, 1500))
}
ws.close()
