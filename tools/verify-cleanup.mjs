// Cleanup semantics: failed/resumable jobs must not be removed by
// "clear finished"; clear_temp must keep their temp dir; explicit clear removes.
// usage: node tools/verify-cleanup.mjs (app with CDP on 9222, saved DeepSeek key)
import fs from 'node:fs'
import path from 'node:path'
import { connectCdp, invoke } from './lib/cdp.mjs'
import { check, createHarness, finish, waitJob } from './lib/production-verify.mjs'
import { startMockAi } from './mock-ai.mjs'

const results = []
const cdp = await connectCdp()
const evaluate = cdp.evaluate
const mock = await startMockAi({ chatFailAt: 2 })
const harness = await createHarness(evaluate, 'verify-cleanup-model')
try {
  await harness.configure(`http://127.0.0.1:${mock.port}/v1`)
  await harness.setPrice()
  const job = await harness.startText('verify-cleanup-text')
  await waitJob(evaluate, job.id, ['error'])
  const info = await invoke(evaluate, 'production_resume_info', { id: job.id })
  check(results, 'failed job has checkpoint before cleanup', info.canResume && info.checkpointCount > 0, JSON.stringify(info))

  await invoke(evaluate, 'production_clear_finished')
  const stillThere = await invoke(evaluate, 'production_get', { id: job.id })
  check(results, 'clear finished keeps failed job', stillThere?.job.status === 'error')

  const temp = await invoke(evaluate, 'production_clear_temp')
  check(results, 'clear temp keeps checkpoint job', temp.keptJobs >= 1, JSON.stringify(temp))
  check(results, 'clear temp reports stats', Number.isFinite(temp.clearedBytes) && Number.isFinite(temp.keptBytes), JSON.stringify(temp))

  const removed = await invoke(evaluate, 'production_clear_resumable')
  const gone = await invoke(evaluate, 'production_get', { id: job.id })
  check(results, 'explicit clear removes failed job', removed >= 1 && gone === null, JSON.stringify({ removed, gone }))
} catch (error) {
  console.error('verification aborted:', error)
  process.exitCode = 1
} finally {
  const before = harness.originalJobCount
  await harness.cleanup()
  const after = (await invoke(evaluate, 'production_list')).length
  check(results, 'cleanup verification restored job list', after === before, `${before} -> ${after}`)
  await mock.close()
  cdp.close()
  finish(results)
}
