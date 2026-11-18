// End-to-end probe for the opt-in production agent step.
// Prereq: dev app running with CDP on 9222 and a saved AI key.
// Starts a small text booklet job (agent=true), then polls the job and the
// pending agent task until the job reaches a terminal status.
const TEXT = `把智能体接进制作台，最关键的不是模型能力，而是它在流程里的位置。

第一，制作任务是一条可恢复的流水线：来源、转写、校对、加工、入库，每一步都有状态、产物和重试。智能体如果只是一个独立按钮，就不在这条链路上，失败不会留下步骤，成功也不会进入产物。

第二，智能体真正需要的工具是看版面。文本模型只能读到 Markdown，读不到溢出、越界、空框和标签重叠。做法是在应用内用同一份渲染器构建 HTML，再用无头浏览器测量，把几何问题变成结构化指标，模型每轮只改一处。

第三，这样做有一个硬约束：Rust 侧没有 Markdown 到 HTML 的小册子构建器。解决办法不是复制渲染器，而是让流水线在组装之后停住，把草稿交给窗口内的执行器，执行器跑完把草稿交回，流水线继续入库。

这条链路的价值在于：同样的运行时既能手动跑，也能作为任务选项自动跑；失败可以重试，产物可以追溯，成本进入任务账本。`

const list = await (await fetch('http://127.0.0.1:9222/json/list')).json()
const page = list.find((item) => item.type === 'page')
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
  templateId: 'booklet',
  title: '智能体制作选项验证 H',
  source: { kind: 'text', text: TEXT, path: '', url: '', videos: [], pages: [] },
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
    agent: true,
  },
}
const job = await invoke('production_start', { request })
console.log('job', job.id, 'steps', job.steps.map((step) => step.key).join(' > '))
const deadline = Date.now() + 8 * 60 * 1000
let lastLine = ''
while (Date.now() < deadline) {
  const detail = await invoke('production_get', { id: job.id })
  if (!detail) throw new Error('job disappeared')
  const current = detail.job
  const steps = current.steps.map((step) => `${step.key}:${step.status}`).join(' ')
  const task = await invoke('production_agent_pending')
  const line = `${current.status} ${current.progress}% ${current.currentStep} | ${steps}${task ? ` | pending ${task.id}` : ''}`
  if (line !== lastLine) {
    console.log(new Date().toISOString().slice(11, 19), line)
    lastLine = line
  }
  if (['success', 'error', 'cancelled', 'interrupted'].includes(current.status)) {
    console.log('note', current.noteRelPath)
    console.log('usage', JSON.stringify(current.usage))
    console.log('error', current.error)
    const agent = current.steps.find((step) => step.key === 'agent')
    console.log('agentStep', JSON.stringify(agent))
    break
  }
  await new Promise((resolve) => setTimeout(resolve, 2500))
}
ws.close()
