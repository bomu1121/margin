//! 历史史料：事件、史料、原文段落与引用关系。
//!
//! 这里刻意不和 `facts` 合成一个概念：
//! - `facts` 是“关于历史的主张”；
//! - `sources` 是“哪本书/哪份档案/哪一版”；
//! - `source_passages` 是“原文段落”；
//! - `citations` 是“哪段原文和哪条主张的关系”。
//!
//! 旧事实库里的 `fact_sources` + `fact_evidence` 会一次性迁移成
//! `source_type = web` 的 source/passage/citation；旧表保留只读。

use std::path::Path;
use std::sync::{Arc, Mutex};

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use url::Url;
use uuid::Uuid;

use super::facts::Fact;
use super::store::now_ms;

#[derive(Clone)]
pub struct HistoryStore {
    conn: Arc<Mutex<Connection>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEvent {
    pub id: String,
    pub canonical_name: String,
    pub aliases: Vec<String>,
    pub start_date: String,
    pub end_date: String,
    pub date_note: String,
    pub places: Vec<String>,
    pub actors: Vec<String>,
    pub summary: String,
    pub fact_count: i64,
    pub source_count: i64,
    pub note_count: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    pub id: String,
    pub work_title: String,
    pub section_title: String,
    pub author: String,
    pub compiler: String,
    pub era: String,
    pub source_type: String,
    pub edition: String,
    pub publisher: String,
    pub publish_year: String,
    pub volume: String,
    pub repository: String,
    pub url: String,
    pub scan_url: String,
    pub call_number: String,
    pub language: String,
    pub authenticity_note: String,
    pub passage_count: i64,
    pub citation_count: i64,
    pub fact_count: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourcePassage {
    pub id: String,
    pub source_id: String,
    pub volume: String,
    pub chapter: String,
    pub page: String,
    pub original_text: String,
    pub translation: String,
    pub variant_of: String,
    pub scan_url: String,
    pub locator: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FactCitation {
    pub id: String,
    pub fact_id: String,
    pub passage_id: String,
    pub relation: String,
    pub quote: String,
    pub locator: String,
    pub confidence: f64,
    pub checked_at: i64,
    pub source_id: String,
    pub work_title: String,
    pub section_title: String,
    pub author: String,
    pub edition: String,
    pub volume: String,
    pub page: String,
    pub original_text: String,
    pub translation: String,
    pub source_type: String,
    pub source_url: String,
    pub scan_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceCitedFact {
    pub citation_id: String,
    pub fact_id: String,
    pub text: String,
    pub status: String,
    pub historical_status: String,
    pub relation: String,
    pub quote: String,
    pub locator: String,
    pub note_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceDetail {
    pub source: Source,
    pub passages: Vec<SourcePassage>,
    pub facts: Vec<SourceCitedFact>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryCandidate {
    pub id: String,
    pub fact_id: String,
    pub provider: String,
    pub title: String,
    pub url: String,
    pub snippet: String,
    pub authority: String,
    pub status: String,
    pub reason: String,
    pub work_title: String,
    pub section_title: String,
    pub source_id: String,
    pub citation_id: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Default)]
pub struct HistoryCandidateInput {
    pub provider: String,
    pub title: String,
    pub url: String,
    pub snippet: String,
    pub authority: String,
    pub status: String,
    pub reason: String,
    pub work_title: String,
    pub section_title: String,
    pub source_id: String,
    pub citation_id: String,
}

#[derive(Debug, Clone)]
pub struct CachedPage {
    pub key: String,
    pub provider: String,
    pub url: String,
    pub title: String,
    pub text: String,
    pub fetched_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventFact {
    pub id: String,
    pub text: String,
    pub status: String,
    pub historical_status: String,
    pub claim_kind: String,
    pub note_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventDetail {
    pub event: HistoryEvent,
    pub facts: Vec<EventFact>,
    pub sources: Vec<Source>,
    pub note_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SourceInput {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub work_title: String,
    #[serde(default)]
    pub section_title: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub compiler: String,
    #[serde(default)]
    pub era: String,
    #[serde(default)]
    pub source_type: String,
    #[serde(default)]
    pub edition: String,
    #[serde(default)]
    pub publisher: String,
    #[serde(default)]
    pub publish_year: String,
    #[serde(default)]
    pub volume: String,
    #[serde(default)]
    pub repository: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub scan_url: String,
    #[serde(default)]
    pub call_number: String,
    #[serde(default)]
    pub language: String,
    #[serde(default)]
    pub authenticity_note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PassageInput {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub source_id: String,
    #[serde(default)]
    pub volume: String,
    #[serde(default)]
    pub chapter: String,
    #[serde(default)]
    pub page: String,
    #[serde(default)]
    pub original_text: String,
    #[serde(default)]
    pub translation: String,
    #[serde(default)]
    pub variant_of: String,
    #[serde(default)]
    pub scan_url: String,
    #[serde(default)]
    pub locator: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CitationInput {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub fact_id: String,
    #[serde(default)]
    pub passage_id: String,
    #[serde(default)]
    pub relation: String,
    #[serde(default)]
    pub quote: String,
    #[serde(default)]
    pub locator: String,
    #[serde(default)]
    pub confidence: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct EventInput {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub canonical_name: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub start_date: String,
    #[serde(default)]
    pub end_date: String,
    #[serde(default)]
    pub date_note: String,
    #[serde(default)]
    pub places: Vec<String>,
    #[serde(default)]
    pub actors: Vec<String>,
    #[serde(default)]
    pub summary: String,
}

fn lock(store: &HistoryStore) -> Result<std::sync::MutexGuard<'_, Connection>, String> {
    store.conn.lock().map_err(|_| "history database lock poisoned".to_string())
}

fn db_err(error: rusqlite::Error) -> String {
    error.to_string()
}

fn clean_text(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut in_tag = false;
    for ch in value.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => output.push(ch),
            _ => {}
        }
    }
    output
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn normalize(value: &str) -> String {
    value
        .chars()
        .filter(|ch| ch.is_alphanumeric() || ('\u{4e00}'..='\u{9fff}').contains(ch))
        .flat_map(|ch| ch.to_lowercase())
        .collect()
}

fn new_id(prefix: &str) -> String {
    format!("{prefix}_{}", Uuid::new_v4().simple())
}

fn json_list(value: &str) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(value).unwrap_or_default()
}

fn source_type_or_default(value: &str) -> &str {
    match value {
        "primary" | "secondary" | "archive" | "newspaper" | "reference" | "web" => value,
        _ => "primary",
    }
}

fn relation_or_default(value: &str) -> &str {
    match value {
        "supports" | "contradicts" | "variant" | "context" => value,
        _ => "supports",
    }
}

fn historical_status_or_empty(value: &str) -> &str {
    match value {
        "direct_source" | "indirect_source" | "source_variant" | "needs_research"
        | "modern_scholarship" | "disputed_interpretation" => value,
        _ => "",
    }
}

impl HistoryStore {
    pub fn open(path: &Path) -> Result<Self, String> {
        let conn = Connection::open(path).map_err(db_err)?;
        conn.execute_batch(
            "
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;
            PRAGMA busy_timeout = 30000;

            CREATE TABLE IF NOT EXISTS history_events (
              id TEXT PRIMARY KEY,
              canonical_name TEXT NOT NULL,
              aliases_json TEXT NOT NULL DEFAULT '[]',
              start_date TEXT NOT NULL DEFAULT '',
              end_date TEXT NOT NULL DEFAULT '',
              date_note TEXT NOT NULL DEFAULT '',
              places_json TEXT NOT NULL DEFAULT '[]',
              actors_json TEXT NOT NULL DEFAULT '[]',
              authority_json TEXT NOT NULL DEFAULT '{}',
              summary TEXT NOT NULL DEFAULT '',
              created_at INTEGER NOT NULL,
              updated_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS sources (
              id TEXT PRIMARY KEY,
              work_title TEXT NOT NULL,
              section_title TEXT NOT NULL DEFAULT '',
              author TEXT NOT NULL DEFAULT '',
              compiler TEXT NOT NULL DEFAULT '',
              era TEXT NOT NULL DEFAULT '',
              source_type TEXT NOT NULL DEFAULT 'primary',
              edition TEXT NOT NULL DEFAULT '',
              publisher TEXT NOT NULL DEFAULT '',
              publish_year TEXT NOT NULL DEFAULT '',
              volume TEXT NOT NULL DEFAULT '',
              repository TEXT NOT NULL DEFAULT '',
              url TEXT NOT NULL DEFAULT '',
              scan_url TEXT NOT NULL DEFAULT '',
              call_number TEXT NOT NULL DEFAULT '',
              language TEXT NOT NULL DEFAULT 'zh',
              authenticity_note TEXT NOT NULL DEFAULT '',
              created_at INTEGER NOT NULL,
              updated_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS source_passages (
              id TEXT PRIMARY KEY,
              source_id TEXT NOT NULL,
              volume TEXT NOT NULL DEFAULT '',
              chapter TEXT NOT NULL DEFAULT '',
              page TEXT NOT NULL DEFAULT '',
              original_text TEXT NOT NULL,
              normalized_text TEXT NOT NULL,
              translation TEXT NOT NULL DEFAULT '',
              variant_of TEXT NOT NULL DEFAULT '',
              scan_url TEXT NOT NULL DEFAULT '',
              locator TEXT NOT NULL DEFAULT '',
              created_at INTEGER NOT NULL,
              UNIQUE(source_id, original_text, page)
            );
            CREATE INDEX IF NOT EXISTS idx_source_passages_source ON source_passages(source_id);

            CREATE TABLE IF NOT EXISTS citations (
              id TEXT PRIMARY KEY,
              fact_id TEXT NOT NULL,
              passage_id TEXT NOT NULL,
              relation TEXT NOT NULL,
              quote TEXT NOT NULL DEFAULT '',
              locator TEXT NOT NULL DEFAULT '',
              confidence REAL NOT NULL DEFAULT 0,
              checked_at INTEGER NOT NULL,
              created_at INTEGER NOT NULL,
              UNIQUE(fact_id, passage_id, quote)
            );
            CREATE INDEX IF NOT EXISTS idx_citations_fact ON citations(fact_id);
            CREATE INDEX IF NOT EXISTS idx_citations_passage ON citations(passage_id);

            CREATE TABLE IF NOT EXISTS history_meta (
              key TEXT PRIMARY KEY,
              value TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS history_search_candidates (
              id TEXT PRIMARY KEY,
              fact_id TEXT NOT NULL,
              provider TEXT NOT NULL,
              title TEXT NOT NULL,
              url TEXT NOT NULL DEFAULT '',
              snippet TEXT NOT NULL DEFAULT '',
              authority TEXT NOT NULL DEFAULT '',
              status TEXT NOT NULL DEFAULT 'candidate',
              reason TEXT NOT NULL DEFAULT '',
              work_title TEXT NOT NULL DEFAULT '',
              section_title TEXT NOT NULL DEFAULT '',
              source_id TEXT NOT NULL DEFAULT '',
              citation_id TEXT NOT NULL DEFAULT '',
              created_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_history_candidates_fact ON history_search_candidates(fact_id);

            CREATE TABLE IF NOT EXISTS history_page_cache (
              key TEXT PRIMARY KEY,
              provider TEXT NOT NULL,
              url TEXT NOT NULL,
              title TEXT NOT NULL,
              text TEXT NOT NULL,
              fetched_at INTEGER NOT NULL
            );
            ",
        )
        .map_err(db_err)?;
        let _ = conn.execute("ALTER TABLE facts ADD COLUMN domain TEXT NOT NULL DEFAULT 'general'", []);
        let _ = conn.execute("ALTER TABLE facts ADD COLUMN event_id TEXT NOT NULL DEFAULT ''", []);
        let _ = conn.execute("ALTER TABLE facts ADD COLUMN historical_status TEXT NOT NULL DEFAULT ''", []);
        let _ = conn.execute("ALTER TABLE facts ADD COLUMN claim_kind TEXT NOT NULL DEFAULT ''", []);
        let store = Self { conn: Arc::new(Mutex::new(conn)) };
        store.migrate_legacy_evidence()?;
        store.migrate_domains()?;
        Ok(store)
    }

    /// 已有引用的事实一律归到历史区：有史料引文，说明它走的是史料溯源。
    /// 这只处理旧数据；新事实的 domain 在主张抽取时就已写好。
    fn migrate_domains(&self) -> Result<(), String> {
        let mut conn = lock(self)?;
        let version: Option<String> = conn
            .query_row("SELECT value FROM history_meta WHERE key = 'domain_version'", [], |row| row.get(0))
            .optional()
            .map_err(db_err)?;
        if version.as_deref() == Some("1") {
            return Ok(());
        }
        let tx = conn.transaction().map_err(db_err)?;
        tx.execute(
            "UPDATE facts SET domain = 'history'
             WHERE domain = 'general' AND id IN (SELECT DISTINCT fact_id FROM citations)",
            [],
        )
        .map_err(db_err)?;
        tx.execute("INSERT OR REPLACE INTO history_meta (key, value) VALUES ('domain_version', '1')", [])
            .map_err(db_err)?;
        tx.commit().map_err(db_err)?;
        Ok(())
    }

    fn migrate_legacy_evidence(&self) -> Result<(), String> {
        let mut conn = lock(self)?;
        let version: Option<String> = conn
            .query_row("SELECT value FROM history_meta WHERE key = 'source_version'", [], |row| row.get(0))
            .optional()
            .map_err(db_err)?;
        if version.as_deref() == Some("3") {
            return Ok(());
        }
        // 旧 fact_sources/fact_evidence 是网页事实核查证据，不是史料。
        // 它们继续留在事实详情「网络证据」里；不要污染史料库。
        let tx = conn.transaction().map_err(db_err)?;
        tx.execute(
            "DELETE FROM citations WHERE passage_id IN
               (SELECT id FROM source_passages WHERE source_id IN
                  (SELECT id FROM sources WHERE source_type = 'web'))",
            [],
        )
        .map_err(db_err)?;
        tx.execute(
            "DELETE FROM source_passages WHERE source_id IN
               (SELECT id FROM sources WHERE source_type = 'web')",
            [],
        )
        .map_err(db_err)?;
        tx.execute("DELETE FROM sources WHERE source_type = 'web'", []).map_err(db_err)?;
        tx.execute("INSERT OR REPLACE INTO history_meta (key, value) VALUES ('source_version', '3')", [])
            .map_err(db_err)?;
        tx.commit().map_err(db_err)?;
        Ok(())
    }

    pub fn list_sources(&self, query: &str, source_type: &str, limit: usize) -> Result<Vec<Source>, String> {
        let conn = lock(self)?;
        let mut statement = conn
            .prepare(
                "SELECT s.id, s.work_title, s.section_title, s.author, s.compiler, s.era,
                        s.source_type, s.edition, s.publisher, s.publish_year, s.volume,
                        s.repository, s.url, s.scan_url, s.call_number, s.language,
                        s.authenticity_note,
                        (SELECT COUNT(*) FROM source_passages p WHERE p.source_id = s.id),
                        (SELECT COUNT(*) FROM citations c JOIN source_passages p ON p.id = c.passage_id WHERE p.source_id = s.id),
                        (SELECT COUNT(DISTINCT c.fact_id) FROM citations c JOIN source_passages p ON p.id = c.passage_id WHERE p.source_id = s.id),
                        s.created_at, s.updated_at
                 FROM sources s
                 WHERE (?1 = '' OR s.work_title LIKE '%' || ?1 || '%' OR s.author LIKE '%' || ?1 || '%' OR s.section_title LIKE '%' || ?1 || '%')
                   AND (?2 = '' OR s.source_type = ?2)
                 ORDER BY CASE s.source_type WHEN 'primary' THEN 0 WHEN 'secondary' THEN 1 WHEN 'archive' THEN 2 WHEN 'newspaper' THEN 3 WHEN 'reference' THEN 4 ELSE 5 END,
                          s.updated_at DESC
                 LIMIT ?3",
            )
            .map_err(db_err)?;
        let rows = statement
            .query_map(params![query.trim(), source_type, limit.clamp(1, 300) as i64], |row| {
                Ok(Source {
                    id: row.get(0)?,
                    work_title: row.get(1)?,
                    section_title: row.get(2)?,
                    author: row.get(3)?,
                    compiler: row.get(4)?,
                    era: row.get(5)?,
                    source_type: row.get(6)?,
                    edition: row.get(7)?,
                    publisher: row.get(8)?,
                    publish_year: row.get(9)?,
                    volume: row.get(10)?,
                    repository: row.get(11)?,
                    url: row.get(12)?,
                    scan_url: row.get(13)?,
                    call_number: row.get(14)?,
                    language: row.get(15)?,
                    authenticity_note: row.get(16)?,
                    passage_count: row.get(17)?,
                    citation_count: row.get(18)?,
                    fact_count: row.get(19)?,
                    created_at: row.get(20)?,
                    updated_at: row.get(21)?,
                })
            })
            .map_err(db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_err)?;
        Ok(rows)
    }

    fn source_summary(conn: &Connection, id: &str) -> Result<Option<Source>, String> {
        let mut statement = conn
            .prepare(
                "SELECT s.id, s.work_title, s.section_title, s.author, s.compiler, s.era,
                        s.source_type, s.edition, s.publisher, s.publish_year, s.volume,
                        s.repository, s.url, s.scan_url, s.call_number, s.language,
                        s.authenticity_note,
                        (SELECT COUNT(*) FROM source_passages p WHERE p.source_id = s.id),
                        (SELECT COUNT(*) FROM citations c JOIN source_passages p ON p.id = c.passage_id WHERE p.source_id = s.id),
                        (SELECT COUNT(DISTINCT c.fact_id) FROM citations c JOIN source_passages p ON p.id = c.passage_id WHERE p.source_id = s.id),
                        s.created_at, s.updated_at
                 FROM sources s WHERE s.id = ?1",
            )
            .map_err(db_err)?;
        let row = statement
            .query_row(params![id], |row| {
                Ok(Source {
                    id: row.get(0)?,
                    work_title: row.get(1)?,
                    section_title: row.get(2)?,
                    author: row.get(3)?,
                    compiler: row.get(4)?,
                    era: row.get(5)?,
                    source_type: row.get(6)?,
                    edition: row.get(7)?,
                    publisher: row.get(8)?,
                    publish_year: row.get(9)?,
                    volume: row.get(10)?,
                    repository: row.get(11)?,
                    url: row.get(12)?,
                    scan_url: row.get(13)?,
                    call_number: row.get(14)?,
                    language: row.get(15)?,
                    authenticity_note: row.get(16)?,
                    passage_count: row.get(17)?,
                    citation_count: row.get(18)?,
                    fact_count: row.get(19)?,
                    created_at: row.get(20)?,
                    updated_at: row.get(21)?,
                })
            })
            .optional()
            .map_err(db_err)?;
        Ok(row)
    }

    pub fn get_source(&self, id: &str) -> Result<Option<SourceDetail>, String> {
        let conn = lock(self)?;
        let Some(source) = Self::source_summary(&conn, id)? else { return Ok(None) };
        let mut passage_stmt = conn
            .prepare(
                "SELECT id, source_id, volume, chapter, page, original_text, translation,
                        variant_of, scan_url, locator, created_at
                 FROM source_passages WHERE source_id = ?1 ORDER BY created_at",
            )
            .map_err(db_err)?;
        let passages = passage_stmt
            .query_map(params![id], |row| {
                Ok(SourcePassage {
                    id: row.get(0)?,
                    source_id: row.get(1)?,
                    volume: row.get(2)?,
                    chapter: row.get(3)?,
                    page: row.get(4)?,
                    original_text: clean_text(&row.get::<_, String>(5)?),
                    translation: clean_text(&row.get::<_, String>(6)?),
                    variant_of: row.get(7)?,
                    scan_url: row.get(8)?,
                    locator: row.get(9)?,
                    created_at: row.get(10)?,
                })
            })
            .map_err(db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_err)?;
        drop(passage_stmt);
        let mut fact_stmt = conn
            .prepare(
                "SELECT c.id, f.id, f.text, f.status, f.historical_status, c.relation, c.quote, c.locator
                 FROM citations c JOIN source_passages p ON p.id = c.passage_id
                 JOIN facts f ON f.id = c.fact_id
                 WHERE p.source_id = ?1 ORDER BY c.created_at",
            )
            .map_err(db_err)?;
        let cited = fact_stmt
            .query_map(params![id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                ))
            })
            .map_err(db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_err)?;
        drop(fact_stmt);
        let mut facts = Vec::new();
        for (citation_id, fact_id, text, status, historical_status, relation, quote, locator) in cited {
            let note_ids = note_ids_for_fact(&conn, &fact_id)?;
            facts.push(SourceCitedFact {
                citation_id,
                fact_id,
                text,
                status,
                historical_status,
                relation,
                quote: clean_text(&quote),
                locator,
                note_ids,
            });
        }
        Ok(Some(SourceDetail { source, passages, facts }))
    }

    pub fn save_source(&self, input: SourceInput) -> Result<Source, String> {
        let title = input.work_title.trim();
        if title.is_empty() {
            return Err("史料书名不能为空".into());
        }
        let now = now_ms();
        let id = if input.id.trim().is_empty() { new_id("src") } else { input.id.trim().to_string() };
        let conn = lock(self)?;
        let source_type = source_type_or_default(input.source_type.trim());
        conn.execute(
            "INSERT INTO sources
              (id, work_title, section_title, author, compiler, era, source_type, edition,
               publisher, publish_year, volume, repository, url, scan_url, call_number,
               language, authenticity_note, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?18)
             ON CONFLICT(id) DO UPDATE SET
               work_title = excluded.work_title, section_title = excluded.section_title,
               author = excluded.author, compiler = excluded.compiler, era = excluded.era,
               source_type = excluded.source_type, edition = excluded.edition,
               publisher = excluded.publisher, publish_year = excluded.publish_year,
               volume = excluded.volume, repository = excluded.repository, url = excluded.url,
               scan_url = excluded.scan_url, call_number = excluded.call_number,
               language = excluded.language, authenticity_note = excluded.authenticity_note,
               updated_at = excluded.updated_at",
            params![
                id,
                title,
                input.section_title.trim(),
                input.author.trim(),
                input.compiler.trim(),
                input.era.trim(),
                source_type,
                input.edition.trim(),
                input.publisher.trim(),
                input.publish_year.trim(),
                input.volume.trim(),
                input.repository.trim(),
                input.url.trim(),
                input.scan_url.trim(),
                input.call_number.trim(),
                if input.language.trim().is_empty() { "zh" } else { input.language.trim() },
                input.authenticity_note.trim(),
                now
            ],
        )
        .map_err(db_err)?;
        Self::source_summary(&conn, &id)?.ok_or_else(|| "史料保存后读取失败".to_string())
    }

    pub fn save_passage(&self, input: PassageInput) -> Result<SourcePassage, String> {
        let original = input.original_text.trim();
        if original.is_empty() {
            return Err("原文不能为空".into());
        }
        let source_id = input.source_id.trim();
        if source_id.is_empty() {
            return Err("缺少史料 id".into());
        }
        let conn = lock(self)?;
        let exists: Option<String> = conn
            .query_row("SELECT id FROM sources WHERE id = ?1", params![source_id], |row| row.get(0))
            .optional()
            .map_err(db_err)?;
        if exists.is_none() {
            return Err("史料不存在".into());
        }
        let now = now_ms();
        let id = if input.id.trim().is_empty() { new_id("passage") } else { input.id.trim().to_string() };
        let normalized = normalize(original);
        conn.execute(
            "INSERT INTO source_passages
              (id, source_id, volume, chapter, page, original_text, normalized_text,
               translation, variant_of, scan_url, locator, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
             ON CONFLICT(id) DO UPDATE SET
               volume = excluded.volume, chapter = excluded.chapter, page = excluded.page,
               original_text = excluded.original_text, normalized_text = excluded.normalized_text,
               translation = excluded.translation, variant_of = excluded.variant_of,
               scan_url = excluded.scan_url, locator = excluded.locator",
            params![
                id,
                source_id,
                input.volume.trim(),
                input.chapter.trim(),
                input.page.trim(),
                original,
                normalized,
                input.translation.trim(),
                input.variant_of.trim(),
                input.scan_url.trim(),
                input.locator.trim(),
                now
            ],
        )
        .map_err(db_err)?;
        Ok(SourcePassage {
            id,
            source_id: source_id.to_string(),
            volume: input.volume.trim().to_string(),
            chapter: input.chapter.trim().to_string(),
            page: input.page.trim().to_string(),
            original_text: original.to_string(),
            translation: input.translation.trim().to_string(),
            variant_of: input.variant_of.trim().to_string(),
            scan_url: input.scan_url.trim().to_string(),
            locator: input.locator.trim().to_string(),
            created_at: now,
        })
    }

    pub fn save_citation(&self, input: CitationInput) -> Result<FactCitation, String> {
        let fact_id = input.fact_id.trim();
        let passage_id = input.passage_id.trim();
        if fact_id.is_empty() || passage_id.is_empty() {
            return Err("缺少事实或原文 id".into());
        }
        let conn = lock(self)?;
        let fact_exists: Option<String> = conn
            .query_row("SELECT id FROM facts WHERE id = ?1", params![fact_id], |row| row.get(0))
            .optional()
            .map_err(db_err)?;
        if fact_exists.is_none() {
            return Err("事实不存在".into());
        }
        let original: Option<String> = conn
            .query_row("SELECT original_text FROM source_passages WHERE id = ?1", params![passage_id], |row| row.get(0))
            .optional()
            .map_err(db_err)?;
        let Some(original) = original else { return Err("原文段落不存在".into()) };
        let quote = if input.quote.trim().is_empty() { original.trim().to_string() } else { input.quote.trim().to_string() };
        if !original.contains(&quote) {
            return Err("引文必须是原文段落的逐字子串；请从原文中复制".into());
        }
        let now = now_ms();
        let id = if input.id.trim().is_empty() { new_id("cite") } else { input.id.trim().to_string() };
        let relation = relation_or_default(input.relation.trim());
        conn.execute(
            "INSERT INTO citations
              (id, fact_id, passage_id, relation, quote, locator, confidence, checked_at, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)
             ON CONFLICT(fact_id, passage_id, quote) DO UPDATE SET
               relation = excluded.relation, locator = excluded.locator,
               confidence = excluded.confidence, checked_at = excluded.checked_at",
            params![id, fact_id, passage_id, relation, quote, input.locator.trim(), input.confidence, now],
        )
        .map_err(db_err)?;
        let mut statement = conn
            .prepare(
                "SELECT c.id, c.fact_id, c.passage_id, c.relation, c.quote, c.locator, c.confidence, c.checked_at,
                        s.id, s.work_title, s.section_title, s.author, s.edition, s.volume,
                        p.page, p.original_text, p.translation, s.source_type, s.url, p.scan_url
                 FROM citations c
                 JOIN source_passages p ON p.id = c.passage_id
                 JOIN sources s ON s.id = p.source_id
                 WHERE c.fact_id = ?1 AND c.passage_id = ?2 AND c.quote = ?3
                 ORDER BY c.created_at DESC LIMIT 1",
            )
            .map_err(db_err)?;
        let result = statement
            .query_row(params![fact_id, passage_id, quote], |row| {
                Ok(FactCitation {
                    id: row.get(0)?,
                    fact_id: row.get(1)?,
                    passage_id: row.get(2)?,
                    relation: row.get(3)?,
                    quote: clean_text(&row.get::<_, String>(4)?),
                    locator: row.get(5)?,
                    confidence: row.get(6)?,
                    checked_at: row.get(7)?,
                    source_id: row.get(8)?,
                    work_title: row.get(9)?,
                    section_title: row.get(10)?,
                    author: row.get(11)?,
                    edition: row.get(12)?,
                    volume: row.get(13)?,
                    page: row.get(14)?,
                    original_text: clean_text(&row.get::<_, String>(15)?),
                    translation: clean_text(&row.get::<_, String>(16)?),
                    source_type: row.get(17)?,
                    source_url: row.get(18)?,
                    scan_url: row.get(19)?,
                })
            })
            .map_err(db_err)?;
        Ok(result)
    }

    pub fn attach_citation_counts(&self, facts: &mut [Fact]) -> Result<(), String> {
        let conn = lock(self)?;
        for fact in facts {
            let count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM citations WHERE fact_id = ?1",
                    params![fact.id],
                    |row| row.get(0),
                )
                .unwrap_or(0);
            fact.citation_count = count;
        }
        Ok(())
    }

    pub fn replace_candidates(
        &self,
        fact_id: &str,
        candidates: &[HistoryCandidateInput],
    ) -> Result<Vec<HistoryCandidate>, String> {
        let now = now_ms();
        let mut conn = lock(self)?;
        let tx = conn.transaction().map_err(db_err)?;
        tx.execute("DELETE FROM history_search_candidates WHERE fact_id = ?1", params![fact_id])
            .map_err(db_err)?;
        for item in candidates {
            tx.execute(
                "INSERT INTO history_search_candidates
                  (id, fact_id, provider, title, url, snippet, authority, status, reason,
                   work_title, section_title, source_id, citation_id, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
                params![
                    new_id("cand"),
                    fact_id,
                    item.provider,
                    item.title,
                    item.url,
                    item.snippet,
                    item.authority,
                    if item.status.trim().is_empty() { "candidate" } else { item.status.trim() },
                    item.reason,
                    item.work_title,
                    item.section_title,
                    item.source_id,
                    item.citation_id,
                    now,
                ],
            )
            .map_err(db_err)?;
        }
        tx.commit().map_err(db_err)?;
        drop(conn);
        self.candidates_for_fact(fact_id)
    }

    pub fn candidates_for_fact(&self, fact_id: &str) -> Result<Vec<HistoryCandidate>, String> {
        let conn = lock(self)?;
        let mut statement = conn
            .prepare(
                "SELECT id, fact_id, provider, title, url, snippet, authority, status, reason,
                        work_title, section_title, source_id, citation_id, created_at
                 FROM history_search_candidates WHERE fact_id = ?1 ORDER BY rowid",
            )
            .map_err(db_err)?;
        let rows = statement
            .query_map(params![fact_id], |row| {
                Ok(HistoryCandidate {
                    id: row.get(0)?,
                    fact_id: row.get(1)?,
                    provider: row.get(2)?,
                    title: row.get(3)?,
                    url: row.get(4)?,
                    snippet: row.get(5)?,
                    authority: row.get(6)?,
                    status: row.get(7)?,
                    reason: row.get(8)?,
                    work_title: row.get(9)?,
                    section_title: row.get(10)?,
                    source_id: row.get(11)?,
                    citation_id: row.get(12)?,
                    created_at: row.get(13)?,
                })
            })
            .map_err(db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_err)?;
        Ok(rows)
    }

    pub fn list_candidates(&self, status: &str, limit: usize) -> Result<Vec<HistoryCandidate>, String> {
        let conn = lock(self)?;
        let mut statement = conn
            .prepare(
                "SELECT id, fact_id, provider, title, url, snippet, authority, status, reason,
                        work_title, section_title, source_id, citation_id, created_at
                 FROM history_search_candidates
                 WHERE (?1 = '' OR status = ?1)
                 ORDER BY created_at DESC, rowid DESC LIMIT ?2",
            )
            .map_err(db_err)?;
        let rows = statement
            .query_map(params![status, limit.clamp(1, 500) as i64], |row| {
                Ok(HistoryCandidate {
                    id: row.get(0)?,
                    fact_id: row.get(1)?,
                    provider: row.get(2)?,
                    title: row.get(3)?,
                    url: row.get(4)?,
                    snippet: row.get(5)?,
                    authority: row.get(6)?,
                    status: row.get(7)?,
                    reason: row.get(8)?,
                    work_title: row.get(9)?,
                    section_title: row.get(10)?,
                    source_id: row.get(11)?,
                    citation_id: row.get(12)?,
                    created_at: row.get(13)?,
                })
            })
            .map_err(db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_err)?;
        Ok(rows)
    }

    pub fn candidate_by_id(&self, id: &str) -> Result<Option<HistoryCandidate>, String> {
        let conn = lock(self)?;
        conn.query_row(
            "SELECT id, fact_id, provider, title, url, snippet, authority, status, reason,
                    work_title, section_title, source_id, citation_id, created_at
             FROM history_search_candidates WHERE id = ?1",
            params![id],
            |row| {
                Ok(HistoryCandidate {
                    id: row.get(0)?,
                    fact_id: row.get(1)?,
                    provider: row.get(2)?,
                    title: row.get(3)?,
                    url: row.get(4)?,
                    snippet: row.get(5)?,
                    authority: row.get(6)?,
                    status: row.get(7)?,
                    reason: row.get(8)?,
                    work_title: row.get(9)?,
                    section_title: row.get(10)?,
                    source_id: row.get(11)?,
                    citation_id: row.get(12)?,
                    created_at: row.get(13)?,
                })
            },
        )
        .optional()
        .map_err(db_err)
    }

    pub fn update_candidate(
        &self,
        id: &str,
        status: &str,
        reason: &str,
        source_id: &str,
        citation_id: &str,
    ) -> Result<HistoryCandidate, String> {
        let conn = lock(self)?;
        let changed = conn
            .execute(
                "UPDATE history_search_candidates
                 SET status = ?2, reason = ?3, source_id = ?4, citation_id = ?5
                 WHERE id = ?1",
                params![id, status, reason, source_id, citation_id],
            )
            .map_err(db_err)?;
        if changed == 0 {
            return Err("候选不存在".into());
        }
        drop(conn);
        self.candidate_by_id(id)?.ok_or_else(|| "候选更新后读取失败".to_string())
    }

    pub fn cache_get(&self, key: &str, max_age_ms: i64) -> Result<Option<CachedPage>, String> {
        let conn = lock(self)?;
        let row = conn
            .query_row(
                "SELECT key, provider, url, title, text, fetched_at
                 FROM history_page_cache WHERE key = ?1",
                params![key],
                |row| {
                    Ok(CachedPage {
                        key: row.get(0)?,
                        provider: row.get(1)?,
                        url: row.get(2)?,
                        title: row.get(3)?,
                        text: row.get(4)?,
                        fetched_at: row.get(5)?,
                    })
                },
            )
            .optional()
            .map_err(db_err)?;
        Ok(row.filter(|page| now_ms().saturating_sub(page.fetched_at) <= max_age_ms))
    }

    pub fn cache_put(&self, page: &CachedPage) -> Result<(), String> {
        let conn = lock(self)?;
        conn.execute(
            "INSERT INTO history_page_cache (key, provider, url, title, text, fetched_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(key) DO UPDATE SET
               provider = excluded.provider, url = excluded.url, title = excluded.title,
               text = excluded.text, fetched_at = excluded.fetched_at",
            params![page.key, page.provider, page.url, page.title, page.text, page.fetched_at],
        )
        .map_err(db_err)?;
        Ok(())
    }

    pub fn fact_citations(&self, fact_id: &str) -> Result<Vec<FactCitation>, String> {
        let conn = lock(self)?;
        let mut statement = conn
            .prepare(
                "SELECT c.id, c.fact_id, c.passage_id, c.relation, c.quote, c.locator, c.confidence, c.checked_at,
                        s.id, s.work_title, s.section_title, s.author, s.edition, s.volume,
                        p.page, p.original_text, p.translation, s.source_type, s.url, p.scan_url
                 FROM citations c
                 JOIN source_passages p ON p.id = c.passage_id
                 JOIN sources s ON s.id = p.source_id
                 WHERE c.fact_id = ?1
                 ORDER BY c.created_at,
                   CASE c.relation WHEN 'direct' THEN 0 WHEN 'supports' THEN 1 WHEN 'variant' THEN 2 WHEN 'contradicts' THEN 3 ELSE 4 END",
            )
            .map_err(db_err)?;
        let rows = statement
            .query_map(params![fact_id], |row| {
                Ok(FactCitation {
                    id: row.get(0)?,
                    fact_id: row.get(1)?,
                    passage_id: row.get(2)?,
                    relation: row.get(3)?,
                    quote: clean_text(&row.get::<_, String>(4)?),
                    locator: row.get(5)?,
                    confidence: row.get(6)?,
                    checked_at: row.get(7)?,
                    source_id: row.get(8)?,
                    work_title: row.get(9)?,
                    section_title: row.get(10)?,
                    author: row.get(11)?,
                    edition: row.get(12)?,
                    volume: row.get(13)?,
                    page: row.get(14)?,
                    original_text: clean_text(&row.get::<_, String>(15)?),
                    translation: clean_text(&row.get::<_, String>(16)?),
                    source_type: row.get(17)?,
                    source_url: row.get(18)?,
                    scan_url: row.get(19)?,
                })
            })
            .map_err(db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_err)?;
        Ok(rows)
    }

    pub fn delete_citation(&self, id: &str) -> Result<(), String> {
        let conn = lock(self)?;
        conn.execute("DELETE FROM citations WHERE id = ?1", params![id]).map_err(db_err)?;
        Ok(())
    }

    pub fn delete_passage(&self, id: &str) -> Result<(), String> {
        let conn = lock(self)?;
        let citations: i64 = conn
            .query_row("SELECT COUNT(*) FROM citations WHERE passage_id = ?1", params![id], |row| row.get(0))
            .map_err(db_err)?;
        if citations > 0 {
            return Err(format!("这段原文还有 {citations} 条引用，先解除引用再删除"));
        }
        conn.execute("DELETE FROM source_passages WHERE id = ?1", params![id]).map_err(db_err)?;
        Ok(())
    }

    pub fn delete_source(&self, id: &str) -> Result<(), String> {
        let conn = lock(self)?;
        let citations: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM citations c JOIN source_passages p ON p.id = c.passage_id WHERE p.source_id = ?1",
                params![id],
                |row| row.get(0),
            )
            .map_err(db_err)?;
        if citations > 0 {
            return Err(format!("这份史料还有 {citations} 条引用，先解除引用再删除"));
        }
        conn.execute("DELETE FROM source_passages WHERE source_id = ?1", params![id]).map_err(db_err)?;
        conn.execute("DELETE FROM sources WHERE id = ?1", params![id]).map_err(db_err)?;
        Ok(())
    }

    pub fn list_events(&self, query: &str, limit: usize) -> Result<Vec<HistoryEvent>, String> {
        let conn = lock(self)?;
        let mut statement = conn
            .prepare(
                "SELECT e.id, e.canonical_name, e.aliases_json, e.start_date, e.end_date, e.date_note,
                        e.places_json, e.actors_json, e.summary,
                        (SELECT COUNT(*) FROM facts f WHERE f.event_id = e.id),
                        (SELECT COUNT(DISTINCT p.source_id) FROM citations c JOIN source_passages p ON p.id = c.passage_id JOIN facts f ON f.id = c.fact_id WHERE f.event_id = e.id),
                        (SELECT COUNT(DISTINCT m.note_id) FROM note_fact_mentions m JOIN facts f ON f.id = m.fact_id WHERE f.event_id = e.id),
                        e.created_at, e.updated_at
                 FROM history_events e
                 WHERE (?1 = '' OR e.canonical_name LIKE '%' || ?1 || '%' OR e.aliases_json LIKE '%' || ?1 || '%')
                 ORDER BY e.updated_at DESC LIMIT ?2",
            )
            .map_err(db_err)?;
        let rows = statement
            .query_map(params![query.trim(), limit.clamp(1, 300) as i64], |row| {
                Ok(HistoryEvent {
                    id: row.get(0)?,
                    canonical_name: row.get(1)?,
                    aliases: json_list(&row.get::<_, String>(2)?),
                    start_date: row.get(3)?,
                    end_date: row.get(4)?,
                    date_note: row.get(5)?,
                    places: json_list(&row.get::<_, String>(6)?),
                    actors: json_list(&row.get::<_, String>(7)?),
                    summary: row.get(8)?,
                    fact_count: row.get(9)?,
                    source_count: row.get(10)?,
                    note_count: row.get(11)?,
                    created_at: row.get(12)?,
                    updated_at: row.get(13)?,
                })
            })
            .map_err(db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_err)?;
        Ok(rows)
    }

    pub fn get_event(&self, id: &str) -> Result<Option<EventDetail>, String> {
        let conn = lock(self)?;
        let event = conn
            .query_row(
                "SELECT e.id, e.canonical_name, e.aliases_json, e.start_date, e.end_date, e.date_note,
                        e.places_json, e.actors_json, e.summary,
                        (SELECT COUNT(*) FROM facts f WHERE f.event_id = e.id),
                        (SELECT COUNT(DISTINCT p.source_id) FROM citations c JOIN source_passages p ON p.id = c.passage_id JOIN facts f ON f.id = c.fact_id WHERE f.event_id = e.id),
                        (SELECT COUNT(DISTINCT m.note_id) FROM note_fact_mentions m JOIN facts f ON f.id = m.fact_id WHERE f.event_id = e.id),
                        e.created_at, e.updated_at
                 FROM history_events e WHERE e.id = ?1",
                params![id],
                |row| {
                    Ok(HistoryEvent {
                        id: row.get(0)?,
                        canonical_name: row.get(1)?,
                        aliases: json_list(&row.get::<_, String>(2)?),
                        start_date: row.get(3)?,
                        end_date: row.get(4)?,
                        date_note: row.get(5)?,
                        places: json_list(&row.get::<_, String>(6)?),
                        actors: json_list(&row.get::<_, String>(7)?),
                        summary: row.get(8)?,
                        fact_count: row.get(9)?,
                        source_count: row.get(10)?,
                        note_count: row.get(11)?,
                        created_at: row.get(12)?,
                        updated_at: row.get(13)?,
                    })
                },
            )
            .optional()
            .map_err(db_err)?;
        let Some(event) = event else { return Ok(None) };
        let mut fact_stmt = conn
            .prepare(
                "SELECT id, text, status, historical_status, claim_kind FROM facts WHERE event_id = ?1 ORDER BY updated_at DESC",
            )
            .map_err(db_err)?;
        let facts = fact_stmt
            .query_map(params![id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            })
            .map_err(db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_err)?;
        drop(fact_stmt);
        let mut facts_out = Vec::new();
        let mut note_ids = Vec::new();
        for (fact_id, text, status, historical_status, claim_kind) in facts {
            let notes = note_ids_for_fact(&conn, &fact_id)?;
            for note_id in &notes {
                if !note_ids.contains(note_id) {
                    note_ids.push(note_id.clone());
                }
            }
            facts_out.push(EventFact { id: fact_id, text, status, historical_status, claim_kind, note_ids: notes });
        }
        let mut source_stmt = conn
            .prepare(
                "SELECT DISTINCT s.id FROM sources s
                 JOIN source_passages p ON p.source_id = s.id
                 JOIN citations c ON c.passage_id = p.id
                 JOIN facts f ON f.id = c.fact_id
                 WHERE f.event_id = ?1",
            )
            .map_err(db_err)?;
        let source_ids = source_stmt
            .query_map(params![id], |row| row.get::<_, String>(0))
            .map_err(db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_err)?;
        drop(source_stmt);
        let mut sources = Vec::new();
        for source_id in source_ids {
            if let Some(source) = Self::source_summary(&conn, &source_id)? {
                sources.push(source);
            }
        }
        Ok(Some(EventDetail { event, facts: facts_out, sources, note_ids }))
    }

    pub fn save_event(&self, input: EventInput) -> Result<HistoryEvent, String> {
        let name = input.canonical_name.trim();
        if name.is_empty() {
            return Err("事件名不能为空".into());
        }
        let now = now_ms();
        let id = if input.id.trim().is_empty() { new_id("event") } else { input.id.trim().to_string() };
        let conn = lock(self)?;
        conn.execute(
            "INSERT INTO history_events
              (id, canonical_name, aliases_json, start_date, end_date, date_note,
               places_json, actors_json, authority_json, summary, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, '{}', ?9, ?10, ?10)
             ON CONFLICT(id) DO UPDATE SET
               canonical_name = excluded.canonical_name, aliases_json = excluded.aliases_json,
               start_date = excluded.start_date, end_date = excluded.end_date,
               date_note = excluded.date_note, places_json = excluded.places_json,
               actors_json = excluded.actors_json, summary = excluded.summary,
               updated_at = excluded.updated_at",
            params![
                id,
                name,
                serde_json::to_string(&input.aliases).map_err(|error| error.to_string())?,
                input.start_date.trim(),
                input.end_date.trim(),
                input.date_note.trim(),
                serde_json::to_string(&input.places).map_err(|error| error.to_string())?,
                serde_json::to_string(&input.actors).map_err(|error| error.to_string())?,
                input.summary.trim(),
                now
            ],
        )
        .map_err(db_err)?;
        drop(conn);
        self.get_event(&id)?
            .map(|detail| detail.event)
            .ok_or_else(|| "事件保存后读取失败".to_string())
    }

    pub fn set_fact_event(&self, fact_id: &str, event_id: &str) -> Result<(), String> {
        let conn = lock(self)?;
        if !event_id.trim().is_empty() {
            let exists: Option<String> = conn
                .query_row("SELECT id FROM history_events WHERE id = ?1", params![event_id], |row| row.get(0))
                .optional()
                .map_err(db_err)?;
            if exists.is_none() {
                return Err("事件不存在".into());
            }
        }
        let domain = if event_id.trim().is_empty() { "general" } else { "history" };
        conn.execute(
            "UPDATE facts SET event_id = ?2, domain = ?3, updated_at = ?4 WHERE id = ?1",
            params![fact_id, event_id.trim(), domain, now_ms()],
        )
        .map_err(db_err)?;
        Ok(())
    }

    pub fn set_fact_history(&self, fact_id: &str, historical_status: &str, claim_kind: &str) -> Result<(), String> {
        let status = historical_status_or_empty(historical_status.trim());
        if !historical_status.trim().is_empty() && status.is_empty() {
            return Err("未知的历史状态".into());
        }
        let conn = lock(self)?;
        conn.execute(
            "UPDATE facts SET historical_status = ?2, claim_kind = ?3,
               domain = CASE WHEN ?2 = '' THEN domain ELSE 'history' END,
               updated_at = ?4 WHERE id = ?1",
            params![fact_id, status, claim_kind.trim(), now_ms()],
        )
        .map_err(db_err)?;
        Ok(())
    }
}

fn note_ids_for_fact(conn: &Connection, fact_id: &str) -> Result<Vec<String>, String> {
    let mut statement = conn
        .prepare("SELECT DISTINCT note_id FROM note_fact_mentions WHERE fact_id = ?1 ORDER BY note_id")
        .map_err(db_err)?;
    let rows = statement
        .query_map(params![fact_id], |row| row.get::<_, String>(0))
        .map_err(db_err)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(db_err)?;
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::production::facts::{FactEvidenceInput, FactStore};

    fn temp_dir(prefix: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("yebian-history-{}-{}", prefix, Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn source_input(title: &str) -> SourceInput {
        SourceInput {
            work_title: title.into(),
            author: "陈寿".into(),
            era: "西晋".into(),
            source_type: "primary".into(),
            edition: "中华书局点校本".into(),
            volume: "卷五十四".into(),
            ..Default::default()
        }
    }

    #[test]
    fn candidates_round_trip_and_replace() {
        let dir = temp_dir("candidates");
        let path = dir.join("history.sqlite");
        let facts = FactStore::open(&path).unwrap();
        let fact_id = facts.record_candidate("note-a", "run-a", "测试候选事实。", "", "", &[]).unwrap();
        let history = HistoryStore::open(&path).unwrap();
        let first = HistoryCandidateInput {
            provider: "ctext".into(),
            title: "明史".into(),
            url: "https://ctext.org/wb410835".into(),
            authority: "link_only".into(),
            status: "candidate".into(),
            reason: "需要 API Key".into(),
            work_title: "明史".into(),
            ..Default::default()
        };
        let second = HistoryCandidateInput {
            provider: "Wikisource".into(),
            title: "明史/卷三百八".into(),
            url: "https://zh.wikisource.org/wiki/明史/卷三百八".into(),
            authority: "transcription".into(),
            status: "cited".into(),
            reason: "已自动引用".into(),
            work_title: "明史".into(),
            section_title: "卷三百八".into(),
            ..Default::default()
        };
        let saved = history.replace_candidates(&fact_id, &[first, second]).unwrap();
        assert_eq!(saved.len(), 2);
        assert_eq!(saved[0].provider, "ctext");
        assert_eq!(saved[1].status, "cited");
        let replaced = history
            .replace_candidates(
                &fact_id,
                &[HistoryCandidateInput {
                    provider: "NLC".into(),
                    title: "中华古籍资源库：明史".into(),
                    authority: "link_only".into(),
                    status: "candidate".into(),
                    ..Default::default()
                }],
            )
            .unwrap();
        assert_eq!(replaced.len(), 1);
        assert_eq!(replaced[0].provider, "NLC");
    }

    #[test]
    fn page_cache_round_trip_and_expiry() {
        let dir = temp_dir("cache");
        let path = dir.join("history.sqlite");
        let _facts = FactStore::open(&path).unwrap();
        let history = HistoryStore::open(&path).unwrap();
        let page = CachedPage {
            key: "Wikisource:明史/卷三百八".into(),
            provider: "Wikisource".into(),
            url: "https://zh.wikisource.org/wiki/明史/卷三百八".into(),
            title: "明史/卷三百八".into(),
            text: "严嵩无他才略，唯一意媚上，窃权罔利。".into(),
            fetched_at: now_ms(),
        };
        history.cache_put(&page).unwrap();
        let cached = history.cache_get(&page.key, 60_000).unwrap().unwrap();
        assert_eq!(cached.text, page.text);
        assert!(history.cache_get(&page.key, -1).unwrap().is_none());
    }

    #[test]
    fn source_passage_citation_round_trip_and_quote_guard() {
        let dir = temp_dir("citation");
        let path = dir.join("history.sqlite");
        let facts = FactStore::open(&path).unwrap();
        let fact_id = facts.record_candidate("note-a", "run-a", "周瑜判断曹军不过十五六万。", "", "", &[]).unwrap();
        let history = HistoryStore::open(&path).unwrap();
        let source = history.save_source(source_input("三国志")).unwrap();
        let passage = history
            .save_passage(PassageInput {
                source_id: source.id.clone(),
                chapter: "吴书·周瑜传".into(),
                page: "1262".into(),
                original_text: "今以实校之，彼所将中国人不过十五六万。".into(),
                ..Default::default()
            })
            .unwrap();
        let citation = history
            .save_citation(CitationInput {
                fact_id: fact_id.clone(),
                passage_id: passage.id.clone(),
                relation: "supports".into(),
                quote: "不过十五六万".into(),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(citation.relation, "supports");
        let cited = history.fact_citations(&fact_id).unwrap();
        assert_eq!(cited.len(), 1);
        assert_eq!(cited[0].work_title, "三国志");
        let detail = history.get_source(&source.id).unwrap().expect("source");
        assert_eq!(detail.passages.len(), 1);
        assert_eq!(detail.facts.len(), 1);
        assert!(history
            .save_citation(CitationInput {
                fact_id,
                passage_id: passage.id,
                relation: "supports".into(),
                quote: "这句话不在原文里".into(),
                ..Default::default()
            })
            .is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn event_links_facts_and_sources() {
        let dir = temp_dir("event");
        let path = dir.join("history.sqlite");
        let facts = FactStore::open(&path).unwrap();
        let fact_id = facts.record_candidate("note-a", "run-a", "赤壁之战发生于建安十三年。", "", "", &[]).unwrap();
        let history = HistoryStore::open(&path).unwrap();
        let event = history
            .save_event(EventInput {
                canonical_name: "赤壁之战".into(),
                aliases: vec!["赤壁".into()],
                start_date: "208".into(),
                ..Default::default()
            })
            .unwrap();
        history.set_fact_event(&fact_id, &event.id).unwrap();
        let detail = history.get_event(&event.id).unwrap().expect("event");
        assert_eq!(detail.event.canonical_name, "赤壁之战");
        assert_eq!(detail.facts.len(), 1);
        assert_eq!(detail.facts[0].note_ids, vec!["note-a"]);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn legacy_web_evidence_is_not_imported_as_history_sources() {
        let dir = temp_dir("migration");
        let path = dir.join("history.sqlite");
        let facts = FactStore::open(&path).unwrap();
        let fact_id = facts.record_candidate("note-a", "run-a", "测试迁移事实。", "", "", &[]).unwrap();
        facts
            .record_verification(
                &fact_id,
                "外部可印证",
                "deepseek",
                "deepseek-v4-flash",
                &[],
                &[FactEvidenceInput {
                    source_url: "https://example.com/report".into(),
                    title: "report".into(),
                    quote: "the report confirms it".into(),
                    relation: "supports".into(),
                }],
            )
            .unwrap();
        let history = HistoryStore::open(&path).unwrap();
        assert!(history.list_sources("", "", 10).unwrap().is_empty());
        assert!(history.fact_citations(&fact_id).unwrap().is_empty());
        assert_eq!(facts.get_fact(&fact_id).unwrap().expect("fact").evidence.len(), 1);
        let _ = std::fs::remove_dir_all(dir);
    }
}
