import fs from 'node:fs'
import os from 'node:os'
import path from 'node:path'
import { invoke, sleep } from './cdp.mjs'

export function check(results, name, ok, detail = '') {
  results.push({ name, ok, detail })
  console.log(`${ok ? 'PASS' : 'FAIL'}  ${name}${detail ? `  ${detail}` : ''}`)
}

export function finish(results) {
  const failed = results.filter((item) => !item.ok)
  console.log(`\n${results.length - failed.length}/${results.length} checks passed`)
  if (failed.length) process.exitCode = 1
}

export async function waitJob(evaluate, id, statuses, timeoutMs = 30000) {
  const deadline = Date.now() + timeoutMs
  let last = null
  while (Date.now() < deadline) {
    last = await invoke(evaluate, 'production_get', { id })
    if (last?.job && statuses.includes(last.job.status)) return last
    await sleep(300)
  }
  throw new Error(`job ${id} did not reach ${statuses.join('/')}; last=${JSON.stringify(last?.job)}`)
}

export async function createHarness(evaluate, model = 'verify-model') {
  const originalVault = await invoke(evaluate, 'get_vault')
  const originalSettings = await invoke(evaluate, 'production_settings')
  const originalJobCount = (await invoke(evaluate, 'production_list')).length
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'yebian-verify-'))
  await invoke(evaluate, 'set_vault', { path: dir.replaceAll('\\', '/') })
  const jobs = []
  let priced = false

  async function configure(baseUrl, asr = false) {
    const input = {
      aiProvider: 'deepseek',
      aiProtocol: 'openai',
      aiBaseUrl: baseUrl,
      aiModel: model,
      retries: 0,
    }
    if (asr) {
      input.asrEngine = 'mimo'
      input.asrBaseUrl = baseUrl
      input.asrModel = 'mimo-v2.5-asr'
    }
    await invoke(evaluate, 'production_save_settings', { input })
    const settings = await invoke(evaluate, 'production_settings')
    if (!settings.hasAiKey) throw new Error('verification needs a saved DeepSeek API key')
    return settings
  }

  async function setPrice() {
    await invoke(evaluate, 'production_set_model_price', {
      provider: 'deepseek', model, inputCny: 10, outputCny: 30,
    })
    priced = true
  }

  async function startText(title, fields = []) {
    const job = await invoke(evaluate, 'production_start', {
      request: {
        templateId: 'text-note',
        title,
        source: { kind: 'text', text: `验证素材：${title}` },
        options: { prompt: '成文版', fields, split: true },
      },
    })
    jobs.push(job.id)
    return job
  }

  async function startFile(title, file) {
    const job = await invoke(evaluate, 'production_start', {
      request: {
        templateId: 'video-note',
        title,
        source: { kind: 'file', path: file.replaceAll('\\', '/') },
        options: { prompt: '成文版', fields: [], split: true },
      },
    })
    jobs.push(job.id)
    return job
  }

  async function cleanup() {
    for (const id of jobs) {
      await invoke(evaluate, 'production_delete', { id }).catch(() => {})
    }
    if (priced) {
      await invoke(evaluate, 'production_delete_model_price', { provider: 'deepseek', model }).catch(() => {})
    }
    if (originalVault) {
      await invoke(evaluate, 'set_vault', { path: originalVault }).catch(() => {})
    }
    if (originalSettings) {
      await invoke(evaluate, 'production_save_settings', {
        input: {
          aiProvider: originalSettings.aiProvider,
          aiProtocol: originalSettings.aiProtocol,
          aiBaseUrl: originalSettings.aiBaseUrl,
          aiModel: originalSettings.aiModel,
          retries: originalSettings.retries,
          asrEngine: originalSettings.asrEngine,
          asrBaseUrl: originalSettings.asrBaseUrl,
          asrModel: originalSettings.asrModel,
        },
      }).catch(() => {})
    }
    fs.rmSync(dir, { recursive: true, force: true })
    return originalJobCount
  }

  return { dir, originalVault, originalSettings, originalJobCount, configure, setPrice, startText, startFile, cleanup }
}
