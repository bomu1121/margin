export interface NoteMeta {
  id: string
  path: string
  relPath: string
  title: string
  up: string
  date: string
  domain: string
  tags: string[]
  people: string[]
  events: string[]
  periods: string[]
  runId: string
  mtime: number
  chars: number
  annotationCount: number
}

export interface ArtifactSource {
  kind: string
  label: string
  url: string
  author: string
  publishedAt: string
}

export interface ArtifactRun {
  projectId: string
  projectName: string
  runId: string
  nodeId: string
  prompt: string
  model: string
}

export interface ArtifactUsage {
  promptTokens: number
  completionTokens: number
  estimatedCost: number
  currency: string
}

export interface ArtifactAsset {
  name: string
  path: string
  kind: string
  bytes: number
}

export interface StudioArtifact {
  artifactId: string
  noteId: string
  noteRelPath: string
  title: string
  status: string
  revision: number
  generatedAt: string
  source: ArtifactSource | null
  run: ArtifactRun | null
  usage: ArtifactUsage | null
  assets: ArtifactAsset[]
  noteExists: boolean
  annotationCount: number
  derived: boolean
  error: string | null
}

export interface StudioInfo {
  folder: string
  url: string
  running: boolean
}

export interface OutboxReport {
  published: number
  skipped: number
  errors: string[]
  notes: string[]
}

export interface NoteDetail {
  meta: NoteMeta
  body: string
  artifact?: StudioArtifact | null
}

export interface Annotation {
  id: string
  noteId: string
  blockId: string
  quote: string
  body: string
  kind: string
  createdAt: number
}

export interface IndexStats {
  notes: number
  added: number
  updated: number
  removed: number
  skipped: number
  unchanged?: number
  elapsedMs: number
}

export interface IndexStatus {
  state: 'idle' | 'scanning' | 'indexing' | 'writing' | 'committing' | 'error'
  done: number
  total: number
  current: string
  generation: number
  error: string | null
  root: string | null
}

export interface AppStats {
  notes: number
  annotations: number
  vault: string | null
}

export interface CreatedNote {
  relPath: string
  id: string
}

export interface MoveReport {
  relPath: string
  linksRewritten: number
  filesScanned: number
}

export interface DeletedEntry {
  relPath: string
  trashPath: string
}

export interface ProductionTemplateStep {
  key: string
  label: string
}

export interface ProductionTemplate {
  id: string
  name: string
  group: string
  badge: string
  description: string
  pipeline: string[]
  sources: string[]
  steps: ProductionTemplateStep[]
}

export interface BiliVideoRef {
  bvid: string
  title: string
  author: string
  duration: number
  pubdate: number
  cover: string
  cid: number
}

export interface ProductionSource {
  kind: 'text' | 'file' | 'bili' | 'favorites' | string
  text: string
  path: string
  url: string
  videos: BiliVideoRef[]
  pages: number[]
}

export interface ProductionOptions {
  prompt: string
  tags: string[]
  fields: string[]
  split: boolean
  exportMarkdown: boolean
  conflictAction?: string
  chapter: string
  maxChapters: number
  traceProvider: string
  traceMax: number
  drillCount: number
  mindDepth: number
  styles: string[]
}

export interface ProductionUsage {
  promptTokens: number
  uncachedInputTokens: number
  completionTokens: number
  cachedTokens: number
  cacheWriteTokens: number
  reasoningTokens: number
  audioSeconds: number
  estimatedCost: number
  currency: string
  priced: boolean
  providerCost: number
  providerCurrency: string
}

export interface ProductionStep {
  index: number
  key: string
  label: string
  status: string
  detail: string
}

export interface ProductionVariant {
  name: string
  body: string
  summary: string
}

export interface ProductionDrillItem {
  id: string
  question: string
  kind: string
  options: string[]
  answer: string
  note: string
}

export interface ProductionTraceItem {
  claim: string
  verdict: string
  source: string
  quote: string
}

export interface ProductionResult {
  body: string
  summary: string
  variants: ProductionVariant[]
  mindmap: string | null
  drill: ProductionDrillItem[]
  trace: ProductionTraceItem[]
  related: string[]
  notePath: string
}

export interface ProductionJob {
  id: string
  title: string
  templateId: string
  sourceKind: string
  source: ProductionSource
  options: ProductionOptions
  status: string
  progress: number
  currentStep: string
  error: string
  noteId: string
  noteRelPath: string
  result: ProductionResult | null
  usage: ProductionUsage
  createdAt: number
  updatedAt: number
  startedAt: number
  finishedAt: number
  steps: ProductionStep[]
}

export interface ProductionLog {
  id: number
  stepIndex: number | null
  level: string
  message: string
  createdAt: number
}

export interface ProductionJobDetail {
  job: ProductionJob
  logs: ProductionLog[]
}

export interface ProductionConflict {
  noteId: string
  title: string
  notePath: string
  currentChars: number
  generatedChars: number
  generatedRevision: number
}

export interface TestModelResult {
  provider: string
  model: string
  latencyMs: number
  reply: string
  promptTokens: number
  completionTokens: number
  providerCost: number
  providerCurrency: string
}

export interface ProductionVersion {
  id: string
  kind: 'generated' | 'manual' | string
  label: string
  revision: number | null
  chars: number
  bytes: number
  createdAt: number
}

export interface ProductionStartRequest {
  templateId: string
  title: string
  source: ProductionSource
  options: ProductionOptions
}

export interface ProductionSettings {
  aiProvider: string
  aiProtocol: string
  aiBaseUrl: string
  aiModel: string
  hasAiKey: boolean
  aiExchangeRate: number
  monthlyBudgetCny: number
  asrEngine: string
  asrBaseUrl: string
  asrModel: string
  hasAsrKey: boolean
  asrPricePerMinute: number
  searchProvider: string
  hasSearchKey: boolean
  searchMaxResults: number
  biliLoggedIn: boolean
  biliName: string
  biliMid: number
  defaultTemplate: string
  retries: number
  notify: boolean
  sound: boolean
  ffmpegPath: string
  defaultPrompt: string
  defaultFields: string[]
}

export interface ProductionSaveSettings {
  aiProvider?: string
  aiProtocol?: string
  aiBaseUrl?: string
  aiModel?: string
  aiKey?: string
  aiExchangeRate?: number
  monthlyBudgetCny?: number
  asrEngine?: string
  asrBaseUrl?: string
  asrModel?: string
  asrKey?: string
  asrPricePerMinute?: number
  searchProvider?: string
  searchKey?: string
  searchMaxResults?: number
  defaultTemplate?: string
  retries?: number
  notify?: boolean
  sound?: boolean
  ffmpegPath?: string
  defaultPrompt?: string
  defaultFields?: string[]
}

export interface ProductionPrompt {
  id: string
  name: string
  description: string
  system: string
  builtin: boolean
  createdAt: number
  updatedAt: number
}

export interface BiliAccount {
  mid: number
  name: string
  face: string
}

export interface BiliVideoPage {
  items: BiliVideoRef[]
  page: number
  hasMore: boolean
  total: number
}

export interface BiliFolder {
  id: number
  title: string
  count: number
}

export interface BiliPage {
  cid: number
  page: number
  part: string
  duration: number
}

export interface BiliVideoPreview {
  bvid: string
  title: string
  author: string
  duration: number
  pubdate: number
  cover: string
  pages: BiliPage[]
}

export interface BiliLoginStart {
  qrcodeKey: string
  url: string
  modules: boolean[][]
  expiresIn: number
}

export interface BiliLoginPoll {
  status: string
  message: string
  account: BiliAccount | null
}

export interface ProductionDataUsage {
  jobs: number
  finished: number
  tempBytes: number
  outboxBytes: number
}

export interface ProviderPreset {
  id: string
  name: string
  protocol: string
  baseUrl: string
  defaultModel: string
  keyRequired: boolean
  local: boolean
  supportsModels: boolean
  supportsBalance: boolean
  docsUrl: string
  catalogId: string
  note: string
}

export interface PriceQuote {
  inputUsd: number
  outputUsd: number
  cacheReadUsd: number
  cacheWriteUsd: number
  inputCny: number
  outputCny: number
  cacheReadCny: number
  cacheWriteCny: number
  cacheReadPriced: boolean
  cacheWritePriced: boolean
  exchangeRate: number
  source: string
  updatedAt: number
  matchedKey: string
  note: string
}

export interface ModelOption {
  id: string
  name: string
  contextLength: number
  maxOutput: number
  price: PriceQuote | null
  source: string
}

export interface ModelList {
  models: ModelOption[]
  source: string
  warning: string
  fetchedAt: number
}

export interface BalanceEntry {
  label: string
  amount: string
  currency: string
}

export interface BalanceResult {
  provider: string
  providerLabel: string
  entries: BalanceEntry[]
  fetchedAt: number
}

export interface UsageSummary {
  month: string
  spendCny: number
  budgetCny: number
  percent: number
  jobs: number
  pricedJobs: number
  tokens: number
  byModel: Array<{ model: string; spendCny: number; tokens: number; jobs: number }>
}
