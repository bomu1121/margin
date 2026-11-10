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

export interface AnnotationSegment {
  blockId: string
  start: number
  end: number
}

export interface Annotation {
  id: string
  noteId: string
  blockId: string
  segments: AnnotationSegment[]
  quote: string
  body: string
  kind: string
  color: string
  createdAt: number
}

export interface AnnotationRelocation {
  id: string
  blockId: string
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
  traceClaims?: number
  drillCount: number
  mindDepth: number
  styles: string[]
  auto?: boolean
  subscriptionId?: string
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

export interface FactMention {
  noteId: string
  runId: string
  quote: string
  section: string
  blockId: string
  stance: string
}

export interface FactEvidence {
  url: string
  title: string
  domain: string
  relation: string
  quote: string
  checkedAt: number
}

export interface FactRelation {
  otherId: string
  text: string
  status: string
  relation: string
  confidence: number
}

export interface Fact {
  id: string
  text: string
  status: string
  createdAt: number
  updatedAt: number
  lastCheckedAt: number
  queries: string[]
  noteCount: number
  domain: string
  eventId: string
  historicalStatus: string
  claimKind: string
  citationCount: number
  mentions: FactMention[]
  evidence: FactEvidence[]
  related: FactRelation[]
}

export interface FactSplitChild {
  id: string
  text: string
  status: string
  existed: boolean
  evidenceCount: number
  citationCount: number
}

export interface FactSplit {
  id: string
  parentFactId: string
  parentText: string
  parentStatus: string
  children: FactSplitChild[]
  splitAt: number
  undoneAt: number
  canUndo: boolean
}

export interface HistoryIngestReport {
  searched: number
  fetched: number
  candidates: number
  autoEligible: number
  blocked: number
  sourcesWritten: number
  passagesWritten: number
  citationsCreated: number
  skippedExisting: boolean
  planSource: string
  work: string
  section: string
  notes: string[]
}

export interface HistoryCandidate {
  id: string
  factId: string
  provider: string
  title: string
  url: string
  snippet: string
  authority: string
  status: string
  reason: string
  workTitle: string
  sectionTitle: string
  sourceId: string
  citationId: string
  createdAt: number
}

export interface FactConflictResolution {
  id: string
  factA: string
  factB: string
  resolution: string
  note: string
  updatedAt: number
}

export interface FactReview {
  factId: string
  status: string
  note: string
  updatedAt: number
}

export interface ConflictResolutionInput {
  factA: string
  factB: string
  resolution: string
  note?: string
}

export interface SourceSearchHit {
  provider: string
  title: string
  url: string
  snippet: string
  sourceType: string
}

export interface SourceSearchLink {
  provider: string
  label: string
  url: string
  note: string
}

export interface SourceSearchResult {
  hits: SourceSearchHit[]
  links: SourceSearchLink[]
  notes: string[]
}

export interface Source {
  id: string
  workTitle: string
  sectionTitle: string
  author: string
  compiler: string
  era: string
  sourceType: string
  edition: string
  publisher: string
  publishYear: string
  volume: string
  repository: string
  url: string
  scanUrl: string
  callNumber: string
  language: string
  authenticityNote: string
  passageCount: number
  citationCount: number
  factCount: number
  createdAt: number
  updatedAt: number
}

export interface SourcePassage {
  id: string
  sourceId: string
  volume: string
  chapter: string
  page: string
  originalText: string
  translation: string
  variantOf: string
  scanUrl: string
  locator: string
  createdAt: number
}

export interface SourceInput {
  id?: string
  workTitle: string
  sectionTitle?: string
  author?: string
  compiler?: string
  era?: string
  sourceType?: string
  edition?: string
  publisher?: string
  publishYear?: string
  volume?: string
  repository?: string
  url?: string
  scanUrl?: string
  callNumber?: string
  language?: string
  authenticityNote?: string
}

export interface PassageInput {
  id?: string
  sourceId: string
  volume?: string
  chapter?: string
  page?: string
  originalText: string
  translation?: string
  variantOf?: string
  scanUrl?: string
  locator?: string
}

export interface FactCitation {
  id: string
  factId: string
  passageId: string
  relation: string
  quote: string
  locator: string
  confidence: number
  checkedAt: number
  sourceId: string
  workTitle: string
  sectionTitle: string
  author: string
  edition: string
  volume: string
  page: string
  originalText: string
  translation: string
  sourceType: string
  sourceUrl: string
  scanUrl: string
}

export interface SourceCitedFact {
  citationId: string
  factId: string
  text: string
  status: string
  historicalStatus: string
  relation: string
  quote: string
  locator: string
  noteIds: string[]
}

export interface SourceDetail {
  source: Source
  passages: SourcePassage[]
  facts: SourceCitedFact[]
}

export interface HistoryEvent {
  id: string
  canonicalName: string
  aliases: string[]
  startDate: string
  endDate: string
  dateNote: string
  places: string[]
  actors: string[]
  summary: string
  factCount: number
  sourceCount: number
  noteCount: number
  createdAt: number
  updatedAt: number
}

export interface EventFact {
  id: string
  text: string
  status: string
  historicalStatus: string
  claimKind: string
  noteIds: string[]
}

export interface EventDetail {
  event: HistoryEvent
  facts: EventFact[]
  sources: Source[]
  noteIds: string[]
}

export interface EventInput {
  id?: string
  canonicalName: string
  aliases?: string[]
  startDate?: string
  endDate?: string
  dateNote?: string
  places?: string[]
  actors?: string[]
  summary?: string
}

export interface CitationInput {
  id?: string
  factId: string
  passageId: string
  relation: string
  quote?: string
  locator?: string
  confidence?: number
}

export interface FactCheck {
  id: string
  verdict: string
  provider: string
  model: string
  queries: string[]
  evidenceCount: number
  checkedAt: number
}

export interface FactMerge {
  id: string
  targetFactId: string
  sourceFactId: string
  targetText: string
  sourceText: string
  sourceStatus: string
  mergedAt: number
  undoneAt: number
  canUndo: boolean
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

export interface ProductionResumeStep {
  key: string
  label: string
  units: number
  tokens: number
}

export interface ProductionResumeInfo {
  canResume: boolean
  reason: string
  checkpointCount: number
  checkpointBytes: number
  savedTokens: number
  steps: ProductionResumeStep[]
  checkpointAt: number
  published: boolean
  vaultMatches: boolean
  configReady: boolean
  configChanged: boolean
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
  hasCtextKey: boolean
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
  ctextKey?: string
  searchMaxResults?: number
  defaultTemplate?: string
  retries?: number
  notify?: boolean
  sound?: boolean
  ffmpegPath?: string
  defaultPrompt?: string
  defaultFields?: string[]
}

export interface SubscriptionFilters {
  minDurationSec: number
  maxDurationSec: number
  includeKeywords: string
  excludeKeywords: string
}

export interface ProductionSubscription {
  id: string
  mid: number
  name: string
  avatar: string
  enabled: boolean
  intervalMinutes: number
  templateId: string
  prompt: string
  filters: SubscriptionFilters
  baselineAt: number
  lastCheckedAt: number
  nextCheckAt: number
  lastError: string
  lastMessage: string
  consecutiveFailures: number
  createdAt: number
  updatedAt: number
  latestTitle: string
  latestBvid: string
  latestPubdate: number
  latestCover: string
  queuedCount: number
  seenCount: number
}

export interface SubscriptionInput {
  id?: string
  mid: number
  name?: string
  avatar?: string
  enabled?: boolean
  intervalMinutes?: number
  templateId?: string
  prompt?: string
  filters?: SubscriptionFilters
  backfill?: number
}

export interface SeenVideo {
  bvid: string
  title: string
  pubdate: number
  duration: number
  cover: string
  status: string
  jobId: string
  noteId: string
  firstSeenAt: number
  updatedAt: number
}

export interface AutoSettings {
  enabled: boolean
  pauseOnBudget: boolean
  maxPerCheck: number
  maxPerDay: number
  defaultIntervalMinutes: number
  closeToTray: boolean
  autostart: boolean
  monthlySpendCny: number
  monthlyBudgetCny: number
  biliLoggedIn: boolean
}

export interface SaveAutoSettings {
  enabled?: boolean
  pauseOnBudget?: boolean
  maxPerCheck?: number
  maxPerDay?: number
  defaultIntervalMinutes?: number
  closeToTray?: boolean
  autostart?: boolean
}

export interface SubscriptionCheckResult {
  checkedAt: number
  baseline: boolean
  newTasks: number
  skippedExisting: number
  skippedFilter: number
  remaining: number
  message: string
}

export interface BiliUpProfile {
  mid: number
  name: string
  face: string
  fans: number
}

export interface PromptVersion {
  version: number
  name: string
  description: string
  system: string
  chars: number
  createdAt: number
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
  ownerMid: number
  duration: number
  pubdate: number
  cover: string
  pages: BiliPage[]
}

export interface BiliFollowing {
  mid: number
  name: string
  face: string
  sign: string
}

export interface BiliFollowingPage {
  items: BiliFollowing[]
  page: number
  hasMore: boolean
  total: number
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
  checkpointBytes: number
}

export interface ProductionClearTempResult {
  clearedBytes: number
  keptJobs: number
  keptBytes: number
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

export interface ModelPrice {
  provider: string
  model: string
  inputCny: number
  outputCny: number
  updatedAt: number
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
