<script setup lang="ts">
import { computed, onMounted, reactive, ref } from 'vue'
import { useRouter } from 'vue-router'
import { ArrowLeft, CheckCircle2, ChevronRight, GitMerge, Layers, LoaderCircle, RefreshCw, Scissors, TriangleAlert } from 'lucide-vue-next'

import * as api from '../lib/api'
import FactSplitDialog from '../components/FactSplitDialog.vue'
import { factSourceSummary, factStatusClass, factStatusLabel, isCompositeFact } from '../lib/facts'
import type { Fact, FactConflictResolution, FactReview, FactSplit } from '../types'

const router = useRouter()
const facts = ref<Fact[]>([])
const reviews = ref<FactReview[]>([])
const resolutions = ref<FactConflictResolution[]>([])
const loading = ref(false)
const error = ref('')
const notice = ref('')
const merging = ref('')
const splitTarget = ref<Fact | null>(null)
const savingResolution = ref('')
const savingReview = ref('')
const mergeTargets = reactive<Record<string, string>>({})
const resolutionDrafts = reactive<Record<string, { resolution: string; note: string }>>({})
let noticeTimer = 0

const resolutionOptions = [
  { id: 'needs_research', label: '待查' },
  { id: 'different_scope', label: '口径不同' },
  { id: 'keep_both', label: '保留双方' },
  { id: 'one_side_wrong', label: '一方有误' },
] as const

const factById = computed(() => {
  const map = new Map<string, Fact>()
  for (const fact of facts.value) map.set(fact.id, fact)
  return map
})

const reviewByFact = computed(() => {
  const map = new Map<string, FactReview>()
  for (const review of reviews.value) map.set(review.factId, review)
  return map
})

const resolutionByPair = computed(() => {
  const map = new Map<string, FactConflictResolution>()
  for (const resolution of resolutions.value) map.set(pairKey(resolution.factA, resolution.factB), resolution)
  return map
})

function pairKey(left: string, right: string): string {
  return [left, right].sort().join('|')
}

const duplicatePairs = computed(() => {
  const seen = new Set<string>()
  const pairs: { key: string; a: Fact; b: Fact; confidence: number }[] = []
  for (const fact of facts.value) {
    if (fact.status === 'split') continue
    for (const relation of fact.related) {
      if (relation.relation !== 'same_as') continue
      const other = factById.value.get(relation.otherId)
      if (!other) continue
      const key = pairKey(fact.id, other.id)
      if (seen.has(key)) continue
      seen.add(key)
      pairs.push({ key, a: fact, b: other, confidence: relation.confidence })
    }
  }
  return pairs.sort((left, right) => right.confidence - left.confidence)
})

const conflictPairs = computed(() => {
  const seen = new Set<string>()
  const pairs: { key: string; a: Fact; b: Fact; confidence: number; resolution?: FactConflictResolution }[] = []
  for (const fact of facts.value) {
    if (fact.status === 'split') continue
    for (const relation of fact.related) {
      if (relation.relation !== 'contradicts') continue
      const other = factById.value.get(relation.otherId)
      if (!other) continue
      const key = pairKey(fact.id, other.id)
      if (seen.has(key)) continue
      seen.add(key)
      pairs.push({ key, a: fact, b: other, confidence: relation.confidence, resolution: resolutionByPair.value.get(key) })
    }
  }
  return pairs.sort((left, right) => right.confidence - left.confidence)
})

const compositeFacts = computed(() =>
  facts.value
    .filter((fact) => fact.status !== 'split' && isCompositeFact(fact))
    .sort((left, right) => right.related.length - left.related.length || right.text.length - left.text.length),
)

const unsourcedFacts = computed(() =>
  facts.value.filter((fact) => fact.status !== 'split' && fact.evidence.length === 0 && fact.citationCount === 0),
)

const unreviewedCount = computed(() =>
  facts.value.filter((fact) => fact.status !== 'split' && !reviewByFact.value.has(fact.id)).length,
)

const summary = computed(() => ({
  duplicates: duplicatePairs.value.length,
  conflicts: conflictPairs.value.length,
  composites: compositeFacts.value.length,
  unsourced: unsourcedFacts.value.length,
  unreviewed: unreviewedCount.value,
}))

function flash(message: string): void {
  notice.value = message
  window.clearTimeout(noticeTimer)
  noticeTimer = window.setTimeout(() => { if (notice.value === message) notice.value = '' }, 3600)
}

function reviewFor(fact: Fact): FactReview | undefined {
  return reviewByFact.value.get(fact.id)
}

function reviewLabel(fact: Fact): string {
  const status = reviewFor(fact)?.status
  return status === 'reviewed' ? '已整理' : status === 'needs_split' ? '待拆分' : '未整理'
}

function ensureDraft(pair: { key: string; a: Fact; b: Fact }): { resolution: string; note: string } {
  if (!resolutionDrafts[pair.key]) {
    const existing = resolutionByPair.value.get(pair.key)
    resolutionDrafts[pair.key] = {
      resolution: existing?.resolution ?? 'needs_research',
      note: existing?.note ?? '',
    }
  }
  return resolutionDrafts[pair.key]
}

function openFact(id: string): void {
  if (id) void router.push(`/facts/${id}`)
}

function back(): void {
  void router.push('/facts')
}

async function load(): Promise<void> {
  loading.value = true
  error.value = ''
  try {
    const [nextFacts, nextReviews, nextResolutions] = await Promise.all([
      api.productionFacts('', '', 300),
      api.productionFactReviews(),
      api.productionFactConflictResolutions(),
    ])
    facts.value = nextFacts
    reviews.value = nextReviews
    resolutions.value = nextResolutions
    for (const pair of duplicatePairs.value) {
      if (!mergeTargets[pair.key]) mergeTargets[pair.key] = pair.a.id
    }
    for (const pair of conflictPairs.value) {
      ensureDraft(pair)
    }
  } catch (cause) {
    error.value = api.errorText(cause)
  } finally {
    loading.value = false
  }
}

async function mergePair(pair: { key: string; a: Fact; b: Fact }): Promise<void> {
  if (merging.value) return
  const targetId = mergeTargets[pair.key] ?? pair.a.id
  const source = targetId === pair.a.id ? pair.b : pair.a
  const target = targetId === pair.a.id ? pair.a : pair.b
  const ok = window.confirm(`把「${source.text.slice(0, 50)}…」合并到「${target.text.slice(0, 50)}…」？来源、证据、提及和引用会转移，可在「合并记录」撤销。`)
  if (!ok) return
  merging.value = pair.key
  error.value = ''
  try {
    await api.productionMergeFact(target.id, source.id)
    await load()
    flash('已合并 1 对重复事实')
  } catch (cause) {
    error.value = api.errorText(cause)
  } finally {
    merging.value = ''
  }
}

function openSplit(fact: Fact): void {
  splitTarget.value = fact
}

async function onSplit(split: FactSplit): Promise<void> {
  splitTarget.value = null
  await load()
  flash(`已拆成 ${split.children.length} 条子事实；原事实保留为审计记录。`)
}

async function saveResolution(pair: { key: string; a: Fact; b: Fact }): Promise<void> {
  if (savingResolution.value) return
  const draft = ensureDraft(pair)
  savingResolution.value = pair.key
  error.value = ''
  try {
    await api.productionFactConflictResolve({
      factA: pair.a.id,
      factB: pair.b.id,
      resolution: draft.resolution,
      note: draft.note,
    })
    await load()
    flash('冲突处理已记录')
  } catch (cause) {
    error.value = api.errorText(cause)
  } finally {
    savingResolution.value = ''
  }
}

async function setReview(fact: Fact, status: 'reviewed' | 'needs_split' | ''): Promise<void> {
  if (savingReview.value) return
  savingReview.value = fact.id
  error.value = ''
  try {
    await api.productionFactReviewSet(fact.id, status)
    reviews.value = await api.productionFactReviews()
    flash(status === '' ? '已清除整理标记' : status === 'reviewed' ? '已标记为已整理' : '已标记为待拆分')
  } catch (cause) {
    error.value = api.errorText(cause)
  } finally {
    savingReview.value = ''
  }
}

function relatedFacts(fact: Fact): Fact[] {
  return fact.related
    .filter((relation) => relation.relation === 'related')
    .map((relation) => factById.value.get(relation.otherId))
    .filter((item): item is Fact => Boolean(item))
    .slice(0, 5)
}

onMounted(load)
</script>

<template>
  <div class="curation-page">
    <div class="curation-top">
      <button type="button" class="fact-detail-back" @click="back"><ArrowLeft :size="13" />返回事实库</button>
      <button type="button" class="prod-btn is-small" :disabled="loading" @click="load"><RefreshCw :size="12" :class="{ 'sub-spin': loading }" />刷新</button>
    </div>

    <header class="curation-head">
      <div class="prod-eyebrow">知识库 / 事实整理</div>
      <h1>事实整理</h1>
      <p>合并直接重复、给冲突留下处理结论、把复合事实拆成可独立核验的原子事实。拆分不复制证据/引用，原事实保留为可撤销的审计记录。</p>
    </header>

    <div v-if="notice" class="facts-notice"><CheckCircle2 :size="14" />{{ notice }}</div>
    <div v-if="error" class="prod-error-box"><p>{{ error }}</p></div>

    <div class="curation-summary">
      <article><span>重复候选</span><b>{{ summary.duplicates }}</b><small>同一条事实的不同写法</small></article>
      <article><span>冲突</span><b>{{ summary.conflicts }}</b><small>数值/口径不一致</small></article>
      <article><span>复合事实</span><b>{{ summary.composites }}</b><small>一条里塞了多个断言</small></article>
      <article><span>无来源</span><b>{{ summary.unsourced }}</b><small>没有网页证据也没有史料</small></article>
    </div>
    <p class="curation-summary-note">未标记 {{ summary.unreviewed }} 条（可选，不影响事实质量；只有需要拆分/核验的才需要处理）</p>

    <div v-if="loading" class="facts-loading"><LoaderCircle :size="16" class="sub-spin" />正在整理事实…</div>

    <template v-else>
      <section v-if="duplicatePairs.length" class="curation-section">
        <div class="curation-section-head">
          <div><h2><GitMerge :size="17" />重复候选</h2><p>只处理词面/数字都指向同一事实的 same_as 对；保留一条，来源/证据/提及/引用转移。</p></div>
          <span>{{ duplicatePairs.length }} 对</span>
        </div>
        <div class="curation-list">
          <article v-for="pair in duplicatePairs" :key="pair.key" class="curation-card">
            <div class="curation-card-head">
              <span>词面重合 {{ Math.round(pair.confidence * 100) }}%</span>
              <button type="button" class="curation-link" @click="openFact(pair.a.id)">打开事实 <ChevronRight :size="11" /></button>
            </div>
            <label v-for="item in [pair.a, pair.b]" :key="item.id" class="curation-choice" :class="{ 'is-picked': (mergeTargets[pair.key] ?? pair.a.id) === item.id }">
              <input v-model="mergeTargets[pair.key]" type="radio" :value="item.id" :disabled="Boolean(merging)" />
              <span class="curation-choice-body">
                <span class="curation-choice-meta"><span :class="['fact-status', factStatusClass(item)]">{{ factStatusLabel(item) }}</span><span>{{ item.noteCount }} 篇笔记</span><span>{{ factSourceSummary(item) }}</span></span>
                <b>{{ item.text }}</b>
              </span>
            </label>
            <div class="curation-card-foot">
              <span>选中项会保留；另一条被合并。</span>
              <button type="button" class="prod-btn is-small is-danger" :disabled="Boolean(merging)" @click="mergePair(pair)">
                <LoaderCircle v-if="merging === pair.key" :size="12" class="sub-spin" />{{ merging === pair.key ? '合并中…' : '合并这一对' }}
              </button>
            </div>
          </article>
        </div>
      </section>
      <p v-else class="curation-empty-line"><b>重复候选 0</b> · 暂无需要合并的同一事实</p>

      <section v-if="conflictPairs.length" class="curation-section">
        <div class="curation-section-head">
          <div><h2><TriangleAlert :size="17" />冲突</h2><p>先记录口径，不要急着合并。历史事实的异文也走这里。</p></div>
          <span>{{ conflictPairs.length }} 对</span>
        </div>
        <div class="curation-list">
          <article v-for="pair in conflictPairs" :key="pair.key" class="curation-card is-conflict">
            <div class="curation-card-head">
              <span>冲突候选 · 词面重合 {{ Math.round(pair.confidence * 100) }}%</span>
              <span v-if="pair.resolution" class="curation-resolution-badge">{{ resolutionOptions.find((item) => item.id === pair.resolution?.resolution)?.label ?? pair.resolution.resolution }}</span>
            </div>
            <div class="curation-conflict-body">
              <button type="button" class="curation-conflict-fact" @click="openFact(pair.a.id)">{{ pair.a.text }}</button>
              <button type="button" class="curation-conflict-fact" @click="openFact(pair.b.id)">{{ pair.b.text }}</button>
            </div>
            <div class="curation-resolution-row">
              <select v-model="resolutionDrafts[pair.key].resolution" class="prod-select" :disabled="savingResolution === pair.key">
                <option v-for="option in resolutionOptions" :key="option.id" :value="option.id">{{ option.label }}</option>
              </select>
              <input v-model="resolutionDrafts[pair.key].note" class="prod-input" placeholder="备注：差在哪个口径/年份/数字" :disabled="savingResolution === pair.key" />
              <button type="button" class="prod-btn is-small" :disabled="savingResolution === pair.key" @click="saveResolution(pair)">
                <LoaderCircle v-if="savingResolution === pair.key" :size="12" class="sub-spin" />{{ savingResolution === pair.key ? '保存中…' : '记录处理' }}
              </button>
            </div>
          </article>
        </div>
      </section>
      <p v-else class="curation-empty-line"><b>冲突 0</b> · 暂无需要记录口径的矛盾事实</p>

      <section v-if="compositeFacts.length" class="curation-section">
        <div class="curation-section-head">
          <div><h2><Layers :size="17" />复合事实</h2><p>一条事实里出现多个断言。拆成原子事实；证据和史料留在原事实，子事实需要逐条核验。</p></div>
          <span>{{ compositeFacts.length }} 条</span>
        </div>
        <div class="curation-list">
          <article v-for="fact in compositeFacts" :key="fact.id" class="curation-card">
            <div class="curation-card-head">
              <span>{{ fact.noteCount }} 篇笔记 · {{ factSourceSummary(fact) }}</span>
              <span class="curation-review" :class="`is-${reviewFor(fact)?.status || 'none'}`">{{ reviewLabel(fact) }}</span>
            </div>
            <button type="button" class="curation-fact-text" @click="openFact(fact.id)">{{ fact.text }}</button>
            <div v-if="relatedFacts(fact).length" class="curation-children">
              <span>相关原子事实</span>
              <button v-for="child in relatedFacts(fact)" :key="child.id" type="button" @click="openFact(child.id)">{{ child.text }}</button>
            </div>
            <div class="curation-card-foot">
              <span>子事实默认不继承证据/引用；原事实会保留为可撤销的审计记录。</span>
              <span class="curation-actions">
                <button type="button" class="curation-primary" @click="openSplit(fact)"><Scissors :size="11" />拆分</button>
                <details class="curation-more">
                  <summary aria-label="更多整理操作">⋯</summary>
                  <div class="curation-more-menu">
                    <button type="button" :disabled="savingReview === fact.id" @click="setReview(fact, 'needs_split')">标记待拆分</button>
                    <button type="button" :disabled="savingReview === fact.id" @click="setReview(fact, 'reviewed')">标记已整理</button>
                    <button v-if="reviewFor(fact)" type="button" :disabled="savingReview === fact.id" @click="setReview(fact, '')">清除标记</button>
                  </div>
                </details>
              </span>
            </div>
          </article>
        </div>
      </section>
      <p v-else class="curation-empty-line"><b>复合事实 0</b> · 没有检测到需要拆分的多断言事实</p>

      <section v-if="unsourcedFacts.length" class="curation-section">
        <div class="curation-section-head">
          <div><h2><TriangleAlert :size="17" />无来源</h2><p>既没有网页证据，也没有史料引用；先确认它是不是值得保留的事实。</p></div>
          <span>{{ unsourcedFacts.length }} 条</span>
        </div>
        <div class="curation-list is-compact">
          <article v-for="fact in unsourcedFacts" :key="fact.id" class="curation-row">
            <button type="button" class="curation-row-main" @click="openFact(fact.id)">
              <span class="curation-choice-meta"><span :class="['fact-status', factStatusClass(fact)]">{{ factStatusLabel(fact) }}</span><span>{{ fact.noteCount }} 篇笔记</span><span>{{ fact.domain === 'history' ? '历史' : '通用' }}</span></span>
              <b>{{ fact.text }}</b>
            </button>
            <span class="curation-review" :class="`is-${reviewFor(fact)?.status || 'none'}`">{{ reviewLabel(fact) }}</span>
            <span class="curation-actions">
              <button type="button" :disabled="savingReview === fact.id" @click="setReview(fact, 'reviewed')">已看过</button>
              <button v-if="reviewFor(fact)" type="button" :disabled="savingReview === fact.id" @click="setReview(fact, '')">清除</button>
            </span>
          </article>
        </div>
      </section>
      <p v-else class="curation-empty-line"><b>无来源 0</b> · 每条事实都至少有一条网页证据或史料</p>
    </template>
    <FactSplitDialog :fact="splitTarget" @close="splitTarget = null" @split="onSplit" />
  </div>
</template>
