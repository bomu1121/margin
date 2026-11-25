import workerSource from './pluginWorker.js?raw'

import * as api from './api'
import type { PluginCommand, PluginInfo, PluginRunResult } from '../types'

const RUN_TIMEOUT_MS = 12_000
const MAX_RESULT_CHARS = 200_000

interface WorkerRpc {
  type: 'rpc'
  id: number
  method: string
  params: Record<string, unknown>
}

export interface PluginContext {
  noteId: string
  noteTitle: string
}

/** Run one command in a fresh Worker. The Worker is terminated on success,
 * error or timeout; plugins never share state between runs. */
export async function runPluginCommand(
  plugin: PluginInfo,
  command: PluginCommand,
  context: PluginContext,
): Promise<PluginRunResult> {
  // The caller passes a Vue reactive object. Clone once so no Proxy leaks into
  // checks or messages (structured clone of a Proxy is not reliable).
  const info = JSON.parse(JSON.stringify(plugin)) as PluginInfo
  const cmd = info.commands.find((item) => item.id === command.id)
  if (!info.enabled) throw new Error('插件未启用')
  if (info.error) throw new Error(info.error)
  if (info.needsPermission) throw new Error('插件缺少已授权的权限，请先在插件页重新授权')
  if (!cmd) throw new Error('插件没有声明这个命令')
  const canRead = info.grantedPermissions.includes('notes:read')
  const source = await api.pluginSource(info.id)
  if (!source.trim()) throw new Error('插件入口为空')
  const settings = { ...(info.settingsValues || {}) }

  const workerCode = `${workerSource}\n//# sourceURL=plugin-${info.id}.js\n${source}\nself.postMessage({ type: 'ready' })\n`
  const blob = new Blob([workerCode], { type: 'text/javascript' })
  const url = URL.createObjectURL(blob)
  const worker = new Worker(url)

  try {
    return await new Promise<PluginRunResult>((resolve, reject) => {
      let settled = false
      const finish = (error: Error | null, result?: PluginRunResult) => {
        if (settled) return
        settled = true
        window.clearTimeout(timer)
        if (error) reject(error)
        else resolve(result ?? {})
      }
      const timer = window.setTimeout(
        () => finish(new Error(`插件运行超过 ${RUN_TIMEOUT_MS / 1000} 秒，已终止`)),
        RUN_TIMEOUT_MS,
      )

      worker.addEventListener('message', (event: MessageEvent) => {
        const message = event.data as
          | { type: 'ready' }
          | { type: 'result'; ok: boolean; data?: unknown; error?: string }
          | WorkerRpc
        if (!message || typeof message !== 'object') return
        if (message.type === 'ready') {
          worker.postMessage({
            type: 'run',
            commandId: cmd.id,
            context,
            settings,
          })
          return
        }
        if (message.type === 'rpc') {
          void handleRpc(message, canRead)
            .then((data) => worker.postMessage({ type: 'rpc-result', id: message.id, ok: true, data }))
            .catch((error: unknown) => {
              worker.postMessage({
                type: 'rpc-result',
                id: message.id,
                ok: false,
                error: api.errorText(error),
              })
            })
          return
        }
        if (message.type === 'result') {
          if (!message.ok) {
            finish(new Error(message.error || '插件运行失败'))
            return
          }
          finish(null, normalizeResult(message.data))
        }
      })
      worker.addEventListener('error', (event) => {
        finish(new Error(event.message || '插件 Worker 出错'))
      })
      worker.addEventListener('messageerror', () => finish(new Error('插件返回了无法读取的数据')))
    })
  } finally {
    worker.terminate()
    URL.revokeObjectURL(url)
  }
}

async function handleRpc(message: WorkerRpc, canRead: boolean): Promise<unknown> {
  if (message.method === 'notes.read') {
    if (!canRead) throw new Error('插件没有获得 notes:read 权限')
    const noteId = String(message.params?.noteId ?? '')
    if (!noteId) throw new Error('notes.read 需要 noteId')
    const note = await api.getNote(noteId)
    if (!note) throw new Error('笔记不存在')
    return { id: note.meta.id, title: note.meta.title, body: note.body }
  }
  throw new Error(`不支持的插件 API：${message.method}`)
}

function normalizeResult(data: unknown): PluginRunResult {
  if (typeof data === 'string') return { message: data.slice(0, MAX_RESULT_CHARS) }
  if (data && typeof data === 'object') {
    const record = data as Record<string, unknown>
    const markdown = typeof record.markdown === 'string' ? record.markdown : ''
    const message = typeof record.message === 'string' ? record.message : ''
    if (markdown || message) {
      return {
        markdown: markdown.slice(0, MAX_RESULT_CHARS),
        message: message.slice(0, MAX_RESULT_CHARS),
      }
    }
    return { message: JSON.stringify(data, null, 2).slice(0, MAX_RESULT_CHARS) }
  }
  return { message: String(data ?? '') }
}
