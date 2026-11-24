#!/usr/bin/env node
// Push the server state at `--baseline` (default 8350) back as a synthetic
// device. The app treats these as ordinary remote ops, so no local DB or vault
// surgery is needed: rows/files that exist now but not in the baseline become
// deletes, and rows whose payload differs are restored to the baseline value.
//
// The synthetic HLC starts one millisecond after the newest op currently on the
// server, so it beats the stale device's push but still loses to any newer local
// edit another device will push later.
//
//   npm run sync:push-baseline -- --dry-run
//   npm run sync:push-baseline -- --checklist "待整理/_同步基线-删除清单.md"
//   npm run sync:push-baseline

const fs = require('node:fs')
const path = require('node:path')
const crypto = require('node:crypto')
const { DatabaseSync } = require('node:sqlite')

const SPECS = [
  { table: 'annotations', pk: ['id'], kind: 'plain' },
  { table: 'settings', pk: ['key'], kind: 'settings' },
  { table: 'facts', pk: ['id'], kind: 'plain' },
  { table: 'note_new', pk: ['note_id'], kind: 'plain' },
  { table: 'note_fact_mentions', pk: ['id'], kind: 'plain' },
  { table: 'fact_sources', pk: ['id'], kind: 'plain' },
  { table: 'fact_evidence', pk: ['id'], kind: 'plain' },
  { table: 'fact_checks', pk: ['id'], kind: 'plain' },
  { table: 'fact_relations', pk: ['id'], kind: 'plain' },
  { table: 'fact_merges', pk: ['id'], kind: 'plain' },
  { table: 'fact_splits', pk: ['id'], kind: 'plain' },
  { table: 'fact_conflict_resolutions', pk: ['id'], kind: 'plain' },
  { table: 'fact_reviews', pk: ['fact_id'], kind: 'plain' },
  { table: 'fact_meta', pk: ['key'], kind: 'plain' },
  { table: 'history_events', pk: ['id'], kind: 'plain' },
  { table: 'sources', pk: ['id'], kind: 'plain' },
  { table: 'citations', pk: ['id'], kind: 'plain' },
  { table: 'history_meta', pk: ['key'], kind: 'plain' },
  { table: 'history_search_candidates', pk: ['id'], kind: 'plain' },
  { table: 'production_jobs', pk: ['id'], kind: 'vault' },
  { table: 'production_prompts', pk: ['id'], kind: 'plain' },
  { table: 'production_prompt_versions', pk: ['prompt_id', 'version'], kind: 'plain' },
  { table: 'production_model_prices', pk: ['provider', 'model'], kind: 'plain' },
  { table: 'production_subscriptions', pk: ['id'], kind: 'vault' },
  { table: 'production_seen_videos', pk: ['vault_path', 'mid', 'bvid'], kind: 'vault' },
]
const SPEC_BY_ENTITY = new Map(SPECS.map((spec) => ['table:' + spec.table, spec]))
const DEVICE_LOCAL = new Set([
  'vault_path',
  'index_root',
  'production.ffmpeg_path',
  'production.auto.autostart',
  'production.auto.daily_cap_v2',
])
const SECRET_MARKERS = ['api_key', 'apikey', 'cookie', 'account', 'token', 'secret', 'password', 'credential']

const args = process.argv.slice(2)
const dryRun = args.includes('--dry-run')
const baselineSeq = (() => {
  const index = args.indexOf('--baseline')
  return index >= 0 ? Number(args[index + 1]) : 8350
})()
const checklistIndex = args.indexOf('--checklist')
const checklistArg = checklistIndex >= 0 ? args[checklistIndex + 1] : ''

function syncableSetting(key) {
  const lower = String(key).toLowerCase()
  if (DEVICE_LOCAL.has(lower)) return false
  return !SECRET_MARKERS.some((marker) => lower.includes(marker))
}

function stable(value) {
  if (Array.isArray(value)) return '[' + value.map(stable).join(',') + ']'
  if (value && typeof value === 'object') {
    return '{' + Object.keys(value).sort().map((key) => JSON.stringify(key) + ':' + stable(value[key])).join(',') + '}'
  }
  return JSON.stringify(value)
}

function samePayload(left, right) {
  return stable(left ?? null) === stable(right ?? null)
}

function keyOf(op) {
  return op.entity + '\u001e' + op.pk
}

async function fetchAllOps(base, headers) {
  const ops = []
  let cursor = 0
  while (true) {
    const response = await fetch(base + '/ops?cursor=' + cursor + '&limit=500', { headers })
    if (!response.ok) throw new Error('pull failed: HTTP ' + response.status)
    const body = await response.json()
    ops.push(...(body.ops || []))
    cursor = body.cursor ?? cursor
    if (!body.has_more) break
  }
  return { ops, head: cursor }
}

async function pushOps(base, headers, ops) {
  for (let index = 0; index < ops.length; index += 200) {
    const batch = ops.slice(index, index + 200)
    const response = await fetch(base + '/ops', {
      method: 'POST',
      headers: { ...headers, 'content-type': 'application/json' },
      body: JSON.stringify({ ops: batch }),
    })
    if (!response.ok) throw new Error('push failed: HTTP ' + response.status + ' ' + (await response.text()).slice(0, 200))
    const body = await response.json()
    if ((body.accepted || []).length !== batch.length) throw new Error('server accepted ' + (body.accepted || []).length + '/' + batch.length)
  }
}

async function main() {
  const dbPath = process.env.APPDATA + '/com.yebian.notes/yebian.sqlite'
  const db = new DatabaseSync(dbPath, { readOnly: true })
  const state = db.prepare('SELECT vault_id, token FROM sync_state WHERE id = 1').get()
  const vault = db.prepare("SELECT value FROM settings WHERE key = 'vault_path'").get().value
  db.close()
  const base = 'http://127.0.0.1:18787/v1/vaults/' + encodeURIComponent(state.vault_id)
  const headers = { authorization: 'Bearer ' + state.token }
  const { ops, head } = await fetchAllOps(base, headers)

  const baseline = new Map()
  const current = new Map()
  const tailFileHashes = new Map()
  let maxPhysical = 0
  for (const entry of ops) {
    const op = entry.op
    if (!op) continue
    current.set(keyOf(op), op)
    if (entry.seq <= baselineSeq) baseline.set(keyOf(op), op)
    else if (op.entity === 'file' && op.kind === 'upsert') {
      const hashes = tailFileHashes.get(op.pk) || []
      hashes.push(op.payload?.hash || '')
      tailFileHashes.set(op.pk, hashes)
    }
    const physical = Number(String(op.hlc || '').split('-')[0]) || 0
    if (physical > maxPhysical) maxPhysical = physical
  }

  const changes = []
  const deletedFiles = []
  const restoredFiles = []
  const alreadyRestoredFiles = []
  const deletedRows = new Map()
  const restoredRows = new Map()
  const deletedDetails = []
  for (const [key, baseOp] of baseline) {
    const cur = current.get(key)
    if (!cur) continue
    const spec = baseOp.entity === 'file' ? null : SPEC_BY_ENTITY.get(baseOp.entity)
    if (baseOp.entity === 'table:settings' && !syncableSetting(baseOp.pk)) continue
    if (baseOp.kind === 'upsert') {
      if (cur.kind === 'upsert' && samePayload(baseOp.payload, cur.payload)) continue
      changes.push({ ...baseOp, op_id: crypto.randomUUID(), device_id: '', hlc: '' })
      if (baseOp.entity === 'file') restoredFiles.push(baseOp.pk)
      else restoredRows.set(baseOp.entity, (restoredRows.get(baseOp.entity) || 0) + 1)
    } else if (cur.kind === 'upsert') {
      changes.push({
        op_id: crypto.randomUUID(),
        entity: cur.entity,
        pk: cur.pk,
        kind: 'delete',
        hlc: '',
        device_id: '',
        codec: 'json',
        payload: cur.entity === 'file' ? { path: cur.pk } : null,
      })
      if (cur.entity === 'file') deletedFiles.push({ path: cur.pk, payload: cur.payload })
      else {
        deletedRows.set(cur.entity, (deletedRows.get(cur.entity) || 0) + 1)
        deletedDetails.push({ entity: cur.entity, pk: cur.pk, payload: cur.payload })
      }
    }
  }
  for (const [key, cur] of current) {
    if (baseline.has(key) || cur.kind !== 'upsert') continue
    if (cur.entity === 'table:settings' && !syncableSetting(cur.pk)) continue
    changes.push({
      op_id: crypto.randomUUID(),
      entity: cur.entity,
      pk: cur.pk,
      kind: 'delete',
      hlc: '',
      device_id: '',
      codec: 'json',
      payload: cur.entity === 'file' ? { path: cur.pk } : null,
    })
    if (cur.entity === 'file') deletedFiles.push({ path: cur.pk, payload: cur.payload })
    else {
      deletedRows.set(cur.entity, (deletedRows.get(cur.entity) || 0) + 1)
      deletedDetails.push({ entity: cur.entity, pk: cur.pk, payload: cur.payload })
    }
  }
  for (const [key, cur] of current) {
    if (cur.entity !== 'file') continue
    const baseOp = baseline.get(key)
    if (!baseOp || baseOp.kind !== 'upsert' || cur.kind !== 'upsert') continue
    const full = path.join(vault, cur.pk)
    if (!fs.existsSync(full)) continue
    const hash = crypto.createHash('sha256').update(fs.readFileSync(full)).digest('hex')
    const changedTail = (tailFileHashes.get(cur.pk) || []).some((value) => value && value !== baseOp.payload?.hash)
    if (hash === baseOp.payload?.hash && changedTail) {
      alreadyRestoredFiles.push(cur.pk)
      continue
    }
    if (samePayload(baseOp.payload, cur.payload)) continue
  }

  const syntheticDevice = 'baseline' + crypto.randomUUID().replace(/-/g, '')
  let counter = 0
  const physical = maxPhysical + 1
  for (const change of changes) {
    change.hlc = `${physical}-${String(counter++).padStart(5, '0')}-${syntheticDevice}`
    change.device_id = syntheticDevice
  }

  console.log(
    `baseline seq ${baselineSeq}, server head ${head}, changes ${changes.length}: ` +
      `upserts ${restoredFiles.length + [...restoredRows.values()].reduce((a, b) => a + b, 0)}, ` +
      `deletes ${deletedFiles.length + [...deletedRows.values()].reduce((a, b) => a + b, 0)}`,
  )
  console.log('deleted files:', deletedFiles.length, 'restored files:', restoredFiles.length, 'already restored:', alreadyRestoredFiles.length)
  console.log('deleted rows:', JSON.stringify(Object.fromEntries(deletedRows)))
  console.log('restored rows:', JSON.stringify(Object.fromEntries(restoredRows)))
  if (checklistArg) {
    let indexDb = null
    try {
      indexDb = new DatabaseSync(process.env.APPDATA + '/com.yebian.notes/yebian-index.sqlite', { readOnly: true })
    } catch {
      indexDb = null
    }
    const noteLabel = (id) => {
      try {
        const row = indexDb?.prepare('SELECT title, rel_path FROM notes WHERE id = ?').get(id)
        if (row) return `${row.title || '(无标题)'}（${row.rel_path}）`
      } catch {
        // Index may not have the office-only note; fall back to the id.
      }
      return id
    }
    const jobs = deletedDetails.filter((item) => item.entity === 'table:production_jobs')
    const notes = deletedDetails.filter((item) => item.entity === 'table:note_new')
    const facts = deletedDetails.filter((item) => item.entity === 'table:facts')
    const otherDeleted = [...deletedRows].filter(
      ([entity]) => !['table:production_jobs', 'table:note_new', 'table:facts'].includes(entity),
    )
    const lines = [
      '# 同步基线删除清单（2026-11-24）',
      '',
      `基线：家里设备最后同步位置 \`seq ${baselineSeq}\`；生成时间 ${new Date().toLocaleString('zh-CN', { hour12: false })}。`,
      '这份清单记录把办公室侧 12:01 之后多出来的内容回滚到家里基线时，被删除或改回的文件/行，便于后续更新时核对是否遗漏。',
      '',
      `## 删除的办公室-only 文件（${deletedFiles.length}）`,
      '',
      ...deletedFiles.map(
        (item) => `- \`${item.path}\` \`${item.payload?.hash ?? ''}\`（${item.payload?.size ?? '?'} B）`,
      ),
      '',
      `## 已回滚为家里版本的文件（${alreadyRestoredFiles.length + restoredFiles.length}）`,
      '',
      ...[...new Set([...alreadyRestoredFiles, ...restoredFiles])].map((item) => `- \`${item}\``),
      '',
      `## 删除的办公室-only 任务（${jobs.length}）`,
      '',
      ...jobs.map((item) => `- ${item.payload?.title || '(无标题)'}（${item.pk}）`),
      '',
      `## 删除的办公室-only 笔记记录（${notes.length}）`,
      '',
      ...notes.map((item) => `- ${noteLabel(item.pk)}`),
      '',
      `## 删除的办公室-only 事实（${facts.length}）`,
      '',
      ...facts.map((item) => `- ${String(item.payload?.text || '').slice(0, 100)}（${item.pk}）`),
      '',
      '## 其它删除的数据行',
      '',
      ...(otherDeleted.length ? otherDeleted.map(([entity, count]) => `- \`${entity}\`：${count}`) : ['- 无']),
      '',
      '## 恢复为家里版本的数据行',
      '',
      ...[...restoredRows].map(([entity, count]) => `- \`${entity}\`：${count}`),
      '',
      '> 注：\`production_jobs\` 的 \`config_json\` 可能包含家里那台机器的 ffmpeg 路径；历史任务不重跑时不影响使用。',
      '',
    ]
    const target = path.isAbsolute(checklistArg) ? checklistArg : path.join(vault, checklistArg)
    fs.mkdirSync(path.dirname(target), { recursive: true })
    fs.writeFileSync(target, lines.join('\n'), 'utf8')
    indexDb?.close()
    console.log('checklist:', target)
  }
  if (dryRun) {
    console.log('dry-run; nothing pushed')
    return
  }
  await pushOps(base, headers, changes)
  console.log(`pushed ${changes.length} ops as ${syntheticDevice}`)
}

main().catch((error) => {
  console.error('✗', error.message)
  process.exit(1)
})
