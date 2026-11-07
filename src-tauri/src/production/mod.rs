//! Native production engine: video/text -> transcript -> note -> yebian inbox.
//!
//! The engine lives in the desktop process. It owns jobs and progress, talks to
//! OpenAI-compatible AI/ASR endpoints and Bilibili, and publishes through the
//! existing artifact outbox so the note/frontmatter/index rules stay identical
//! to every other producer.

mod clients;
mod media;
mod pipeline;
mod prompts;
mod providers;
mod checkpoint;
mod publish;
mod store;
mod templates;

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;

use crate::indexer::IndexService;
use providers::PriceCatalog;
use store::ProductionStore;

pub use clients::{BiliAccount, BiliFolder, BiliLoginPoll, BiliLoginStart, BiliVideoPage, BiliVideoPreview};
pub use prompts::{PromptEntry, PromptVersion, builtin_prompts};
pub use templates::{TemplateInfo, builtin_templates};
pub use providers::{ProviderPreset, ModelList, ModelOption, BalanceResult, PriceQuote};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BiliVideoRef {
    pub bvid: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub duration: i64,
    #[serde(default)]
    pub pubdate: i64,
    #[serde(default)]
    pub cover: String,
    #[serde(default)]
    pub cid: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SourceSpec {
    pub kind: String,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub videos: Vec<BiliVideoRef>,
    #[serde(default)]
    pub pages: Vec<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobOptions {
    #[serde(default)]
    pub prompt: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub fields: Vec<String>,
    #[serde(default = "default_true")]
    pub split: bool,
    #[serde(default)]
    pub export_markdown: bool,
    #[serde(default)]
    pub chapter: String,
    #[serde(default = "default_max_chapters")]
    pub max_chapters: u32,
    #[serde(default)]
    pub trace_provider: String,
    #[serde(default = "default_trace_max")]
    pub trace_max: u32,
    #[serde(default = "default_drill_count")]
    pub drill_count: u32,
    #[serde(default = "default_mind_depth")]
    pub mind_depth: u32,
    #[serde(default)]
    pub styles: Vec<String>,
    #[serde(default)]
    pub conflict_action: String,
}

fn default_true() -> bool { true }
fn default_max_chapters() -> u32 { 20 }
fn default_trace_max() -> u32 { 5 }
fn default_drill_count() -> u32 { 6 }
fn default_mind_depth() -> u32 { 3 }

impl Default for JobOptions {
    fn default() -> Self {
        Self {
            prompt: "成文版".into(),
            tags: Vec::new(),
            fields: vec!["人物".into(), "事件".into()],
            split: true,
            export_markdown: false,
            chapter: "medium".into(),
            max_chapters: default_max_chapters(),
            trace_provider: String::new(),
            trace_max: default_trace_max(),
            drill_count: default_drill_count(),
            mind_depth: default_mind_depth(),
            styles: Vec::new(),
            conflict_action: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelPrice {
    pub provider: String,
    pub model: String,
    pub input_cny: f64,
    pub output_cny: f64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct JobConfig {
    pub pipeline_version: u32,
    pub provider: String,
    pub protocol: String,
    pub base_url: String,
    pub model: String,
    pub asr_engine: String,
    pub asr_base_url: String,
    pub asr_model: String,
    pub asr_price_per_minute: f64,
    pub ffmpeg_path: String,
    pub search_provider: String,
    pub default_prompt: String,
    pub exchange_rate: f64,
    pub retries: u32,
    pub price: Option<PriceQuote>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckpointRow {
    pub job_id: String,
    pub unit_key: String,
    pub step_key: String,
    pub kind: String,
    pub input_hash: String,
    pub payload_json: String,
    pub usage_json: String,
    pub payload_bytes: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClearTempResult {
    pub cleared_bytes: u64,
    pub kept_jobs: usize,
    pub kept_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResumeStepInfo {
    pub key: String,
    pub label: String,
    pub units: usize,
    pub tokens: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResumeInfo {
    pub can_resume: bool,
    pub reason: String,
    pub checkpoint_count: usize,
    pub checkpoint_bytes: i64,
    pub saved_tokens: u64,
    pub steps: Vec<ResumeStepInfo>,
    pub checkpoint_at: i64,
    pub published: bool,
    pub vault_matches: bool,
    pub config_ready: bool,
    pub config_changed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    pub prompt_tokens: u64,
    #[serde(default)]
    pub uncached_input_tokens: u64,
    pub completion_tokens: u64,
    #[serde(default)]
    pub cached_tokens: u64,
    #[serde(default)]
    pub cache_write_tokens: u64,
    #[serde(default)]
    pub reasoning_tokens: u64,
    pub audio_seconds: f64,
    pub estimated_cost: f64,
    pub currency: String,
    #[serde(default)]
    pub priced: bool,
    #[serde(default)]
    pub provider_cost: f64,
    #[serde(default)]
    pub provider_currency: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobStep {
    pub index: usize,
    pub key: String,
    pub label: String,
    pub status: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct JobVariant {
    pub name: String,
    pub body: String,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DrillItem {
    pub id: String,
    pub question: String,
    pub kind: String,
    pub options: Vec<String>,
    pub answer: String,
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TraceItem {
    pub claim: String,
    pub verdict: String,
    pub source: String,
    pub quote: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct JobResult {
    pub body: String,
    pub summary: String,
    pub variants: Vec<JobVariant>,
    pub mindmap: Option<String>,
    pub drill: Vec<DrillItem>,
    pub trace: Vec<TraceItem>,
    pub related: Vec<String>,
    pub note_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    pub id: String,
    pub title: String,
    pub template_id: String,
    pub source_kind: String,
    pub provider: String,
    pub model: String,
    #[serde(default)]
    pub vault_path: String,
    pub source: SourceSpec,
    pub options: JobOptions,
    pub status: String,
    pub progress: i64,
    pub current_step: String,
    pub error: String,
    pub note_id: String,
    pub note_rel_path: String,
    pub result: Option<JobResult>,
    pub usage: Usage,
    pub created_at: i64,
    pub updated_at: i64,
    pub started_at: i64,
    pub finished_at: i64,
    pub steps: Vec<JobStep>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobDetail {
    pub job: Job,
    pub logs: Vec<LogEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogEntry {
    pub id: i64,
    pub step_index: Option<usize>,
    pub level: String,
    pub message: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartJobRequest {
    pub template_id: String,
    #[serde(default)]
    pub title: String,
    pub source: SourceSpec,
    #[serde(default)]
    pub options: JobOptions,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictInfo {
    pub note_id: String,
    pub title: String,
    pub note_path: String,
    pub current_chars: usize,
    pub generated_chars: usize,
    pub generated_revision: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TestModelResult {
    pub provider: String,
    pub model: String,
    pub latency_ms: u64,
    pub reply: String,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub provider_cost: f64,
    pub provider_currency: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionEntry {
    pub id: String,
    pub kind: String,
    pub label: String,
    pub revision: Option<i64>,
    pub chars: usize,
    pub bytes: u64,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProductionSettings {
    pub ai_provider: String,
    pub ai_protocol: String,
    pub ai_base_url: String,
    pub ai_model: String,
    pub has_ai_key: bool,
    pub ai_exchange_rate: f64,
    pub monthly_budget_cny: f64,
    pub asr_engine: String,
    pub asr_base_url: String,
    pub asr_model: String,
    pub has_asr_key: bool,
    pub asr_price_per_minute: f64,
    pub search_provider: String,
    pub has_search_key: bool,
    pub search_max_results: u32,
    pub bili_logged_in: bool,
    pub bili_name: String,
    pub bili_mid: i64,
    pub default_template: String,
    pub retries: u32,
    pub notify: bool,
    pub sound: bool,
    pub ffmpeg_path: String,
    pub default_prompt: String,
    pub default_fields: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SaveSettings {
    pub ai_provider: Option<String>,
    pub ai_protocol: Option<String>,
    pub ai_base_url: Option<String>,
    pub ai_model: Option<String>,
    pub ai_key: Option<String>,
    pub ai_exchange_rate: Option<f64>,
    pub monthly_budget_cny: Option<f64>,
    pub asr_engine: Option<String>,
    pub asr_base_url: Option<String>,
    pub asr_model: Option<String>,
    pub asr_key: Option<String>,
    pub asr_price_per_minute: Option<f64>,
    pub search_provider: Option<String>,
    pub search_key: Option<String>,
    pub search_max_results: Option<u32>,
    pub default_template: Option<String>,
    pub retries: Option<u32>,
    pub notify: Option<bool>,
    pub sound: Option<bool>,
    pub ffmpeg_path: Option<String>,
    pub default_prompt: Option<String>,
    pub default_fields: Option<Vec<String>>,
}

pub struct ProductionService {
    app: AppHandle,
    store: ProductionStore,
    index: Arc<IndexService>,
    cache_dir: std::path::PathBuf,
    jobs: Mutex<HashMap<String, CancellationToken>>,
    gate: Arc<Semaphore>,
}

/// Manual price wins over the catalog, including when the catalog is offline.
/// Keeping this in one place makes the settings preview and the task cost agree.
pub async fn resolve_price(
    store: &ProductionStore,
    cache_dir: &Path,
    provider_id: &str,
    model: &str,
) -> Option<PriceQuote> {
    let settings = store.settings();
    if let Ok(Some(price)) = store.model_price(provider_id, model) {
        return Some(providers::manual_quote(&price, settings.ai_exchange_rate));
    }
    let preset = providers::preset_by_id(provider_id)?;
    if preset.catalog_id.is_empty() {
        return None;
    }
    let catalog = PriceCatalog::load(cache_dir, false).await.ok()?;
    catalog.price(&preset.catalog_id, model, settings.ai_exchange_rate)
}

impl ProductionService {
    pub fn open(app: AppHandle, db_path: &Path, index: Arc<IndexService>) -> Result<Self, String> {
        let store = ProductionStore::open(db_path)?;
        let cache_dir = db_path.parent().unwrap_or_else(|| Path::new(".")).join("production");
        std::fs::create_dir_all(&cache_dir).map_err(|error| error.to_string())?;
        let interrupted = store.recover_interrupted()?;
        for job in interrupted {
            let _ = app.emit("production-update", &job);
        }
        Ok(Self {
            app,
            store,
            index,
            cache_dir,
            jobs: Mutex::new(HashMap::new()),
            gate: Arc::new(Semaphore::new(1)),
        })
    }

    pub fn templates(&self) -> Vec<TemplateInfo> {
        builtin_templates()
    }

    pub fn prompts(&self) -> Result<Vec<PromptEntry>, String> {
        let mut items = builtin_prompts();
        items.extend(self.store.list_prompts()?);
        Ok(items)
    }

    pub fn save_prompt(&self, entry: PromptEntry) -> Result<PromptEntry, String> {
        self.store.save_prompt(&entry)
    }

    pub fn delete_prompt(&self, id: &str) -> Result<(), String> {
        self.store.delete_prompt(id)
    }

    pub fn prompt_versions(&self, prompt_id: &str) -> Result<Vec<PromptVersion>, String> {
        self.store.list_prompt_versions(prompt_id)
    }

    pub fn prompt_version(&self, prompt_id: &str, version: i64) -> Result<PromptVersion, String> {
        self.store.prompt_version(prompt_id, version)
    }

    pub fn restore_prompt_version(&self, prompt_id: &str, version: i64) -> Result<PromptEntry, String> {
        self.store.restore_prompt_version(prompt_id, version)
    }

    pub fn model_prices(&self) -> Result<Vec<ModelPrice>, String> {
        self.store.list_model_prices(None)
    }

    pub fn set_model_price(
        &self,
        provider: &str,
        model: &str,
        input_cny: f64,
        output_cny: f64,
    ) -> Result<ModelPrice, String> {
        self.store.set_model_price(provider, model, input_cny, output_cny)
    }

    pub fn delete_model_price(&self, provider: &str, model: &str) -> Result<(), String> {
        self.store.delete_model_price(provider, model)
    }

    pub fn settings(&self) -> ProductionSettings {
        self.store.settings()
    }

    pub fn save_settings(&self, input: &SaveSettings) -> Result<ProductionSettings, String> {
        self.store.save_settings(input)?;
        Ok(self.store.settings())
    }

    pub fn list(&self) -> Result<Vec<Job>, String> {
        self.store.list_jobs(100)
    }

    pub fn detail(&self, id: &str) -> Result<Option<JobDetail>, String> {
        let Some(job) = self.store.get_job(id)? else {
            return Ok(None);
        };
        let logs = self.store.logs(id)?;
        Ok(Some(JobDetail { job, logs }))
    }

    pub fn start(&self, root: &Path, request: StartJobRequest) -> Result<Job, String> {
        let template = templates::template(&request.template_id)
            .ok_or_else(|| format!("未知模板：{}", request.template_id))?;
        if !template.sources.iter().any(|source| source == &request.source.kind) {
            return Err(format!("模板「{}」不支持这个来源", template.name));
        }
        if request.source.kind == "text" && request.source.text.trim().is_empty() {
            return Err("文稿为空".into());
        }
        if request.source.kind == "file" && request.source.path.trim().is_empty() {
            return Err("还没有选择本地文件".into());
        }
        if request.source.kind == "bili" && request.source.url.trim().is_empty() {
            return Err("还没有粘贴 B 站链接".into());
        }
        if request.source.kind == "favorites" && request.source.videos.is_empty() {
            return Err("还没有选择收藏视频".into());
        }

        let job = self.store.create_job(root, request, &template)?;
        let token = CancellationToken::new();
        if let Ok(mut jobs) = self.jobs.lock() {
            jobs.insert(job.id.clone(), token.clone());
        }
        let service = ProductionServiceHandle {
            app: self.app.clone(),
            store: self.store.clone(),
            index: self.index.clone(),
            cache_dir: self.cache_dir.clone(),
            gate: self.gate.clone(),
        };
        let root = root.to_path_buf();
        let id = job.id.clone();
        tauri::async_runtime::spawn(async move {
            service.run(root, id, token).await;
        });
        Ok(job)
    }

    pub fn cancel(&self, id: &str) -> Result<(), String> {
        if let Ok(jobs) = self.jobs.lock() {
            if let Some(token) = jobs.get(id) {
                token.cancel();
            }
        }
        self.store.request_cancel(id)
    }

    pub fn resume_info(&self, root: &Path, id: &str) -> Result<ResumeInfo, String> {
        let Some(job) = self.store.get_job(id)? else {
            return Err("任务不存在".into());
        };
        let checkpoints = self.store.list_checkpoints(id)?;
        let config = self.store.job_config(id)?;
        let config_ready = config.is_some();
        let current = self.store.settings();
        let config_changed = config.as_ref().map(|item| {
            item.provider != current.ai_provider
                || item.model != current.ai_model
                || item.base_url != current.ai_base_url
                || item.default_prompt != current.default_prompt
        }).unwrap_or(false);
        let vault_matches = !job.vault_path.is_empty() && same_vault(&job.vault_path, root);
        let published = crate::artifact::registry_for_note(root, &job.note_id)
            .map(|item| !item.note_rel_path.is_empty() && root.join(&item.note_rel_path).is_file())
            .unwrap_or(false);
        let mut saved_tokens = 0u64;
        let mut checkpoint_bytes = 0i64;
        let mut checkpoint_at = 0i64;
        let mut groups: std::collections::BTreeMap<String, (usize, u64)> = std::collections::BTreeMap::new();
        for row in &checkpoints {
            checkpoint_bytes += row.payload_bytes;
            checkpoint_at = checkpoint_at.max(row.updated_at);
            let usage: Usage = serde_json::from_str(&row.usage_json).unwrap_or_default();
            let tokens = usage.prompt_tokens + usage.completion_tokens;
            saved_tokens += tokens;
            let entry = groups.entry(row.step_key.clone()).or_insert((0, 0));
            entry.0 += 1;
            entry.1 += tokens;
        }
        let steps = groups
            .into_iter()
            .map(|(key, (units, tokens))| ResumeStepInfo {
                label: job.steps.iter().find(|step| step.key == key).map(|step| step.label.clone()).unwrap_or_else(|| key.clone()),
                key,
                units,
                tokens,
            })
            .collect::<Vec<_>>();
        let status_ok = matches!(job.status.as_str(), "error" | "interrupted" | "cancelled");
        let reason = if !status_ok {
            "任务不是失败、中断或取消状态".to_string()
        } else if checkpoints.is_empty() {
            "还没有可复用的中间结果；只能重跑全部".to_string()
        } else if job.vault_path.is_empty() {
            "旧任务没有绑定笔记库，不能继续；可以重跑全部".to_string()
        } else if !vault_matches {
            "这个任务属于另一个笔记库，切换回原库后才能继续".to_string()
        } else if !config_ready {
            "旧任务没有执行配置快照；可以重跑全部".to_string()
        } else {
            "可以继续".to_string()
        };
        Ok(ResumeInfo {
            can_resume: status_ok && !checkpoints.is_empty() && vault_matches && config_ready,
            reason,
            checkpoint_count: checkpoints.len(),
            checkpoint_bytes,
            saved_tokens,
            steps,
            checkpoint_at,
            published,
            vault_matches,
            config_ready,
            config_changed,
        })
    }

    pub fn resume(&self, root: &Path, id: &str) -> Result<Job, String> {
        let info = self.resume_info(root, id)?;
        if !info.can_resume {
            return Err(info.reason);
        }
        let job = self.store.prepare_resume(id)?;
        let token = CancellationToken::new();
        if let Ok(mut jobs) = self.jobs.lock() {
            jobs.insert(job.id.clone(), token.clone());
        }
        self.spawn_run(root.to_path_buf(), job.id.clone(), token);
        Ok(job)
    }

    fn spawn_run(&self, root: std::path::PathBuf, job_id: String, token: CancellationToken) {
        let service = ProductionServiceHandle {
            app: self.app.clone(),
            store: self.store.clone(),
            index: self.index.clone(),
            cache_dir: self.cache_dir.clone(),
            gate: self.gate.clone(),
        };
        tauri::async_runtime::spawn(async move {
            service.run(root, job_id, token).await;
        });
    }

    pub fn retry(&self, root: &Path, id: &str) -> Result<Job, String> {
        let Some(existing) = self.store.get_job(id)? else {
            return Err("任务不存在".into());
        };
        if !existing.vault_path.is_empty() && !same_vault(&existing.vault_path, root) {
            return Err("这个任务属于另一个笔记库，切换回原库后才能重跑".into());
        }
        if !matches!(existing.status.as_str(), "error" | "cancelled" | "interrupted") {
            return Err("任务当前状态不能重跑（可能正在运行或已完成）".into());
        }
        self.store.clear_checkpoints(id)?;
        publish::cleanup_temp(root, id);
        let job = self.store.reset_for_retry(root, id)?;
        let token = CancellationToken::new();
        if let Ok(mut jobs) = self.jobs.lock() {
            jobs.insert(job.id.clone(), token.clone());
        }
        self.spawn_run(root.to_path_buf(), job.id.clone(), token);
        Ok(job)
    }

    pub fn rename(&self, id: &str, title: &str) -> Result<Job, String> {
        let title = title.trim();
        if title.is_empty() {
            return Err("任务标题不能为空".into());
        }
        self.store.set_title(id, title)?;
        self.store.get_job(id)?.ok_or_else(|| "任务不存在".into())
    }

    pub async fn test_model(
        &self,
        provider_id: &str,
        base_url: &str,
        model: &str,
        api_key: &str,
    ) -> Result<TestModelResult, String> {
        let preset = providers::preset_by_id(provider_id)
            .ok_or_else(|| format!("未知 provider：{provider_id}"))?;
        let model = model.trim();
        if model.is_empty() {
            return Err("还没有选择模型".into());
        }
        let settings = self.store.settings();
        let key = if api_key.trim().is_empty() {
            self.store.ai_key(provider_id)
        } else {
            api_key.trim().to_string()
        };
        if key.is_empty() && !preset.local {
            return Err("还没有配置 API Key".into());
        }
        let base = if base_url.trim().is_empty() {
            if provider_id == settings.ai_provider && !settings.ai_base_url.trim().is_empty() {
                settings.ai_base_url.clone()
            } else {
                preset.base_url.clone()
            }
        } else {
            base_url.trim().to_string()
        };
        if base.trim().is_empty() {
            return Err("接口地址为空".into());
        }
        let config = clients::AiConfig {
            provider: provider_id.into(),
            protocol: preset.protocol.clone(),
            base_url: base,
            model: model.into(),
            api_key: if key.is_empty() { "local".into() } else { key },
            retries: 0,
        };
        let started = std::time::Instant::now();
        let token = CancellationToken::new();
        let reply = tokio::time::timeout(
            std::time::Duration::from_secs(20),
            clients::chat(&config, "你是连接测试助手，只回复 OK。", "只回复 OK，不要解释。", &token),
        )
        .await
        .map_err(|_| "连接超时（20s），请检查地址、网络或模型".to_string())?
        .map_err(classify_model_error)?;
        let content = reply.content.trim().to_string();
        if content.is_empty() {
            return Err("模型返回了空内容".into());
        }
        Ok(TestModelResult {
            provider: provider_id.into(),
            model: model.into(),
            latency_ms: started.elapsed().as_millis() as u64,
            reply: content.chars().take(120).collect(),
            prompt_tokens: reply.prompt_tokens,
            completion_tokens: reply.completion_tokens,
            provider_cost: reply.provider_cost.unwrap_or(0.0),
            provider_currency: reply.provider_currency.unwrap_or_default(),
        })
    }

    pub fn versions(&self, root: &Path, note_id: &str) -> Result<Vec<VersionEntry>, String> {
        let artifact = crate::artifact::registry_for_note(root, note_id)
            .ok_or_else(|| "没有找到这篇笔记的生成记录".to_string())?;
        let mut entries = Vec::new();
        let outbox = root.join(crate::artifact::OUTBOX_DIR).join(note_id);
        if let Ok(dirs) = std::fs::read_dir(&outbox) {
            for entry in dirs.flatten() {
                let Some(revision) = entry.file_name().to_str().and_then(|value| value.parse::<i64>().ok()) else { continue };
                let note = entry.path().join("note.md");
                if !entry.path().join("ACK").exists() {
                    continue;
                }
                let Ok(body) = std::fs::read_to_string(&note) else { continue };
                let mut created_at = note.metadata().ok().and_then(|meta| meta.modified().ok()).and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok()).map(|duration| duration.as_millis() as i64).unwrap_or(0);
                if let Ok(raw) = std::fs::read_to_string(entry.path().join("artifact.json")) {
                    if let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw) {
                        if let Some(text) = value.get("generatedAt").and_then(serde_json::Value::as_str) {
                            if let Ok(date) = chrono::DateTime::parse_from_rfc3339(text) {
                                created_at = date.timestamp_millis();
                            }
                        }
                    }
                }
                entries.push(VersionEntry {
                    id: format!("gen:{revision}"),
                    kind: "generated".into(),
                    label: format!("生成 v{revision}"),
                    revision: Some(revision),
                    chars: body.chars().count(),
                    bytes: note.metadata().map(|meta| meta.len()).unwrap_or(0),
                    created_at,
                });
            }
        }
        let manual_dir = root.join(".yebian").join("versions").join(note_id);
        if let Ok(files) = std::fs::read_dir(&manual_dir) {
            for entry in files.flatten() {
                let Some(name) = entry.file_name().to_str().map(str::to_string) else { continue };
                if !name.starts_with("manual-") || !name.ends_with(".md") {
                    continue;
                }
                let path = entry.path();
                let Ok(body) = std::fs::read_to_string(&path) else { continue };
                let created_at = path.metadata().ok().and_then(|meta| meta.modified().ok()).and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok()).map(|duration| duration.as_millis() as i64).unwrap_or(0);
                entries.push(VersionEntry {
                    id: format!("manual:{name}"),
                    kind: "manual".into(),
                    label: "手改备份".into(),
                    revision: None,
                    chars: body.chars().count(),
                    bytes: path.metadata().map(|meta| meta.len()).unwrap_or(0),
                    created_at,
                });
            }
        }
        entries.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        let _ = artifact;
        Ok(entries)
    }

    pub fn version_content(&self, root: &Path, note_id: &str, version_id: &str) -> Result<String, String> {
        crate::artifact::registry_for_note(root, note_id)
            .ok_or_else(|| "没有找到这篇笔记的生成记录".to_string())?;
        if let Some(revision) = version_id.strip_prefix("gen:").and_then(|value| value.parse::<i64>().ok()) {
            return crate::outbox::generated_note(root, note_id, revision);
        }
        if let Some(name) = version_id.strip_prefix("manual:") {
            if name.starts_with("manual-") && name.ends_with(".md") && std::path::Path::new(name).file_name().map(|value| value.to_string_lossy() == name).unwrap_or(false) {
                return std::fs::read_to_string(root.join(".yebian").join("versions").join(note_id).join(name))
                    .map_err(|error| error.to_string());
            }
        }
        Err("版本不存在".into())
    }

    pub fn restore_version(&self, root: &Path, note_id: &str, version_id: &str) -> Result<String, String> {
        let artifact = crate::artifact::registry_for_note(root, note_id)
            .ok_or_else(|| "没有找到这篇笔记的生成记录".to_string())?;
        let target = root.join(&artifact.note_rel_path);
        let content = self.version_content(root, note_id, version_id)?;
        if target.is_file() {
            crate::outbox::snapshot_note(root, &target, note_id)?;
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        crate::vault::write_atomic(&target, &content)?;
        self.index.delta_wait(root, vec![target])?;
        Ok(artifact.note_rel_path)
    }

    pub fn conflict_check(&self, root: &Path, request: &StartJobRequest) -> Result<Option<ConflictInfo>, String> {
        let note_id = ProductionStore::note_id_for(&request.template_id, &request.source);
        let Some(artifact) = crate::artifact::registry_for_note(root, &note_id) else { return Ok(None) };
        let path = root.join(&artifact.note_rel_path);
        if !path.is_file() {
            return Ok(None);
        }
        let generated_revision = artifact.revision;
        let generated_body = std::fs::read_to_string(
            root.join(crate::artifact::OUTBOX_DIR)
                .join(&note_id)
                .join(generated_revision.to_string())
                .join("note.md"),
        )
        .ok()
        .or_else(|| crate::outbox::latest_generated(root, &note_id).map(|(_, body)| body));
        let Some(generated_body) = generated_body else { return Ok(None) };
        let current = std::fs::read_to_string(&path).map_err(|error| error.to_string())?;
        let current_body = crate::outbox::body_only(&current);
        let normalize = |value: &str| value.replace("\r\n", "\n").trim().to_string();
        if normalize(current_body) == normalize(&generated_body) {
            return Ok(None);
        }
        Ok(Some(ConflictInfo {
            note_id,
            title: artifact.title,
            note_path: artifact.note_rel_path,
            current_chars: current_body.chars().count(),
            generated_chars: generated_body.chars().count(),
            generated_revision,
        }))
    }

    pub fn delete(&self, root: Option<&Path>, id: &str) -> Result<(), String> {
        if let Ok(jobs) = self.jobs.lock() {
            if let Some(token) = jobs.get(id) {
                token.cancel();
            }
        }
        let result = self.store.delete_job(id);
        if let Some(root) = root {
            publish::cleanup_temp(root, id);
        }
        result
    }

    pub fn data_usage(&self, root: &Path) -> Result<serde_json::Value, String> {
        self.store.data_usage(root)
    }

    pub fn clear_temp(&self, root: &Path) -> Result<ClearTempResult, String> {
        let dir = root.join(".yebian").join("production-tmp");
        let protected = self.store.protected_temp_jobs().unwrap_or_default();
        let mut cleared_bytes = 0u64;
        let mut kept_jobs = 0usize;
        let mut kept_bytes = 0u64;
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let id = entry.file_name().to_string_lossy().to_string();
                if protected.iter().any(|item| item == &id) {
                    kept_jobs += 1;
                    kept_bytes += path_size(&path);
                    continue;
                }
                cleared_bytes += path_size(&path);
                let _ = if path.is_dir() { std::fs::remove_dir_all(&path) } else { std::fs::remove_file(&path) };
            }
        }
        Ok(ClearTempResult { cleared_bytes, kept_jobs, kept_bytes })
    }

    pub fn clear_finished(&self) -> Result<usize, String> {
        self.store.clear_finished()
    }

    pub fn clear_resumable(&self, root: &Path) -> Result<usize, String> {
        let ids = self.store.job_ids_with_status(&["error", "cancelled", "interrupted"])?;
        for id in &ids {
            publish::cleanup_temp(root, id);
        }
        self.store.delete_jobs(&ids)
    }

    pub fn providers(&self) -> Vec<ProviderPreset> {
        providers::presets()
    }

    pub async fn models(
        &self,
        provider_id: &str,
        base_url: &str,
        api_key: &str,
    ) -> Result<ModelList, String> {
        let preset = providers::preset_by_id(provider_id)
            .ok_or_else(|| format!("未知 provider：{provider_id}"))?;
        let settings = self.store.settings();
        let key = if api_key.trim().is_empty() {
            self.store.ai_key(provider_id)
        } else {
            api_key.to_string()
        };
        let catalog = PriceCatalog::load(&self.cache_dir, false)
            .await
            .unwrap_or_else(|_| PriceCatalog::empty());
        let mut list = providers::fetch_models(&preset, base_url, &key, settings.ai_exchange_rate, &catalog, false).await?;
        let overrides = self.store.list_model_prices(Some(provider_id)).unwrap_or_default();
        providers::apply_manual_prices(&mut list.models, &overrides, settings.ai_exchange_rate);
        if catalog.is_empty() {
            let note = "价目目录暂时不可用：只记录 token，不显示金额";
            list.warning = if list.warning.is_empty() { note.into() } else { format!("{}；{note}", list.warning) };
        }
        Ok(list)
    }

    pub async fn price(&self, provider_id: &str, model: &str) -> Option<PriceQuote> {
        resolve_price(&self.store, &self.cache_dir, provider_id, model).await
    }

    pub async fn balance(
        &self,
        provider_id: &str,
        base_url: &str,
        api_key: &str,
    ) -> Result<BalanceResult, String> {
        let preset = providers::preset_by_id(provider_id)
            .ok_or_else(|| format!("未知 provider：{provider_id}"))?;
        let key = if api_key.trim().is_empty() {
            self.store.ai_key(provider_id)
        } else {
            api_key.to_string()
        };
        providers::fetch_balance(&preset, base_url, &key).await
    }

    pub fn usage_summary(&self) -> Result<serde_json::Value, String> {
        self.store.usage_summary()
    }

    pub async fn bili_login_start(&self) -> Result<BiliLoginStart, String> {
        clients::bili_login_start().await
    }

    pub async fn bili_login_poll(&self, key: &str) -> Result<BiliLoginPoll, String> {
        let result = clients::bili_login_poll(key).await?;
        if result.status == "success" {
            if let Some(cookies) = &result.cookies {
                self.store.set_setting("production.bili.cookies", cookies)?;
            }
            if let Some(account) = &result.account {
                self.store.set_setting("production.bili.account", &serde_json::to_string(account).map_err(|e| e.to_string())?)?;
            }
        }
        Ok(result)
    }

    pub fn bili_logout(&self) -> Result<(), String> {
        self.store.set_setting("production.bili.cookies", "")?;
        self.store.set_setting("production.bili.account", "")?;
        Ok(())
    }

    pub fn bili_account(&self) -> Option<BiliAccount> {
        self.store
            .get_setting("production.bili.account")
            .ok()
            .flatten()
            .and_then(|raw| serde_json::from_str(&raw).ok())
    }

    pub async fn bili_favorites(&self) -> Result<Vec<BiliFolder>, String> {
        let cookies = self.store.get_setting("production.bili.cookies").ok().flatten().unwrap_or_default();
        clients::fetch_bili_folders(&cookies).await
    }

    pub async fn bili_folder_videos(&self, media_id: i64, page: u32) -> Result<BiliVideoPage, String> {
        let cookies = self.store.get_setting("production.bili.cookies").ok().flatten().unwrap_or_default();
        clients::fetch_bili_folder_videos(&cookies, media_id, page).await
    }

    pub async fn bili_watch_later(&self) -> Result<Vec<BiliVideoRef>, String> {
        let cookies = self.store.get_setting("production.bili.cookies").ok().flatten().unwrap_or_default();
        clients::fetch_bili_watch_later(&cookies).await
    }

    pub async fn bili_history(&self) -> Result<Vec<BiliVideoRef>, String> {
        let cookies = self.store.get_setting("production.bili.cookies").ok().flatten().unwrap_or_default();
        clients::fetch_bili_history(&cookies).await
    }

    pub async fn bili_preview(&self, url: &str) -> Result<BiliVideoPreview, String> {
        clients::fetch_bili_preview(url).await
    }
}

fn same_vault(stored: &str, current: &Path) -> bool {
    let left = stored.replace('\\', "/").trim_end_matches('/').to_lowercase();
    let right = current.to_string_lossy().replace('\\', "/").trim_end_matches('/').to_lowercase();
    left == right
}

fn path_size(path: &Path) -> u64 {
    if path.is_file() {
        return path.metadata().map(|meta| meta.len()).unwrap_or(0);
    }
    let mut total = 0;
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            total += path_size(&entry.path());
        }
    }
    total
}

fn classify_model_error(error: String) -> String {
    let lower = error.to_ascii_lowercase();
    if lower.contains("401") || lower.contains("403") {
        return "Key 无效，或这个 Key 没有该模型权限".into();
    }
    if lower.contains("404") {
        return "模型 ID 或接口地址不对（HTTP 404）".into();
    }
    if lower.contains("429") {
        return "触发限流或配额不足（HTTP 429）".into();
    }
    if lower.contains("ai 请求失败") || lower.contains("error sending request") {
        return "连接失败，请检查接口地址、网络或代理".into();
    }
    if lower.contains("返回了空内容") {
        return "模型返回了空内容".into();
    }
    if lower.contains("400") {
        return format!("请求被服务商拒绝：{error}");
    }
    error
}

#[derive(Clone)]
struct ProductionServiceHandle {
    app: AppHandle,
    store: ProductionStore,
    index: Arc<IndexService>,
    cache_dir: std::path::PathBuf,
    gate: Arc<Semaphore>,
}

impl ProductionServiceHandle {
    async fn run(self, root: std::path::PathBuf, id: String, token: CancellationToken) {
        let permit = match self.gate.clone().acquire_owned().await {
            Ok(permit) => permit,
            Err(_) => return,
        };
        let result = pipeline::run(&self.app, &self.store, &self.cache_dir, &root, &id, &token).await;
        drop(permit);
        match result {
            Ok(note) => {
                if !note.is_empty() {
                    let abs = root.join(note.replace('/', std::path::MAIN_SEPARATOR_STR));
                    let _ = self.index.delta_wait(&root, vec![abs]);
                }
            }
            Err(error) => {
                let _ = self.store.fail(&id, &error);
            }
        }
        if let Ok(Some(job)) = self.store.get_job(&id) {
            let _ = self.app.emit("production-update", &job);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::classify_model_error;
    use super::store::ProductionStore;

    #[tokio::test]
    async fn manual_price_wins_for_custom_provider_without_a_catalog() {
        let dir = std::env::temp_dir().join(format!("yebian-manual-price-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = ProductionStore::open(&dir.join("yebian.sqlite")).unwrap();
        assert!(super::resolve_price(&store, &dir, "custom", "my-model").await.is_none());
        store.set_model_price("custom", "my-model", 10.0, 30.0).unwrap();
        let quote = super::resolve_price(&store, &dir, "custom", "my-model").await.unwrap();
        assert_eq!(quote.source, "manual");
        assert_eq!(quote.input_cny, 10.0);
        assert_eq!(quote.output_cny, 30.0);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn classifies_model_connection_errors() {
        assert!(classify_model_error("AI 接口 401 Unauthorized：bad key".into()).contains("Key 无效"));
        assert!(classify_model_error("AI 接口 403 Forbidden：no access".into()).contains("没有该模型权限"));
        assert!(classify_model_error("AI 接口 404 Not Found：no model".into()).contains("模型 ID"));
        assert!(classify_model_error("AI 接口 429 Too Many Requests".into()).contains("限流"));
        assert!(classify_model_error("AI 请求失败：connection refused".into()).contains("连接失败"));
        assert!(classify_model_error("AI 接口 400 Bad Request：bad".into()).contains("请求被服务商拒绝"));
    }
}
