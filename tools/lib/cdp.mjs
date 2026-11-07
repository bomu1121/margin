// Minimal CDP client shared by verification scripts.
// The app must be running with --remote-debugging-port=9222.

export async function connectCdp(port = 9222) {
  const list = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json()
  const page = list.find((item) => item.type === 'page')
  if (!page) throw new Error(`no CDP page target on ${port}`)
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
    if (result.exceptionDetails) throw new Error(`page error: ${JSON.stringify(result.exceptionDetails)}`)
    return result.result?.value
  }
  return { send, evaluate, close: () => ws.close() }
}

export const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms))

export async function invoke(evaluate, command, args = {}) {
  return evaluate(`window.__TAURI_INTERNALS__.invoke(${JSON.stringify(command)}, ${JSON.stringify(args)})`)
}
