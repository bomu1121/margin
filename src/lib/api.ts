import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { open } from '@tauri-apps/plugin-dialog'

import type {
  FactCheck,
  FactMerge,
  FactCitation,
  Source,
  SourceDetail,
  SourceSearchResult,
  SourceInput,
  SourcePassage,
  PassageInput,
  CitationInput,
  HistoryEvent,
  EventDetail,
  EventInput,
  Annotation,
  AnnotationRelocation,
  AppStats,
  CreatedNote,
  DeletedEntry,
  IndexStats,
  IndexStatus,
  MoveReport,
  NoteDetail,
  NoteMeta,
  OutboxReport,
  StudioArtifact,
  StudioInfo,
  ProductionTemplate,
  ProductionJob,
  ProductionConflict,
  ProductionJobDetail,
  ProductionResumeInfo,
  ProductionStartRequest,
  ProductionVersion,
  TestModelResult,
  ProductionSettings,
  ProductionSaveSettings,
  ProductionPrompt,
  PromptVersion,
  BiliAccount,
  BiliFolder,
  BiliFollowingPage,
  BiliVideoRef,
  BiliVideoPage,
  BiliVideoPreview,
  BiliLoginStart,
  BiliLoginPoll,
  ProductionClearTempResult,
  ProductionDataUsage,
  ProviderPreset,
  ModelList,
  ModelPrice,
  PriceQuote,
  BalanceResult,
  UsageSummary,
  AutoSettings,
  Fact,
  FactConflictResolution,
  FactReview,
  FactSplit,
  ConflictResolutionInput,
  HistoryCandidate,
  HistoryIngestReport,
  BiliUpProfile,
  ProductionSubscription,
  SaveAutoSettings,
  SeenVideo,
  SubscriptionCheckResult,
  SubscriptionInput,
} from '../types'

export function getVault(): Promise<string | null> {
  return invoke<string | null>('get_vault')
}

export function setVault(path: string): Promise<IndexStats> {
  return invoke<IndexStats>('set_vault', { path })
}

export function reindex(): Promise<IndexStats> {
  return invoke<IndexStats>('reindex')
}

export function listNotes(): Promise<NoteMeta[]> {
  return invoke<NoteMeta[]>('list_notes')
}

export function getNote(id: string): Promise<NoteDetail | null> {
  return invoke<NoteDetail | null>('get_note', { id })
}

export function searchNotes(query: string): Promise<NoteMeta[]> {
  return invoke<NoteMeta[]>('search_notes', { query })
}

export function listAnnotations(noteId: string): Promise<Annotation[]> {
  return invoke<Annotation[]>('list_annotations', { noteId })
}

export function addAnnotation(input: {
  noteId: string
  blockId: string
  quote: string
  body: string
  kind: string
}): Promise<Annotation> {
  return invoke<Annotation>('add_annotation', input)
}

export function deleteAnnotation(id: string): Promise<void> {
  return invoke<void>('delete_annotation', { id })
}

export function relocateAnnotations(noteId: string, mappings: AnnotationRelocation[]): Promise<number> {
  return invoke<number>('relocate_annotations', { noteId, mappings })
}

export function listFolders(): Promise<string[]> {
  return invoke<string[]>('list_folders')
}

export function createNote(parent: string, title: string): Promise<CreatedNote> {
  return invoke<CreatedNote>('create_note', { parent, title })
}

export function createFolder(parent: string, name: string): Promise<string> {
  return invoke<string>('create_folder', { parent, name })
}

export function renameEntry(relPath: string, newName: string): Promise<MoveReport> {
  return invoke<MoveReport>('rename_entry', { relPath, newName })
}

export function moveEntry(relPath: string, newParent: string): Promise<MoveReport> {
  return invoke<MoveReport>('move_entry', { relPath, newParent })
}

export function deleteEntry(relPath: string): Promise<DeletedEntry> {
  return invoke<DeletedEntry>('delete_entry', { relPath })
}

export function restoreEntry(trashPath: string, relPath: string): Promise<string> {
  return invoke<string>('restore_entry', { trashPath, relPath })
}

export function listArtifacts(): Promise<StudioArtifact[]> {
  return invoke<StudioArtifact[]>('list_artifacts')
}

export function getStudioInfo(): Promise<StudioInfo> {
  return invoke<StudioInfo>('studio_info')
}

export function openScribeFlow(newProject = false): Promise<void> {
  return invoke<void>('open_scribeflow', { newProject })
}

export function drainOutbox(): Promise<OutboxReport> {
  return invoke<OutboxReport>('drain_outbox')
}

export function getStats(): Promise<AppStats> {
  return invoke<AppStats>('get_stats')
}

export function getIndexStatus(): Promise<IndexStatus> {
  return invoke<IndexStatus>('get_index_status')
}

export function onIndexStatus(
  handler: (status: IndexStatus) => void,
): Promise<UnlistenFn> {
  return listen<IndexStatus>('index-status', (event) => handler(event.payload))
}

export function appReady(): Promise<void> {
  return invoke<void>('app_ready')
}

export function onVaultChanged(
  handler: (stats: IndexStats) => void,
): Promise<UnlistenFn> {
  return listen<IndexStats>('vault-changed', (event) => handler(event.payload))
}

export async function chooseVaultFolder(): Promise<string | null> {
  const selected = await open({
    directory: true,
    multiple: false,
    title: '选择笔记库',
  })
  return typeof selected === 'string' ? selected : null
}

export function errorText(error: unknown): string {
  if (typeof error === 'string') return error
  if (error instanceof Error) return error.message
  return String(error)
}

export function productionTemplates(): Promise<ProductionTemplate[]> {
  return invoke<ProductionTemplate[]>('production_templates')
}

export function productionList(): Promise<ProductionJob[]> {
  return invoke<ProductionJob[]>('production_list')
}

export function productionGet(id: string): Promise<ProductionJobDetail | null> {
  return invoke<ProductionJobDetail | null>('production_get', { id })
}

export function productionTestModel(provider: string, baseUrl: string, model: string, apiKey = ''): Promise<TestModelResult> {
  return invoke<TestModelResult>('production_test_model', { provider, baseUrl, model, apiKey })
}

export function productionVersions(noteId: string): Promise<ProductionVersion[]> {
  return invoke<ProductionVersion[]>('production_versions', { noteId })
}

export function productionVersionContent(noteId: string, versionId: string): Promise<string> {
  return invoke<string>('production_version_content', { noteId, versionId })
}

export function productionRestoreVersion(noteId: string, versionId: string): Promise<string> {
  return invoke<string>('production_restore_version', { noteId, versionId })
}

export function productionConflictCheck(request: ProductionStartRequest): Promise<ProductionConflict | null> {
  return invoke<ProductionConflict | null>('production_conflict_check', { request })
}

export function productionStart(request: ProductionStartRequest): Promise<ProductionJob> {
  return invoke<ProductionJob>('production_start', { request })
}

export function productionCancel(id: string): Promise<void> {
  return invoke<void>('production_cancel', { id })
}

export function productionRetry(id: string): Promise<ProductionJob> {
  return invoke<ProductionJob>('production_retry', { id })
}

export function productionResumeInfo(id: string): Promise<ProductionResumeInfo> {
  return invoke<ProductionResumeInfo>('production_resume_info', { id })
}

export function productionResume(id: string): Promise<ProductionJob> {
  return invoke<ProductionJob>('production_resume', { id })
}

export function productionRename(id: string, title: string): Promise<ProductionJob> {
  return invoke<ProductionJob>('production_rename', { id, title })
}

export function productionDelete(id: string): Promise<void> {
  return invoke<void>('production_delete', { id })
}

export function productionSettings(): Promise<ProductionSettings> {
  return invoke<ProductionSettings>('production_settings')
}

export function productionSaveSettings(input: ProductionSaveSettings): Promise<ProductionSettings> {
  return invoke<ProductionSettings>('production_save_settings', { input })
}

export function productionPrompts(): Promise<ProductionPrompt[]> {
  return invoke<ProductionPrompt[]>('production_prompts')
}

export function productionSavePrompt(prompt: ProductionPrompt): Promise<ProductionPrompt> {
  return invoke<ProductionPrompt>('production_save_prompt', { prompt })
}

export function productionPromptVersions(promptId: string): Promise<PromptVersion[]> {
  return invoke<PromptVersion[]>('production_prompt_versions', { promptId })
}

export function productionPromptVersion(promptId: string, version: number): Promise<PromptVersion> {
  return invoke<PromptVersion>('production_prompt_version', { promptId, version })
}

export function productionRestorePromptVersion(promptId: string, version: number): Promise<ProductionPrompt> {
  return invoke<ProductionPrompt>('production_restore_prompt_version', { promptId, version })
}

export function productionDeletePrompt(id: string): Promise<void> {
  return invoke<void>('production_delete_prompt', { id })
}

export function productionBiliLoginStart(): Promise<BiliLoginStart> {
  return invoke<BiliLoginStart>('production_bili_login_start')
}

export function productionBiliLoginPoll(key: string): Promise<BiliLoginPoll> {
  return invoke<BiliLoginPoll>('production_bili_login_poll', { key })
}

export function productionBiliLogout(): Promise<void> {
  return invoke<void>('production_bili_logout')
}

export function productionBiliAccount(): Promise<BiliAccount | null> {
  return invoke<BiliAccount | null>('production_bili_account')
}

export function productionBiliFavorites(): Promise<BiliFolder[]> {
  return invoke<BiliFolder[]>('production_bili_favorites')
}

export function productionBiliFollowings(page = 1): Promise<BiliFollowingPage> {
  return invoke<BiliFollowingPage>('production_bili_followings', { page })
}

export function productionBiliFolderVideos(mediaId: number, page = 1): Promise<BiliVideoPage> {
  return invoke<BiliVideoPage>('production_bili_folder_videos', { mediaId, page })
}

export function productionBiliWatchLater(): Promise<BiliVideoRef[]> {
  return invoke<BiliVideoRef[]>('production_bili_watch_later')
}

export function productionBiliPreview(url: string): Promise<BiliVideoPreview> {
  return invoke<BiliVideoPreview>('production_bili_preview', { url })
}

export function productionUpLookup(query: string): Promise<BiliUpProfile> {
  return invoke<BiliUpProfile>('production_up_lookup', { query })
}

export function productionSubscriptions(): Promise<ProductionSubscription[]> {
  return invoke<ProductionSubscription[]>('production_subscriptions')
}

export function productionSubscriptionSave(input: SubscriptionInput): Promise<ProductionSubscription> {
  return invoke<ProductionSubscription>('production_subscription_save', { input })
}

export function productionSubscriptionDelete(id: string): Promise<void> {
  return invoke<void>('production_subscription_delete', { id })
}

export function productionSubscriptionSeen(id: string): Promise<SeenVideo[]> {
  return invoke<SeenVideo[]>('production_subscription_seen', { id })
}

export function productionSubscriptionCheck(id: string): Promise<SubscriptionCheckResult> {
  return invoke<SubscriptionCheckResult>('production_subscription_check', { id })
}

export function productionAutoSettings(): Promise<AutoSettings> {
  return invoke<AutoSettings>('production_auto_settings')
}

export function productionSaveAutoSettings(input: SaveAutoSettings): Promise<AutoSettings> {
  return invoke<AutoSettings>('production_save_auto_settings', { input })
}

export function onSubscriptionsChanged(handler: () => void): Promise<UnlistenFn> {
  return listen('production-subscriptions-changed', () => handler())
}

export function productionNoteFacts(noteId: string, runId = ''): Promise<Fact[]> {
  return invoke<Fact[]>('production_note_facts', { noteId, runId })
}

export function productionFacts(status = '', query = '', limit = 100): Promise<Fact[]> {
  return invoke<Fact[]>('production_facts', { status, query, limit })
}

export function productionVerifyFact(id: string): Promise<Fact> {
  return invoke<Fact>('production_verify_fact', { id })
}

export function productionMergeFact(targetId: string, sourceId: string): Promise<Fact> {
  return invoke<Fact>('production_merge_fact', { targetId, sourceId })
}

export function productionFactSplit(id: string, children: string[]): Promise<FactSplit> {
  return invoke<FactSplit>('production_fact_split', { id, children })
}

export function productionFactSplitInfo(id: string): Promise<FactSplit | null> {
  return invoke<FactSplit | null>('production_fact_split_info', { id })
}

export function productionFactSplitUndo(splitId: string): Promise<FactSplit> {
  return invoke<FactSplit>('production_fact_split_undo', { splitId })
}

export function productionHistorySearch(query: string): Promise<SourceSearchResult> {
  return invoke<SourceSearchResult>('production_history_search', { query })
}

export function productionSources(query = '', sourceType = '', limit = 100): Promise<Source[]> {
  return invoke<Source[]>('production_sources', { query, sourceType, limit })
}

export function productionSource(id: string): Promise<SourceDetail> {
  return invoke<SourceDetail>('production_source', { id })
}

export function productionSourceSave(input: SourceInput): Promise<Source> {
  return invoke<Source>('production_source_save', { input })
}

export function productionSourcePassageSave(input: PassageInput): Promise<SourcePassage> {
  return invoke<SourcePassage>('production_source_passage_save', { input })
}

export function productionSourcePassageDelete(id: string): Promise<void> {
  return invoke<void>('production_source_passage_delete', { id })
}

export function productionSourceDelete(id: string): Promise<void> {
  return invoke<void>('production_source_delete', { id })
}

export function productionFactCitations(factId: string): Promise<FactCitation[]> {
  return invoke<FactCitation[]>('production_fact_citations', { factId })
}

export function productionCitationSave(input: CitationInput): Promise<FactCitation> {
  return invoke<FactCitation>('production_citation_save', { input })
}

export function productionCitationDelete(id: string): Promise<void> {
  return invoke<void>('production_citation_delete', { id })
}

export function productionEvents(query = '', limit = 100): Promise<HistoryEvent[]> {
  return invoke<HistoryEvent[]>('production_events', { query, limit })
}

export function productionEvent(id: string): Promise<EventDetail> {
  return invoke<EventDetail>('production_event', { id })
}

export function productionEventSave(input: EventInput): Promise<HistoryEvent> {
  return invoke<HistoryEvent>('production_event_save', { input })
}

export function productionFactEventSet(factId: string, eventId: string): Promise<void> {
  return invoke<void>('production_fact_event_set', { factId, eventId })
}

export function productionFactHistorySet(factId: string, historicalStatus: string, claimKind = ''): Promise<void> {
  return invoke<void>('production_fact_history_set', { factId, historicalStatus, claimKind })
}

export function productionFact(id: string): Promise<Fact> {
  return invoke<Fact>('production_fact', { id })
}

export function productionFactDomainSet(factId: string, domain: string): Promise<Fact> {
  return invoke<Fact>('production_fact_domain_set', { factId, domain })
}

export function productionFactHistoryIngest(id: string, force = false): Promise<HistoryIngestReport> {
  return invoke<HistoryIngestReport>('production_fact_history_ingest', { id, force })
}

export function productionFactHistoryCandidates(factId: string): Promise<HistoryCandidate[]> {
  return invoke<HistoryCandidate[]>('production_fact_history_candidates', { factId })
}

export function productionHistoryCandidatesAll(status = 'candidate', limit = 500): Promise<HistoryCandidate[]> {
  return invoke<HistoryCandidate[]>('production_history_candidates_all', { status, limit })
}

export function productionFactHistoryCandidateImport(candidateId: string, text: string): Promise<HistoryCandidate> {
  return invoke<HistoryCandidate>('production_fact_history_candidate_import', { candidateId, text })
}

export function productionFactChecks(id: string, limit = 30): Promise<FactCheck[]> {
  return invoke<FactCheck[]>('production_fact_checks', { id, limit })
}

export function productionFactMerges(limit = 20): Promise<FactMerge[]> {
  return invoke<FactMerge[]>('production_fact_merges', { limit })
}

export function productionUndoFactMerge(id: string): Promise<Fact> {
  return invoke<Fact>('production_undo_fact_merge', { id })
}

export function productionFactConflictResolutions(): Promise<FactConflictResolution[]> {
  return invoke<FactConflictResolution[]>('production_fact_conflict_resolutions')
}

export function productionFactConflictResolve(input: ConflictResolutionInput): Promise<FactConflictResolution> {
  return invoke<FactConflictResolution>('production_fact_conflict_resolve', { input })
}

export function productionFactReviews(): Promise<FactReview[]> {
  return invoke<FactReview[]>('production_fact_reviews')
}

export function productionFactReviewSet(factId: string, status: string, note = ''): Promise<FactReview | null> {
  return invoke<FactReview | null>('production_fact_review_set', { factId, status, note })
}

export function productionDataUsage(): Promise<ProductionDataUsage> {
  return invoke<ProductionDataUsage>('production_data_usage')
}

export function productionClearTemp(): Promise<ProductionClearTempResult> {
  return invoke<ProductionClearTempResult>('production_clear_temp')
}

export function productionClearFinished(): Promise<number> {
  return invoke<number>('production_clear_finished')
}

export function productionClearResumable(): Promise<number> {
  return invoke<number>('production_clear_resumable')
}

export function onProductionUpdate(handler: (job: ProductionJob) => void): Promise<UnlistenFn> {
  return listen<ProductionJob>('production-update', (event) => handler(event.payload))
}

export async function chooseMediaFile(): Promise<string | null> {
  const selected = await open({
    multiple: false,
    title: '选择本地音视频',
    filters: [
      { name: '音视频', extensions: ['mp4', 'm4a', 'mkv', 'flv', 'mov', 'wav', 'mp3', 'aac', 'webm', 'm4v'] },
    ],
  })
  return typeof selected === 'string' ? selected : null
}

export function openExternalUrl(url: string): Promise<void> {
  return invoke<void>('open_external_url', { url })
}

export function productionBiliHistory(): Promise<BiliVideoRef[]> {
  return invoke<BiliVideoRef[]>('production_bili_history')
}

export function productionProviders(): Promise<ProviderPreset[]> {
  return invoke<ProviderPreset[]>('production_providers')
}

export function productionModels(provider: string, baseUrl = '', apiKey = ''): Promise<ModelList> {
  return invoke<ModelList>('production_models', { provider, baseUrl, apiKey })
}

export function productionPrice(provider: string, model: string): Promise<PriceQuote | null> {
  return invoke<PriceQuote | null>('production_price', { provider, model })
}

export function productionModelPrices(): Promise<ModelPrice[]> {
  return invoke<ModelPrice[]>('production_model_prices')
}

export function productionSetModelPrice(provider: string, model: string, inputCny: number, outputCny: number): Promise<ModelPrice> {
  return invoke<ModelPrice>('production_set_model_price', { provider, model, inputCny, outputCny })
}

export function productionDeleteModelPrice(provider: string, model: string): Promise<void> {
  return invoke<void>('production_delete_model_price', { provider, model })
}

export function productionBalance(provider: string, baseUrl = '', apiKey = ''): Promise<BalanceResult> {
  return invoke<BalanceResult>('production_balance', { provider, baseUrl, apiKey })
}

export function productionUsageSummary(): Promise<UsageSummary> {
  return invoke<UsageSummary>('production_usage_summary')
}

export interface SyncState {
  configured: boolean
  vaultId: string
  deviceId: string
  cursor: number
  serverUrl: string
  hasToken: boolean
  autoEnabled: boolean
  autoMinutes: number
  lastSyncAt: number
  lastError: string
}

export interface SyncReport {
  pushed: number
  pulled: number
  applied: number
  skipped: number
  conflicts: number
  blobsUploaded: number
  blobsDownloaded: number
}

export function syncStatus(): Promise<SyncState> {
  return invoke<SyncState>('sync_status')
}

export function syncConfigure(serverUrl: string, token: string, vaultId: string): Promise<void> {
  return invoke<void>('sync_configure', { serverUrl, token, vaultId })
}

export function syncSetAuto(enabled: boolean, minutes: number): Promise<void> {
  return invoke<void>('sync_set_auto', { enabled, minutes })
}

export function syncNow(): Promise<SyncReport> {
  return invoke<SyncReport>('sync_now')
}
