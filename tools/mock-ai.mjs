// Local OpenAI-compatible mock used by the production verification scripts.
// It never talks to a real provider and logs every call so retry/resume
// assertions can compare request counts instead of guessing.
//
// CLI: node tools/mock-ai.mjs [--port 18090] [--chat-fail-at 2] [--asr-fail-at 2] [--log file]
import fs from 'node:fs'
import http from 'node:http'
import { pathToFileURL } from 'node:url'

export async function startMockAi(options = {}) {
  const {
    port = 0,
    chatFailAt = 0,
    asrFailAt = 0,
    delayMs = 0,
    logPath = '',
    promptTokens = 1_000_000,
    completionTokens = 1_000_000,
  } = options
  let chatCalls = 0
  let asrCalls = 0
  const log = []

  const server = http.createServer((req, res) => {
    let body = ''
    req.on('data', (chunk) => { body += chunk })
    req.on('end', async () => {
      const isAsr = req.url?.includes('/audio/transcriptions') || body.includes('input_audio')
      const kind = isAsr ? 'asr' : 'chat'
      const n = isAsr ? ++asrCalls : ++chatCalls
      const failAt = isAsr ? asrFailAt : chatFailAt
      const failed = failAt > 0 && n === failAt
      if (delayMs > 0) await new Promise((resolve) => setTimeout(resolve, delayMs))
      const entry = {
        kind, call: n, status: failed ? 500 : 200,
        promptTokens: isAsr ? 0 : promptTokens,
        completionTokens: isAsr ? 0 : completionTokens,
      }
      log.push(entry)
      if (logPath) fs.appendFileSync(logPath, `${JSON.stringify(entry)}\n`)
      if (failed) {
        res.writeHead(500, { 'content-type': 'application/json' })
        res.end(JSON.stringify({ error: { message: `injected ${kind} failure on call ${n}` } }))
        return
      }
      res.writeHead(200, { 'content-type': 'application/json' })
      if (isAsr) {
        if (req.url?.includes('/audio/transcriptions')) {
          res.end(JSON.stringify({ text: `第${n}段转写文本` }))
        } else {
          res.end(JSON.stringify({ choices: [{ message: { content: `第${n}段转写文本` } }] }))
        }
        return
      }
      const content = body.includes('信息抽取器')
        ? '{"公司":["字节跳动"],"概念":["Agent"]}'
        : `# 验证笔记\n\n## 正文\n\n这是第 ${n} 次 mock 生成的正文。`
      res.end(JSON.stringify({
        choices: [{ message: { content } }],
        usage: { prompt_tokens: promptTokens, completion_tokens: completionTokens },
      }))
    })
  })
  await new Promise((resolve) => server.listen(port, '127.0.0.1', resolve))
  const address = server.address()
  return {
    port: typeof address === 'object' && address ? address.port : port,
    log,
    chatCalls: () => chatCalls,
    asrCalls: () => asrCalls,
    close: () => new Promise((resolve) => server.close(resolve)),
  }
}

if (import.meta.url === pathToFileURL(process.argv[1]).href) {
  const args = process.argv.slice(2)
  const value = (name, fallback) => {
    const index = args.indexOf(name)
    return index >= 0 ? args[index + 1] : fallback
  }
  const server = await startMockAi({
    port: Number(value('--port', 18090)),
    chatFailAt: Number(value('--chat-fail-at', 0)),
    asrFailAt: Number(value('--asr-fail-at', 0)),
    delayMs: Number(value('--delay-ms', 0)),
    logPath: value('--log', ''),
  })
  console.log(JSON.stringify({ port: server.port }))
  process.on('SIGINT', async () => { await server.close(); process.exit(0) })
  process.on('SIGTERM', async () => { await server.close(); process.exit(0) })
}
