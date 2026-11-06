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

pub use clients::{BiliAccount, BiliFolder, BiliLoginPoll, BiliLoginStart, BiliVideoPreview};
pub use prompts::{PromptEntry, builtin_prompts};
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
        }
    }
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

    pub fn retry(&self, root: &Path, id: &str) -> Result<Job, String> {
        let job = self.store.reset_for_retry(id)?;
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
        let job_id = job.id.clone();
        tauri::async_runtime::spawn(async move {
            service.run(root, job_id, token).await;
        });
        Ok(job)
    }

    pub fn delete(&self, id: &str) -> Result<(), String> {
        if let Ok(jobs) = self.jobs.lock() {
            if let Some(token) = jobs.get(id) {
                token.cancel();
            }
        }
        self.store.delete_job(id)
    }

    pub fn data_usage(&self, root: &Path) -> Result<serde_json::Value, String> {
        self.store.data_usage(root)
    }

    pub fn clear_temp(&self, root: &Path) -> Result<u64, String> {
        let dir = root.join(".yebian").join("production-tmp");
        let mut bytes = 0u64;
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                if let Ok(meta) = entry.metadata() {
                    bytes += meta.len();
                }
                let _ = std::fs::remove_file(entry.path());
            }
        }
        Ok(bytes)
    }

    pub fn clear_finished(&self) -> Result<usize, String> {
        self.store.clear_finished()
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
        if catalog.is_empty() {
            let note = "价目目录暂时不可用：只记录 token，不显示金额";
            list.warning = if list.warning.is_empty() { note.into() } else { format!("{}；{note}", list.warning) };
        }
        Ok(list)
    }

    pub async fn price(&self, provider_id: &str, model: &str) -> Option<PriceQuote> {
        let preset = providers::preset_by_id(provider_id)?;
        let settings = self.store.settings();
        let catalog = PriceCatalog::load(&self.cache_dir, false).await.ok()?;
        catalog.price(&preset.catalog_id, model, settings.ai_exchange_rate)
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

    pub async fn bili_folder_videos(&self, media_id: i64) -> Result<Vec<BiliVideoRef>, String> {
        let cookies = self.store.get_setting("production.bili.cookies").ok().flatten().unwrap_or_default();
        clients::fetch_bili_folder_videos(&cookies, media_id).await
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
