<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, reactive, ref, watch } from 'vue'
import {
  AlertTriangle,
  Clock3,
  ExternalLink,
  LoaderCircle,
  Plus,
  RefreshCw,
  Rss,
  Settings2,
  SlidersHorizontal,
  Trash2,
  UserRound,
  Video,
  X,
} from 'lucide-vue-next'

import * as api from '../../lib/api'
import { production, productionUi, visiblePrompts, visibleTemplates } from '../../stores/production'
import type {
  AutoSettings,
  BiliFollowing,
  BiliUpProfile,
  BiliVideoPreview,
  ProductionSubscription,
  SaveAutoSettings,
  SeenVideo,
  SubscriptionInput,
} from '../../types'

const emit = defineEmits<{ login: []; changed: [] }>()

const subscriptions = ref<ProductionSubscription[]>([])
const auto = ref<AutoSettings | null>(null)
const loading = ref(false)
const error = ref('')
const notice = ref('')
const autoSaving = ref(false)
const busyId = ref('')
const now = ref(Date.now())

const dialogOpen = ref(false)
const editing = ref<ProductionSubscription | null>(null)
const addTab = ref<'following' | 'video' | 'link'>('following')
const followItems = ref<BiliFollowing[]>([])
const followQuery = ref('')
const followPage = ref(0)
const followTotal = ref(0)
const followHasMore = ref(false)
const followLoading = ref(false)
const followError = ref('')
const videoUrl = ref('')
const videoPreview = ref<BiliVideoPreview | null>(null)
const videoLoading = ref(false)
const videoError = ref('')
const lookupQuery = ref('')
const lookupLoading = ref(false)
const profile = ref<BiliUpProfile | null>(null)
const formError = ref('')
const submitting = ref(false)
const form = reactive({
  intervalMinutes: 30,
  templateId: 'video-note',
  prompt: '速览版',
  backfill: 0,
  minMinutes: 0,
  maxMinutes: 0,
  includeKeywords: '',
  excludeKeywords: '',
})

const seenOpen = ref(false)
const seenLoading = ref(false)
const seenError = ref('')
const seenItems = ref<SeenVideo[]>([])
const seenTitle = ref('')
const deleteTarget = ref<ProductionSubscription | null>(null)

let clockTimer = 0
let lookupTimer = 0
let lookupSeq = 0
let videoTimer = 0
let videoSeq = 0
let stopSubscriptionEvents: (() => void) | null = null
let noticeTimer = 0

const templates = computed(() => visibleTemplates.value.filter((template) => template.sources.includes('bili')))
const loginReady = computed(() => Boolean(production.settings?.biliLoggedIn))
const budgetPaused = computed(() => Boolean(
  auto.value
  && auto.value.pauseOnBudget
  && auto.value.monthlyBudgetCny > 0
  && auto.value.monthlySpendCny >= auto.value.monthlyBudgetCny,
))
const enabledCount = computed(() => subscriptions.value.filter((item) => item.enabled).length)
const autoTaskCount = computed(() => subscriptions.value.reduce((sum, item) => sum + item.queuedCount, 0))
const nextCheck = computed(() => {
  if (!auto.value?.enabled || !loginReady.value || budgetPaused.value) return 0
  const times = subscriptions.value.filter((item) => item.enabled && item.nextCheckAt > 0).map((item) => item.nextCheckAt)
  return times.length ? Math.min(...times) : 0
})
const nextCheckHint = computed(() => {
  if (!auto.value?.enabled) return '自动检查总开关已关闭'
  if (!loginReady.value) return '等待登录 B 站'
  if (budgetPaused.value) return '本月预算暂停'
  return '没有启用中的订阅'
})
const intervalChoices = [15, 30, 60, 120, 360, 720]
const backfillChoices = [
  { value: 0, label: '不补跑', hint: '只处理添加之后发布的新视频' },
  { value: 3, label: '最近 3 条', hint: '适合先试一下效果' },
  { value: 5, label: '最近 5 条', hint: '会创建 5 个任务，按顺序执行' },
  { value: 10, label: '最近 10 条', hint: '成本最高；只建议在确认模板后使用' },
]
const intervalOptions = computed(() => {
  const values = [...intervalChoices]
  if (form.intervalMinutes && !values.includes(form.intervalMinutes)) values.push(form.intervalMinutes)
  return values.sort((a, b) => a - b)
})
const watchedTemplates = computed(() => {
  const list = templates.value.map((item) => item.id)
  if (form.templateId && !list.includes(form.templateId)) list.unshift(form.templateId)
  return list
})
const prompts = computed(() => {
  const names = visiblePrompts.value.map((item) => item.name)
  if (form.prompt && !names.includes(form.prompt)) names.unshift(form.prompt)
  return names
})
const filteredFollows = computed(() => {
  const query = followQuery.value.trim().toLowerCase()
  if (!query) return followItems.value
  return followItems.value.filter((item) => item.name.toLowerCase().includes(query) || String(item.mid).includes(query))
})
const canSubmit = computed(() => Boolean(profile.value) && !lookupLoading.value && !submitting.value)
const parsedMid = computed(() => parseMidText(lookupQuery.value))
const lookupHint = computed(() => {
  if (editing.value) return ''
  if (lookupLoading.value) return '正在读取 UP 主资料…'
  if (profile.value) return ''
  if (!lookupQuery.value.trim()) return ''
  if (!parsedMid.value) return '没有识别到 UID；支持 UID、space.bilibili.com 或 m.bilibili.com 空间链接'
  return `已识别 UID ${parsedMid.value}，正在自动读取`
})
const footerHint = computed(() => {
  if (editing.value) return '保存不会重跑历史视频。'
  if (!profile.value) return '先从关注、视频链接或 UID 里选一个 UP。'
  return '添加后会立即建立基线；旧视频默认不补跑。'
})
const title = computed(() => (editing.value ? '订阅设置' : '添加 UP 订阅'))

function flash(message: string): void {
  notice.value = message
  window.clearTimeout(noticeTimer)
  noticeTimer = window.setTimeout(() => {
    if (notice.value === message) notice.value = ''
  }, 4200)
}

function errorText(cause: unknown): string {
  return api.errorText(cause)
}

function parseMidText(value: string): number | null {
  const text = value.trim()
  if (!text) return null
  if (/^\d+$/.test(text)) {
    const mid = Number(text)
    return Number.isSafeInteger(mid) && mid > 0 ? mid : null
  }
  const patterns = [
    /space\.bilibili\.com\/(\d+)/i,
    /bilibili\.com\/space\/(\d+)/i,
    /[?&](?:mid|uid)=(\d+)/i,
  ]
  for (const pattern of patterns) {
    const match = text.match(pattern)
    if (match) return Number(match[1])
  }
  return null
}

function avatarUrl(face: string): string {
  if (!face) return ''
  if (face.startsWith('//')) return `https:${face}`
  if (face.startsWith('http://')) return `https://${face.slice(7)}`
  return face
}

function formatNumber(value: number): string {
  if (value >= 10_000) return `${(value / 10_000).toFixed(value >= 100_000 ? 0 : 1)} 万`
  return value.toLocaleString()
}

function formatDateTime(value: number): string {
  if (!value) return '—'
  const date = new Date(value < 1_000_000_000_000 ? value * 1000 : value)
  if (Number.isNaN(date.getTime())) return '—'
  return `${date.getMonth() + 1} 月 ${date.getDate()} 日 ${String(date.getHours()).padStart(2, '0')}:${String(date.getMinutes()).padStart(2, '0')}`
}

function relativeTime(value: number): string {
  if (!value) return '—'
  const ms = value < 1_000_000_000_000 ? value * 1000 : value
  const diff = now.value - ms
  if (diff < 60_000) return '刚刚'
  if (diff < 3600_000) return `${Math.floor(diff / 60_000)} 分钟前`
  if (diff < 86400_000) return `${Math.floor(diff / 3600_000)} 小时前`
  return `${Math.floor(diff / 86400_000)} 天前`
}

function futureTime(value: number): string {
  if (!value) return '—'
  const diff = value - now.value
  if (diff <= 0) return '即将检查'
  if (diff < 3600_000) return `${Math.max(1, Math.ceil(diff / 60_000))} 分钟后`
  if (diff < 86400_000) return `${Math.ceil(diff / 3600_000)} 小时后`
  return formatDateTime(value)
}

function nextText(sub: ProductionSubscription): string {
  if (!sub.enabled) return '已暂停自动检查'
  if (!auto.value?.enabled) return '自动检查总开关已关闭'
  if (!loginReady.value) return '登录 B 站后才会自动检查'
  if (budgetPaused.value) return '本月预算已用完，自动检查暂停'
  if (!sub.nextCheckAt) return '等待首次检查'
  const diff = sub.nextCheckAt - now.value
  if (diff <= 0) return '即将检查'
  if (diff < 3600_000) return `${Math.max(1, Math.ceil(diff / 60_000))} 分钟后检查`
  if (diff < 86400_000) return `${Math.ceil(diff / 3600_000)} 小时后检查`
  return formatDateTime(sub.nextCheckAt)
}

function statusText(sub: ProductionSubscription): string {
  if (!sub.enabled) return '自动检查已关闭'
  if (sub.lastError) return sub.lastError
  if (!sub.baselineAt) return '还没有建立基线；下一次检查会把现有视频记为基线'
  if (sub.lastMessage) return sub.lastMessage
  return '等待第一次检查'
}

function statusTone(sub: ProductionSubscription): string {
  if (sub.lastError || sub.consecutiveFailures > 0) return 'is-error'
  if (!sub.enabled) return 'is-muted'
  if (!sub.baselineAt) return 'is-warn'
  return 'is-ok'
}

function statusLabel(status: string): string {
  return ({
    baseline: '基线',
    queued: '已排任务',
    running: '制作中',
    success: '已完成',
    failed: '启动失败',
    skipped_existing: '已有笔记',
    skipped_filter: '被筛选',
  } as Record<string, string>)[status] ?? status
}

function statusClass(status: string): string {
  if (status === 'queued' || status === 'running') return 'is-blue'
  if (status === 'success') return 'is-green'
  if (status === 'failed') return 'is-red'
  if (status === 'skipped_filter') return 'is-gold'
  return 'is-muted'
}

async function load(silent = false): Promise<void> {
  if (!silent) loading.value = true
  error.value = ''
  try {
    const [items, settings] = await Promise.all([
      api.productionSubscriptions(),
      api.productionAutoSettings(),
    ])
    subscriptions.value = items
    auto.value = settings
  } catch (cause) {
    error.value = errorText(cause)
  } finally {
    loading.value = false
  }
}

async function saveAuto(patch: SaveAutoSettings): Promise<void> {
  autoSaving.value = true
  error.value = ''
  try {
    auto.value = await api.productionSaveAutoSettings(patch)
    flash(patch.enabled === false ? '自动检查已关闭；仍可手动点「立即检查」' : '自动检查设置已保存')
  } catch (cause) {
    error.value = errorText(cause)
  } finally {
    autoSaving.value = false
  }
}

async function toggleAuto(): Promise<void> {
  if (!auto.value || autoSaving.value) return
  await saveAuto({ enabled: !auto.value.enabled })
}

async function toggleSubscription(sub: ProductionSubscription): Promise<void> {
  if (busyId.value) return
  busyId.value = sub.id
  error.value = ''
  try {
    await api.productionSubscriptionSave({
      id: sub.id,
      mid: sub.mid,
      enabled: !sub.enabled,
      intervalMinutes: sub.intervalMinutes,
      templateId: sub.templateId,
      prompt: sub.prompt,
      filters: sub.filters,
    })
    await load(true)
    flash(!sub.enabled ? `已恢复「${sub.name}」的自动检查` : `已暂停「${sub.name}」`)
  } catch (cause) {
    error.value = errorText(cause)
  } finally {
    busyId.value = ''
  }
}

async function checkNow(sub: ProductionSubscription): Promise<void> {
  if (busyId.value) return
  busyId.value = sub.id
  error.value = ''
  try {
    const result = await api.productionSubscriptionCheck(sub.id)
    flash(`${sub.name}：${result.message}`)
    await load(true)
    if (result.newTasks > 0) emit('changed')
  } catch (cause) {
    error.value = errorText(cause)
    await load(true)
  } finally {
    busyId.value = ''
  }
}

function openAdd(): void {
  window.clearTimeout(lookupTimer)
  window.clearTimeout(videoTimer)
  editing.value = null
  profile.value = null
  lookupQuery.value = ''
  addTab.value = loginReady.value ? 'following' : 'link'
  followItems.value = []
  followQuery.value = ''
  followPage.value = 0
  followTotal.value = 0
  followHasMore.value = false
  followError.value = ''
  videoUrl.value = ''
  videoPreview.value = null
  videoError.value = ''
  formError.value = ''
  form.intervalMinutes = auto.value?.defaultIntervalMinutes || 30
  form.templateId = templates.value.some((item) => item.id === production.settings?.defaultTemplate)
    ? production.settings?.defaultTemplate || 'video-note'
    : templates.value[0]?.id || 'video-note'
  const preferredPrompt = production.settings?.defaultPrompt || ''
  form.prompt = visiblePrompts.value.some((item) => item.name === preferredPrompt)
    ? preferredPrompt
    : (visiblePrompts.value[0]?.name ?? '速览版')
  form.backfill = 0
  form.minMinutes = 0
  form.maxMinutes = 0
  form.includeKeywords = ''
  form.excludeKeywords = ''
  dialogOpen.value = true
  if (loginReady.value) void loadFollows(true)
}

function setTab(tab: 'following' | 'video' | 'link'): void {
  if (tab === 'following' && !loginReady.value) return
  addTab.value = tab
  formError.value = ''
  videoError.value = ''
  if (tab === 'following' && !followItems.value.length) void loadFollows(true)
  if (tab === 'video' && videoUrl.value.trim() && !videoPreview.value) void parseVideo()
}

async function loadFollows(reset = false): Promise<void> {
  if (!loginReady.value || followLoading.value) return
  if (!reset && !followHasMore.value) return
  const page = reset ? 1 : followPage.value + 1
  followLoading.value = true
  followError.value = ''
  try {
    const result = await api.productionBiliFollowings(page)
    const known = new Set(reset ? [] : followItems.value.map((item) => item.mid))
    const fresh = result.items.filter((item) => !known.has(item.mid))
    followItems.value = reset ? fresh : followItems.value.concat(fresh)
    followPage.value = result.page
    followTotal.value = result.total
    followHasMore.value = result.hasMore
  } catch (cause) {
    followError.value = errorText(cause)
  } finally {
    followLoading.value = false
  }
}

async function chooseFollowing(item: BiliFollowing): Promise<void> {
  profile.value = { mid: item.mid, name: item.name, face: item.face, fans: 0 }
  lookupQuery.value = String(item.mid)
  formError.value = ''
  // 关注列表没有粉丝数；补一次公开资料，失败也不阻塞选择。
  const seq = ++lookupSeq
  lookupLoading.value = true
  try {
    const result = await api.productionUpLookup(String(item.mid))
    if (seq === lookupSeq) profile.value = result
  } catch {
    // 名字/头像已经足够创建订阅，忽略资料补全失败。
  } finally {
    if (seq === lookupSeq) lookupLoading.value = false
  }
}

async function chooseMid(mid: number): Promise<void> {
  if (!mid) return
  const seq = ++lookupSeq
  lookupLoading.value = true
  formError.value = ''
  try {
    const result = await api.productionUpLookup(String(mid))
    if (seq !== lookupSeq) return
    profile.value = result
    lookupQuery.value = String(result.mid)
  } catch (cause) {
    if (seq === lookupSeq) formError.value = errorText(cause)
  } finally {
    if (seq === lookupSeq) lookupLoading.value = false
  }
}

async function parseVideo(manual = false): Promise<void> {
  const url = videoUrl.value.trim()
  if (!url || !dialogOpen.value) {
    if (manual && !url) videoError.value = '先粘贴一个视频链接'
    return
  }
  const seq = ++videoSeq
  videoLoading.value = true
  videoError.value = ''
  videoPreview.value = null
  try {
    const result = await api.productionBiliPreview(url)
    if (seq === videoSeq) videoPreview.value = result
  } catch (cause) {
    if (seq === videoSeq) videoError.value = errorText(cause)
  } finally {
    if (seq === videoSeq) videoLoading.value = false
  }
}

async function useVideoOwner(): Promise<void> {
  const preview = videoPreview.value
  if (!preview?.ownerMid) {
    videoError.value = '没有从视频里识别到 UP 主；可以改用关注列表或空间链接'
    return
  }
  await chooseMid(preview.ownerMid)
}

function resetProfile(): void {
  window.clearTimeout(lookupTimer)
  profile.value = null
  lookupQuery.value = ''
  formError.value = ''
  if (addTab.value === 'following' && !followItems.value.length) void loadFollows(true)
}

watch(
  () => videoUrl.value,
  (next) => {
    window.clearTimeout(videoTimer)
    videoPreview.value = null
    videoError.value = ''
    if (!next.trim() || editing.value) return
    videoTimer = window.setTimeout(() => { void parseVideo(false) }, 520)
  },
)

function openEdit(sub: ProductionSubscription): void {
  window.clearTimeout(lookupTimer)
  window.clearTimeout(videoTimer)
  editing.value = sub
  profile.value = {
    mid: sub.mid,
    name: sub.name,
    face: sub.avatar,
    fans: 0,
  }
  lookupQuery.value = String(sub.mid)
  formError.value = ''
  form.intervalMinutes = sub.intervalMinutes
  form.templateId = sub.templateId
  form.prompt = sub.prompt
  form.backfill = 0
  form.minMinutes = Math.round((sub.filters.minDurationSec || 0) / 60)
  form.maxMinutes = Math.round((sub.filters.maxDurationSec || 0) / 60)
  form.includeKeywords = sub.filters.includeKeywords || ''
  form.excludeKeywords = sub.filters.excludeKeywords || ''
  dialogOpen.value = true
}

async function lookup(manual = false): Promise<void> {
  const query = lookupQuery.value.trim()
  if (!query || !dialogOpen.value) return
  const mid = parseMidText(query)
  if (!mid) {
    if (manual) formError.value = '没有识别到 UID；支持 UID、space.bilibili.com 或 m.bilibili.com 空间链接'
    return
  }
  const seq = ++lookupSeq
  lookupLoading.value = true
  formError.value = ''
  if (!profile.value || profile.value.mid !== mid) profile.value = null
  try {
    const result = await api.productionUpLookup(String(mid))
    if (seq !== lookupSeq) return
    profile.value = result
    // 读取成功后把长分享链接收成 UID，避免 ?spm_id_from=... 一直占着输入框。
    lookupQuery.value = String(result.mid)
  } catch (cause) {
    if (seq === lookupSeq) formError.value = errorText(cause)
  } finally {
    if (seq === lookupSeq) lookupLoading.value = false
  }
}

watch(
  () => lookupQuery.value,
  (next) => {
    if (editing.value) return
    window.clearTimeout(lookupTimer)
    if (profile.value && next.trim() !== String(profile.value.mid)) {
      profile.value = null
    }
    formError.value = ''
    if (profile.value && next.trim() === String(profile.value.mid)) return
    if (!parseMidText(next)) return
    // 粘贴/输入到可识别内容后自动读取；按钮仍保留为失败重试。
    lookupTimer = window.setTimeout(() => { void lookup(false) }, 380)
  },
)

function inputPayload(): SubscriptionInput {
  return {
    id: editing.value?.id,
    mid: profile.value?.mid || editing.value?.mid || 0,
    name: profile.value?.name || editing.value?.name || '',
    avatar: profile.value?.face || editing.value?.avatar || '',
    enabled: editing.value?.enabled ?? true,
    intervalMinutes: Math.max(10, Math.min(1440, Number(form.intervalMinutes) || 30)),
    templateId: form.templateId,
    prompt: form.prompt,
    filters: {
      minDurationSec: Math.max(0, Math.round((Number(form.minMinutes) || 0) * 60)),
      maxDurationSec: Math.max(0, Math.round((Number(form.maxMinutes) || 0) * 60)),
      includeKeywords: form.includeKeywords.trim(),
      excludeKeywords: form.excludeKeywords.trim(),
    },
    backfill: editing.value ? 0 : form.backfill,
  }
}

async function submit(): Promise<void> {
  if (!profile.value || submitting.value) return
  const min = Number(form.minMinutes) || 0
  const max = Number(form.maxMinutes) || 0
  if (min > 0 && max > 0 && min > max) {
    formError.value = '最短时长不能大于最长时长'
    return
  }
  submitting.value = true
  formError.value = ''
  try {
    const saved = await api.productionSubscriptionSave(inputPayload())
    dialogOpen.value = false
    await load(true)
    emit('changed')
    if (editing.value) {
      flash(`已保存「${saved.name}」的订阅设置`)
    } else if (saved.lastError) {
      flash(`已添加「${saved.name}」，但首次检查失败：${saved.lastError}`)
    } else {
      flash(`已添加「${saved.name}」；${saved.lastMessage || '自动检查已开启'}`)
    }
  } catch (cause) {
    formError.value = errorText(cause)
  } finally {
    submitting.value = false
  }
}

async function confirmDelete(): Promise<void> {
  const sub = deleteTarget.value
  if (!sub) return
  busyId.value = sub.id
  try {
    await api.productionSubscriptionDelete(sub.id)
    deleteTarget.value = null
    await load(true)
    flash(`已取消订阅「${sub.name}」；已生成的笔记不会被删除`)
  } catch (cause) {
    error.value = errorText(cause)
  } finally {
    busyId.value = ''
  }
}

async function openSeen(sub: ProductionSubscription): Promise<void> {
  seenOpen.value = true
  seenTitle.value = sub.name
  seenItems.value = []
  seenError.value = ''
  seenLoading.value = true
  try {
    seenItems.value = await api.productionSubscriptionSeen(sub.id)
  } catch (cause) {
    seenError.value = errorText(cause)
  } finally {
    seenLoading.value = false
  }
}

function openVideo(bvid: string): void {
  void api.openExternalUrl(`https://www.bilibili.com/video/${bvid}`)
}

function openTask(video: SeenVideo): void {
  if (!video.jobId) return
  productionUi.taskId = video.jobId
  seenOpen.value = false
}

onMounted(async () => {
  if (!production.loaded) await load(true)
  await load()
  clockTimer = window.setInterval(() => { now.value = Date.now() }, 30_000)
  void api.onSubscriptionsChanged(() => { void load(true) }).then((stop) => {
    stopSubscriptionEvents = stop
  })
})

onBeforeUnmount(() => {
  window.clearInterval(clockTimer)
  window.clearTimeout(noticeTimer)
  window.clearTimeout(lookupTimer)
  window.clearTimeout(videoTimer)
  stopSubscriptionEvents?.()
})
</script>

<template>
  <div class="sub-page">
    <div class="prod-page-head">
      <div>
        <div class="prod-eyebrow">制作 / UP 订阅</div>
        <h1>UP 主更新，自动开始制作</h1>
        <p>只处理你选定的 UP 主；新视频先建立基线，再把基线之后发布的视频排成任务。自动生成的笔记先放到「ScribeFlow/待整理」，最终放哪由你移动。</p>
      </div>
      <div class="prod-page-actions">
        <button type="button" class="prod-btn is-primary" :disabled="!templates.length" @click="openAdd">
          <Plus :size="14" />添加 UP 主
        </button>
      </div>
    </div>

    <div v-if="notice" class="prod-banner sub-banner"><span class="prod-banner-tag">完成</span>{{ notice }}</div>
    <div v-if="error" class="prod-error-box sub-banner"><p>{{ error }}</p></div>

    <div v-if="!loginReady" class="sub-callout is-warn">
      <AlertTriangle :size="16" />
      <div>
        <b>还没有登录 B 站</b>
        <span>UP 主空间视频列表需要登录 Cookie。你可以先添加订阅；登录后点一次「立即检查」补上基线，自动检查才会开始。</span>
      </div>
      <button type="button" class="prod-btn is-small" @click="emit('login')">扫码登录</button>
    </div>
    <div v-else-if="budgetPaused" class="sub-callout is-error">
      <AlertTriangle :size="16" />
      <div>
        <b>本月预算已用完，自动检查暂停</b>
        <span>已花 ¥{{ auto?.monthlySpendCny.toFixed(2) }} / 预算 ¥{{ auto?.monthlyBudgetCny.toFixed(2) }}。提高预算或关闭「预算用完时暂停」后恢复。</span>
      </div>
      <button type="button" class="prod-btn is-small" @click="productionUi.view = 'settings'">打开设置</button>
    </div>

    <section class="sub-control">
      <div class="sub-control-main">
        <span class="sub-control-icon"><Rss :size="17" /></span>
        <div>
          <b>自动检查</b>
          <span>{{ auto?.enabled ? '开启：应用运行时按间隔检查；关闭后只能手动检查。' : '已关闭：不会自动检查，已有任务仍会继续制作。' }}</span>
        </div>
        <button
          type="button"
          class="sub-switch"
          role="switch"
          :aria-checked="Boolean(auto?.enabled)"
          :aria-label="auto?.enabled ? '关闭自动检查' : '开启自动检查'"
          :class="{ 'is-on': auto?.enabled }"
          :disabled="autoSaving || !auto"
          @click="toggleAuto"
        >
          <span />
        </button>
      </div>
      <div class="sub-control-stats">
        <div><span>订阅</span><b>{{ subscriptions.length }}</b><small>{{ enabledCount }} 个在检查</small></div>
        <div><span>自动任务</span><b>{{ autoTaskCount }}</b><small>累计排队</small></div>
        <div><span>下次检查</span><b>{{ nextCheck ? futureTime(nextCheck) : '—' }}</b><small>{{ nextCheck ? formatDateTime(nextCheck) : nextCheckHint }}</small></div>
      </div>
      <p class="sub-control-note"><Clock3 :size="12" /> 检查在应用进程内运行，建议保持页边笔记打开。每次最多排 {{ auto?.maxPerCheck ?? 3 }} 个任务，每天最多 {{ auto?.maxPerDay ?? 10 }} 个；间隔至少 2 秒，遇到 B 站风控会自动退避。</p>
    </section>

    <div class="sub-section-head">
      <div><h2>订阅的 UP 主</h2><p>暂停只停止检查；已经创建的制作任务不会取消。</p></div>
      <button type="button" class="prod-btn is-small" :disabled="loading" @click="load()"><RefreshCw :size="12" :class="{ 'sub-spin': loading }" />刷新</button>
    </div>

    <div v-if="loading && !subscriptions.length" class="sub-loading"><LoaderCircle :size="16" class="sub-spin" />正在读取订阅…</div>
    <div v-else-if="!subscriptions.length" class="sub-empty">
      <span class="sub-empty-icon"><UserRound :size="24" /></span>
      <b>还没有订阅 UP 主</b>
      <p>添加一个 UID 或空间链接。第一次检查会把当前视频记为基线，不会把旧视频一次性全跑一遍；需要补跑时可以显式选择最近 3 / 5 / 10 条。</p>
      <button type="button" class="prod-btn is-primary" :disabled="!templates.length" @click="openAdd"><Plus :size="14" />添加第一个 UP 主</button>
    </div>

    <div v-else class="sub-list">
      <article v-for="sub in subscriptions" :key="sub.id" class="sub-row" :class="{ 'is-paused': !sub.enabled }">
        <div class="sub-row-head">
          <img v-if="sub.avatar" class="sub-avatar" :src="avatarUrl(sub.avatar)" alt="" referrerpolicy="no-referrer" loading="lazy" />
          <span v-else class="sub-avatar sub-avatar-fallback">{{ sub.name.slice(0, 1) || '?' }}</span>
          <div class="sub-identity">
            <div class="sub-name-line">
              <b>{{ sub.name }}</b>
              <span v-if="!sub.enabled" class="sub-pill">已暂停</span>
              <span v-else-if="sub.lastError" class="sub-pill is-error">需要处理</span>
              <span v-else class="sub-pill is-ok">自动检查</span>
            </div>
            <span class="sub-meta">UID {{ sub.mid }} · 每 {{ sub.intervalMinutes }} 分钟 · {{ sub.templateId === 'video-note' ? '视频笔记' : sub.templateId }} · {{ sub.prompt }}</span>
          </div>
          <div class="sub-row-actions">
            <button
              type="button"
              class="sub-switch is-small"
              role="switch"
              :aria-checked="sub.enabled"
              :aria-label="sub.enabled ? `暂停 ${sub.name}` : `恢复 ${sub.name}`"
              :class="{ 'is-on': sub.enabled }"
              :disabled="Boolean(busyId)"
              @click="toggleSubscription(sub)"
            >
              <span />
            </button>
            <button type="button" class="prod-btn is-small" :disabled="Boolean(busyId)" @click="openEdit(sub)"><Settings2 :size="12" />设置</button>
          </div>
        </div>

        <div class="sub-latest">
          <Video :size="14" />
          <div>
            <b>{{ sub.latestTitle || '还没有读取到视频' }}</b>
            <span v-if="sub.latestPubdate">最新发布 {{ formatDateTime(sub.latestPubdate) }} · 已记录 {{ sub.seenCount }} 条 · 自动任务 {{ sub.queuedCount }} 个</span>
            <span v-else>第一次检查后会显示最新视频和基线数量</span>
          </div>
          <button v-if="sub.latestBvid" type="button" class="sub-inline-link" @click="openVideo(sub.latestBvid)">打开 B 站<ExternalLink :size="11" /></button>
        </div>

        <div class="sub-status" :class="statusTone(sub)">
          <span class="sub-status-dot" />
          <div>
            <b>{{ statusText(sub) }}</b>
            <span v-if="sub.lastCheckedAt">上次检查 {{ relativeTime(sub.lastCheckedAt) }} · {{ nextText(sub) }}</span>
            <span v-else>{{ nextText(sub) }}</span>
          </div>
        </div>

        <div class="sub-row-foot">
          <button type="button" class="sub-action" :disabled="Boolean(busyId)" @click="checkNow(sub)">
            <LoaderCircle v-if="busyId === sub.id" :size="12" class="sub-spin" /><RefreshCw v-else :size="12" />立即检查
          </button>
          <button type="button" class="sub-action" @click="openSeen(sub)"><Clock3 :size="12" />最近视频</button>
          <button type="button" class="sub-action is-danger" :disabled="Boolean(busyId)" @click="deleteTarget = sub"><Trash2 :size="12" />取消订阅</button>
          <span class="sub-foot-note">{{ sub.baselineAt ? `基线已建立 · ${formatDateTime(sub.baselineAt)}` : '等待建立基线' }}</span>
        </div>

        <div v-if="deleteTarget?.id === sub.id" class="sub-confirm">
          <div>
            <b>取消订阅「{{ sub.name }}」？</b>
            <span>停止自动检查，并清除这个 UP 的已见记录；已经生成的笔记和正在运行的任务不会删除。重新订阅会重新建立基线。</span>
          </div>
          <div class="sub-confirm-actions">
            <button type="button" class="prod-btn is-small" @click="deleteTarget = null">保留订阅</button>
            <button type="button" class="prod-btn is-small is-danger" :disabled="Boolean(busyId)" @click="confirmDelete">确认取消</button>
          </div>
        </div>
      </article>
    </div>

    <div v-if="dialogOpen" class="prod-overlay" @click.self="dialogOpen = false">
      <div class="prod-dialog sub-dialog" role="dialog" aria-modal="true" aria-labelledby="subDialogTitle">
        <header class="prod-dialog-head">
          <div>
            <div class="prod-eyebrow">{{ editing ? 'UP 订阅 / 设置' : 'UP 订阅 / 新增' }}</div>
            <h2 id="subDialogTitle">{{ title }}</h2>
          </div>
          <button type="button" class="prod-icon-btn" aria-label="关闭" @click="dialogOpen = false"><X :size="15" /></button>
        </header>

        <div class="sub-dialog-body">
          <div v-if="!editing && !profile" class="sub-source-tabs" role="tablist" aria-label="选择 UP 主的方式">
            <button type="button" role="tab" :aria-selected="addTab === 'following'" :class="{ 'is-active': addTab === 'following' }" :disabled="!loginReady" @click="setTab('following')">我的关注<span v-if="followTotal">{{ followTotal }}</span></button>
            <button type="button" role="tab" :aria-selected="addTab === 'video'" :class="{ 'is-active': addTab === 'video' }" @click="setTab('video')">视频链接</button>
            <button type="button" role="tab" :aria-selected="addTab === 'link'" :class="{ 'is-active': addTab === 'link' }" @click="setTab('link')">UID / 空间链接</button>
          </div>

          <div v-if="!editing && !profile && addTab === 'following'" class="sub-follow-panel">
            <div v-if="!loginReady" class="sub-lookup-empty">
              <UserRound :size="18" />
              <span>登录 B 站后可以在这里直接选关注列表；也可以先用视频链接或 UID 添加。</span>
            </div>
            <template v-else>
              <div class="sub-follow-search">
                <input v-model="followQuery" class="prod-input" placeholder="在已加载的关注里搜索名字或 UID…" />
                <span>{{ filteredFollows.length }} / {{ followTotal || '—' }}</span>
              </div>
              <p class="sub-field-hint">只读取你已有的关注关系，不会自动关注或修改账号。当前加载 {{ followItems.length }} 个<template v-if="followHasMore">，可继续加载</template>。</p>
              <div v-if="followLoading && !followItems.length" class="sub-loading"><LoaderCircle :size="15" class="sub-spin" />正在读取关注列表…</div>
              <div v-else-if="followError" class="prod-error-box">
                <p>{{ followError }}</p>
                <button type="button" class="prod-btn is-small" @click="loadFollows(true)">重试</button>
              </div>
              <div v-else-if="!filteredFollows.length" class="sub-lookup-empty">
                <UserRound :size="18" />
                <span>{{ followQuery ? '已加载的关注里没有匹配项；可以点“加载更多”再搜索。' : '关注列表为空，改用视频链接或 UID 添加。' }}</span>
              </div>
              <div v-else class="sub-follow-list">
                <button v-for="item in filteredFollows" :key="item.mid" type="button" class="sub-follow-row" @click="chooseFollowing(item)">
                  <img v-if="item.face" class="sub-follow-avatar" :src="avatarUrl(item.face)" alt="" referrerpolicy="no-referrer" loading="lazy" />
                  <span v-else class="sub-follow-avatar sub-avatar-fallback">{{ item.name.slice(0, 1) || '?' }}</span>
                  <span class="sub-follow-copy"><b>{{ item.name }}</b><small>UID {{ item.mid }}<template v-if="item.sign"> · {{ item.sign }}</template></small></span>
                  <span class="sub-follow-pick">选择</span>
                </button>
              </div>
              <button v-if="followHasMore" type="button" class="prod-btn is-small sub-follow-more" :disabled="followLoading" @click="loadFollows(false)">
                <LoaderCircle v-if="followLoading" :size="12" class="sub-spin" />{{ followLoading ? '加载中' : `加载更多（已加载 ${followItems.length} / ${followTotal}）` }}
              </button>
            </template>
          </div>

          <div v-if="!editing && !profile && addTab === 'video'" class="sub-video-panel">
            <label class="prod-field is-wide">
              <span>视频链接</span>
              <div class="sub-lookup">
                <input v-model="videoUrl" class="prod-input" placeholder="粘贴 BV / av / b23 视频链接，解析作者" @keyup.enter="parseVideo(true)" />
                <button type="button" class="prod-btn" :disabled="videoLoading || !videoUrl.trim()" @click="parseVideo(true)">
                  <LoaderCircle v-if="videoLoading" :size="13" class="sub-spin" /><Video v-else :size="13" />{{ videoLoading ? '解析中' : '解析视频' }}
                </button>
              </div>
            </label>
            <p class="sub-field-hint">先确认视频标题和封面，再订阅它的作者；适合从某条视频找到 UP 主。</p>
            <div v-if="videoLoading" class="sub-loading"><LoaderCircle :size="15" class="sub-spin" />正在解析视频…</div>
            <div v-else-if="videoError" class="prod-error-box"><p>{{ videoError }}</p></div>
            <div v-else-if="videoPreview" class="sub-video-preview">
              <img v-if="videoPreview.cover" class="sub-video-cover" :src="avatarUrl(videoPreview.cover)" alt="" referrerpolicy="no-referrer" />
              <span v-else class="sub-video-cover sub-seen-cover-empty"><Video :size="16" /></span>
              <div class="sub-video-copy">
                <b>{{ videoPreview.title }}</b>
                <span>{{ videoPreview.author }} · {{ videoPreview.bvid }}</span>
              </div>
              <button type="button" class="prod-btn is-primary is-small" :disabled="!videoPreview.ownerMid || lookupLoading" @click="useVideoOwner">
                <LoaderCircle v-if="lookupLoading" :size="12" class="sub-spin" />{{ lookupLoading ? '读取 UP 主' : '订阅这个 UP' }}
              </button>
            </div>
          </div>

          <div v-if="!editing && !profile && addTab === 'link'" class="sub-link-panel">
            <label class="prod-field is-wide">
              <span>UP 主 UID 或空间链接</span>
              <div class="sub-lookup">
                <input v-model="lookupQuery" class="prod-input" placeholder="UID、空间链接或分享文案，粘贴后自动识别" @keyup.enter="lookup(true)" />
                <button type="button" class="prod-btn" :disabled="lookupLoading || !lookupQuery.trim()" @click="lookup(true)">
                  <LoaderCircle v-if="lookupLoading" :size="13" class="sub-spin" /><RefreshCw v-else-if="profile" :size="13" /><UserRound v-else :size="13" />{{ lookupLoading ? '读取中' : profile ? '重新读取' : '读取 UP 主' }}
                </button>
              </div>
            </label>
            <p v-if="lookupHint" class="sub-field-hint" :class="{ 'is-error': !lookupLoading && !profile && !parsedMid }">{{ lookupHint }}</p>
            <p v-else class="sub-field-hint">只读取公开的 UP 主资料和空间视频列表；不会关注、点赞或修改 B 站账号。</p>
          </div>

          <div v-if="profile" class="sub-profile">
            <img v-if="profile.face" class="sub-profile-avatar" :src="avatarUrl(profile.face)" alt="" referrerpolicy="no-referrer" />
            <span v-else class="sub-profile-avatar sub-avatar-fallback">{{ profile.name.slice(0, 1) || '?' }}</span>
            <div>
              <b>{{ profile.name }}</b>
              <span>UID {{ profile.mid }}<template v-if="profile.fans"> · {{ formatNumber(profile.fans) }} 粉丝</template></span>
            </div>
            <button v-if="!editing" type="button" class="prod-btn is-small" @click="resetProfile"><RefreshCw :size="12" />换一个</button>
            <span v-else class="sub-pill">UID 不可修改</span>
          </div>

          <div v-if="profile" class="sub-form">
            <div class="sub-form-grid">
              <label class="prod-field">
                <span>检查间隔</span>
                <select v-model.number="form.intervalMinutes" class="prod-select">
                  <option v-for="minutes in intervalOptions" :key="minutes" :value="minutes">{{ minutes < 60 ? `${minutes} 分钟` : `${minutes / 60} 小时` }}</option>
                </select>
              </label>
              <label class="prod-field">
                <span>使用模板</span>
                <select v-model="form.templateId" class="prod-select">
                  <option v-for="id in watchedTemplates" :key="id" :value="id">{{ templates.find((item) => item.id === id)?.name || production.templates.find((item) => item.id === id)?.name || id }}</option>
                </select>
              </label>
              <label class="prod-field">
                <span>提示词</span>
                <select v-model="form.prompt" class="prod-select">
                  <option v-for="name in prompts" :key="name" :value="name">{{ name }}{{ visiblePrompts.some((item) => item.name === name) ? '' : '（已隐藏）' }}</option>
                </select>
              </label>
            </div>

            <p v-if="form.templateId === 'video-note'" class="prod-wizard-hint" style="margin-top: 8px">视频笔记不跑联网溯源；需要自动溯源，把模板改成「速览与溯源」。</p>

            <div v-if="!editing" class="sub-block">
              <div class="sub-block-head"><b>首次检查</b><span>默认不补跑旧视频，避免一添加就产生一堆成本。</span></div>
              <div class="sub-choice-grid">
                <label v-for="choice in backfillChoices" :key="choice.value" class="sub-choice" :class="{ 'is-selected': form.backfill === choice.value }">
                  <input v-model.number="form.backfill" type="radio" :value="choice.value" />
                  <span><b>{{ choice.label }}</b><small>{{ choice.hint }}</small></span>
                </label>
              </div>
              <p v-if="form.backfill > 0" class="sub-warning"><AlertTriangle :size="12" /> 会立即创建 {{ form.backfill }} 个制作任务，每个任务都会调用语音识别和模型；任务仍受每日上限约束。</p>
            </div>

            <details class="sub-filters">
              <summary><SlidersHorizontal :size="13" />筛选视频（可选）<span>按标题和时长减少无关任务</span></summary>
              <div class="sub-form-grid is-two">
                <label class="prod-field"><span>最短时长（分钟，0 = 不限）</span><input v-model.number="form.minMinutes" type="number" min="0" step="1" class="prod-input" /></label>
                <label class="prod-field"><span>最长时长（分钟，0 = 不限）</span><input v-model.number="form.maxMinutes" type="number" min="0" step="1" class="prod-input" /></label>
                <label class="prod-field is-wide"><span>标题必须包含（逗号分隔，任一命中）</span><input v-model="form.includeKeywords" class="prod-input" placeholder="例如 AI, 模型, 访谈" /></label>
                <label class="prod-field is-wide"><span>标题排除（逗号分隔，命中即跳过）</span><input v-model="form.excludeKeywords" class="prod-input" placeholder="例如 直播, 切片, 预告" /></label>
              </div>
              <p class="sub-field-hint">被筛掉的视频会记录为「被筛选」，不会反复提示；修改筛选不会自动补跑以前被筛掉的视频。</p>
            </details>

            <div class="sub-cost-note">
              <AlertTriangle :size="14" />
              <div>
                <b>自动任务会产生真实成本</b>
                <span>每个任务都会转写音频并调用模型；可在设置里限制每次/每天任务数，并启用「本月预算用完时暂停」。结果先进入 <code>ScribeFlow/待整理</code>，不会自动塞进你的正式目录。</span>
              </div>
            </div>
            <p v-if="!loginReady" class="sub-warning"><AlertTriangle :size="12" /> 当前未登录 B 站：可以先保存订阅，但首次基线会失败；登录后点「立即检查」补上。</p>
          </div>

          <p v-if="formError" class="prod-warning-line is-error">{{ formError }}</p>
        </div>

        <footer class="prod-dialog-foot">
          <span class="sub-foot-hint">{{ footerHint }}</span>
          <span class="prod-spacer" />
          <button type="button" class="prod-btn" @click="dialogOpen = false">取消</button>
          <button type="button" class="prod-btn is-primary" :disabled="!canSubmit" @click="submit">
            <LoaderCircle v-if="submitting" :size="13" class="sub-spin" />
            {{ submitting ? '正在读取视频并建立基线…' : editing ? '保存设置' : form.backfill > 0 ? `添加并补跑 ${form.backfill} 条` : '添加并建立基线' }}
          </button>
        </footer>
      </div>
    </div>

    <div v-if="seenOpen" class="prod-overlay" @click.self="seenOpen = false">
      <div class="prod-dialog sub-seen-dialog" role="dialog" aria-modal="true" aria-labelledby="subSeenTitle">
        <header class="prod-dialog-head">
          <div><div class="prod-eyebrow">UP 订阅 / 最近视频</div><h2 id="subSeenTitle">{{ seenTitle }}</h2></div>
          <button type="button" class="prod-icon-btn" aria-label="关闭" @click="seenOpen = false"><X :size="15" /></button>
        </header>
        <div class="sub-dialog-body">
          <p class="sub-field-hint">这里只展示最近 50 条已经检查过的视频。基线视频不会自动制作；已有笔记和被筛选的视频会标明原因。</p>
          <div v-if="seenLoading" class="sub-loading"><LoaderCircle :size="15" class="sub-spin" />读取中…</div>
          <div v-else-if="seenError" class="prod-error-box"><p>{{ seenError }}</p></div>
          <div v-else-if="!seenItems.length" class="sub-lookup-empty"><Video :size="18" /><span>还没有检查记录。</span></div>
          <div v-else class="sub-seen-list">
            <div v-for="video in seenItems" :key="video.bvid" class="sub-seen-row">
              <img v-if="video.cover" class="sub-seen-cover" :src="avatarUrl(video.cover)" alt="" referrerpolicy="no-referrer" loading="lazy" />
              <span v-else class="sub-seen-cover sub-seen-cover-empty"><Video :size="14" /></span>
              <div class="sub-seen-main">
                <b>{{ video.title }}</b>
                <span>{{ formatDateTime(video.pubdate) }} · {{ video.bvid }}</span>
              </div>
              <span class="sub-pill" :class="statusClass(video.status)">{{ statusLabel(video.status) }}</span>
              <button v-if="video.jobId" type="button" class="sub-inline-link" @click="openTask(video)">查看任务</button>
              <button type="button" class="sub-inline-link" @click="openVideo(video.bvid)">B 站<ExternalLink :size="11" /></button>
            </div>
          </div>
        </div>
        <footer class="prod-dialog-foot"><span class="sub-foot-hint">状态来自本地记录；任务是否成功以任务页为准。</span><span class="prod-spacer" /><button type="button" class="prod-btn" @click="seenOpen = false">关闭</button></footer>
      </div>
    </div>
  </div>
</template>

<style scoped>
.sub-page { max-width: 980px; }
.sub-banner { margin-bottom: 10px; }
.sub-spin { animation: sub-spin 1s linear infinite; }
@keyframes sub-spin { to { transform: rotate(360deg); } }

.sub-callout {
  display: flex;
  align-items: flex-start;
  gap: 10px;
  margin-bottom: 12px;
  padding: 11px 13px;
  border: 1px solid var(--line);
  border-radius: 6px;
  background: var(--surface);
  color: var(--ink-2);
  font-size: 13px;
  line-height: 1.55;
}
.sub-callout svg { flex: none; margin-top: 1px; }
.sub-callout > div { display: grid; gap: 2px; flex: 1; }
.sub-callout b { color: var(--ink); font-weight: 500; }
.sub-callout.is-warn { border-color: var(--gold-line); background: var(--gold-soft); }
.sub-callout.is-warn > svg { color: var(--gold); }
.sub-callout.is-error { border-color: color-mix(in srgb, var(--red) 35%, var(--line)); background: color-mix(in srgb, var(--red) 7%, var(--surface)); }
.sub-callout.is-error > svg { color: var(--red); }

.sub-control {
  padding: 14px 16px 12px;
  border: 1px solid var(--line);
  border-radius: 7px;
  background: var(--surface);
}
.sub-control-main { display: flex; align-items: center; gap: 11px; }
.sub-control-icon { display: grid; place-items: center; width: 34px; height: 34px; flex: none; border: 1px solid var(--blue-line); border-radius: 7px; background: var(--blue-soft); color: var(--blue); }
.sub-control-main > div { display: grid; gap: 2px; flex: 1; min-width: 0; }
.sub-control-main b { color: var(--ink); font-family: var(--sans); font-size: 14px; font-weight: 500; }
.sub-control-main span { color: var(--ink-2); font-size: 13px; line-height: 1.5; }
.sub-control-stats { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 10px; margin-top: 13px; padding-top: 12px; border-top: 1px solid var(--line); }
.sub-control-stats div { display: grid; gap: 1px; }
.sub-control-stats span { color: var(--ink-3); font-size: 12px; }
.sub-control-stats b { color: var(--ink); font-family: var(--serif); font-size: 22px; font-weight: 400; line-height: 1.2; }
.sub-control-stats small { color: var(--ink-3); font-size: 12px; }
.sub-control-note { display: flex; align-items: flex-start; gap: 5px; margin: 11px 0 0; color: var(--ink-3); font-size: 12px; line-height: 1.55; }
.sub-control-note svg { flex: none; margin-top: 2px; }

.sub-switch {
  position: relative;
  width: 38px;
  height: 22px;
  flex: none;
  padding: 0;
  border: 1px solid var(--line-2);
  border-radius: 999px;
  background: var(--rail);
  transition: background .16s ease, border-color .16s ease;
}
.sub-switch span { position: absolute; top: 2px; left: 2px; width: 16px; height: 16px; border-radius: 50%; background: var(--surface); box-shadow: 0 1px 3px rgba(0,0,0,.18); transition: transform .16s ease; }
.sub-switch.is-on { border-color: var(--blue); background: var(--blue); }
.sub-switch.is-on span { transform: translateX(16px); }
.sub-switch.is-small { width: 32px; height: 19px; }
.sub-switch.is-small span { width: 13px; height: 13px; }
.sub-switch.is-small.is-on span { transform: translateX(13px); }
.sub-switch:disabled { opacity: .5; }

.sub-section-head { display: flex; align-items: flex-end; justify-content: space-between; gap: 12px; margin: 22px 0 9px; }
.sub-section-head h2 { margin: 0 0 2px; color: var(--ink); font-family: var(--serif); font-size: 21px; font-weight: 400; }
.sub-section-head p { margin: 0; color: var(--ink-3); font-size: 12px; line-height: 1.5; }
.sub-loading { display: flex; align-items: center; gap: 7px; padding: 22px 4px; color: var(--ink-3); font-size: 13px; }

.sub-empty { display: grid; justify-items: center; gap: 7px; padding: 36px 20px 40px; border: 1px dashed var(--line-2); border-radius: 8px; background: color-mix(in srgb, var(--surface) 65%, transparent); text-align: center; }
.sub-empty-icon { display: grid; place-items: center; width: 48px; height: 48px; margin-bottom: 2px; border: 1px solid var(--line); border-radius: 50%; background: var(--surface); color: var(--ink-3); }
.sub-empty b { color: var(--ink); font-family: var(--serif); font-size: 20px; font-weight: 400; }
.sub-empty p { max-width: 520px; margin: 0; color: var(--ink-2); font-size: 13px; line-height: 1.65; }
.sub-empty .prod-btn { margin-top: 5px; }

.sub-list { display: grid; gap: 9px; }
.sub-row { padding: 13px 15px 10px; border: 1px solid var(--line); border-radius: 7px; background: var(--surface); }
.sub-row.is-paused { background: color-mix(in srgb, var(--surface) 70%, var(--rail)); }
.sub-row-head { display: flex; align-items: center; gap: 10px; }
.sub-avatar { width: 38px; height: 38px; flex: none; border: 1px solid var(--line); border-radius: 50%; object-fit: cover; background: var(--rail); }
.sub-avatar-fallback { display: grid; place-items: center; color: var(--ink-3); font-family: var(--serif); font-size: 17px; }
.sub-identity { min-width: 0; flex: 1; }
.sub-name-line { display: flex; align-items: center; flex-wrap: wrap; gap: 6px; }
.sub-name-line b { color: var(--ink); font-family: var(--sans); font-size: 15px; font-weight: 500; }
.sub-meta { display: block; margin-top: 2px; color: var(--ink-3); font-size: 12px; line-height: 1.45; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.sub-pill { display: inline-flex; align-items: center; min-height: 20px; padding: 0 6px; border: 1px solid var(--line-2); border-radius: 999px; color: var(--ink-2); font-size: 12px; white-space: nowrap; }
.sub-pill.is-ok { border-color: color-mix(in srgb, var(--green) 35%, var(--line)); color: var(--green); }
.sub-pill.is-error { border-color: color-mix(in srgb, var(--red) 35%, var(--line)); color: var(--red); }
.sub-pill.is-blue { border-color: var(--blue-line); color: var(--blue); }
.sub-pill.is-gold { border-color: var(--gold-line); color: var(--gold); }
.sub-pill.is-muted { color: var(--ink-3); }
.sub-row-actions { display: flex; align-items: center; gap: 8px; flex: none; }

.sub-latest { display: flex; align-items: flex-start; gap: 7px; margin-top: 11px; padding: 9px 10px; border: 1px solid var(--line); border-radius: 5px; background: color-mix(in srgb, var(--rail) 55%, var(--surface)); }
.sub-latest > svg { flex: none; margin-top: 1px; color: var(--ink-3); }
.sub-latest > div { display: grid; gap: 1px; min-width: 0; flex: 1; }
.sub-latest b { overflow: hidden; color: var(--ink-2); font-size: 13px; font-weight: 400; line-height: 1.45; text-overflow: ellipsis; white-space: nowrap; }
.sub-latest span { color: var(--ink-3); font-size: 12px; }
.sub-inline-link { display: inline-flex; align-items: center; gap: 3px; flex: none; border: 0; padding: 0; background: transparent; color: var(--blue); font-size: 12px; cursor: pointer; }
.sub-inline-link:hover { text-decoration: underline; }

.sub-status { display: flex; align-items: flex-start; gap: 7px; margin-top: 9px; padding: 0 2px; }
.sub-status-dot { width: 6px; height: 6px; flex: none; margin-top: 6px; border-radius: 50%; background: var(--ink-3); }
.sub-status > div { display: grid; gap: 1px; }
.sub-status b { color: var(--ink-2); font-size: 13px; font-weight: 400; line-height: 1.5; overflow-wrap: anywhere; }
.sub-status span { color: var(--ink-3); font-size: 12px; }
.sub-status.is-ok .sub-status-dot { background: var(--green); }
.sub-status.is-error .sub-status-dot { background: var(--red); }
.sub-status.is-error b { color: var(--red); }
.sub-status.is-warn .sub-status-dot { background: var(--gold); }
.sub-status.is-warn b { color: var(--gold); }

.sub-row-foot { display: flex; align-items: center; flex-wrap: wrap; gap: 2px 10px; margin-top: 8px; padding-top: 8px; border-top: 1px solid var(--line); }
.sub-action { display: inline-flex; align-items: center; gap: 4px; min-height: 26px; padding: 0 3px; border: 0; background: transparent; color: var(--ink-2); font-size: 12px; cursor: pointer; }
.sub-action:hover { color: var(--blue); }
.sub-action.is-danger:hover { color: var(--red); }
.sub-action:disabled { opacity: .45; cursor: default; }
.sub-foot-note { margin-left: auto; color: var(--ink-3); font-size: 12px; }
.sub-confirm { display: flex; align-items: flex-start; gap: 12px; margin-top: 9px; padding: 10px 11px; border: 1px solid color-mix(in srgb, var(--red) 30%, var(--line)); border-radius: 5px; background: color-mix(in srgb, var(--red) 6%, var(--surface)); }
.sub-confirm > div:first-child { display: grid; gap: 2px; flex: 1; }
.sub-confirm b { color: var(--ink); font-size: 13px; font-weight: 500; }
.sub-confirm span { color: var(--ink-2); font-size: 12px; line-height: 1.55; }
.sub-confirm-actions { display: flex; gap: 5px; flex: none; }

.sub-dialog { width: min(760px, 100%); }
.sub-dialog-body { padding: 2px 20px 18px; }
.sub-lookup { display: flex; gap: 6px; }
.sub-lookup .prod-input { flex: 1; }
.sub-field-hint { margin: 6px 0 0; color: var(--ink-3); font-size: 12px; line-height: 1.55; }
.sub-field-hint.is-error { color: var(--red); }
.sub-dialog .prod-dialog-head { position: sticky; top: 0; z-index: 3; background: var(--surface); }
.sub-dialog .prod-dialog-foot { position: sticky; bottom: 0; z-index: 3; background: var(--surface); }

.sub-source-tabs { display: flex; gap: 2px; margin: 0 0 13px; padding-bottom: 8px; border-bottom: 1px solid var(--line); }
.sub-source-tabs button { display: inline-flex; align-items: center; gap: 5px; min-height: 30px; padding: 0 10px; border: 1px solid transparent; border-radius: 4px; background: transparent; color: var(--ink-2); font-family: var(--sans); font-size: 13px; }
.sub-source-tabs button:hover { background: var(--rail); color: var(--ink); }
.sub-source-tabs button.is-active { border-color: var(--blue-line); background: var(--blue-soft); color: var(--blue); }
.sub-source-tabs button:disabled { opacity: .45; cursor: default; }
.sub-source-tabs span { color: var(--ink-3); font-family: var(--mono); font-size: 12px; }
.sub-source-tabs button.is-active span { color: var(--blue); }

.sub-follow-search { display: flex; align-items: center; gap: 8px; }
.sub-follow-search .prod-input { flex: 1; }
.sub-follow-search > span { flex: none; color: var(--ink-3); font-family: var(--mono); font-size: 12px; }
.sub-follow-list { max-height: 300px; margin-top: 9px; overflow: auto; border: 1px solid var(--line); border-radius: 6px; }
.sub-follow-row { display: flex; align-items: center; gap: 9px; width: 100%; min-height: 48px; padding: 6px 9px; border: 0; border-top: 1px solid var(--line); background: var(--surface); text-align: left; }
.sub-follow-row:first-child { border-top: 0; }
.sub-follow-row:hover { background: var(--blue-soft); }
.sub-follow-avatar { width: 32px; height: 32px; flex: none; border: 1px solid var(--line); border-radius: 50%; object-fit: cover; background: var(--rail); }
.sub-follow-copy { display: grid; gap: 1px; min-width: 0; flex: 1; }
.sub-follow-copy b { overflow: hidden; color: var(--ink); font-size: 13px; font-weight: 500; text-overflow: ellipsis; white-space: nowrap; }
.sub-follow-copy small { overflow: hidden; color: var(--ink-3); font-size: 12px; text-overflow: ellipsis; white-space: nowrap; }
.sub-follow-pick { flex: none; color: var(--blue); font-size: 12px; }
.sub-follow-more { margin-top: 8px; }
.sub-video-preview { display: flex; align-items: center; gap: 10px; margin-top: 11px; padding: 9px 10px; border: 1px solid var(--line); border-radius: 6px; background: var(--rail); }
.sub-video-cover { width: 86px; height: 54px; flex: none; border: 1px solid var(--line); border-radius: 4px; object-fit: cover; background: var(--surface); }
.sub-video-copy { display: grid; gap: 2px; min-width: 0; flex: 1; }
.sub-video-copy b { display: -webkit-box; overflow: hidden; color: var(--ink); font-size: 13px; font-weight: 400; line-height: 1.45; -webkit-box-orient: vertical; -webkit-line-clamp: 2; }
.sub-video-copy span { color: var(--ink-3); font-size: 12px; }
.sub-lookup-empty { display: flex; align-items: center; gap: 8px; margin-top: 14px; padding: 14px; border: 1px dashed var(--line-2); border-radius: 6px; color: var(--ink-3); font-size: 13px; }
.sub-profile { display: flex; align-items: center; gap: 10px; margin-top: 13px; padding: 10px 12px; border: 1px solid var(--line); border-radius: 6px; background: var(--rail); }
.sub-profile-avatar { width: 40px; height: 40px; flex: none; border: 1px solid var(--line); border-radius: 50%; object-fit: cover; background: var(--surface); }
.sub-profile > div { display: grid; gap: 2px; flex: 1; min-width: 0; }
.sub-profile b { color: var(--ink); font-family: var(--sans); font-size: 14px; font-weight: 500; }
.sub-profile span { color: var(--ink-3); font-size: 12px; }
.sub-form { margin-top: 14px; }
.sub-form-grid { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 10px; }
.sub-form-grid.is-two { grid-template-columns: repeat(2, minmax(0, 1fr)); }
.sub-block { margin-top: 16px; }
.sub-block-head { display: flex; align-items: baseline; justify-content: space-between; gap: 10px; margin-bottom: 7px; }
.sub-block-head b { color: var(--ink); font-size: 13px; font-weight: 500; }
.sub-block-head span { color: var(--ink-3); font-size: 12px; line-height: 1.45; text-align: right; }
.sub-choice-grid { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: 6px; }
.sub-choice { display: flex; align-items: flex-start; gap: 6px; min-height: 58px; padding: 8px; border: 1px solid var(--line); border-radius: 5px; background: var(--surface); cursor: pointer; }
.sub-choice input { margin-top: 2px; accent-color: var(--blue); }
.sub-choice > span { display: grid; gap: 2px; }
.sub-choice b { color: var(--ink-2); font-size: 12px; font-weight: 500; }
.sub-choice small { color: var(--ink-3); font-size: 12px; line-height: 1.4; }
.sub-choice.is-selected { border-color: var(--blue); background: var(--blue-soft); }
.sub-choice.is-selected b { color: var(--blue); }
.sub-warning { display: flex; align-items: flex-start; gap: 5px; margin: 8px 0 0; color: var(--gold); font-size: 12px; line-height: 1.55; }
.sub-warning svg { flex: none; margin-top: 2px; }
.sub-filters { margin-top: 15px; padding: 9px 10px; border: 1px solid var(--line); border-radius: 6px; background: color-mix(in srgb, var(--rail) 45%, var(--surface)); }
.sub-filters summary { display: flex; align-items: center; gap: 6px; color: var(--ink-2); font-size: 13px; cursor: pointer; list-style: none; }
.sub-filters summary::-webkit-details-marker { display: none; }
.sub-filters summary span { margin-left: auto; color: var(--ink-3); font-size: 12px; }
.sub-filters .sub-form-grid { margin-top: 10px; }
.sub-cost-note { display: flex; align-items: flex-start; gap: 8px; margin-top: 14px; padding: 10px 11px; border: 1px solid var(--gold-line); border-radius: 5px; background: var(--gold-soft); }
.sub-cost-note > svg { flex: none; margin-top: 1px; color: var(--gold); }
.sub-cost-note > div { display: grid; gap: 2px; }
.sub-cost-note b { color: var(--ink); font-size: 13px; font-weight: 500; }
.sub-cost-note span { color: var(--ink-2); font-size: 12px; line-height: 1.55; }
.sub-cost-note code { padding: 1px 4px; border: 1px solid var(--line); border-radius: 3px; background: var(--surface); font-family: var(--mono); font-size: 12px; }
.sub-dialog .prod-warning-line { margin: 12px 0 0; }
.sub-foot-hint { color: var(--ink-3); font-size: 12px; }
.sub-seen-dialog { width: min(760px, 100%); }
.sub-seen-list { display: grid; gap: 1px; margin-top: 10px; }
.sub-seen-row { display: flex; align-items: center; gap: 9px; padding: 8px 2px; border-top: 1px solid var(--line); }
.sub-seen-cover { width: 58px; height: 36px; flex: none; border: 1px solid var(--line); border-radius: 4px; object-fit: cover; background: var(--rail); }
.sub-seen-cover-empty { display: grid; place-items: center; color: var(--ink-3); }
.sub-seen-main { display: grid; gap: 1px; min-width: 0; flex: 1; }
.sub-seen-main b { overflow: hidden; color: var(--ink-2); font-size: 13px; font-weight: 400; text-overflow: ellipsis; white-space: nowrap; }
.sub-seen-main span { color: var(--ink-3); font-size: 12px; }

@media (max-width: 760px) {
  .sub-control-stats { grid-template-columns: 1fr 1fr; }
  .sub-row-head { align-items: flex-start; }
  .sub-row-actions { flex-direction: column; align-items: flex-end; }
  .sub-choice-grid { grid-template-columns: 1fr 1fr; }
  .sub-form-grid, .sub-form-grid.is-two { grid-template-columns: 1fr; }
  .sub-foot-note { width: 100%; margin: 2px 0 0; }
  .sub-confirm { flex-direction: column; }
  .sub-seen-row { flex-wrap: wrap; }
  .sub-seen-main { flex-basis: calc(100% - 110px); }
  .sub-source-tabs { flex-wrap: wrap; }
  .sub-video-preview { flex-wrap: wrap; }
  .sub-video-copy { flex-basis: calc(100% - 100px); }
  .sub-video-preview .prod-btn { margin-left: auto; }
}
@media (max-width: 520px) {
  .sub-page :deep(.prod-page-head h1) { font-size: 25px; }
  .sub-control-stats { grid-template-columns: 1fr; }
  .sub-choice-grid { grid-template-columns: 1fr; }
  .sub-latest { flex-wrap: wrap; }
  .sub-inline-link { margin-left: 21px; }
  .sub-row-actions { flex-direction: row; }
  .sub-row-head { flex-wrap: wrap; }
  .sub-identity { flex-basis: calc(100% - 50px); }
  .sub-row-actions { width: 100%; justify-content: flex-end; }
  .sub-seen-main { flex-basis: calc(100% - 80px); }
}
</style>
