// Worker runtime injected by the host. Plugin code is appended after this file
// and must only register handlers on `margin.commands`; it has no DOM, Tauri
// IPC or Node access. Every capability crosses the RPC bridge below.

const pending = new Map()
let nextId = 0

function callHost(method, params) {
  return new Promise((resolve, reject) => {
    const id = ++nextId
    pending.set(id, { resolve, reject })
    self.postMessage({ type: 'rpc', id, method, params })
  })
}

const margin = {
  commands: {},
  settings: {},
  notes: {
    read(noteId) {
      return callHost('notes.read', { noteId })
    },
  },
}

self.margin = margin

// v1 is offline and single-file. Override the network/loading primitives before
// plugin code runs; CSP is a second line of defence, not the only one.
function denyNetwork() {
  return Promise.reject(new Error('插件 v1 不允许联网'))
}
function lockGlobal(name, value) {
  try {
    Object.defineProperty(self, name, { configurable: true, writable: true, value })
  } catch {
    // Some WebView2 builds expose read-only globals; CSP still blocks requests.
  }
}
lockGlobal('fetch', denyNetwork)
lockGlobal('XMLHttpRequest', class {
  open() {
    throw new Error('插件 v1 不允许联网')
  }
})
lockGlobal('WebSocket', class {
  constructor() {
    throw new Error('插件 v1 不允许联网')
  }
})
lockGlobal('EventSource', class {
  constructor() {
    throw new Error('插件 v1 不允许联网')
  }
})
lockGlobal('importScripts', () => {
  throw new Error('插件 v1 只支持单文件入口')
})
lockGlobal('Worker', class {
  constructor() {
    throw new Error('插件 v1 不允许创建子 Worker')
  }
})
lockGlobal('SharedWorker', class {
  constructor() {
    throw new Error('插件 v1 不允许创建子 Worker')
  }
})

self.addEventListener('message', async (event) => {
  const message = event.data
  if (!message || typeof message !== 'object') return

  if (message.type === 'rpc-result') {
    const entry = pending.get(message.id)
    if (!entry) return
    pending.delete(message.id)
    if (message.ok) entry.resolve(message.data)
    else entry.reject(new Error(message.error || '宿主拒绝请求'))
    return
  }

  if (message.type === 'run') {
    margin.settings = message.settings && typeof message.settings === 'object' ? message.settings : {}
    const command = margin.commands && margin.commands[message.commandId]
    if (typeof command !== 'function') {
      self.postMessage({ type: 'result', ok: false, error: '插件没有注册命令：' + message.commandId })
      return
    }
    try {
      const data = await command(message.context || {})
      self.postMessage({ type: 'result', ok: true, data })
    } catch (error) {
      const detail = error && error.stack ? String(error.stack) : String(error)
      self.postMessage({ type: 'result', ok: false, error: detail })
    }
  }
})
