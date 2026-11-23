// Re-runs pending legacy notes from docs/rerun-queue.json using their original
// Bilibili videos, the conclusion+sections brief and the content-aware tagger.
// usage: node tools/rerun-old-notes.mjs
// App must be running with CDP on YEBian_RERUN_PORT (default 9223).

import fs from 'node:fs'
import path from 'node:path'
import { connectCdp, invoke } from './lib/cdp.mjs'

const port = Number(process.env.YEBian_RERUN_PORT || 9223)
const queuePath = path.resolve('docs/rerun-queue.json')
const queue = JSON.parse(fs.readFileSync(queuePath, 'utf8'))
const pending = Array.isArray(queue.pending) ? queue.pending : []
if (!pending.length) {
  console.log('no pending notes in docs/rerun-queue.json')
  process.exit(0)
}

const cdp = await connectCdp(port)

const BRIEF = `把素材写成一份可直接阅读的笔记。只保留结论和论证小节，不要加速览、关键数字索引或单独的“不确定”章节。

结构：
1. 第一行 # 标题，标题具体，不要写“视频笔记”。
2. 结论段：2–3 句。第一句直接回答；第二句给最强理由；第三句给最大限定。不要用 bullet。
3. 3–5 个 ## 小节。每节只回答一个问题：
   - 标题必须是判断句（有主语和结论），能单独当目录读；不要写“背景/原因/影响/启示”这类空标签。
   - 第一段先给判断，再说它意味着什么/为什么重要。
   - 第二段给素材依据：事实、数字、原话。数字和专名就地出现，不另设索引。
   - 需要时在节末加一句限定/反例；没有就不写，不要机械凑三段。
   - 一节只放一个中心判断；不要把背景、原因、影响拆成多个标签。
4. 小节顺序按论证链，不按视频时间顺序。上一节的结论是下一节的前提；第一节放最关键的机制或反常识点，不要从背景开始。
5. 作者观点写成“作者认为/节目判断”，不要写成客观事实或机构认定；素材用“恐怕/目测/可能”等弱语气时保留弱语气，不要升级成“大概率/必然/已经”。逐字引用前核对主语和否定词，不要把“砍不动”写成“砍得动”，不要漏掉“不/没/未”。遇到【听不清】或转写缺口原样保留，不要补成具体名词；不要补全素材里没有出现的公司全名、机构全名。
6. 不写 YAML，不写“以下是笔记”；不补素材没有的事实、数字、引语、链接。
7. 只有某一节确实需要对比、时间线或流程时，才写一个自包含 HTML 视觉块，并放在那一节里；不要为了配图而配图。
8. 文末按系统要求追加 yebian-claims 清单；只列史载/转述，判断不列；claims 的 quote 必须逐字来自正文和素材，改写句不要放进 quote；找不到逐字句时宁可少列。

不要运行脚本统计字数、比例或检查 Markdown；不要反复重写同一段；写完一稿后只修正事实和结构错误，最多再改 2 轮。`

async function waitJob(id, timeoutMs = 20 * 60 * 1000) {
  const deadline = Date.now() + timeoutMs
  let last = ''
  while (Date.now() < deadline) {
    const detail = await invoke(cdp.evaluate, 'production_get', { id })
    if (!detail?.job) throw new Error(`job ${id} disappeared`)
    if (detail.job.status !== last) {
      console.log(`  ${id} ${detail.job.status} · ${detail.job.currentStep}`)
      last = detail.job.status
    }
    if (['success', 'failed', 'error', 'cancelled'].includes(detail.job.status)) return detail
    await new Promise((resolve) => setTimeout(resolve, 3000))
  }
  throw new Error(`job ${id} timed out`)
}

function frontMatter(text) {
  return text.match(/^---\r?\n([\s\S]*?)\r?\n---/)?.[1] || ''
}

function bodyText(text) {
  return text.replace(/^---\r?\n[\s\S]*?\r?\n---\r?\n?/, '')
}

function listField(front, key) {
  const lines = front.split(/\r?\n/)
  const start = lines.findIndex((line) => line.trim() === `${key}:`)
  if (start < 0) return []
  const out = []
  for (let index = start + 1; index < lines.length; index += 1) {
    const line = lines[index]
    if (!line.startsWith('  ')) break
    const match = line.match(/^\s{2}- "?(.*?)"?\s*$/)
    if (match && match[1].trim()) out.push(match[1].trim())
  }
  return out
}

const before = await invoke(cdp.evaluate, 'production_auto_settings')
if (before.enabled) {
  await invoke(cdp.evaluate, 'production_save_auto_settings', { input: { enabled: false } })
}
console.log('auto paused for rerun batch')

const rows = []
for (const target of pending) {
  console.log(`\n## ${target.title}`)
  const request = {
    templateId: 'ai-note',
    title: target.title,
    source: { kind: 'bili', url: target.url, text: '', path: '', videos: [], pages: [] },
    options: {
      prompt: '重跑待审',
      tags: [],
      fields: [],
      split: false,
      exportMarkdown: false,
      chapter: 'medium',
      maxChapters: 20,
      traceProvider: '',
      traceMax: 5,
      traceClaims: 10,
      drillCount: 6,
      mindDepth: 3,
      styles: [],
      conflictAction: '',
      destination: 'folder',
      folder: '更新待审',
      auto: false,
      subscriptionId: '',
      traceNoteId: '',
      traceNotePath: '',
      noteTask: BRIEF,
    },
  }
  let job
  try {
    job = await invoke(cdp.evaluate, 'production_start', { request })
  } catch (error) {
    target.lastError = String(error)
    target.lastTriedAt = new Date().toISOString()
    rows.push({ ...target, status: 'start_failed', error: String(error) })
    console.log('  start failed', String(error))
    continue
  }
  const startedAt = Date.now()
  try {
    const detail = await waitJob(job.id)
    const final = detail.job
    if (final.status !== 'success') {
      target.lastError = final.error
      target.lastTriedAt = new Date().toISOString()
      rows.push({ ...target, jobId: job.id, status: final.status, error: final.error })
      continue
    }
    const notePath = final.noteRelPath
    const abs = path.resolve('src-tauri/target/debug/sample-vault', notePath)
    const text = fs.existsSync(abs) ? fs.readFileSync(abs, 'utf8') : ''
    const front = frontMatter(text)
    const body = bodyText(text)
    const citations = final.noteId
      ? await invoke(cdp.evaluate, 'production_note_citations', { noteId: final.noteId })
      : []
    const claimsText = (body.match(/<!-- yebian-claims([\s\S]*?)-->/)?.[1] || '').trim()
    let claims = 0
    try {
      claims = JSON.parse(claimsText).length
    } catch {
      claims = 0
    }
    const result = {
      title: target.title,
      author: target.author,
      url: target.url,
      oldRel: target.oldRel,
      jobId: job.id,
      status: final.status,
      noteId: final.noteId,
      notePath,
      tags: listField(front, 'tags'),
      people: listField(front, '人物'),
      periods: listField(front, '时期'),
      claims,
      citations: citations.length,
      cost: final.usage?.estimatedCost || 0,
      durationMs: final.finishedAt && final.startedAt ? final.finishedAt - final.startedAt : Date.now() - startedAt,
    }
    rows.push(result)
    queue.completed = Array.isArray(queue.completed) ? queue.completed : []
    queue.completed.push(result)
    queue.pending = queue.pending.filter((item) => item.oldRel !== target.oldRel)
    queue.updatedAt = new Date().toISOString()
    fs.writeFileSync(queuePath, JSON.stringify(queue, null, 2))
    console.log(`  -> ${notePath}`)
    console.log(`  tags: ${result.tags.join(' | ') || '(none)'} · people ${result.people.length} · period ${result.periods.length} · citations ${result.citations}`)
  } catch (error) {
    target.lastError = String(error)
    target.lastTriedAt = new Date().toISOString()
    rows.push({ ...target, jobId: job.id, status: 'wait_failed', error: String(error) })
  }
}

const out = path.resolve('docs/rerun-batch-latest.json')
fs.writeFileSync(out, JSON.stringify({ measuredAt: new Date().toISOString(), queue: queuePath, rows }, null, 2))
console.log('\nwrote', out)
console.log('auto remains paused; re-enable after review')
cdp.close()
