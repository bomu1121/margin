use std::collections::HashMap;
use std::path::Path;

use chrono::{Datelike, Local, TimeZone};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, OptionalExtension};
use sha2::{Digest, Sha256};

use super::{
    CheckpointRow, Job, JobConfig, JobResult, JobStep, LogEntry, ModelPrice,
    ProductionSettings, PromptEntry, PromptVersion, SaveSettings, SeenVideo, SourceSpec,
    StartJobRequest, Subscription, SubscriptionFilters, SubscriptionInput, Usage,
};
use crate::production::checkpoint::PIPELINE_VERSION;
use crate::production::templates::{self, TemplateInfo};

#[derive(Clone)]
pub struct ProductionStore {
    conn: Arc<Mutex<Connection>>,
}

const DEFAULT_AI_BASE: &str = "https://api.deepseek.com/v1";
const DEFAULT_AI_MODEL: &str = "deepseek-v4-pro";
const DEFAULT_ASR_BASE: &str = "https://api.xiaomimimo.com/v1";
const DEFAULT_ASR_MODEL: &str = "mimo-v2.5-asr";

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

fn db_err(error: rusqlite::Error) -> String {
    error.to_string()
}

impl ProductionStore {
    pub fn open(path: &Path) -> Result<Self, String> {
        let conn = Connection::open(path).map_err(db_err)?;
        conn.execute_batch(
            "
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;
            PRAGMA busy_timeout = 30000;

            CREATE TABLE IF NOT EXISTS settings (
              key TEXT PRIMARY KEY,
              value TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS production_jobs (
              id TEXT PRIMARY KEY,
              title TEXT NOT NULL,
              template_id TEXT NOT NULL,
              source_kind TEXT NOT NULL,
              provider TEXT NOT NULL DEFAULT '',
              model TEXT NOT NULL DEFAULT '',
              source_json TEXT NOT NULL,
              options_json TEXT NOT NULL,
              status TEXT NOT NULL,
              progress INTEGER NOT NULL DEFAULT 0,
              current_step TEXT NOT NULL DEFAULT '',
              error TEXT NOT NULL DEFAULT '',
              note_id TEXT NOT NULL DEFAULT '',
              note_rel_path TEXT NOT NULL DEFAULT '',
              result_json TEXT NOT NULL DEFAULT '',
              usage_json TEXT NOT NULL DEFAULT '{}',
              created_at INTEGER NOT NULL,
              updated_at INTEGER NOT NULL,
              started_at INTEGER NOT NULL DEFAULT 0,
              finished_at INTEGER NOT NULL DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS idx_production_jobs_created
              ON production_jobs(created_at DESC);

            CREATE TABLE IF NOT EXISTS production_steps (
              job_id TEXT NOT NULL,
              idx INTEGER NOT NULL,
              key TEXT NOT NULL,
              label TEXT NOT NULL,
              status TEXT NOT NULL,
              detail TEXT NOT NULL DEFAULT '',
              PRIMARY KEY (job_id, idx)
            );

            CREATE TABLE IF NOT EXISTS production_logs (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              job_id TEXT NOT NULL,
              step_index INTEGER,
              level TEXT NOT NULL,
              message TEXT NOT NULL,
              created_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_production_logs_job
              ON production_logs(job_id, id);

            CREATE TABLE IF NOT EXISTS production_prompts (
              id TEXT PRIMARY KEY,
              name TEXT NOT NULL,
              description TEXT NOT NULL DEFAULT '',
              system TEXT NOT NULL,
              created_at INTEGER NOT NULL,
              updated_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS production_prompt_versions (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              prompt_id TEXT NOT NULL,
              version INTEGER NOT NULL,
              name TEXT NOT NULL,
              description TEXT NOT NULL DEFAULT '',
              system TEXT NOT NULL,
              created_at INTEGER NOT NULL,
              UNIQUE(prompt_id, version)
            );
            CREATE INDEX IF NOT EXISTS idx_prompt_versions_prompt
              ON production_prompt_versions(prompt_id, version DESC);

            CREATE TABLE IF NOT EXISTS production_model_prices (
              provider TEXT NOT NULL,
              model TEXT NOT NULL,
              input_cny REAL NOT NULL,
              output_cny REAL NOT NULL,
              updated_at INTEGER NOT NULL,
              PRIMARY KEY (provider, model)
            );

            CREATE TABLE IF NOT EXISTS production_checkpoints (
              job_id TEXT NOT NULL,
              unit_key TEXT NOT NULL,
              step_key TEXT NOT NULL,
              kind TEXT NOT NULL,
              input_hash TEXT NOT NULL,
              payload_json TEXT NOT NULL,
              usage_json TEXT NOT NULL DEFAULT '{}',
              payload_bytes INTEGER NOT NULL DEFAULT 0,
              created_at INTEGER NOT NULL,
              updated_at INTEGER NOT NULL,
              PRIMARY KEY (job_id, unit_key)
            );
            CREATE INDEX IF NOT EXISTS idx_production_checkpoints_step
              ON production_checkpoints(job_id, step_key, created_at);

            CREATE TABLE IF NOT EXISTS production_subscriptions (
              id TEXT PRIMARY KEY,
              vault_path TEXT NOT NULL,
              mid INTEGER NOT NULL,
              name TEXT NOT NULL,
              avatar TEXT NOT NULL DEFAULT '',
              enabled INTEGER NOT NULL DEFAULT 1,
              interval_minutes INTEGER NOT NULL DEFAULT 30,
              template_id TEXT NOT NULL DEFAULT 'video-note',
              prompt TEXT NOT NULL DEFAULT '速览版',
              filters_json TEXT NOT NULL DEFAULT '{}',
              baseline_at INTEGER NOT NULL DEFAULT 0,
              last_checked_at INTEGER NOT NULL DEFAULT 0,
              next_check_at INTEGER NOT NULL DEFAULT 0,
              last_error TEXT NOT NULL DEFAULT '',
              last_message TEXT NOT NULL DEFAULT '',
              consecutive_failures INTEGER NOT NULL DEFAULT 0,
              created_at INTEGER NOT NULL,
              updated_at INTEGER NOT NULL,
              UNIQUE(vault_path, mid)
            );
            CREATE INDEX IF NOT EXISTS idx_production_subscriptions_due
              ON production_subscriptions(vault_path, enabled, next_check_at);

            CREATE TABLE IF NOT EXISTS production_seen_videos (
              vault_path TEXT NOT NULL,
              mid INTEGER NOT NULL,
              bvid TEXT NOT NULL,
              title TEXT NOT NULL DEFAULT '',
              pubdate INTEGER NOT NULL DEFAULT 0,
              duration INTEGER NOT NULL DEFAULT 0,
              cover TEXT NOT NULL DEFAULT '',
              status TEXT NOT NULL,
              job_id TEXT NOT NULL DEFAULT '',
              note_id TEXT NOT NULL DEFAULT '',
              first_seen_at INTEGER NOT NULL,
              updated_at INTEGER NOT NULL,
              PRIMARY KEY(vault_path, mid, bvid)
            );
            CREATE INDEX IF NOT EXISTS idx_production_seen_latest
              ON production_seen_videos(vault_path, mid, pubdate DESC);
            CREATE INDEX IF NOT EXISTS idx_production_seen_day
              ON production_seen_videos(vault_path, status, first_seen_at);
            ",
        )
        .map_err(db_err)?;
        ensure_column(&conn, "production_jobs", "provider", "TEXT NOT NULL DEFAULT ''")?;
        ensure_column(&conn, "production_jobs", "model", "TEXT NOT NULL DEFAULT ''")?;
        ensure_column(&conn, "production_jobs", "vault_path", "TEXT NOT NULL DEFAULT ''")?;
        ensure_column(&conn, "production_jobs", "config_json", "TEXT NOT NULL DEFAULT ''")?;
        ensure_column(&conn, "production_jobs", "checkpoint_at", "INTEGER NOT NULL DEFAULT 0")?;
        ensure_column(&conn, "production_subscriptions", "last_message", "TEXT NOT NULL DEFAULT ''")?;
        migrate_ai_key(&conn)?;
        conn.execute(
            "INSERT INTO production_prompt_versions (prompt_id, version, name, description, system, created_at)
             SELECT id, 1, name, description, system, updated_at FROM production_prompts
             WHERE NOT EXISTS (SELECT 1 FROM production_prompt_versions WHERE prompt_id = production_prompts.id)",
            [],
        )
        .map_err(db_err)?;
        Ok(Self { conn: Arc::new(Mutex::new(conn)) })
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, Connection>, String> {
        self.conn.lock().map_err(|_| "production database lock poisoned".to_string())
    }

    pub fn get_setting(&self, key: &str) -> Result<Option<String>, String> {
        let conn = self.lock()?;
        conn.query_row("SELECT value FROM settings WHERE key = ?1", params![key], |row| row.get(0))
            .optional()
            .map_err(db_err)
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<(), String> {
        let conn = self.lock()?;
        conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )
        .map_err(db_err)?;
        Ok(())
    }

    pub fn ai_key(&self, provider: &str) -> String {
        self.get_setting(&format!("production.ai.api_key.{provider}"))
            .ok()
            .flatten()
            .unwrap_or_default()
    }

    fn setting_or(&self, key: &str, fallback: &str) -> String {
        self.get_setting(key).ok().flatten().unwrap_or_else(|| fallback.to_string())
    }

    fn setting_number(&self, key: &str, fallback: f64) -> f64 {
        self.get_setting(key)
            .ok()
            .flatten()
            .and_then(|value| value.parse::<f64>().ok())
            .unwrap_or(fallback)
    }

    fn setting_bool(&self, key: &str, fallback: bool) -> bool {
        self.get_setting(key)
            .ok()
            .flatten()
            .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
            .unwrap_or(fallback)
    }

    pub fn settings(&self) -> ProductionSettings {
        let account: Option<super::BiliAccount> = self
            .get_setting("production.bili.account")
            .ok()
            .flatten()
            .and_then(|raw| serde_json::from_str(&raw).ok());
        let cookies = self.get_setting("production.bili.cookies").ok().flatten().unwrap_or_default();
        let ai_provider = self.setting_or("production.ai.provider", "deepseek");
        ProductionSettings {
            has_ai_key: !self.ai_key(&ai_provider).is_empty(),
            ai_provider,
            ai_protocol: self.setting_or("production.ai.protocol", "openai"),
            ai_base_url: self.setting_or("production.ai.base_url", DEFAULT_AI_BASE),
            ai_model: self.setting_or("production.ai.model", DEFAULT_AI_MODEL),
            ai_exchange_rate: self.setting_number("production.ai.exchange_rate", 7.2).max(0.01),
            monthly_budget_cny: self.setting_number("production.monthly_budget_cny", 0.0),
            asr_engine: self.setting_or("production.asr.engine", "mimo"),
            asr_base_url: self.setting_or("production.asr.base_url", DEFAULT_ASR_BASE),
            asr_model: self.setting_or("production.asr.model", DEFAULT_ASR_MODEL),
            has_asr_key: !self.get_setting("production.asr.api_key").ok().flatten().unwrap_or_default().is_empty(),
            asr_price_per_minute: self.setting_number("production.asr.price_per_minute", 0.0),
            search_provider: self.setting_or("production.search.provider", "deepseek"),
            has_search_key: !self.get_setting("production.search.api_key").ok().flatten().unwrap_or_default().is_empty(),
            has_ctext_key: !self
                .get_setting("production.history.ctext_api_key")
                .ok()
                .flatten()
                .unwrap_or_default()
                .is_empty(),
            search_max_results: self.setting_number("production.search.max_results", 5.0).max(1.0) as u32,
            bili_logged_in: !cookies.is_empty(),
            bili_name: account.as_ref().map(|item| item.name.clone()).unwrap_or_default(),
            bili_mid: account.as_ref().map(|item| item.mid).unwrap_or(0),
            default_template: self.setting_or("production.default_template", "video-note"),
            retries: self.setting_number("production.retries", 2.0).max(0.0) as u32,
            notify: self.setting_bool("production.notify", true),
            sound: self.setting_bool("production.sound", true),
            ffmpeg_path: self.setting_or("production.ffmpeg_path", ""),
            default_prompt: self.setting_or("production.default_prompt", "成文版"),
            default_fields: self
                .get_setting("production.default_fields")
                .ok()
                .flatten()
                .and_then(|raw| serde_json::from_str(&raw).ok())
                .unwrap_or_else(|| vec!["人物".into(), "事件".into()]),
        }
    }

    pub fn save_settings(&self, input: &SaveSettings) -> Result<(), String> {
        let pairs: [(&str, Option<String>); 17] = [
            ("production.ai.provider", input.ai_provider.clone()),
            ("production.ai.protocol", input.ai_protocol.clone()),
            ("production.ai.base_url", input.ai_base_url.clone()),
            ("production.ai.model", input.ai_model.clone()),
            ("production.asr.engine", input.asr_engine.clone()),
            ("production.asr.base_url", input.asr_base_url.clone()),
            ("production.asr.model", input.asr_model.clone()),
            ("production.search.provider", input.search_provider.clone()),
            ("production.default_template", input.default_template.clone()),
            ("production.default_prompt", input.default_prompt.clone()),
            ("production.ffmpeg_path", input.ffmpeg_path.clone()),
            ("production.asr.api_key", input.asr_key.clone().filter(|value| !value.trim().is_empty())),
            ("production.search.api_key", input.search_key.clone().filter(|value| !value.trim().is_empty())),
            ("production.history.ctext_api_key", input.ctext_key.clone().filter(|value| !value.trim().is_empty())),
            ("production.ai.exchange_rate", input.ai_exchange_rate.map(|value| value.to_string())),
            ("production.monthly_budget_cny", input.monthly_budget_cny.map(|value| value.to_string())),
            ("production.asr.price_per_minute", input.asr_price_per_minute.map(|value| value.to_string())),
        ];
        for (key, value) in pairs {
            if let Some(value) = value {
                self.set_setting(key, value.trim())?;
            }
        }
        if let Some(key) = input.ai_key.as_ref().filter(|value| !value.trim().is_empty()) {
            let provider = input.ai_provider.clone().unwrap_or_else(|| self.setting_or("production.ai.provider", "deepseek"));
            self.set_setting(&format!("production.ai.api_key.{provider}"), key.trim())?;
        }
        if let Some(value) = input.search_max_results {
            self.set_setting("production.search.max_results", &value.max(1).to_string())?;
        }
        if let Some(value) = input.retries {
            self.set_setting("production.retries", &value.to_string())?;
        }
        if let Some(value) = input.notify {
            self.set_setting("production.notify", if value { "1" } else { "0" })?;
        }
        if let Some(value) = input.sound {
            self.set_setting("production.sound", if value { "1" } else { "0" })?;
        }
        if let Some(fields) = &input.default_fields {
            self.set_setting("production.default_fields", &serde_json::to_string(fields).map_err(|e| e.to_string())?)?;
        }
        Ok(())
    }

    pub fn note_id_for(template_id: &str, source: &SourceSpec) -> String {
        stable_note_id(template_id, &source_key(source))
    }

    pub fn create_job(&self, root: &Path, request: StartJobRequest, template: &TemplateInfo) -> Result<Job, String> {
        let id = format!("run_{}", uuid::Uuid::new_v4().simple());
        let timestamp = now_ms();
        let source_key = source_key(&request.source);
        let note_id = if request.options.conflict_action == "keep_both" {
            stable_note_id(&request.template_id, &format!("{source_key}|keep_both|{timestamp}"))
        } else {
            stable_note_id(&request.template_id, &source_key)
        };
        let title = if request.title.trim().is_empty() {
            match request.source.kind.as_str() {
                "bili" => "B站视频笔记".to_string(),
                "favorites" => format!("B站收藏 · {} 个视频", request.source.videos.len()),
                "file" => Path::new(&request.source.path)
                    .file_stem()
                    .map(|value| value.to_string_lossy().to_string())
                    .unwrap_or_else(|| "本地音视频".into()),
                _ => "粘贴文稿".to_string(),
            }
        } else {
            request.title.trim().to_string()
        };
        let steps = steps_for(template, &request.source);
        let settings = self.settings();
        let config = JobConfig {
            pipeline_version: PIPELINE_VERSION,
            provider: settings.ai_provider.clone(),
            protocol: settings.ai_protocol.clone(),
            base_url: settings.ai_base_url.clone(),
            model: settings.ai_model.clone(),
            asr_engine: settings.asr_engine.clone(),
            asr_base_url: settings.asr_base_url.clone(),
            asr_model: settings.asr_model.clone(),
            asr_price_per_minute: settings.asr_price_per_minute,
            ffmpeg_path: settings.ffmpeg_path.clone(),
            search_provider: settings.search_provider.clone(),
            default_prompt: settings.default_prompt.clone(),
            exchange_rate: settings.ai_exchange_rate,
            retries: settings.retries,
            price: None,
        };
        let vault_path = root.to_string_lossy().to_string();
        let job = Job {
            id: id.clone(),
            title,
            template_id: request.template_id,
            source_kind: request.source.kind.clone(),
            provider: settings.ai_provider,
            model: settings.ai_model,
            vault_path: vault_path.clone(),
            source: request.source.clone(),
            options: request.options.clone(),
            status: "queued".into(),
            progress: 0,
            current_step: String::new(),
            error: String::new(),
            note_id,
            note_rel_path: String::new(),
            result: None,
            usage: Usage { currency: "CNY".into(), ..Usage::default() },
            created_at: timestamp,
            updated_at: timestamp,
            started_at: 0,
            finished_at: 0,
            steps,
        };
        let conn = self.lock()?;
        conn.execute(
            "INSERT INTO production_jobs
              (id, title, template_id, source_kind, provider, model, source_json, options_json,
               status, progress, current_step, error, note_id, note_rel_path, result_json,
               usage_json, created_at, updated_at, vault_path, config_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'queued', 0, '', '', ?9, '', '', ?10, ?11, ?11, ?12, ?13)",
            params![
                job.id,
                job.title,
                job.template_id,
                job.source_kind,
                job.provider,
                job.model,
                serde_json::to_string(&job.source).map_err(|e| e.to_string())?,
                serde_json::to_string(&job.options).map_err(|e| e.to_string())?,
                job.note_id,
                serde_json::to_string(&job.usage).map_err(|e| e.to_string())?,
                timestamp,
                vault_path,
                serde_json::to_string(&config).map_err(|e| e.to_string())?,
            ],
        )
        .map_err(db_err)?;
        for (index, step) in job.steps.iter().enumerate() {
            conn.execute(
                "INSERT INTO production_steps (job_id, idx, key, label, status, detail)
                 VALUES (?1, ?2, ?3, ?4, 'pending', '')",
                params![job.id, index as i64, step.key, step.label],
            )
            .map_err(db_err)?;
        }
        Ok(job)
    }

    pub fn bind_legacy_jobs(&self, vault: &str) -> Result<usize, String> {
        let conn = self.lock()?;
        let changed = conn
            .execute(
                "UPDATE production_jobs SET vault_path = ?1, updated_at = ?2 WHERE vault_path = ''",
                params![vault, now_ms()],
            )
            .map_err(db_err)?;
        Ok(changed)
    }

    pub fn list_jobs(&self, limit: usize) -> Result<Vec<Job>, String> {
        self.list_jobs_for_vault(limit, None)
    }

    pub fn list_jobs_for_vault(&self, limit: usize, vault: Option<&str>) -> Result<Vec<Job>, String> {
        let conn = self.lock()?;
        let mut statement = conn
            .prepare(
                "SELECT id, title, template_id, source_kind, provider, model, source_json,
                        options_json, status, progress, current_step, error, note_id,
                        note_rel_path, result_json, usage_json, created_at, updated_at,
                        started_at, finished_at, vault_path
                 FROM production_jobs
                 WHERE (?2 IS NULL OR vault_path = ?2)
                 ORDER BY created_at DESC LIMIT ?1",
            )
            .map_err(db_err)?;
        let rows = statement
            .query_map(params![limit as i64, vault], row_to_job)
            .map_err(db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_err)?;
        drop(statement);
        drop(conn);
        let mut jobs = rows;
        for job in &mut jobs {
            job.steps = self.steps(&job.id)?;
        }
        Ok(jobs)
    }

    fn list_jobs_since(&self, since: i64) -> Result<Vec<Job>, String> {
        let conn = self.lock()?;
        let mut statement = conn
            .prepare(
                "SELECT id, title, template_id, source_kind, provider, model, source_json,
                        options_json, status, progress, current_step, error, note_id,
                        note_rel_path, result_json, usage_json, created_at, updated_at,
                        started_at, finished_at, vault_path
                 FROM production_jobs WHERE created_at >= ?1 ORDER BY created_at DESC",
            )
            .map_err(db_err)?;
        let rows = statement
            .query_map(params![since], row_to_job)
            .map_err(db_err)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(db_err)
    }

    pub fn get_job(&self, id: &str) -> Result<Option<Job>, String> {
        let job = {
            let conn = self.lock()?;
            conn.query_row(
                "SELECT id, title, template_id, source_kind, provider, model, source_json,
                        options_json, status, progress, current_step, error, note_id,
                        note_rel_path, result_json, usage_json, created_at, updated_at,
                        started_at, finished_at, vault_path
                 FROM production_jobs WHERE id = ?1",
                params![id],
                row_to_job,
            )
            .optional()
            .map_err(db_err)?
        };
        let Some(mut job) = job else { return Ok(None) };
        job.steps = self.steps(id)?;
        Ok(Some(job))
    }

    fn steps(&self, id: &str) -> Result<Vec<JobStep>, String> {
        let conn = self.lock()?;
        let mut statement = conn
            .prepare(
                "SELECT idx, key, label, status, detail FROM production_steps
                 WHERE job_id = ?1 ORDER BY idx",
            )
            .map_err(db_err)?;
        let rows = statement
            .query_map(params![id], |row| {
                Ok(JobStep {
                    index: row.get::<_, i64>(0)? as usize,
                    key: row.get(1)?,
                    label: row.get(2)?,
                    status: row.get(3)?,
                    detail: row.get(4)?,
                })
            })
            .map_err(db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_err)?;
        Ok(rows)
    }



    pub fn set_source(&self, id: &str, source: &SourceSpec) -> Result<(), String> {
        let conn = self.lock()?;
        conn.execute(
            "UPDATE production_jobs SET source_json = ?2, updated_at = ?3 WHERE id = ?1",
            params![id, serde_json::to_string(source).map_err(|e| e.to_string())?, now_ms()],
        )
        .map_err(db_err)?;
        Ok(())
    }

    pub fn set_title(&self, id: &str, title: &str) -> Result<(), String> {
        let title = title.trim();
        if title.is_empty() {
            return Ok(());
        }
        let conn = self.lock()?;
        conn.execute(
            "UPDATE production_jobs SET title = ?2, updated_at = ?3 WHERE id = ?1",
            params![id, title, now_ms()],
        )
        .map_err(db_err)?;
        Ok(())
    }

    pub fn start_job(&self, id: &str) -> Result<(), String> {
        let timestamp = now_ms();
        let conn = self.lock()?;
        conn.execute(
            "UPDATE production_jobs SET status = 'running', started_at = ?2, updated_at = ?2
             WHERE id = ?1 AND status = 'queued'",
            params![id, timestamp],
        )
        .map_err(db_err)?;
        Ok(())
    }

    pub fn set_step(&self, job_id: &str, index: usize, status: &str, detail: &str) -> Result<(), String> {
        let conn = self.lock()?;
        conn.execute(
            "UPDATE production_steps SET status = ?3, detail = ?4 WHERE job_id = ?1 AND idx = ?2",
            params![job_id, index as i64, status, detail],
        )
        .map_err(db_err)?;
        let total: i64 = conn
            .query_row("SELECT COUNT(*) FROM production_steps WHERE job_id = ?1", params![job_id], |row| row.get(0))
            .map_err(db_err)?;
        let done: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM production_steps WHERE job_id = ?1 AND status IN ('done','skipped')",
                params![job_id],
                |row| row.get(0),
            )
            .map_err(db_err)?;
        let current: String = conn
            .query_row(
                "SELECT label FROM production_steps WHERE job_id = ?1 AND idx = ?2",
                params![job_id, index as i64],
                |row| row.get(0),
            )
            .optional()
            .map_err(db_err)?
            .unwrap_or_default();
        let percent = if total > 0 { (done * 100 / total).min(99) } else { 0 };
        conn.execute(
            "UPDATE production_jobs SET progress = ?2, current_step = ?3, updated_at = ?4 WHERE id = ?1",
            params![job_id, percent, current, now_ms()],
        )
        .map_err(db_err)?;
        Ok(())
    }

    pub fn progress(&self, job_id: &str, percent: i64, detail: &str) -> Result<(), String> {
        let conn = self.lock()?;
        conn.execute(
            "UPDATE production_jobs SET progress = ?2, current_step = ?3, updated_at = ?4 WHERE id = ?1",
            params![job_id, percent.clamp(0, 99), detail, now_ms()],
        )
        .map_err(db_err)?;
        Ok(())
    }

    pub fn finish(&self, job_id: &str, result: &JobResult, usage: &Usage, note_rel_path: &str) -> Result<(), String> {
        let conn = self.lock()?;
        let timestamp = now_ms();
        conn.execute(
            "UPDATE production_jobs SET status = 'success', progress = 100, current_step = '完成',
                    error = '', result_json = ?2, usage_json = ?3, note_rel_path = ?4,
                    updated_at = ?5, finished_at = ?5 WHERE id = ?1 AND status = 'running'",
            params![
                job_id,
                serde_json::to_string(result).map_err(|e| e.to_string())?,
                serde_json::to_string(usage).map_err(|e| e.to_string())?,
                note_rel_path,
                timestamp,
            ],
        )
        .map_err(db_err)?;
        conn.execute(
            "UPDATE production_steps SET status = 'done' WHERE job_id = ?1 AND status IN ('pending','running')",
            params![job_id],
        )
        .map_err(db_err)?;
        Ok(())
    }

    pub fn fail(&self, job_id: &str, error: &str) -> Result<(), String> {
        let conn = self.lock()?;
        let current: String = conn
            .query_row("SELECT status FROM production_jobs WHERE id = ?1", params![job_id], |row| row.get(0))
            .optional()
            .map_err(db_err)?
            .unwrap_or_default();
        if current == "cancelled" {
            return Ok(());
        }
        conn.execute(
            "UPDATE production_jobs SET status = 'error', error = ?2, updated_at = ?3, finished_at = ?3 WHERE id = ?1",
            params![job_id, truncate(error, 2000), now_ms()],
        )
        .map_err(db_err)?;
        conn.execute(
            "UPDATE production_steps SET status = 'error', detail = ?2 WHERE job_id = ?1 AND status = 'running'",
            params![job_id, truncate(error, 300)],
        )
        .map_err(db_err)?;
        Ok(())
    }

    pub fn request_cancel(&self, id: &str) -> Result<(), String> {
        let conn = self.lock()?;
        conn.execute(
            "UPDATE production_jobs SET status = 'cancelled', error = '用户取消', updated_at = ?2, finished_at = ?2
             WHERE id = ?1 AND status IN ('queued','running')",
            params![id, now_ms()],
        )
        .map_err(db_err)?;
        conn.execute(
            "UPDATE production_steps SET status = 'cancelled' WHERE job_id = ?1 AND status IN ('pending','running')",
            params![id],
        )
        .map_err(db_err)?;
        Ok(())
    }

    pub fn reset_for_retry(&self, root: &Path, id: &str) -> Result<Job, String> {
        let settings = self.settings();
        let mut conn = self.lock()?;
        let tx = conn.transaction().map_err(db_err)?;
        let timestamp = now_ms();
        let changed = tx.execute(
            "UPDATE production_jobs SET status = 'queued', progress = 0, current_step = '',
                    error = '', result_json = '', usage_json = '{}', started_at = 0,
                    finished_at = 0, updated_at = ?2, provider = ?3, model = ?4,
                    config_json = '', checkpoint_at = 0,
                    vault_path = CASE WHEN vault_path = '' THEN ?5 ELSE vault_path END
             WHERE id = ?1 AND status IN ('error','cancelled','interrupted')",
            params![id, timestamp, settings.ai_provider, settings.ai_model, root.to_string_lossy().to_string()],
        )
        .map_err(db_err)?;
        if changed == 0 {
            return Err("任务当前状态不能重跑（可能正在运行或已完成）".into());
        }
        tx.execute(
            "UPDATE production_steps SET status = 'pending', detail = '' WHERE job_id = ?1",
            params![id],
        )
        .map_err(db_err)?;
        tx.commit().map_err(db_err)?;
        drop(conn);
        self.get_job(id)?.ok_or_else(|| "任务不存在".into())
    }

    pub fn delete_job(&self, id: &str) -> Result<(), String> {
        let conn = self.lock()?;
        conn.execute("DELETE FROM production_checkpoints WHERE job_id = ?1", params![id]).map_err(db_err)?;
        conn.execute("DELETE FROM production_steps WHERE job_id = ?1", params![id]).map_err(db_err)?;
        conn.execute("DELETE FROM production_logs WHERE job_id = ?1", params![id]).map_err(db_err)?;
        conn.execute("DELETE FROM production_jobs WHERE id = ?1", params![id]).map_err(db_err)?;
        Ok(())
    }

    pub fn job_ids_with_status(&self, statuses: &[&str], vault: Option<&str>) -> Result<Vec<String>, String> {
        if statuses.is_empty() {
            return Ok(Vec::new());
        }
        let placeholders = vec!["?"; statuses.len()].join(",");
        let vault_clause = if vault.is_some() { " AND vault_path = ?" } else { "" };
        let sql = format!("SELECT id FROM production_jobs WHERE status IN ({placeholders}){vault_clause} ORDER BY created_at");
        let conn = self.lock()?;
        let mut statement = conn.prepare(&sql).map_err(db_err)?;
        let mut bind: Vec<&str> = statuses.to_vec();
        if let Some(vault) = vault { bind.push(vault); }
        let params = rusqlite::params_from_iter(bind);
        let ids = statement
            .query_map(params, |row| row.get::<_, String>(0))
            .map_err(db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_err)?;
        Ok(ids)
    }

    pub fn delete_jobs(&self, ids: &[String]) -> Result<usize, String> {
        let mut conn = self.lock()?;
        let tx = conn.transaction().map_err(db_err)?;
        for id in ids {
            tx.execute("DELETE FROM production_checkpoints WHERE job_id = ?1", params![id]).map_err(db_err)?;
            tx.execute("DELETE FROM production_steps WHERE job_id = ?1", params![id]).map_err(db_err)?;
            tx.execute("DELETE FROM production_logs WHERE job_id = ?1", params![id]).map_err(db_err)?;
            tx.execute("DELETE FROM production_jobs WHERE id = ?1", params![id]).map_err(db_err)?;
        }
        tx.commit().map_err(db_err)?;
        Ok(ids.len())
    }

    pub fn clear_finished(&self, vault: &str) -> Result<usize, String> {
        // Only successful jobs are "finished". Failed/interrupted jobs may
        // still hold checkpoints and must be cleared explicitly.
        let ids = self.job_ids_with_status(&["success"], Some(vault))?;
        self.delete_jobs(&ids)
    }

    pub fn protected_temp_jobs(&self) -> Result<Vec<String>, String> {
        let conn = self.lock()?;
        let mut statement = conn
            .prepare(
                "SELECT DISTINCT job_id FROM production_checkpoints
                 UNION
                 SELECT id FROM production_jobs WHERE status IN ('queued','running')",
            )
            .map_err(db_err)?;
        let ids = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_err)?;
        Ok(ids)
    }

    pub fn recover_interrupted(&self) -> Result<Vec<Job>, String> {
        let ids = {
            let conn = self.lock()?;
            let mut statement = conn
                .prepare("SELECT id FROM production_jobs WHERE status IN ('running','queued')")
                .map_err(db_err)?;
            let ids = statement
                .query_map([], |row| row.get::<_, String>(0))
                .map_err(db_err)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(db_err)?;
            conn.execute(
                "UPDATE production_jobs SET status = 'interrupted', error = '应用在制作过程中退出',
                        updated_at = ?1, finished_at = ?1 WHERE status IN ('running','queued')",
                params![now_ms()],
            )
            .map_err(db_err)?;
            conn.execute(
                "UPDATE production_steps SET status = 'cancelled' WHERE status IN ('pending','running')",
                [],
            )
            .map_err(db_err)?;
            ids
        };
        let mut jobs = Vec::new();
        for id in ids {
            if let Some(job) = self.get_job(&id)? {
                jobs.push(job);
            }
        }
        Ok(jobs)
    }

    pub fn logs(&self, id: &str) -> Result<Vec<LogEntry>, String> {
        let conn = self.lock()?;
        let mut statement = conn
            .prepare(
                "SELECT id, step_index, level, message, created_at FROM production_logs
                 WHERE job_id = ?1 ORDER BY id DESC LIMIT 500",
            )
            .map_err(db_err)?;
        let mut rows = statement
            .query_map(params![id], |row| {
                Ok(LogEntry {
                    id: row.get(0)?,
                    step_index: row.get::<_, Option<i64>>(1)?.map(|value| value as usize),
                    level: row.get(2)?,
                    message: row.get(3)?,
                    created_at: row.get(4)?,
                })
            })
            .map_err(db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_err)?;
        rows.reverse();
        Ok(rows)
    }

    pub fn log(&self, job_id: &str, step_index: Option<usize>, level: &str, message: &str) -> Result<(), String> {
        let conn = self.lock()?;
        conn.execute(
            "INSERT INTO production_logs (job_id, step_index, level, message, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![job_id, step_index.map(|value| value as i64), level, truncate(message, 8000), now_ms()],
        )
        .map_err(db_err)?;
        Ok(())
    }

    pub fn list_prompts(&self) -> Result<Vec<PromptEntry>, String> {
        let conn = self.lock()?;
        let mut statement = conn
            .prepare("SELECT id, name, description, system, created_at, updated_at FROM production_prompts ORDER BY updated_at DESC")
            .map_err(db_err)?;
        let rows = statement
            .query_map([], |row| {
                Ok(PromptEntry {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    description: row.get(2)?,
                    system: row.get(3)?,
                    builtin: false,
                    created_at: row.get(4)?,
                    updated_at: row.get(5)?,
                })
            })
            .map_err(db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_err)?;
        Ok(rows)
    }

    pub fn list_model_prices(&self, provider: Option<&str>) -> Result<Vec<ModelPrice>, String> {
        let conn = self.lock()?;
        let mut statement = conn
            .prepare(
                "SELECT provider, model, input_cny, output_cny, updated_at
                 FROM production_model_prices
                 WHERE (?1 IS NULL OR provider = ?1)
                 ORDER BY provider, model",
            )
            .map_err(db_err)?;
        let rows = statement
            .query_map(params![provider], |row| {
                Ok(ModelPrice {
                    provider: row.get(0)?,
                    model: row.get(1)?,
                    input_cny: row.get(2)?,
                    output_cny: row.get(3)?,
                    updated_at: row.get(4)?,
                })
            })
            .map_err(db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_err)?;
        Ok(rows)
    }

    pub fn model_price(&self, provider: &str, model: &str) -> Result<Option<ModelPrice>, String> {
        let conn = self.lock()?;
        conn.query_row(
            "SELECT provider, model, input_cny, output_cny, updated_at
             FROM production_model_prices WHERE provider = ?1 AND model = ?2",
            params![provider, model],
            |row| {
                Ok(ModelPrice {
                    provider: row.get(0)?,
                    model: row.get(1)?,
                    input_cny: row.get(2)?,
                    output_cny: row.get(3)?,
                    updated_at: row.get(4)?,
                })
            },
        )
        .optional()
        .map_err(db_err)
    }

    pub fn set_model_price(
        &self,
        provider: &str,
        model: &str,
        input_cny: f64,
        output_cny: f64,
    ) -> Result<ModelPrice, String> {
        let provider = provider.trim();
        let model = model.trim();
        if provider.is_empty() || model.is_empty() {
            return Err("服务商和模型 ID 不能为空".into());
        }
        if !input_cny.is_finite() || !output_cny.is_finite() || input_cny < 0.0 || output_cny < 0.0 {
            return Err("手动价目必须是 0 或正数".into());
        }
        let updated_at = now_ms();
        let conn = self.lock()?;
        conn.execute(
            "INSERT INTO production_model_prices (provider, model, input_cny, output_cny, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(provider, model) DO UPDATE SET
               input_cny = excluded.input_cny,
               output_cny = excluded.output_cny,
               updated_at = excluded.updated_at",
            params![provider, model, input_cny, output_cny, updated_at],
        )
        .map_err(db_err)?;
        Ok(ModelPrice {
            provider: provider.into(),
            model: model.into(),
            input_cny,
            output_cny,
            updated_at,
        })
    }

    pub fn delete_model_price(&self, provider: &str, model: &str) -> Result<(), String> {
        let conn = self.lock()?;
        conn.execute(
            "DELETE FROM production_model_prices WHERE provider = ?1 AND model = ?2",
            params![provider, model],
        )
        .map_err(db_err)?;
        Ok(())
    }

    pub fn job_config(&self, job_id: &str) -> Result<Option<JobConfig>, String> {
        let conn = self.lock()?;
        let raw: Option<String> = conn
            .query_row("SELECT config_json FROM production_jobs WHERE id = ?1", params![job_id], |row| row.get(0))
            .optional()
            .map_err(db_err)?;
        let Some(raw) = raw.filter(|value| !value.trim().is_empty()) else { return Ok(None) };
        serde_json::from_str(&raw).map(Some).map_err(|error| format!("任务配置无法解析：{error}"))
    }

    pub fn set_job_config(&self, job_id: &str, config: &JobConfig) -> Result<(), String> {
        let conn = self.lock()?;
        conn.execute(
            "UPDATE production_jobs SET config_json = ?2, updated_at = ?3 WHERE id = ?1",
            params![job_id, serde_json::to_string(config).map_err(|e| e.to_string())?, now_ms()],
        )
        .map_err(db_err)?;
        Ok(())
    }

    pub fn checkpoint(&self, job_id: &str, unit_key: &str) -> Result<Option<CheckpointRow>, String> {
        let conn = self.lock()?;
        conn.query_row(
            "SELECT job_id, unit_key, step_key, kind, input_hash, payload_json, usage_json,
                    payload_bytes, created_at, updated_at
             FROM production_checkpoints WHERE job_id = ?1 AND unit_key = ?2",
            params![job_id, unit_key],
            row_to_checkpoint,
        )
        .optional()
        .map_err(db_err)
    }

    pub fn save_checkpoint(
        &self,
        job_id: &str,
        unit_key: &str,
        step_key: &str,
        kind: &str,
        input_hash: &str,
        payload_json: &str,
        usage_json: &str,
        payload_bytes: i64,
    ) -> Result<(), String> {
        if payload_json.len() > 1024 * 1024 {
            return Err(format!("检查点 {unit_key} 超过 1MB，拒绝写入"));
        }
        let timestamp = now_ms();
        let mut conn = self.lock()?;
        let tx = conn.transaction().map_err(db_err)?;
        tx.execute(
            "INSERT INTO production_checkpoints
              (job_id, unit_key, step_key, kind, input_hash, payload_json, usage_json,
               payload_bytes, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9)
             ON CONFLICT(job_id, unit_key) DO UPDATE SET
               step_key = excluded.step_key,
               kind = excluded.kind,
               input_hash = excluded.input_hash,
               payload_json = excluded.payload_json,
               usage_json = excluded.usage_json,
               payload_bytes = excluded.payload_bytes,
               updated_at = excluded.updated_at",
            params![job_id, unit_key, step_key, kind, input_hash, payload_json, usage_json, payload_bytes, timestamp],
        )
        .map_err(db_err)?;
        tx.execute(
            "UPDATE production_jobs SET checkpoint_at = ?2, updated_at = ?2 WHERE id = ?1",
            params![job_id, timestamp],
        )
        .map_err(db_err)?;
        tx.commit().map_err(db_err)?;
        Ok(())
    }

    pub fn list_checkpoints(&self, job_id: &str) -> Result<Vec<CheckpointRow>, String> {
        let conn = self.lock()?;
        let mut statement = conn
            .prepare(
                "SELECT job_id, unit_key, step_key, kind, input_hash, payload_json, usage_json,
                        payload_bytes, created_at, updated_at
                 FROM production_checkpoints WHERE job_id = ?1 ORDER BY step_key, created_at, unit_key",
            )
            .map_err(db_err)?;
        let rows = statement
            .query_map(params![job_id], row_to_checkpoint)
            .map_err(db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_err)?;
        Ok(rows)
    }

    pub fn clear_checkpoints(&self, job_id: &str) -> Result<(), String> {
        let conn = self.lock()?;
        conn.execute("DELETE FROM production_checkpoints WHERE job_id = ?1", params![job_id])
            .map_err(db_err)?;
        conn.execute("UPDATE production_jobs SET checkpoint_at = 0 WHERE id = ?1", params![job_id])
            .map_err(db_err)?;
        Ok(())
    }

    pub fn jobs_with_checkpoints(&self) -> Result<Vec<String>, String> {
        let conn = self.lock()?;
        let mut statement = conn
            .prepare("SELECT DISTINCT job_id FROM production_checkpoints")
            .map_err(db_err)?;
        let ids = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_err)?;
        Ok(ids)
    }

    pub fn set_usage(&self, job_id: &str, usage: &Usage) -> Result<(), String> {
        let conn = self.lock()?;
        conn.execute(
            "UPDATE production_jobs SET usage_json = ?2, updated_at = ?3 WHERE id = ?1",
            params![job_id, serde_json::to_string(usage).map_err(|e| e.to_string())?, now_ms()],
        )
        .map_err(db_err)?;
        Ok(())
    }

    pub fn prepare_resume(&self, id: &str) -> Result<Job, String> {
        let timestamp = now_ms();
        let mut conn = self.lock()?;
        let changed = conn
            .execute(
                "UPDATE production_jobs SET status = 'queued', progress = 0, current_step = '',
                        error = '', result_json = '', usage_json = '{}', started_at = 0,
                        finished_at = 0, updated_at = ?2
                 WHERE id = ?1 AND status IN ('error','cancelled','interrupted')",
                params![id, timestamp],
            )
            .map_err(db_err)?;
        if changed == 0 {
            return Err("任务当前状态不能继续（可能已经在运行或已完成）".into());
        }
        conn.execute(
            "UPDATE production_steps SET status = 'pending', detail = ''
             WHERE job_id = ?1 AND status NOT IN ('done','skipped')",
            params![id],
        )
        .map_err(db_err)?;
        drop(conn);
        self.get_job(id)?.ok_or_else(|| "任务不存在".into())
    }

    pub fn save_prompt(&self, entry: &PromptEntry) -> Result<PromptEntry, String> {
        let id = if entry.id.trim().is_empty() {
            format!("custom_{}", uuid::Uuid::new_v4().simple())
        } else {
            entry.id.trim().to_string()
        };
        let timestamp = now_ms();
        let name = entry.name.trim().to_string();
        let description = entry.description.trim().to_string();
        let mut conn = self.lock()?;
        let tx = conn.transaction().map_err(db_err)?;
        let existing: Option<(String, String, String, i64)> = tx
            .query_row(
                "SELECT name, description, system, created_at FROM production_prompts WHERE id = ?1",
                params![id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()
            .map_err(db_err)?;
        if let Some((old_name, old_description, old_system, old_created)) = existing {
            if old_name == name && old_description == description && old_system == entry.system {
                return Ok(PromptEntry {
                    id,
                    name,
                    description,
                    system: entry.system.clone(),
                    builtin: false,
                    created_at: old_created,
                    updated_at: timestamp,
                });
            }
            let version_count: i64 = tx
                .query_row("SELECT COUNT(*) FROM production_prompt_versions WHERE prompt_id = ?1", params![id], |row| row.get(0))
                .map_err(db_err)?;
            if version_count == 0 {
                tx.execute(
                    "INSERT INTO production_prompt_versions (prompt_id, version, name, description, system, created_at)
                     VALUES (?1, 1, ?2, ?3, ?4, ?5)",
                    params![id, old_name, old_description, old_system, old_created],
                )
                .map_err(db_err)?;
            }
            let next_version: i64 = tx
                .query_row("SELECT COALESCE(MAX(version), 0) + 1 FROM production_prompt_versions WHERE prompt_id = ?1", params![id], |row| row.get(0))
                .map_err(db_err)?;
            tx.execute(
                "UPDATE production_prompts SET name = ?2, description = ?3, system = ?4, updated_at = ?5 WHERE id = ?1",
                params![id, name, description, entry.system, timestamp],
            )
            .map_err(db_err)?;
            tx.execute(
                "INSERT INTO production_prompt_versions (prompt_id, version, name, description, system, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![id, next_version, name, description, entry.system, timestamp],
            )
            .map_err(db_err)?;
            tx.commit().map_err(db_err)?;
            Ok(PromptEntry { id, name, description, system: entry.system.clone(), builtin: false, created_at: old_created, updated_at: timestamp })
        } else {
            tx.execute(
                "INSERT INTO production_prompts (id, name, description, system, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
                params![id, name, description, entry.system, timestamp],
            )
            .map_err(db_err)?;
            tx.execute(
                "INSERT INTO production_prompt_versions (prompt_id, version, name, description, system, created_at)
                 VALUES (?1, 1, ?2, ?3, ?4, ?5)",
                params![id, name, description, entry.system, timestamp],
            )
            .map_err(db_err)?;
            tx.commit().map_err(db_err)?;
            Ok(PromptEntry { id, name, description, system: entry.system.clone(), builtin: false, created_at: timestamp, updated_at: timestamp })
        }
    }

    pub fn list_prompt_versions(&self, prompt_id: &str) -> Result<Vec<PromptVersion>, String> {
        let conn = self.lock()?;
        let mut statement = conn
            .prepare(
                "SELECT version, name, description, system, created_at FROM production_prompt_versions
                 WHERE prompt_id = ?1 ORDER BY version DESC",
            )
            .map_err(db_err)?;
        let rows = statement
            .query_map(params![prompt_id], |row| {
                let system: String = row.get(3)?;
                Ok(PromptVersion {
                    version: row.get(0)?,
                    name: row.get(1)?,
                    description: row.get(2)?,
                    system,
                    chars: 0,
                    created_at: row.get(4)?,
                })
            })
            .map_err(db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_err)?;
        Ok(rows.into_iter().map(|mut item| {
            item.chars = item.system.chars().count();
            item.system.clear();
            item
        }).collect())
    }

    pub fn prompt_version(&self, prompt_id: &str, version: i64) -> Result<PromptVersion, String> {
        let conn = self.lock()?;
        let (name, description, system, created_at): (String, String, String, i64) = conn
            .query_row(
                "SELECT name, description, system, created_at FROM production_prompt_versions WHERE prompt_id = ?1 AND version = ?2",
                params![prompt_id, version],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()
            .map_err(db_err)?
            .ok_or_else(|| "提示词版本不存在".to_string())?;
        let chars = system.chars().count();
        Ok(PromptVersion { version, name, description, system, chars, created_at })
    }

    pub fn restore_prompt_version(&self, prompt_id: &str, version: i64) -> Result<PromptEntry, String> {
        let saved = self.prompt_version(prompt_id, version)?;
        let timestamp = now_ms();
        let mut conn = self.lock()?;
        let tx = conn.transaction().map_err(db_err)?;
        let created_at: i64 = tx
            .query_row("SELECT created_at FROM production_prompts WHERE id = ?1", params![prompt_id], |row| row.get(0))
            .optional()
            .map_err(db_err)?
            .ok_or_else(|| "提示词不存在".to_string())?;
        let next_version: i64 = tx
            .query_row("SELECT COALESCE(MAX(version), 0) + 1 FROM production_prompt_versions WHERE prompt_id = ?1", params![prompt_id], |row| row.get(0))
            .map_err(db_err)?;
        tx.execute(
            "UPDATE production_prompts SET name = ?2, description = ?3, system = ?4, updated_at = ?5 WHERE id = ?1",
            params![prompt_id, saved.name, saved.description, saved.system, timestamp],
        )
        .map_err(db_err)?;
        tx.execute(
            "INSERT INTO production_prompt_versions (prompt_id, version, name, description, system, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![prompt_id, next_version, saved.name, saved.description, saved.system, timestamp],
        )
        .map_err(db_err)?;
        tx.commit().map_err(db_err)?;
        Ok(PromptEntry { id: prompt_id.into(), name: saved.name, description: saved.description, system: saved.system, builtin: false, created_at, updated_at: timestamp })
    }

    pub fn delete_prompt(&self, id: &str) -> Result<(), String> {
        let mut conn = self.lock()?;
        let tx = conn.transaction().map_err(db_err)?;
        tx.execute("DELETE FROM production_prompt_versions WHERE prompt_id = ?1", params![id]).map_err(db_err)?;
        tx.execute("DELETE FROM production_prompts WHERE id = ?1", params![id]).map_err(db_err)?;
        tx.commit().map_err(db_err)?;
        Ok(())
    }

    pub fn usage_summary(&self) -> Result<serde_json::Value, String> {
        let now = Local::now();
        let month_start = Local
            .with_ymd_and_hms(now.year(), now.month(), 1, 0, 0, 0)
            .single()
            .map(|date| date.timestamp_millis())
            .unwrap_or(0);
        let jobs = self.list_jobs_since(month_start)?;
        let budget = self.setting_number("production.monthly_budget_cny", 0.0);
        let mut spend = 0.0;
        let mut tokens = 0u64;
        let mut priced_jobs = 0usize;
        let mut count = 0usize;
        let mut by_model: HashMap<String, (f64, u64, usize)> = HashMap::new();
        for job in jobs {
            count += 1;
            let job_tokens = job.usage.prompt_tokens + job.usage.completion_tokens;
            tokens += job_tokens;
            if job.usage.priced {
                priced_jobs += 1;
            }
            spend += job.usage.estimated_cost;
            if job_tokens == 0 && job.usage.estimated_cost <= 0.0 && job.usage.provider_cost <= 0.0 {
                continue;
            }
            let key = if job.model.is_empty() { "未知模型".to_string() } else { job.model.clone() };
            let entry = by_model.entry(key).or_insert((0.0, 0, 0));
            entry.0 += job.usage.estimated_cost;
            entry.1 += job_tokens;
            entry.2 += 1;
        }
        let mut models = by_model
            .into_iter()
            .map(|(model, (spend, tokens, jobs))| serde_json::json!({
                "model": model,
                "spendCny": round_money(spend),
                "tokens": tokens,
                "jobs": jobs
            }))
            .collect::<Vec<_>>();
        models.sort_by(|a, b| {
            b.get("spendCny").and_then(serde_json::Value::as_f64).unwrap_or(0.0)
                .partial_cmp(&a.get("spendCny").and_then(serde_json::Value::as_f64).unwrap_or(0.0))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        Ok(serde_json::json!({
            "month": now.format("%Y-%m").to_string(),
            "spendCny": round_money(spend),
            "budgetCny": round_money(budget),
            "percent": if budget > 0.0 { ((spend / budget) * 100.0).min(999.0) } else { 0.0 },
            "jobs": count,
            "pricedJobs": priced_jobs,
            "tokens": tokens,
            "byModel": models
        }))
    }

    pub fn list_subscriptions(&self, vault: &str) -> Result<Vec<Subscription>, String> {
        let conn = self.lock()?;
        let mut statement = conn.prepare(&subscription_select(
            "WHERE s.vault_path = ?1 ORDER BY s.created_at DESC",
        )).map_err(db_err)?;
        let rows = statement
            .query_map(params![vault], subscription_from_row)
            .map_err(db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_err)?;
        Ok(rows)
    }

    pub fn get_subscription(&self, id: &str) -> Result<Option<Subscription>, String> {
        let conn = self.lock()?;
        conn.query_row(
            &subscription_select("WHERE s.id = ?1"),
            params![id],
            subscription_from_row,
        )
        .optional()
        .map_err(db_err)
    }

    pub fn subscription_by_mid(&self, vault: &str, mid: i64) -> Result<Option<Subscription>, String> {
        let conn = self.lock()?;
        conn.query_row(
            &subscription_select("WHERE s.vault_path = ?1 AND s.mid = ?2"),
            params![vault, mid],
            subscription_from_row,
        )
        .optional()
        .map_err(db_err)
    }

    pub fn save_subscription(&self, vault: &str, input: &SubscriptionInput) -> Result<Subscription, String> {
        if input.mid <= 0 {
            return Err("没有识别到 UP 主 UID".into());
        }
        let now = now_ms();
        let id = if input.id.trim().is_empty() {
            format!("sub_{}", uuid::Uuid::new_v4().simple())
        } else {
            input.id.trim().to_string()
        };
        let filters_json = serde_json::to_string(&input.filters).map_err(|error| error.to_string())?;
        let interval = input.interval_minutes.clamp(10, 24 * 60) as i64;
        let existing_by_mid: Option<String> = {
            let conn = self.lock()?;
            conn.query_row(
                "SELECT id FROM production_subscriptions WHERE vault_path = ?1 AND mid = ?2",
                params![vault, input.mid],
                |row| row.get(0),
            )
            .optional()
            .map_err(db_err)?
        };
        if let Some(existing) = existing_by_mid {
            if existing != id {
                return Err("这个 UP 已经在订阅列表里".into());
            }
        }
        let existing: Option<(i64, i64, i64)> = {
            let conn = self.lock()?;
            conn.query_row(
                "SELECT baseline_at, next_check_at, enabled FROM production_subscriptions
                 WHERE vault_path = ?1 AND id = ?2",
                params![vault, id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()
            .map_err(db_err)?
        };
        let conn = self.lock()?;
        if let Some((baseline_at, next_check, was_enabled)) = existing {
            let next_check = if input.enabled && (was_enabled == 0 || next_check == 0) {
                now
            } else {
                next_check
            };
            conn.execute(
                "UPDATE production_subscriptions
                 SET mid = ?3, name = ?4, avatar = ?5, enabled = ?6, interval_minutes = ?7,
                     template_id = ?8, prompt = ?9, filters_json = ?10, baseline_at = ?11,
                     next_check_at = ?12, updated_at = ?13
                 WHERE vault_path = ?1 AND id = ?2",
                params![
                    vault,
                    id,
                    input.mid,
                    input.name.trim(),
                    input.avatar.trim(),
                    if input.enabled { 1 } else { 0 },
                    interval,
                    input.template_id.trim(),
                    input.prompt.trim(),
                    filters_json,
                    baseline_at,
                    next_check,
                    now,
                ],
            )
            .map_err(db_err)?;
        } else {
            conn.execute(
                "INSERT INTO production_subscriptions
                  (id, vault_path, mid, name, avatar, enabled, interval_minutes, template_id,
                   prompt, filters_json, baseline_at, last_checked_at, next_check_at,
                   last_error, consecutive_failures, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 0, 0, ?11, '', 0, ?12, ?12)",
                params![
                    id,
                    vault,
                    input.mid,
                    input.name.trim(),
                    input.avatar.trim(),
                    if input.enabled { 1 } else { 0 },
                    interval,
                    input.template_id.trim(),
                    input.prompt.trim(),
                    filters_json,
                    now,
                    now,
                ],
            )
            .map_err(db_err)?;
        }
        drop(conn);
        self.get_subscription(&id)?
            .ok_or_else(|| "订阅保存后读取失败".to_string())
    }

    pub fn set_subscription_enabled(&self, vault: &str, id: &str, enabled: bool) -> Result<(), String> {
        let conn = self.lock()?;
        let changed = conn
            .execute(
                "UPDATE production_subscriptions
                 SET enabled = ?3, next_check_at = CASE WHEN ?3 = 1 AND enabled = 0 THEN ?4 ELSE next_check_at END,
                     last_error = CASE WHEN ?3 = 1 THEN '' ELSE last_error END,
                     updated_at = ?4
                 WHERE vault_path = ?1 AND id = ?2",
                params![vault, id, if enabled { 1 } else { 0 }, now_ms()],
            )
            .map_err(db_err)?;
        if changed == 0 {
            return Err("订阅不存在".into());
        }
        Ok(())
    }

    pub fn delete_subscription(&self, vault: &str, id: &str) -> Result<(), String> {
        let mut conn = self.lock()?;
        let tx = conn.transaction().map_err(db_err)?;
        let mid: Option<i64> = tx
            .query_row(
                "SELECT mid FROM production_subscriptions WHERE vault_path = ?1 AND id = ?2",
                params![vault, id],
                |row| row.get(0),
            )
            .optional()
            .map_err(db_err)?;
        let Some(mid) = mid else {
            return Err("订阅不存在".into());
        };
        tx.execute(
            "DELETE FROM production_seen_videos WHERE vault_path = ?1 AND mid = ?2",
            params![vault, mid],
        )
        .map_err(db_err)?;
        tx.execute(
            "DELETE FROM production_subscriptions WHERE vault_path = ?1 AND id = ?2",
            params![vault, id],
        )
        .map_err(db_err)?;
        tx.commit().map_err(db_err)?;
        Ok(())
    }

    pub fn due_subscriptions(&self, vault: &str, now: i64) -> Result<Vec<Subscription>, String> {
        let conn = self.lock()?;
        let mut statement = conn.prepare(&subscription_select(
            "WHERE s.vault_path = ?1 AND s.enabled = 1 AND s.next_check_at <= ?2
             ORDER BY s.next_check_at ASC LIMIT 8",
        )).map_err(db_err)?;
        let rows = statement
            .query_map(params![vault, now], subscription_from_row)
            .map_err(db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_err)?;
        Ok(rows)
    }

    pub fn record_subscription_check(
        &self,
        id: &str,
        checked_at: i64,
        next_check_at: i64,
        error: &str,
        message: &str,
        failures: u32,
    ) -> Result<(), String> {
        let conn = self.lock()?;
        conn.execute(
            "UPDATE production_subscriptions
             SET last_checked_at = ?2, next_check_at = ?3, last_error = ?4,
                 last_message = ?5, consecutive_failures = ?6, updated_at = ?2
             WHERE id = ?1",
            params![
                id,
                checked_at,
                next_check_at,
                truncate(error, 500),
                truncate(message, 500),
                failures
            ],
        )
        .map_err(db_err)?;
        Ok(())
    }

    pub fn set_subscription_baseline(&self, id: &str, baseline_at: i64) -> Result<(), String> {
        let conn = self.lock()?;
        conn.execute(
            "UPDATE production_subscriptions SET baseline_at = ?2, updated_at = ?3 WHERE id = ?1",
            params![id, baseline_at, now_ms()],
        )
        .map_err(db_err)?;
        Ok(())
    }

    pub fn seen_exists(&self, vault: &str, mid: i64, bvid: &str) -> Result<bool, String> {
        let conn = self.lock()?;
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM production_seen_videos
                 WHERE vault_path = ?1 AND mid = ?2 AND bvid = ?3",
                params![vault, mid, bvid],
                |row| row.get(0),
            )
            .map_err(db_err)?;
        Ok(count > 0)
    }

    pub fn insert_seen(
        &self,
        vault: &str,
        mid: i64,
        video: &super::BiliVideoRef,
        status: &str,
        job_id: &str,
        note_id: &str,
    ) -> Result<bool, String> {
        let now = now_ms();
        let conn = self.lock()?;
        let changed = conn
            .execute(
                "INSERT OR IGNORE INTO production_seen_videos
                  (vault_path, mid, bvid, title, pubdate, duration, cover, status, job_id,
                   note_id, first_seen_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?11)",
                params![
                    vault,
                    mid,
                    video.bvid,
                    video.title,
                    video.pubdate,
                    video.duration,
                    video.cover,
                    status,
                    job_id,
                    note_id,
                    now,
                ],
            )
            .map_err(db_err)?;
        Ok(changed > 0)
    }

    pub fn mark_seen(
        &self,
        vault: &str,
        mid: i64,
        bvid: &str,
        status: &str,
        job_id: &str,
        note_id: &str,
    ) -> Result<(), String> {
        let conn = self.lock()?;
        conn.execute(
            "UPDATE production_seen_videos
             SET status = ?4, job_id = ?5, note_id = ?6, updated_at = ?7
             WHERE vault_path = ?1 AND mid = ?2 AND bvid = ?3",
            params![vault, mid, bvid, status, job_id, note_id, now_ms()],
        )
        .map_err(db_err)?;
        Ok(())
    }

    pub fn recent_seen(&self, vault: &str, mid: i64, limit: usize) -> Result<Vec<SeenVideo>, String> {
        let conn = self.lock()?;
        let mut statement = conn
            .prepare(
                "SELECT bvid, title, pubdate, duration, cover, status, job_id, note_id,
                        first_seen_at, updated_at
                 FROM production_seen_videos
                 WHERE vault_path = ?1 AND mid = ?2
                 ORDER BY pubdate DESC, first_seen_at DESC LIMIT ?3",
            )
            .map_err(db_err)?;
        let rows = statement
            .query_map(params![vault, mid, limit as i64], |row| {
                Ok(SeenVideo {
                    bvid: row.get(0)?,
                    title: row.get(1)?,
                    pubdate: row.get(2)?,
                    duration: row.get(3)?,
                    cover: row.get(4)?,
                    status: row.get(5)?,
                    job_id: row.get(6)?,
                    note_id: row.get(7)?,
                    first_seen_at: row.get(8)?,
                    updated_at: row.get(9)?,
                })
            })
            .map_err(db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_err)?;
        Ok(rows)
    }

    pub fn queued_auto_today(&self, vault: &str) -> Result<u32, String> {
        let start = Local::now()
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .and_then(|date| Local.from_local_datetime(&date).single())
            .map(|date| date.timestamp_millis())
            .unwrap_or(0);
        let conn = self.lock()?;
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM production_seen_videos
                 WHERE vault_path = ?1 AND status = 'queued' AND first_seen_at >= ?2",
                params![vault, start],
                |row| row.get(0),
            )
            .map_err(db_err)?;
        Ok(count as u32)
    }

    pub fn wake_subscriptions_after_login(&self) -> Result<usize, String> {
        let conn = self.lock()?;
        let changed = conn
            .execute(
                "UPDATE production_subscriptions
                 SET next_check_at = 0, last_error = '', updated_at = ?1
                 WHERE enabled = 1 AND (last_error LIKE '%登录%' OR last_error LIKE '%扫码%')",
                params![now_ms()],
            )
            .map_err(db_err)?;
        Ok(changed)
    }

    pub fn data_usage(&self, root: &Path) -> Result<serde_json::Value, String> {
        let conn = self.lock()?;
        let total: i64 = conn
            .query_row("SELECT COUNT(*) FROM production_jobs", [], |row| row.get(0))
            .map_err(db_err)?;
        let finished: i64 = conn
            .query_row("SELECT COUNT(*) FROM production_jobs WHERE status = 'success'", [], |row| row.get(0))
            .map_err(db_err)?;
        drop(conn);
        let temp = dir_bytes(&root.join(".yebian").join("production-tmp"));
        let outbox = dir_bytes(&root.join(".yebian").join("outbox"));
        let checkpoint_bytes: i64 = {
            let conn = self.lock()?;
            conn.query_row("SELECT COALESCE(SUM(payload_bytes), 0) FROM production_checkpoints", [], |row| row.get(0))
                .map_err(db_err)?
        };
        Ok(serde_json::json!({
            "jobs": total,
            "finished": finished,
            "tempBytes": temp,
            "outboxBytes": outbox,
            "checkpointBytes": checkpoint_bytes,
        }))
    }
}

fn subscription_select(where_clause: &str) -> String {
    format!(
        "SELECT s.id, s.vault_path, s.mid, s.name, s.avatar, s.enabled, s.interval_minutes,
                s.template_id, s.prompt, s.filters_json, s.baseline_at, s.last_checked_at,
                s.next_check_at, s.last_error, s.last_message, s.consecutive_failures,
                s.created_at, s.updated_at,
                COALESCE((SELECT v.title FROM production_seen_videos v
                          WHERE v.vault_path = s.vault_path AND v.mid = s.mid
                          ORDER BY v.pubdate DESC, v.first_seen_at DESC LIMIT 1), ''),
                COALESCE((SELECT v.bvid FROM production_seen_videos v
                          WHERE v.vault_path = s.vault_path AND v.mid = s.mid
                          ORDER BY v.pubdate DESC, v.first_seen_at DESC LIMIT 1), ''),
                COALESCE((SELECT v.pubdate FROM production_seen_videos v
                          WHERE v.vault_path = s.vault_path AND v.mid = s.mid
                          ORDER BY v.pubdate DESC, v.first_seen_at DESC LIMIT 1), 0),
                COALESCE((SELECT v.cover FROM production_seen_videos v
                          WHERE v.vault_path = s.vault_path AND v.mid = s.mid
                          ORDER BY v.pubdate DESC, v.first_seen_at DESC LIMIT 1), ''),
                (SELECT COUNT(*) FROM production_seen_videos v
                 WHERE v.vault_path = s.vault_path AND v.mid = s.mid AND v.status = 'queued'),
                (SELECT COUNT(*) FROM production_seen_videos v
                 WHERE v.vault_path = s.vault_path AND v.mid = s.mid)
         FROM production_subscriptions s {where_clause}"
    )
}

fn subscription_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Subscription> {
    let filters_json: String = row.get(9)?;
    Ok(Subscription {
        id: row.get(0)?,
        vault_path: row.get(1)?,
        mid: row.get(2)?,
        name: row.get(3)?,
        avatar: row.get(4)?,
        enabled: row.get::<_, i64>(5)? != 0,
        interval_minutes: row.get::<_, i64>(6)?.max(10) as u32,
        template_id: row.get(7)?,
        prompt: row.get(8)?,
        filters: serde_json::from_str::<SubscriptionFilters>(&filters_json).unwrap_or_default(),
        baseline_at: row.get(10)?,
        last_checked_at: row.get(11)?,
        next_check_at: row.get(12)?,
        last_error: row.get(13)?,
        last_message: row.get(14)?,
        consecutive_failures: row.get::<_, i64>(15)?.max(0) as u32,
        created_at: row.get(16)?,
        updated_at: row.get(17)?,
        latest_title: row.get(18)?,
        latest_bvid: row.get(19)?,
        latest_pubdate: row.get(20)?,
        latest_cover: row.get(21)?,
        queued_count: row.get(22)?,
        seen_count: row.get(23)?,
    })
}

fn migrate_ai_key(conn: &Connection) -> Result<(), String> {
    let legacy: Option<String> = conn
        .query_row("SELECT value FROM settings WHERE key = 'production.ai.api_key'", [], |row| row.get(0))
        .optional()
        .map_err(db_err)?;
    let Some(legacy) = legacy.filter(|value| !value.trim().is_empty()) else { return Ok(()) };
    let provider: String = conn
        .query_row("SELECT value FROM settings WHERE key = 'production.ai.provider'", [], |row| row.get(0))
        .optional()
        .map_err(db_err)?
        .unwrap_or_else(|| "deepseek".to_string());
    let key = format!("production.ai.api_key.{provider}");
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO NOTHING",
        params![key, legacy],
    )
    .map_err(db_err)?;
    conn.execute("DELETE FROM settings WHERE key = 'production.ai.api_key'", [])
        .map_err(db_err)?;
    Ok(())
}

fn row_to_job(row: &rusqlite::Row<'_>) -> rusqlite::Result<Job> {
    let source_json: String = row.get(6)?;
    let options_json: String = row.get(7)?;
    let result_json: String = row.get(14)?;
    let usage_json: String = row.get(15)?;
    Ok(Job {
        id: row.get(0)?,
        title: row.get(1)?,
        template_id: row.get(2)?,
        source_kind: row.get(3)?,
        provider: row.get(4)?,
        model: row.get(5)?,
        source: serde_json::from_str(&source_json).unwrap_or_default(),
        options: serde_json::from_str(&options_json).unwrap_or_default(),
        status: row.get(8)?,
        progress: row.get(9)?,
        current_step: row.get(10)?,
        error: row.get(11)?,
        note_id: row.get(12)?,
        note_rel_path: row.get(13)?,
        result: serde_json::from_str(&result_json).ok(),
        usage: serde_json::from_str(&usage_json).unwrap_or_default(),
        created_at: row.get(16)?,
        updated_at: row.get(17)?,
        started_at: row.get(18)?,
        finished_at: row.get(19)?,
        vault_path: row.get(20)?,
        steps: Vec::new(),
    })
}

fn row_to_checkpoint(row: &rusqlite::Row<'_>) -> rusqlite::Result<CheckpointRow> {
    Ok(CheckpointRow {
        job_id: row.get(0)?,
        unit_key: row.get(1)?,
        step_key: row.get(2)?,
        kind: row.get(3)?,
        input_hash: row.get(4)?,
        payload_json: row.get(5)?,
        usage_json: row.get(6)?,
        payload_bytes: row.get(7)?,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
    })
}

fn steps_for(template: &TemplateInfo, source: &SourceSpec) -> Vec<JobStep> {
    let multiple = source.kind == "favorites" || source.videos.len() > 1 || source.pages.len() > 1;
    templates::steps_for(&template.id, &source.kind, multiple)
        .into_iter()
        .enumerate()
        .map(|(index, step)| JobStep {
            index,
            key: step.key,
            label: step.label,
            status: "pending".into(),
            detail: String::new(),
        })
        .collect()
}

fn source_key(source: &SourceSpec) -> String {
    match source.kind.as_str() {
        "text" => {
            let mut hasher = Sha256::new();
            hasher.update(source.text.as_bytes());
            hex::encode(&hasher.finalize()[..12])
        }
        "file" => source.path.clone(),
        "bili" => format!("{}|{:?}", source.url, source.pages),
        "favorites" => source.videos.iter().map(|item| item.bvid.clone()).collect::<Vec<_>>().join(","),
        _ => source.kind.clone(),
    }
}

fn stable_note_id(template_id: &str, source_key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(template_id.as_bytes());
    hasher.update(b"\0");
    hasher.update(source_key.as_bytes());
    format!("yn-{}", hex::encode(&hasher.finalize()[..12]))
}

fn truncate(value: &str, limit: usize) -> String {
    if value.chars().count() <= limit {
        value.to_string()
    } else {
        value.chars().take(limit).collect::<String>() + "…"
    }
}

fn round_money(value: f64) -> f64 {
    (value * 10000.0).round() / 10000.0
}

fn dir_bytes(path: &Path) -> u64 {
    let mut total = 0u64;
    let Ok(entries) = std::fs::read_dir(path) else { return 0 };
    for entry in entries.flatten() {
        let path = entry.path();
        if let Ok(meta) = entry.metadata() {
            if meta.is_file() {
                total += meta.len();
            } else if meta.is_dir() {
                total += dir_bytes(&path);
            }
        }
    }
    total
}

fn ensure_column(conn: &Connection, table: &str, column: &str, definition: &str) -> Result<(), String> {
    let exists: bool = conn
        .prepare(&format!("PRAGMA table_info({table})"))
        .map_err(db_err)?
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(db_err)?
        .filter_map(Result::ok)
        .any(|name| name == column);
    if !exists {
        conn.execute(&format!("ALTER TABLE {table} ADD COLUMN {column} {definition}"), [])
            .map_err(db_err)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::production::{JobOptions, StartJobRequest, templates};

    fn temp_db() -> (std::path::PathBuf, ProductionStore) {
        let dir = std::env::temp_dir().join(format!("yebian-production-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = ProductionStore::open(&dir.join("yebian.sqlite")).unwrap();
        (dir, store)
    }

    fn request() -> StartJobRequest {
        StartJobRequest {
            template_id: "text-note".into(),
            title: "测试笔记".into(),
            source: SourceSpec {
                kind: "text".into(),
                text: "一篇测试文稿".into(),
                ..SourceSpec::default()
            },
            options: JobOptions::default(),
        }
    }

    #[test]
    fn creates_lists_and_finishes_job() {
        let (dir, store) = temp_db();
        let template = templates::template("text-note").unwrap();
        let job = store.create_job(&dir, request(), &template).unwrap();
        assert_eq!(job.steps.first().map(|step| step.key.as_str()), Some("source"));
        assert!(job.steps.iter().all(|step| step.key != "transcribe"));
        let listed = store.list_jobs(10).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].title, "测试笔记");
        store.start_job(&job.id).unwrap();
        store.set_step(&job.id, 0, "done", "ok").unwrap();
        let result = JobResult {
            body: "# 测试笔记\n\n正文".into(),
            summary: "测试".into(),
            ..JobResult::default()
        };
        store.finish(&job.id, &result, &Usage { currency: "CNY".into(), ..Usage::default() }, "ScribeFlow/测试笔记.md").unwrap();
        let saved = store.get_job(&job.id).unwrap().unwrap();
        assert_eq!(saved.status, "success");
        assert_eq!(saved.note_rel_path, "ScribeFlow/测试笔记.md");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn saves_settings_defaults_and_prompt() {
        let (dir, store) = temp_db();
        let settings = store.settings();
        assert_eq!(settings.ai_model, DEFAULT_AI_MODEL);
        assert_eq!(settings.default_fields, vec!["人物", "事件"]);
        store.save_settings(&SaveSettings {
            ai_key: Some("sk-test".into()),
            default_fields: Some(vec!["人物".into(), "公司".into()]),
            ..SaveSettings::default()
        }).unwrap();
        let updated = store.settings();
        assert!(updated.has_ai_key);
        assert_eq!(updated.default_fields, vec!["人物", "公司"]);
        let saved = store.save_prompt(&PromptEntry {
            id: String::new(),
            name: "自定义".into(),
            description: "测试".into(),
            system: "你是测试".into(),
            builtin: false,
            created_at: 0,
            updated_at: 0,
        }).unwrap();
        assert_eq!(store.list_prompts().unwrap().len(), 1);
        store.delete_prompt(&saved.id).unwrap();
        assert!(store.list_prompts().unwrap().is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn ai_key_is_scoped_to_its_provider() {
        let (dir, store) = temp_db();
        store.save_settings(&SaveSettings {
            ai_provider: Some("deepseek".into()),
            ai_key: Some("sk-deepseek".into()),
            ..SaveSettings::default()
        }).unwrap();
        assert_eq!(store.ai_key("deepseek"), "sk-deepseek");
        assert!(store.settings().has_ai_key);
        store.save_settings(&SaveSettings {
            ai_provider: Some("openai".into()),
            ..SaveSettings::default()
        }).unwrap();
        assert_eq!(store.ai_key("deepseek"), "sk-deepseek");
        assert_eq!(store.ai_key("openai"), "");
        assert!(!store.settings().has_ai_key);
        store.save_settings(&SaveSettings {
            ai_key: Some("sk-openai".into()),
            ..SaveSettings::default()
        }).unwrap();
        assert_eq!(store.ai_key("openai"), "sk-openai");
        assert!(store.settings().has_ai_key);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn migrates_legacy_ai_key_to_the_saved_provider() {
        let (dir, store) = temp_db();
        store.set_setting("production.ai.provider", "deepseek").unwrap();
        store.set_setting("production.ai.api_key", "sk-legacy").unwrap();
        drop(store);
        let reopened = ProductionStore::open(&dir.join("yebian.sqlite")).unwrap();
        assert_eq!(reopened.ai_key("deepseek"), "sk-legacy");
        assert_eq!(reopened.get_setting("production.ai.api_key").unwrap(), None);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn keep_both_gets_a_new_note_id() {
        let (dir, store) = temp_db();
        let template = templates::template("text-note").unwrap();
        let normal = store.create_job(&dir, request(), &template).unwrap();
        let mut copy_request = request();
        copy_request.options.conflict_action = "keep_both".into();
        let copy = store.create_job(&dir, copy_request, &template).unwrap();
        assert_ne!(normal.note_id, copy.note_id);
        assert!(copy.note_id.starts_with("yn-"));
        assert_eq!(copy.options.conflict_action, "keep_both");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn usage_summary_groups_current_month_and_marks_unpriced_jobs() {
        let (dir, store) = temp_db();
        let template = templates::template("text-note").unwrap();
        store.save_settings(&SaveSettings {
            monthly_budget_cny: Some(4.0),
            ..SaveSettings::default()
        }).unwrap();
        for (index, cost, priced) in [(1, 1.5, true), (2, 0.5, false)] {
            let mut req = request();
            req.title = format!("用量测试 {index}");
            let job = store.create_job(&dir, req, &template).unwrap();
            store.start_job(&job.id).unwrap();
            store.finish(
                &job.id,
                &JobResult { body: "# 测试".into(), ..JobResult::default() },
                &Usage {
                    prompt_tokens: 600,
                    completion_tokens: 400,
                    estimated_cost: cost,
                    currency: "CNY".into(),
                    priced,
                    ..Usage::default()
                },
                "ScribeFlow/测试.md",
            ).unwrap();
        }
        let summary = store.usage_summary().unwrap();
        assert_eq!(summary.get("jobs").and_then(|value| value.as_u64()), Some(2));
        assert_eq!(summary.get("pricedJobs").and_then(|value| value.as_u64()), Some(1));
        assert_eq!(summary.get("tokens").and_then(|value| value.as_u64()), Some(2000));
        assert!((summary.get("spendCny").and_then(|value| value.as_f64()).unwrap() - 2.0).abs() < 0.001);
        assert!((summary.get("percent").and_then(|value| value.as_f64()).unwrap() - 50.0).abs() < 0.001);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn keeps_prompt_versions_and_restores_as_a_new_version() {
        let (dir, store) = temp_db();
        let first = store.save_prompt(&PromptEntry {
            id: String::new(),
            name: "版本测试".into(),
            description: "v1".into(),
            system: "你是第一版".into(),
            builtin: false,
            created_at: 0,
            updated_at: 0,
        }).unwrap();
        let second = store.save_prompt(&PromptEntry {
            id: first.id.clone(),
            name: "版本测试".into(),
            description: "v2".into(),
            system: "你是第二版".into(),
            builtin: false,
            created_at: 0,
            updated_at: 0,
        }).unwrap();
        assert_eq!(first.id, second.id);
        let versions = store.list_prompt_versions(&first.id).unwrap();
        assert_eq!(versions.len(), 2);
        assert_eq!(versions[0].version, 2);
        assert_eq!(versions[0].chars, "你是第二版".chars().count());
        let restored = store.restore_prompt_version(&first.id, 1).unwrap();
        assert_eq!(restored.system, "你是第一版");
        let after = store.list_prompt_versions(&first.id).unwrap();
        assert_eq!(after.len(), 3);
        assert_eq!(after[0].version, 3);
        store.delete_prompt(&first.id).unwrap();
        assert!(store.list_prompt_versions(&first.id).unwrap().is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn cleanup_keeps_failed_accessories_until_explicitly_deleted() {
        let (dir, store) = temp_db();
        let template = templates::template("text-note").unwrap();

        let done = store.create_job(&dir, request(), &template).unwrap();
        store.start_job(&done.id).unwrap();
        store.finish(&done.id, &JobResult::default(), &Usage { currency: "CNY".into(), ..Usage::default() }, "").unwrap();

        let failed = store.create_job(&dir, request(), &template).unwrap();
        store.start_job(&failed.id).unwrap();
        store.fail(&failed.id, "injected").unwrap();
        store.save_checkpoint(&failed.id, "refine:item:0:part:0", "refine", "text", "hash", r#"ok"#, "{}", 4).unwrap();

        assert_eq!(store.clear_finished(&dir.to_string_lossy()).unwrap(), 1);
        assert!(store.get_job(&done.id).unwrap().is_none());
        assert!(store.get_job(&failed.id).unwrap().is_some());
        assert_eq!(store.list_checkpoints(&failed.id).unwrap().len(), 1);

        let queued = store.create_job(&dir, request(), &template).unwrap();
        let protected = store.protected_temp_jobs().unwrap();
        assert!(protected.contains(&failed.id));
        assert!(protected.contains(&queued.id));
        let ids = store.job_ids_with_status(&["error"], Some(&dir.to_string_lossy())).unwrap();
        assert_eq!(ids, vec![failed.id.clone()]);
        assert_eq!(store.delete_jobs(&ids).unwrap(), 1);
        assert!(store.get_job(&failed.id).unwrap().is_none());
        assert!(store.list_checkpoints(&failed.id).unwrap().is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn keeps_manual_model_prices_and_overwrites_by_provider_and_model() {
        let (dir, store) = temp_db();
        assert!(store.list_model_prices(None).unwrap().is_empty());
        let first = store.set_model_price("custom", "my-model", 10.0, 30.0).unwrap();
        assert_eq!(first.input_cny, 10.0);
        store.set_model_price("custom", "my-model", 12.5, 40.0).unwrap();
        store.set_model_price("openai", "my-model", 1.0, 2.0).unwrap();
        let custom = store.list_model_prices(Some("custom")).unwrap();
        assert_eq!(custom.len(), 1);
        assert_eq!(custom[0].input_cny, 12.5);
        assert_eq!(custom[0].output_cny, 40.0);
        assert_eq!(store.model_price("openai", "my-model").unwrap().unwrap().input_cny, 1.0);
        assert!(store.set_model_price("custom", "bad", -1.0, 2.0).is_err());
        assert!(store.set_model_price("", "bad", 1.0, 2.0).is_err());
        store.delete_model_price("custom", "my-model").unwrap();
        assert!(store.model_price("custom", "my-model").unwrap().is_none());
        assert_eq!(store.list_model_prices(None).unwrap().len(), 1);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn keeps_checkpoints_and_prepares_resume() {
        let (dir, store) = temp_db();
        let template = templates::template("text-note").unwrap();
        let job = store.create_job(&dir, request(), &template).unwrap();
        assert_eq!(job.vault_path, dir.to_string_lossy());
        assert!(store.job_config(&job.id).unwrap().is_some());
        store
            .save_checkpoint(&job.id, "refine:item:0:part:0", "refine", "text", "hash", r#"ok"#, "{}", 4)
            .unwrap();
        assert_eq!(store.list_checkpoints(&job.id).unwrap().len(), 1);
        store.start_job(&job.id).unwrap();
        store.fail(&job.id, "injected").unwrap();
        let resumed = store.prepare_resume(&job.id).unwrap();
        assert_eq!(resumed.status, "queued");
        assert_eq!(resumed.usage.prompt_tokens, 0);
        assert_eq!(store.list_checkpoints(&job.id).unwrap().len(), 1);
        assert!(resumed.steps.iter().all(|step| step.status == "pending"));
        store.clear_checkpoints(&job.id).unwrap();
        assert!(store.list_checkpoints(&job.id).unwrap().is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn recovers_running_jobs_as_interrupted() {
        let (dir, store) = temp_db();
        let template = templates::template("text-note").unwrap();
        let job = store.create_job(&dir, request(), &template).unwrap();
        store.start_job(&job.id).unwrap();
        let reopened = ProductionStore::open(&dir.join("yebian.sqlite")).unwrap();
        let recovered = reopened.recover_interrupted().unwrap();
        assert_eq!(recovered.len(), 1);
        assert_eq!(recovered[0].status, "interrupted");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn subscriptions_keep_baseline_seen_dedupe_and_stats() {
        let (dir, store) = temp_db();
        let vault = dir.to_string_lossy().to_string();
        let input = SubscriptionInput {
            mid: 946974,
            name: "测试 UP".into(),
            avatar: "https://example.com/avatar.png".into(),
            template_id: "video-note".into(),
            prompt: "速览版".into(),
            enabled: true,
            filters: SubscriptionFilters {
                min_duration_sec: 60,
                max_duration_sec: 3600,
                include_keywords: "AI".into(),
                exclude_keywords: "直播".into(),
            },
            ..SubscriptionInput::default()
        };
        let sub = store.save_subscription(&vault, &input).unwrap();
        assert_eq!(sub.seen_count, 0);
        assert_eq!(store.list_subscriptions(&vault).unwrap().len(), 1);
        assert!(store.save_subscription(&vault, &input).is_err());

        let video = crate::production::BiliVideoRef {
            bvid: "BV1test00001".into(),
            title: "AI 新进展".into(),
            duration: 600,
            pubdate: 1_700_000_000,
            ..Default::default()
        };
        assert!(store.insert_seen(&vault, sub.mid, &video, "baseline", "", "").unwrap());
        assert!(!store.insert_seen(&vault, sub.mid, &video, "queued", "run-1", "note-1").unwrap());
        assert!(store.seen_exists(&vault, sub.mid, &video.bvid).unwrap());
        let queued = crate::production::BiliVideoRef {
            bvid: "BV1test00002".into(),
            title: "AI 第二条".into(),
            duration: 900,
            pubdate: 1_700_000_100,
            ..Default::default()
        };
        assert!(store.insert_seen(&vault, sub.mid, &queued, "queued", "run-2", "note-2").unwrap());
        let refreshed = store.get_subscription(&sub.id).unwrap().unwrap();
        assert_eq!(refreshed.seen_count, 2);
        assert_eq!(refreshed.queued_count, 1);
        assert_eq!(refreshed.latest_bvid, queued.bvid);
        assert_eq!(refreshed.filters.min_duration_sec, 60);
        assert!(store.queued_auto_today(&vault).unwrap() >= 1);
        assert_eq!(store.due_subscriptions(&vault, now_ms() + 1).unwrap().len(), 1);

        store
            .record_subscription_check(&sub.id, 123, 456, "错误", "没有新视频", 2)
            .unwrap();
        let checked = store.get_subscription(&sub.id).unwrap().unwrap();
        assert_eq!(checked.last_checked_at, 123);
        assert_eq!(checked.last_message, "没有新视频");
        assert_eq!(checked.consecutive_failures, 2);

        store.delete_subscription(&vault, &sub.id).unwrap();
        assert!(store.list_subscriptions(&vault).unwrap().is_empty());
        assert!(!store.seen_exists(&vault, sub.mid, &queued.bvid).unwrap());
        let _ = std::fs::remove_dir_all(dir);
    }
}
