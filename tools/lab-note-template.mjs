// Controlled AI-note template lab.
// usage: node tools/lab-note-template.mjs [--videos liubei,gongdou,ide,byd] [--arms A,B]
//
// Uses refined transcripts from real subscribed-UP jobs (no ASR re-run) and
// starts two ai-note jobs per video: A = current 成文版, B = structured brief.
// The app must be running with CDP on YEBian_LAB_PORT (default 9223).

import fs from 'node:fs'
import path from 'node:path'
import { connectCdp, invoke } from './lib/cdp.mjs'

const port = Number(process.env.YEBian_LAB_PORT || 9223)
const args = process.argv.slice(2)
const pick = (name) => {
  const index = args.indexOf(name)
  return index >= 0 ? (args[index + 1] || '') : ''
}
const videoFilter = pick('--videos').split(',').filter(Boolean)
const armFilter = pick('--arms').split(',').filter(Boolean)

const RUNS = 'src-tauri/target/debug/sample-vault/.yebian/runs'
const VIDEOS = [
  {
    key: 'liubei',
    up: '王山水读通鉴',
    title: '刘备和刘邦互换，谁能统一天下？',
    run: 'run_580c19e594dc4c658c1562e09fda3bad',
  },
  {
    key: 'gongdou',
    up: '发烧好了呢',
    title: '宫斗大戏再一次开场 美国刺破美国 美国收割美国',
    run: 'run_4e90eeb554fc41fca7ca99b84d8054e0',
  },
  {
    key: 'ide',
    up: '极海Channel',
    title: '大厂都在开发自己的IDE，我也半年没打开Intelliji',
    run: 'run_d2f47b0201b94c65aa8593581087a271',
  },
  {
    key: 'byd',
    up: '小丑朱迪',
    title: '荷兰禁比亚迪Ti7，路虎这招影响有多大呢？',
    run: 'run_b630d358f96945e99b5d4a6aa7a50167',
  },
]

const STRUCTURED_BRIEF = `把素材写成一份可直接阅读、可回查的视频笔记。以信息完整为准，不为了凑结构重复；能一页说清就不写两页。

必须按以下结构输出：
1. 第一行 \`# 标题\`，标题具体，不要写“视频笔记”。
2. \`## 一句话结论\`：一句话写出视频最核心的判断或结论。
3. \`## 速览\`：3–5 条，每条一句话，只写结论，不展开。速览必须与正文一致，不得引入正文没有的断言。
4. 速览之后直接写 4–6 个 \`##\` 小节，不要再加一个空的 \`## 正文\` 标题。每节标题必须是判断句（有主语和结论），不要写“背景/原因/影响”这类空标签。每节 2–3 段，按“判断 → 素材依据 → 限定条件或反例”组织；不要重复速览里已经说过的原句。作者观点写成“作者认为/节目判断”，不要写成客观事实；不要把作者推断写成法院/机构认定。素材用“恐怕/目测/可能”等弱语气时保留弱语气，不要升级成“大概率/必然/已经”。逐字引用前核对主语和否定词，不要把“砍不动”写成“砍得动”，不要漏掉“不/没/未”。遇到【听不清】或转写缺口原样保留，不要补成具体名词；不要补全素材里没有出现的公司全名、机构全名。每节至少保留 1 处素材原句或关键短语（逐字引用），全文至少 3 处；这些逐字句子要能直接进入 claims。
5. \`## 关键数字与专名\`：3–8 条值得回查的数字、人名、机构、书名/文章名/产品名；每条一行 \`- **条目**（类别）：一句话说明\`；要覆盖素材里影响判断的关键数量、金额、日期、车型/产品/公司名，不得为了压到 3 条而丢掉关键数字；单位、身份、口径不明确时写“素材未说明”，不要自行补成基点、亿美元等；数字和专名尽量保留素材原写法，不要为了“规范”把中文数字改成阿拉伯数字后当作逐字引用；不要凑数，不要用表格。
6. \`## 不确定与未展开\`：2–3 条视频没说清、前后矛盾或需要外部核查的点；没有就写“未见明显缺口”。
7. 不写 YAML，不写“以下是笔记”；不补素材没有的事实、数字、引语、链接。
8. 只有内容确实需要对比、时间线或流程时，才写一个自包含 HTML 视觉块；不要为了配图而配图。
9. 文末按系统要求追加 yebian-claims 清单；只列史载/转述，判断不列；claims 的 quote 必须逐字来自正文和素材，改写句不要放进 quote；找不到逐字句时宁可少列，不要凑条数。

不要运行脚本统计字数、比例或检查 Markdown；不要反复重写同一段；写完一稿后只修正事实、结构或速览与正文的矛盾，最多再改 2 轮。`

const cdp = await connectCdp(port)
const prompts = await invoke(cdp.evaluate, 'production_prompts')
const chengwen = prompts.find((item) => item.name === '成文版')
if (!chengwen) throw new Error('成文版 prompt not found')
const ARMS = {
  A: { name: '成文版（当前默认）', brief: chengwen.system },
  B: { name: '结构版（新实验）', brief: STRUCTURED_BRIEF },
}

const outRoot = path.resolve('docs/experiments/template-lab')
fs.mkdirSync(outRoot, { recursive: true })

function claims(body) {
  const match = body.match(/<!-- yebian-claims([\s\S]*?)-->/)
  if (!match) return []
  try {
    return JSON.parse(match[1].trim())
  } catch {
    return []
  }
}

function numbers(text) {
  return new Set((text.match(/\d+(?:\.\d+)?%?/g) || []).filter((value) => value.length > 0))
}

function headings(body) {
  return body.split(/\r?\n/).filter((line) => /^##\s+/.test(line)).map((line) => line.replace(/^##\s+/, '').trim())
}

async function waitJob(id, timeoutMs = 12 * 60 * 1000) {
  const deadline = Date.now() + timeoutMs
  let last = ''
  while (Date.now() < deadline) {
    const detail = await invoke(cdp.evaluate, 'production_get', { id })
    if (!detail?.job) throw new Error(`job ${id} disappeared`)
    const status = detail.job.status
    if (status !== last) {
      console.log(`  ${id} ${status} · ${detail.job.currentStep}`)
      last = status
    }
    if (['success', 'failed', 'error', 'cancelled'].includes(status)) return detail
    await new Promise((resolve) => setTimeout(resolve, 2000))
  }
  throw new Error(`job ${id} timed out`)
}

const rows = []
const logs = []
for (const video of VIDEOS) {
  if (videoFilter.length && !videoFilter.includes(video.key)) continue
  const transcriptPath = path.join(RUNS, video.run, 'refine', 'refined-01.md')
  if (!fs.existsSync(transcriptPath)) {
    console.warn(`skip ${video.key}: transcript missing ${transcriptPath}`)
    continue
  }
  const transcript = fs.readFileSync(transcriptPath, 'utf8').trim()
  if (transcript.length < 100) {
    console.warn(`skip ${video.key}: transcript too short`)
    continue
  }
  const dir = path.join(outRoot, video.key)
  fs.mkdirSync(dir, { recursive: true })
  fs.writeFileSync(path.join(dir, 'source.md'), transcript, 'utf8')

  for (const [arm, spec] of Object.entries(ARMS)) {
    if (armFilter.length && !armFilter.includes(arm)) continue
    logs.push(`\n## ${video.key} / ${arm} / ${video.up}`)
    console.log(`\n## ${video.key} / ${arm} / ${video.up}`)
    const source = `${transcript}\n\n<!-- lab-arm:${arm} -->`
    const request = {
      templateId: 'ai-note',
      title: `[实验${arm}] ${video.title}`,
      source: { kind: 'text', text: source, path: '', url: '', videos: [], pages: [] },
      options: {
        prompt: `模板实验${arm}`,
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
        folder: '实验/模板对照',
        auto: false,
        subscriptionId: '',
        traceNoteId: '',
        traceNotePath: '',
        noteTask: spec.brief,
      },
    }
    const startedAt = Date.now()
    const job = await invoke(cdp.evaluate, 'production_start', { request })
    logs.push(`job ${job.id} started`)
    console.log(`  job ${job.id} started`)
    const detail = await waitJob(job.id)
    const final = detail.job
    if (final.status !== 'success') {
      logs.push(`FAILED ${final.status}: ${final.error}`)
      console.log(`  FAILED ${final.status}: ${final.error}`)
      rows.push({ video: video.key, up: video.up, arm, status: final.status, error: final.error, jobId: final.id })
      continue
    }
    const body = final.result?.body || ''
    fs.writeFileSync(path.join(dir, `${arm}.md`), body, 'utf8')
    const citations = final.noteId
      ? await invoke(cdp.evaluate, 'production_note_citations', { noteId: final.noteId })
      : []
    const sourceNumbers = numbers(transcript)
    const noteNumbers = numbers(body)
    const structure = {
      oneLine: /^## 一句话结论/m.test(body),
      overview: /^## 速览/m.test(body),
      body: /^## 正文/m.test(body),
      keyRefs: /^## 关键数字与专名/m.test(body),
      gaps: /^## 不确定与未展开/m.test(body),
    }
    const row = {
      video: video.key,
      up: video.up,
      title: video.title,
      arm,
      armName: spec.name,
      status: final.status,
      jobId: final.id,
      noteId: final.noteId,
      noteRelPath: final.noteRelPath,
      durationMs: final.finishedAt && final.startedAt ? final.finishedAt - final.startedAt : Date.now() - startedAt,
      sourceChars: transcript.length,
      chars: body.length,
      headings: headings(body),
      h2: headings(body).length,
      structure,
      claims: claims(body).length,
      citations: citations.length,
      citationsFound: citations.filter((item) => item.status === 'found').length,
      citationsCandidate: citations.filter((item) => item.status === 'candidate').length,
      sourceNumbers: sourceNumbers.size,
      noteNumbers: noteNumbers.size,
      numberOverlap: [...sourceNumbers].filter((value) => noteNumbers.has(value)).length,
      usage: final.usage,
      steps: final.steps?.map((step) => ({ key: step.key, status: step.status, detail: step.detail })),
    }
    rows.push(row)
    logs.push(JSON.stringify({ job: final.id, chars: row.chars, h2: row.h2, claims: row.claims, citations: row.citations, cost: row.usage?.estimatedCost, durationMs: row.durationMs }))
    console.log(JSON.stringify({ job: final.id, chars: row.chars, h2: row.h2, claims: row.claims, citations: row.citations, cost: row.usage?.estimatedCost, durationMs: row.durationMs }))
  }
}

const metricsPath = path.join(outRoot, 'metrics.json')
let previousRows = []
try {
  previousRows = JSON.parse(fs.readFileSync(metricsPath, 'utf8')).rows || []
} catch {
  previousRows = []
}
const nextRows = previousRows.filter(
  (row) => !rows.some((item) => item.video === row.video && item.arm === row.arm),
)
fs.writeFileSync(
  metricsPath,
  JSON.stringify({ measuredAt: new Date().toISOString(), arms: ARMS, rows: [...nextRows, ...rows] }, null, 2),
)
fs.writeFileSync(path.join(outRoot, 'run-log.txt'), logs.join('\n'), 'utf8')
console.log(`\nwrote ${outRoot}`)
cdp.close()
