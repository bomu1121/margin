// End-to-end resume verification against the running packaged app.
// Prereq: app running with --remote-debugging-port=9222 and a saved DeepSeek key.
// usage: node tools/verify-resume.mjs
import { execFileSync } from 'node:child_process'
import fs from 'node:fs'
import path from 'node:path'
import { connectCdp, invoke } from './lib/cdp.mjs'
import { check, createHarness, finish, waitJob } from './lib/production-verify.mjs'
import { startMockAi } from './mock-ai.mjs'

const results = []
const cdp = await connectCdp()
const evaluate = cdp.evaluate

async function textResume() {
  const mock = await startMockAi({ chatFailAt: 2 })
  const harness = await createHarness(evaluate, 'verify-resume-model')
  try {
    await harness.configure(`http://127.0.0.1:${mock.port}/v1`)
    await harness.setPrice()
    const job = await harness.startText('verify-resume-text')
    const first = await waitJob(evaluate, job.id, ['error'])
    check(results, 'text failure status', first.job.status === 'error')
    check(results, 'failed usage is persisted', first.job.usage.promptTokens === 1_000_000 && first.job.usage.completionTokens === 1_000_000 && Math.abs(first.job.usage.estimatedCost - 40) < 1e-9, JSON.stringify(first.job.usage))
    const info = await invoke(evaluate, 'production_resume_info', { id: job.id })
    check(results, 'text has resumable checkpoint', info.canResume && info.checkpointCount > 0, JSON.stringify(info))
    await invoke(evaluate, 'production_resume', { id: job.id })
    const done = await waitJob(evaluate, job.id, ['success', 'error'])
    check(results, 'text resume succeeds', done.job.status === 'success', done.job.error)
    check(results, 'resume only repeats the missing chat', mock.chatCalls() === 3, `chatCalls=${mock.chatCalls()}`)
    check(results, 'resume final usage is 2M/2M ¥80', done.job.usage.promptTokens === 2_000_000 && done.job.usage.completionTokens === 2_000_000 && Math.abs(done.job.usage.estimatedCost - 80) < 1e-9, JSON.stringify(done.job.usage))
    check(results, 'text resume output exists', fs.existsSync(path.join(harness.dir, done.job.noteRelPath)), done.job.noteRelPath)
  } finally {
    const before = harness.originalJobCount
    await harness.cleanup()
    const after = (await invoke(evaluate, 'production_list')).length
    check(results, 'text verification restored job list', after === before, `${before} -> ${after}`)
    await mock.close()
  }
}

async function fullRetry() {
  const mock = await startMockAi({ chatFailAt: 2 })
  const harness = await createHarness(evaluate, 'verify-retry-model')
  try {
    await harness.configure(`http://127.0.0.1:${mock.port}/v1`)
    await harness.setPrice()
    const job = await harness.startText('verify-retry-text')
    await waitJob(evaluate, job.id, ['error'])
    await invoke(evaluate, 'production_retry', { id: job.id })
    const done = await waitJob(evaluate, job.id, ['success', 'error'])
    check(results, 'full retry succeeds', done.job.status === 'success', done.job.error)
    check(results, 'full retry starts from call 1', mock.chatCalls() === 4, `chatCalls=${mock.chatCalls()}`)
    check(results, 'full retry final usage is 2M/2M ¥80', done.job.usage.promptTokens === 2_000_000 && done.job.usage.completionTokens === 2_000_000 && Math.abs(done.job.usage.estimatedCost - 80) < 1e-9, JSON.stringify(done.job.usage))
  } finally {
    const before = harness.originalJobCount
    await harness.cleanup()
    const after = (await invoke(evaluate, 'production_list')).length
    check(results, 'full-retry verification restored job list', after === before, `${before} -> ${after}`)
    await mock.close()
  }
}

async function asrResume() {
  const settings = await invoke(evaluate, 'production_settings')
  if (!settings.hasAsrKey) {
    console.log('SKIP  ASR resume  no saved ASR key')
    return
  }
  const mock = await startMockAi({ asrFailAt: 2 })
  const harness = await createHarness(evaluate, 'verify-asr-model')
  try {
    await harness.configure(`http://127.0.0.1:${mock.port}/v1`, true)
    await harness.setPrice()
    const audio = path.join(harness.dir, 'long.mp3')
    const ffmpeg = process.env.FFMPEG_PATH || 'ffmpeg'
    execFileSync(ffmpeg, ['-hide_banner', '-loglevel', 'error', '-y', '-f', 'lavfi', '-i', 'sine=frequency=440:sample_rate=16000', '-t', '720', '-ac', '1', '-c:a', 'libmp3lame', '-b:a', '48k', audio], { stdio: 'inherit' })
    check(results, 'ASR fixture is longer than one MiMo segment', fs.statSync(audio).size > 4_000_000, `${fs.statSync(audio).size} B`)
    const job = await harness.startFile('verify-asr-text', audio)
    const first = await waitJob(evaluate, job.id, ['error'])
    const info = await invoke(evaluate, 'production_resume_info', { id: job.id })
    check(results, 'ASR failure leaves a segment checkpoint', first.job.status === 'error' && info.canResume && info.steps.some((step) => step.key === 'transcribe' && step.units >= 1), JSON.stringify(info))
    await invoke(evaluate, 'production_resume', { id: job.id })
    const done = await waitJob(evaluate, job.id, ['success', 'error'])
    check(results, 'ASR resume succeeds', done.job.status === 'success', done.job.error)
    check(results, 'ASR retries only the failed segment', mock.asrCalls() === 3, `asrCalls=${mock.asrCalls()}`)
    check(results, 'ASR downstream chats run once', mock.chatCalls() === 2, `chatCalls=${mock.chatCalls()}`)
  } finally {
    const before = harness.originalJobCount
    await harness.cleanup()
    const after = (await invoke(evaluate, 'production_list')).length
    check(results, 'ASR verification restored job list', after === before, `${before} -> ${after}`)
    await mock.close()
  }
}

try {
  await textResume()
  await fullRetry()
  await asrResume()
} catch (error) {
  console.error('verification aborted:', error)
  process.exitCode = 1
} finally {
  cdp.close()
  finish(results)
}
