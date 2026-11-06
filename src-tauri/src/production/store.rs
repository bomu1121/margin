use std::collections::HashMap;
use std::path::Path;

use chrono::{Datelike, Local, TimeZone};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, OptionalExtension};
use sha2::{Digest, Sha256};

use super::{
    Job, JobResult, JobStep, LogEntry, ProductionSettings, PromptEntry,
    SaveSettings, SourceSpec, StartJobRequest, Usage,
};
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
            ",
        )
        .map_err(db_err)?;
        ensure_column(&conn, "production_jobs", "provider", "TEXT NOT NULL DEFAULT ''")?;
        ensure_column(&conn, "production_jobs", "model", "TEXT NOT NULL DEFAULT ''")?;
        migrate_ai_key(&conn)?;
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
            search_provider: self.setting_or("production.search.provider", "zhipu"),
            has_search_key: !self.get_setting("production.search.api_key").ok().flatten().unwrap_or_default().is_empty(),
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
        let pairs: [(&str, Option<String>); 16] = [
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

    pub fn create_job(&self, _root: &Path, request: StartJobRequest, template: &TemplateInfo) -> Result<Job, String> {
        let id = format!("run_{}", uuid::Uuid::new_v4().simple());
        let source_key = source_key(&request.source);
        let note_id = stable_note_id(&request.template_id, &source_key);
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
        let timestamp = now_ms();
        let settings = self.settings();
        let job = Job {
            id: id.clone(),
            title,
            template_id: request.template_id,
            source_kind: request.source.kind.clone(),
            provider: settings.ai_provider,
            model: settings.ai_model,
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
               usage_json, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'queued', 0, '', '', ?9, '', '', ?10, ?11, ?11)",
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

    pub fn list_jobs(&self, limit: usize) -> Result<Vec<Job>, String> {
        let conn = self.lock()?;
        let mut statement = conn
            .prepare(
                "SELECT id, title, template_id, source_kind, provider, model, source_json,
                        options_json, status, progress, current_step, error, note_id,
                        note_rel_path, result_json, usage_json, created_at, updated_at,
                        started_at, finished_at
                 FROM production_jobs ORDER BY created_at DESC LIMIT ?1",
            )
            .map_err(db_err)?;
        let rows = statement
            .query_map(params![limit as i64], row_to_job)
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
                        started_at, finished_at
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
                        started_at, finished_at
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

    pub fn reset_for_retry(&self, id: &str) -> Result<Job, String> {
        let settings = self.settings();
        let conn = self.lock()?;
        let timestamp = now_ms();
        conn.execute(
            "UPDATE production_jobs SET status = 'queued', progress = 0, current_step = '',
                    error = '', result_json = '', usage_json = '{}', started_at = 0,
                    finished_at = 0, updated_at = ?2,
                    provider = CASE WHEN provider = '' THEN ?3 ELSE provider END,
                    model = CASE WHEN model = '' THEN ?4 ELSE model END
             WHERE id = ?1 AND status IN ('error','cancelled','interrupted')",
            params![id, timestamp, settings.ai_provider, settings.ai_model],
        )
        .map_err(db_err)?;
        conn.execute(
            "UPDATE production_steps SET status = 'pending', detail = '' WHERE job_id = ?1",
            params![id],
        )
        .map_err(db_err)?;
        drop(conn);
        self.get_job(id)?.ok_or_else(|| "任务不存在".into())
    }

    pub fn delete_job(&self, id: &str) -> Result<(), String> {
        let conn = self.lock()?;
        conn.execute("DELETE FROM production_steps WHERE job_id = ?1", params![id]).map_err(db_err)?;
        conn.execute("DELETE FROM production_logs WHERE job_id = ?1", params![id]).map_err(db_err)?;
        conn.execute("DELETE FROM production_jobs WHERE id = ?1", params![id]).map_err(db_err)?;
        Ok(())
    }

    pub fn clear_finished(&self) -> Result<usize, String> {
        let conn = self.lock()?;
        let ids = {
            let mut statement = conn
                .prepare("SELECT id FROM production_jobs WHERE status IN ('success','error','cancelled','interrupted')")
                .map_err(db_err)?;
            statement
                .query_map([], |row| row.get::<_, String>(0))
                .map_err(db_err)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(db_err)?
        };
        for id in &ids {
            conn.execute("DELETE FROM production_steps WHERE job_id = ?1", params![id]).map_err(db_err)?;
            conn.execute("DELETE FROM production_logs WHERE job_id = ?1", params![id]).map_err(db_err)?;
            conn.execute("DELETE FROM production_jobs WHERE id = ?1", params![id]).map_err(db_err)?;
        }
        Ok(ids.len())
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

    pub fn save_prompt(&self, entry: &PromptEntry) -> Result<PromptEntry, String> {
        let id = if entry.id.trim().is_empty() {
            format!("custom_{}", uuid::Uuid::new_v4().simple())
        } else {
            entry.id.trim().to_string()
        };
        let timestamp = now_ms();
        let conn = self.lock()?;
        conn.execute(
            "INSERT INTO production_prompts (id, name, description, system, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?5)
             ON CONFLICT(id) DO UPDATE SET name = excluded.name, description = excluded.description,
                    system = excluded.system, updated_at = excluded.updated_at",
            params![id, entry.name.trim(), entry.description.trim(), entry.system, timestamp],
        )
        .map_err(db_err)?;
        Ok(PromptEntry {
            id,
            name: entry.name.trim().to_string(),
            description: entry.description.trim().to_string(),
            system: entry.system.clone(),
            builtin: false,
            created_at: timestamp,
            updated_at: timestamp,
        })
    }

    pub fn delete_prompt(&self, id: &str) -> Result<(), String> {
        let conn = self.lock()?;
        conn.execute("DELETE FROM production_prompts WHERE id = ?1", params![id]).map_err(db_err)?;
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
        Ok(serde_json::json!({
            "jobs": total,
            "finished": finished,
            "tempBytes": temp,
            "outboxBytes": outbox,
        }))
    }
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
        steps: Vec::new(),
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
}
