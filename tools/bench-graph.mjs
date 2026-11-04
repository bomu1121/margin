import { performance } from 'node:perf_hooks'
import { localGraph } from '../src/lib/graph.ts'
import { layoutLocalGraph } from '../src/lib/graphLayout.ts'

function mulberry32(seed) {
  return function random() {
    seed |= 0
    seed = (seed + 0x6d2b79f5) | 0
    let t = Math.imul(seed ^ (seed >>> 15), 1 | seed)
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296
  }
}

function makeNotes(count) {
  const random = mulberry32(20261104)
  const topics = Array.from({ length: 24 }, (_, i) => `主题/主题${i}`)
  const domains = ['领域/财经', '领域/历史', '领域/AI', '领域/社会']
  const types = ['类型/视频笔记', '类型/学习笔记', '类型/文章']
  const people = Array.from({ length: 48 }, (_, i) => `人物${i}`)
  const periods = ['当代', '明清', '民国', '宋代', '唐代', '魏晋', '先秦', '近现代']
  const notes = []
  for (let i = 0; i < count; i += 1) {
    const pick = (list) => list[Math.floor(random() * list.length)]
    const tags = [pick(domains), pick(types), pick(topics), pick(topics)]
    notes.push({
      id: `id${i}`,
      path: `C:/vault/note-${i}.md`,
      relPath: `note-${i}.md`,
      title: `【测试】第${i}篇：养老金、财政与制度的一篇笔记`,
      up: `UP主${i % 20}`,
      date: `2026-${String((i % 12) + 1).padStart(2, '0')}-01`,
      domain: tags[0].split('/')[1],
      tags,
      people: [pick(people), pick(people)],
      events: [],
      periods: [pick(periods)],
      runId: '',
      mtime: 0,
      chars: 1200,
      annotationCount: 0,
    })
  }
  return notes
}

for (const count of [1000, 10000]) {
  const notes = makeNotes(count)
  const samples = []
  const pickRandom = mulberry32(7)
  for (let i = 0; i < 30; i += 1) {
    const note = notes[Math.floor(pickRandom() * notes.length)]
    const started = performance.now()
    localGraph(notes, note.id)
    samples.push(performance.now() - started)
  }
  samples.sort((a, b) => a - b)
  const p50 = samples[Math.floor(samples.length / 2)]
  const p95 = samples[Math.floor((samples.length * 95) / 100)]

  const model = localGraph(notes, notes[0].id)
  const layoutSamples = []
  if (model) {
    for (let i = 0; i < 300; i += 1) {
      const started = performance.now()
      layoutLocalGraph(model)
      layoutSamples.push(performance.now() - started)
    }
    layoutSamples.sort((a, b) => a - b)
  }
  const layoutP50 = layoutSamples[Math.floor(layoutSamples.length / 2)] ?? 0
  const layoutP95 = layoutSamples[Math.floor((layoutSamples.length * 95) / 100)] ?? 0

  console.log(
    JSON.stringify({
      notes: count,
      localGraphP50Ms: +p50.toFixed(2),
      localGraphP95Ms: +p95.toFixed(2),
      maxMs: +samples[samples.length - 1].toFixed(2),
      layoutNodes: model ? model.attrs.length + model.related.length + 1 : 0,
      layoutP50Ms: +layoutP50.toFixed(3),
      layoutP95Ms: +layoutP95.toFixed(3),
    }),
  )
}
