//! 事实库：事实、笔记提及、来源、证据、核验记录。
//!
//! 事实是独立实体；一篇笔记“提及”事实，事实再和来源/证据关联。
//! 这里的 normalized 文本只用于精确去重，不把语义相似或数字冲突自动合并。

use std::collections::HashSet;
use std::path::Path;
use std::sync::{Arc, Mutex};

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use url::Url;
use uuid::Uuid;

use super::store::now_ms;

#[derive(Clone)]
pub struct FactStore {
    conn: Arc<Mutex<Connection>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FactMention {
    pub note_id: String,
    pub run_id: String,
    pub quote: String,
    pub section: String,
    pub block_id: String,
    pub stance: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FactEvidence {
    pub url: String,
    pub title: String,
    pub domain: String,
    pub relation: String,
    pub quote: String,
    pub checked_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FactRelation {
    pub other_id: String,
    pub text: String,
    pub status: String,
    pub relation: String,
    pub confidence: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Fact {
    pub id: String,
    pub text: String,
    pub status: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub last_checked_at: i64,
    pub queries: Vec<String>,
    pub note_count: i64,
    pub domain: String,
    pub event_id: String,
    pub historical_status: String,
    pub claim_kind: String,
    pub citation_count: i64,
    pub mentions: Vec<FactMention>,
    pub evidence: Vec<FactEvidence>,
    pub related: Vec<FactRelation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FactCandidate {
    pub id: String,
    pub text: String,
    pub status: String,
    pub relation: String,
    pub confidence: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FactCheck {
    pub id: String,
    pub verdict: String,
    pub provider: String,
    pub model: String,
    pub queries: Vec<String>,
    pub evidence_count: usize,
    pub checked_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FactMerge {
    pub id: String,
    pub target_fact_id: String,
    pub source_fact_id: String,
    pub target_text: String,
    pub source_text: String,
    pub source_status: String,
    pub merged_at: i64,
    pub undone_at: i64,
    pub can_undo: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FactConflictResolution {
    pub id: String,
    pub fact_a: String,
    pub fact_b: String,
    pub resolution: String,
    pub note: String,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FactReview {
    pub fact_id: String,
    pub status: String,
    pub note: String,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FactSplitChild {
    pub id: String,
    pub text: String,
    pub status: String,
    pub existed: bool,
    pub evidence_count: i64,
    pub citation_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FactSplit {
    pub id: String,
    pub parent_fact_id: String,
    pub parent_text: String,
    /// Original status before the split, kept for audit.
    pub parent_status: String,
    pub children: Vec<FactSplitChild>,
    pub split_at: i64,
    pub undone_at: i64,
    pub can_undo: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ConflictResolutionInput {
    #[serde(default)]
    pub fact_a: String,
    #[serde(default)]
    pub fact_b: String,
    #[serde(default)]
    pub resolution: String,
    #[serde(default)]
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FactEvidenceInput {
    pub source_url: String,
    pub title: String,
    pub quote: String,
    pub relation: String,
}

fn lock(store: &FactStore) -> Result<std::sync::MutexGuard<'_, Connection>, String> {
    store.conn.lock().map_err(|_| "facts database lock poisoned".to_string())
}

fn db_err(error: rusqlite::Error) -> String {
    error.to_string()
}

fn normalize(text: &str) -> String {
    text.chars()
        .filter(|ch| ch.is_alphanumeric() || ('\u{4e00}'..='\u{9fff}').contains(ch))
        .flat_map(|ch| ch.to_lowercase())
        .collect()
}

fn clean_quote(value: &str) -> String {
    let mut without_sup = String::with_capacity(value.len());
    let lower = value.to_ascii_lowercase();
    let mut cursor = 0usize;
    while let Some(relative_start) = lower[cursor..].find("<sup") {
        let start = cursor + relative_start;
        without_sup.push_str(&value[cursor..start]);
        if let Some(relative_end) = lower[start..].find("</sup>") {
            cursor = start + relative_end + 6;
        } else {
            cursor = value.len();
            break;
        }
    }
    without_sup.push_str(&value[cursor..]);
    let value = without_sup;
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

fn fact_id_for(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(normalize(text).as_bytes());
    format!("fact_{}", hex::encode(&hasher.finalize()[..12]))
}

fn source_id_for(url: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(url.trim().to_lowercase().as_bytes());
    format!("src_{}", hex::encode(&hasher.finalize()[..12]))
}

fn evidence_id_for(fact_id: &str, source_id: &str, quote: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(fact_id.as_bytes());
    hasher.update(b"\0");
    hasher.update(source_id.as_bytes());
    hasher.update(b"\0");
    hasher.update(normalize(quote).as_bytes());
    format!("ev_{}", hex::encode(&hasher.finalize()[..12]))
}

fn domain_of(url: &str) -> String {
    Url::parse(url)
        .ok()
        .and_then(|value| value.host_str().map(str::to_string))
        .unwrap_or_default()
}

fn status_for_verification(verdict: &str, evidence: &[FactEvidenceInput]) -> &'static str {
    if verdict.contains("非权威") {
        return "candidate";
    }
    if evidence.iter().any(|item| item.relation == "contradicts") {
        return "disputed";
    }
    if verdict.contains("可印证") || evidence.iter().any(|item| item.relation == "supports") {
        return "verified";
    }
    if verdict.contains("未找到") {
        "unsupported"
    } else {
        "candidate"
    }
}

fn status_for_verdict(verdict: &str) -> &'static str {
    if verdict.contains("可印证") {
        "verified"
    } else if verdict.contains("争议") {
        "disputed"
    } else if verdict.contains("未找到") {
        "unsupported"
    } else {
        "candidate"
    }
}

fn count_rows(conn: &Connection, sql: &str, id: &str) -> i64 {
    conn.query_row(sql, params![id], |row| row.get(0)).unwrap_or(0)
}

fn split_snapshot_children(snapshot: &Value) -> Vec<Value> {
    snapshot.get("children").and_then(Value::as_array).cloned().unwrap_or_default()
}

/// Children of an active split are verification units, not reading units.
/// The note rail keeps the parent composite fact and hides these children.
fn active_split_child_ids(conn: &Connection) -> Result<HashSet<String>, String> {
    let mut statement = conn
        .prepare("SELECT child_ids_json FROM fact_splits WHERE undone_at = 0")
        .map_err(db_err)?;
    let rows = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(db_err)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(db_err)?;
    let mut output = HashSet::new();
    for raw in rows {
        if let Ok(ids) = serde_json::from_str::<Vec<String>>(&raw) {
            output.extend(ids);
        }
    }
    Ok(output)
}

/// A split can be undone only while every newly-created child is untouched.
/// Existing atomic facts are never deleted, so they do not block undo.
fn split_can_undo(conn: &Connection, snapshot: &Value) -> bool {
    for child in split_snapshot_children(snapshot) {
        if child.get("existed").and_then(Value::as_bool).unwrap_or(false) {
            continue;
        }
        let Some(child_id) = child.get("id").and_then(Value::as_str) else { return false };
        let exists: Option<String> = conn
            .query_row("SELECT id FROM facts WHERE id = ?1", params![child_id], |row| row.get(0))
            .optional()
            .unwrap_or(None);
        if exists.is_none() {
            return false;
        }
        let inserted_mentions = child
            .get("mentions")
            .and_then(Value::as_array)
            .map(|items| items.len() as i64)
            .unwrap_or(0);
        let untouched = count_rows(conn, "SELECT COUNT(*) FROM note_fact_mentions WHERE fact_id = ?1", child_id) <= inserted_mentions
            && count_rows(conn, "SELECT COUNT(*) FROM fact_evidence WHERE fact_id = ?1", child_id) == 0
            && count_rows(conn, "SELECT COUNT(*) FROM citations WHERE fact_id = ?1", child_id) == 0
            && count_rows(conn, "SELECT COUNT(*) FROM fact_checks WHERE fact_id = ?1", child_id) == 0
            && count_rows(conn, "SELECT COUNT(*) FROM fact_reviews WHERE fact_id = ?1", child_id) == 0
            && count_rows(conn, "SELECT COUNT(*) FROM fact_relations WHERE fact_id = ?1 OR other_fact_id = ?1", child_id) == 0
            && count_rows(conn, "SELECT COUNT(*) FROM history_search_candidates WHERE fact_id = ?1", child_id) == 0;
        if !untouched {
            return false;
        }
    }
    true
}

fn load_split(conn: &Connection, split_id: &str) -> Result<Option<FactSplit>, String> {
    let row = conn
        .query_row(
            "SELECT id, parent_fact_id, child_ids_json, parent_status, snapshot_json, split_at, undone_at
             FROM fact_splits WHERE id = ?1",
            params![split_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, i64>(6)?,
                ))
            },
        )
        .optional()
        .map_err(db_err)?;
    let Some((id, parent_id, child_ids_json, parent_status, snapshot_json, split_at, undone_at)) = row else {
        return Ok(None);
    };
    let parent_text = conn
        .query_row("SELECT text FROM facts WHERE id = ?1", params![parent_id], |row| row.get(0))
        .optional()
        .map_err(db_err)?
        .unwrap_or_default();
    let child_ids: Vec<String> = serde_json::from_str(&child_ids_json).unwrap_or_default();
    let snapshot: Value = serde_json::from_str(&snapshot_json).unwrap_or_else(|_| json!({}));
    let snapshots = split_snapshot_children(&snapshot);
    let mut children = Vec::new();
    for child_id in child_ids {
        let state = conn
            .query_row(
                "SELECT text, status FROM facts WHERE id = ?1",
                params![child_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()
            .map_err(db_err)?
            .unwrap_or_default();
        let existed = snapshots.iter().any(|item| {
            item.get("id").and_then(Value::as_str) == Some(child_id.as_str())
                && item.get("existed").and_then(Value::as_bool).unwrap_or(false)
        });
        children.push(FactSplitChild {
            id: child_id.clone(),
            text: state.0,
            status: state.1,
            existed,
            evidence_count: count_rows(conn, "SELECT COUNT(*) FROM fact_evidence WHERE fact_id = ?1", &child_id),
            citation_count: count_rows(conn, "SELECT COUNT(*) FROM citations WHERE fact_id = ?1", &child_id),
        });
    }
    let can_undo = undone_at == 0 && split_can_undo(conn, &snapshot);
    Ok(Some(FactSplit {
        id,
        parent_fact_id: parent_id,
        parent_text,
        parent_status,
        children,
        split_at,
        undone_at,
        can_undo,
    }))
}

fn bigrams(value: &str) -> HashSet<String> {
    let chars = value.chars().collect::<Vec<_>>();
    chars
        .windows(2)
        .map(|window| window.iter().collect::<String>())
        .collect()
}

fn similarity(left: &str, right: &str) -> f64 {
    let left = bigrams(left);
    let right = bigrams(right);
    if left.is_empty() || right.is_empty() {
        return 0.0;
    }
    let common = left.intersection(&right).count();
    (2.0 * common as f64) / (left.len() + right.len()) as f64
}

const RELATED_THRESHOLD: f64 = 0.34;
const SAME_AS_THRESHOLD: f64 = 0.62;
const CONFLICT_THRESHOLD: f64 = 0.55;
const MAX_RELATED: usize = 3;

fn numbers(value: &str) -> HashSet<String> {
    let mut output = HashSet::new();
    let mut current = String::new();
    for ch in value.chars() {
        if ch.is_ascii_digit() {
            current.push(ch);
        } else if !current.is_empty() {
            output.insert(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        output.insert(current);
    }
    output
}

/// 词面关系只是候选：same_as 供人工合并，contradicts 只提示数值不一致。
fn relation_for(left: &str, right: &str) -> Option<(&'static str, f64)> {
    let confidence = similarity(left, right);
    if confidence < RELATED_THRESHOLD {
        return None;
    }
    let left_numbers = numbers(left);
    let right_numbers = numbers(right);
    let numeric_conflict = !left_numbers.is_empty()
        && !right_numbers.is_empty()
        && left_numbers != right_numbers;
    let relation = if numeric_conflict && confidence >= CONFLICT_THRESHOLD {
        "contradicts"
    } else if confidence >= SAME_AS_THRESHOLD {
        "same_as"
    } else {
        "related"
    };
    Some((relation, confidence))
}

fn link_related(conn: &Connection, fact_id: &str, normalized: &str, now: i64) -> Result<(), String> {
    let mut statement = conn
        .prepare("SELECT id, normalized FROM facts WHERE id != ?1")
        .map_err(db_err)?;
    let rows = statement
        .query_map(params![fact_id], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))
        .map_err(db_err)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(db_err)?;
    drop(statement);
    let mut candidates = rows
        .into_iter()
        .filter_map(|(other_id, other_normalized)| {
            relation_for(normalized, &other_normalized).map(|(relation, confidence)| (confidence, relation, other_id))
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    for (confidence, relation, other_id) in candidates.into_iter().take(MAX_RELATED) {
        conn.execute(
            "INSERT OR IGNORE INTO fact_relations
              (id, fact_id, other_fact_id, relation, confidence, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                format!("rel_{}", Uuid::new_v4().simple()),
                fact_id,
                other_id,
                relation,
                confidence,
                now
            ],
        )
        .map_err(db_err)?;
    }
    Ok(())
}

fn related_for(conn: &Connection, fact_id: &str) -> Result<Vec<FactRelation>, String> {
    let mut statement = conn
        .prepare(
            "SELECT r.other_fact_id, f.text, f.status, r.relation, r.confidence
             FROM fact_relations r JOIN facts f ON f.id = r.other_fact_id
             WHERE r.fact_id = ?1
             UNION
             SELECT r.fact_id, f.text, f.status, r.relation, r.confidence
             FROM fact_relations r JOIN facts f ON f.id = r.fact_id
             WHERE r.other_fact_id = ?1
             ORDER BY confidence DESC LIMIT 8",
        )
        .map_err(db_err)?;
    let rows = statement
        .query_map(params![fact_id], |row| {
            Ok(FactRelation {
                other_id: row.get(0)?,
                text: row.get(1)?,
                status: row.get(2)?,
                relation: row.get(3)?,
                confidence: row.get(4)?,
            })
        })
        .map_err(db_err)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(db_err)?;
    Ok(rows)
}

impl FactStore {
    pub fn open(path: &Path) -> Result<Self, String> {
        let conn = Connection::open(path).map_err(db_err)?;
        conn.execute_batch(
            "
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;
            PRAGMA busy_timeout = 30000;

            CREATE TABLE IF NOT EXISTS facts (
              id TEXT PRIMARY KEY,
              text TEXT NOT NULL,
              normalized TEXT NOT NULL UNIQUE,
              status TEXT NOT NULL DEFAULT 'candidate',
              queries_json TEXT NOT NULL DEFAULT '[]',
              created_at INTEGER NOT NULL,
              updated_at INTEGER NOT NULL,
              last_checked_at INTEGER NOT NULL DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS idx_facts_status ON facts(status, updated_at DESC);

            CREATE TABLE IF NOT EXISTS note_fact_mentions (
              id TEXT PRIMARY KEY,
              fact_id TEXT NOT NULL,
              note_id TEXT NOT NULL,
              run_id TEXT NOT NULL DEFAULT '',
              quote TEXT NOT NULL DEFAULT '',
              section TEXT NOT NULL DEFAULT '',
              block_id TEXT NOT NULL DEFAULT '',
              stance TEXT NOT NULL DEFAULT 'asserts',
              created_at INTEGER NOT NULL,
              UNIQUE(fact_id, note_id, quote)
            );
            CREATE INDEX IF NOT EXISTS idx_note_fact_mentions_note
              ON note_fact_mentions(note_id, created_at);

            CREATE TABLE IF NOT EXISTS fact_sources (
              id TEXT PRIMARY KEY,
              url TEXT NOT NULL UNIQUE,
              title TEXT NOT NULL DEFAULT '',
              domain TEXT NOT NULL DEFAULT '',
              created_at INTEGER NOT NULL,
              updated_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS fact_evidence (
              id TEXT PRIMARY KEY,
              fact_id TEXT NOT NULL,
              source_id TEXT NOT NULL,
              relation TEXT NOT NULL,
              quote TEXT NOT NULL DEFAULT '',
              created_at INTEGER NOT NULL,
              UNIQUE(fact_id, source_id, quote)
            );
            CREATE INDEX IF NOT EXISTS idx_fact_evidence_fact ON fact_evidence(fact_id, created_at);

            CREATE TABLE IF NOT EXISTS fact_checks (
              id TEXT PRIMARY KEY,
              fact_id TEXT NOT NULL,
              verdict TEXT NOT NULL,
              provider TEXT NOT NULL DEFAULT '',
              model TEXT NOT NULL DEFAULT '',
              queries_json TEXT NOT NULL DEFAULT '[]',
              evidence_json TEXT NOT NULL DEFAULT '[]',
              checked_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_fact_checks_fact ON fact_checks(fact_id, checked_at DESC);

            CREATE TABLE IF NOT EXISTS fact_relations (
              id TEXT PRIMARY KEY,
              fact_id TEXT NOT NULL,
              other_fact_id TEXT NOT NULL,
              relation TEXT NOT NULL,
              confidence REAL NOT NULL DEFAULT 0,
              created_at INTEGER NOT NULL,
              UNIQUE(fact_id, other_fact_id, relation)
            );

            CREATE TABLE IF NOT EXISTS fact_merges (
              id TEXT PRIMARY KEY,
              target_fact_id TEXT NOT NULL,
              source_fact_id TEXT NOT NULL,
              target_text TEXT NOT NULL DEFAULT '',
              source_text TEXT NOT NULL DEFAULT '',
              source_status TEXT NOT NULL DEFAULT '',
              snapshot_json TEXT NOT NULL DEFAULT '{}',
              merged_at INTEGER NOT NULL,
              undone_at INTEGER NOT NULL DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS idx_fact_merges_target
              ON fact_merges(target_fact_id, merged_at DESC);

            CREATE TABLE IF NOT EXISTS fact_splits (
              id TEXT PRIMARY KEY,
              parent_fact_id TEXT NOT NULL,
              child_ids_json TEXT NOT NULL DEFAULT '[]',
              parent_status TEXT NOT NULL DEFAULT '',
              snapshot_json TEXT NOT NULL DEFAULT '{}',
              split_at INTEGER NOT NULL,
              undone_at INTEGER NOT NULL DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS idx_fact_splits_parent
              ON fact_splits(parent_fact_id, split_at DESC);

            CREATE TABLE IF NOT EXISTS fact_conflict_resolutions (
              id TEXT PRIMARY KEY,
              fact_a TEXT NOT NULL,
              fact_b TEXT NOT NULL,
              resolution TEXT NOT NULL,
              note TEXT NOT NULL DEFAULT '',
              created_at INTEGER NOT NULL,
              updated_at INTEGER NOT NULL,
              UNIQUE(fact_a, fact_b)
            );

            CREATE TABLE IF NOT EXISTS fact_reviews (
              fact_id TEXT PRIMARY KEY,
              status TEXT NOT NULL,
              note TEXT NOT NULL DEFAULT '',
              updated_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS fact_meta (
              key TEXT PRIMARY KEY,
              value TEXT NOT NULL
            );
            ",
        )
        .map_err(db_err)?;
        let store = Self { conn: Arc::new(Mutex::new(conn)) };
        // Old traces were seeded without run_id and across all historical runs.
        // Rebuild once from the latest trace of each note; later runs keep facts.
        let needs_seed = {
            let conn = lock(&store)?;
            let _ = conn.execute(
                "ALTER TABLE note_fact_mentions ADD COLUMN run_id TEXT NOT NULL DEFAULT ''",
                [],
            );
            let _ = conn.execute(
                "ALTER TABLE facts ADD COLUMN queries_json TEXT NOT NULL DEFAULT '[]'",
                [],
            );
            let _ = conn.execute(
                "ALTER TABLE fact_merges ADD COLUMN undone_at INTEGER NOT NULL DEFAULT 0",
                [],
            );
            let _ = conn.execute("ALTER TABLE facts ADD COLUMN domain TEXT NOT NULL DEFAULT 'general'", []);
            let _ = conn.execute("ALTER TABLE facts ADD COLUMN event_id TEXT NOT NULL DEFAULT ''", []);
            let _ = conn.execute("ALTER TABLE facts ADD COLUMN historical_status TEXT NOT NULL DEFAULT ''", []);
            let _ = conn.execute("ALTER TABLE facts ADD COLUMN claim_kind TEXT NOT NULL DEFAULT ''", []);
            let version: Option<String> = conn
                .query_row(
                    "SELECT value FROM fact_meta WHERE key = 'seed_version'",
                    [],
                    |row| row.get(0),
                )
                .optional()
                .map_err(db_err)?;
            version.as_deref() != Some("2")
        };
        if needs_seed {
            {
                let conn = lock(&store)?;
                let _ = conn.execute("DELETE FROM fact_relations", []);
                let _ = conn.execute("DELETE FROM fact_evidence", []);
                let _ = conn.execute("DELETE FROM note_fact_mentions", []);
                let _ = conn.execute("DELETE FROM facts", []);
                let _ = conn.execute("DELETE FROM fact_sources", []);
            }
            let _ = store.seed_existing_traces();
            if let Ok(conn) = lock(&store) {
                let _ = conn.execute(
                    "INSERT INTO fact_meta (key, value) VALUES ('seed_version', '2')
                     ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                    [],
                );
            }
        }
        let needs_relations = {
            let conn = lock(&store)?;
            let version: Option<String> = conn
                .query_row(
                    "SELECT value FROM fact_meta WHERE key = 'relation_version'",
                    [],
                    |row| row.get(0),
                )
                .optional()
                .map_err(db_err)?;
            version.as_deref() != Some("3")
        };
        if needs_relations {
            store.rebuild_relations()?;
            if let Ok(conn) = lock(&store) {
                let _ = conn.execute(
                    "INSERT INTO fact_meta (key, value) VALUES ('relation_version', '3')
                     ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                    [],
                );
            }
        }
        Ok(store)
    }

    pub fn record_candidate(
        &self,
        note_id: &str,
        run_id: &str,
        claim: &str,
        source_quote: &str,
        section: &str,
        queries: &[String],
    ) -> Result<String, String> {
        self.record_candidate_with_domain(note_id, run_id, claim, source_quote, section, queries, "general")
    }

    /// `domain` 只有 `history` 和 `general` 两档。
    /// 历史事实走史料引用；普通事实只走网页证据，两者在 UI 上分开。
    pub fn record_candidate_with_domain(
        &self,
        note_id: &str,
        run_id: &str,
        claim: &str,
        source_quote: &str,
        section: &str,
        queries: &[String],
        domain: &str,
    ) -> Result<String, String> {
        let claim = claim.trim();
        if claim.is_empty() {
            return Err("事实为空".into());
        }
        let normalized = normalize(claim);
        if normalized.is_empty() {
            return Err("事实没有有效字符".into());
        }
        let domain = if domain == "history" { "history" } else { "general" };
        let id = fact_id_for(claim);
        let now = now_ms();
        let mut conn = lock(self)?;
        let tx = conn.transaction().map_err(db_err)?;
        tx.execute(
            "INSERT OR IGNORE INTO facts
              (id, text, normalized, status, queries_json, domain, created_at, updated_at, last_checked_at)
             VALUES (?1, ?2, ?3, 'candidate', ?4, ?5, ?6, ?6, 0)",
            params![
                id,
                claim,
                normalized,
                serde_json::to_string(queries).map_err(|error| error.to_string())?,
                domain,
                now
            ],
        )
        .map_err(db_err)?;
        // 同一条事实先由旧路径以 general 建过时，后续明确判定为 history 就升级；
        // 反向不降级，避免一次误判把历史事实踢回普通区。
        tx.execute(
            "UPDATE facts SET domain = 'history' WHERE id = ?1 AND domain = 'general' AND ?2 = 'history'",
            params![id, domain],
        )
        .map_err(db_err)?;
        let quote = if source_quote.trim().is_empty() { claim } else { source_quote.trim() };
        tx.execute(
            "INSERT INTO note_fact_mentions
              (id, fact_id, note_id, run_id, quote, section, block_id, stance, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, '', 'asserts', ?7)
             ON CONFLICT(fact_id, note_id, quote) DO UPDATE SET run_id = excluded.run_id",
            params![format!("mention_{}", Uuid::new_v4().simple()), id, note_id, run_id, quote, section, now],
        )
        .map_err(db_err)?;
        link_related(&tx, &id, &normalized, now)?;
        tx.execute(
            "UPDATE facts SET updated_at = ?2 WHERE id = ?1 AND updated_at < ?2",
            params![id, now],
        )
        .map_err(db_err)?;
        tx.commit().map_err(db_err)?;
        Ok(id)
    }

    pub fn record_verification(
        &self,
        fact_id: &str,
        verdict: &str,
        provider: &str,
        model: &str,
        queries: &[String],
        evidence: &[FactEvidenceInput],
    ) -> Result<(), String> {
        let now = now_ms();
        let status = status_for_verification(verdict, evidence);
        let mut conn = lock(self)?;
        let tx = conn.transaction().map_err(db_err)?;
        let exists: Option<String> = tx
            .query_row("SELECT id FROM facts WHERE id = ?1", params![fact_id], |row| row.get(0))
            .optional()
            .map_err(db_err)?;
        if exists.is_none() {
            return Err("事实不存在".into());
        }
        tx.execute(
            "UPDATE facts SET status = ?2, last_checked_at = ?3, updated_at = ?3 WHERE id = ?1",
            params![fact_id, status, now],
        )
        .map_err(db_err)?;
        tx.execute("DELETE FROM fact_evidence WHERE fact_id = ?1", params![fact_id])
            .map_err(db_err)?;
        for item in evidence {
            if item.source_url.trim().is_empty() {
                continue;
            }
            let quote = clean_quote(&item.quote);
            let source_id = source_id_for(&item.source_url);
            let domain = domain_of(&item.source_url);
            tx.execute(
                "INSERT INTO fact_sources (id, url, title, domain, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?5)
                 ON CONFLICT(url) DO UPDATE SET title = excluded.title,
                   domain = excluded.domain, updated_at = excluded.updated_at",
                params![source_id, item.source_url.trim(), item.title.trim(), domain, now],
            )
            .map_err(db_err)?;
            tx.execute(
                "INSERT OR IGNORE INTO fact_evidence
                  (id, fact_id, source_id, relation, quote, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    evidence_id_for(fact_id, &source_id, &quote),
                    fact_id,
                    source_id,
                    item.relation,
                    quote,
                    now,
                ],
            )
            .map_err(db_err)?;
        }
        tx.execute(
            "INSERT INTO fact_checks
              (id, fact_id, verdict, provider, model, queries_json, evidence_json, checked_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                format!("check_{}", Uuid::new_v4().simple()),
                fact_id,
                verdict,
                provider,
                model,
                serde_json::to_string(queries).map_err(|error| error.to_string())?,
                serde_json::to_string(evidence).map_err(|error| error.to_string())?,
                now,
            ],
        )
        .map_err(db_err)?;
        tx.commit().map_err(db_err)?;
        Ok(())
    }

    /// 人工确认后的重复事实合并。只允许被判定为 same_as 的两条事实，避免误删。
    /// 被合并事实的来源、证据、提及和核验历史转移到保留事实，并写一条可撤销的审计记录。
    pub fn merge(&self, target_id: &str, source_id: &str) -> Result<Fact, String> {
        if target_id == source_id {
            return Err("不能把一条事实合并到它自己".into());
        }
        let now = now_ms();
        let mut conn = lock(self)?;
        let tx = conn.transaction().map_err(db_err)?;
        let target = tx
            .query_row(
                "SELECT text, status, normalized, queries_json, created_at, updated_at, last_checked_at
                 FROM facts WHERE id = ?1",
                params![target_id],
                |row| Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, i64>(6)?,
                )),
            )
            .optional()
            .map_err(db_err)?
            .ok_or_else(|| "保留的事实不存在".to_string())?;
        let source = tx
            .query_row(
                "SELECT text, status, normalized, queries_json, created_at, updated_at, last_checked_at
                 FROM facts WHERE id = ?1",
                params![source_id],
                |row| Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, i64>(6)?,
                )),
            )
            .optional()
            .map_err(db_err)?
            .ok_or_else(|| "要合并的事实不存在".to_string())?;
        let (target_text, target_status, target_normalized, target_queries, target_created, target_updated, target_last) = target;
        let (source_text, source_status, source_normalized, source_queries, source_created, source_updated, source_last) = source;
        let relation = relation_for(&target_normalized, &source_normalized);
        let Some(("same_as", confidence)) = relation else {
            let confidence = relation.map(|(_, value)| value).unwrap_or_else(|| similarity(&target_normalized, &source_normalized));
            return Err(format!(
                "两条事实不是“同一事实”候选（词面相似度 {:.0}%，数值集合不一致或文本差异过大）；不能合并",
                confidence * 100.0
            ));
        };

        let mut mention_stmt = tx
            .prepare(
                "SELECT id, note_id, run_id, quote, section, block_id, stance, created_at
                 FROM note_fact_mentions WHERE fact_id = ?1 ORDER BY created_at",
            )
            .map_err(db_err)?;
        let mentions = mention_stmt
            .query_map(params![source_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, i64>(7)?,
                ))
            })
            .map_err(db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_err)?;
        drop(mention_stmt);

        let mut evidence_stmt = tx
            .prepare(
                "SELECT e.id, e.source_id, e.relation, e.quote, e.created_at, s.url, s.title, s.domain
                 FROM fact_evidence e JOIN fact_sources s ON s.id = e.source_id
                 WHERE e.fact_id = ?1 ORDER BY e.created_at",
            )
            .map_err(db_err)?;
        let evidence = evidence_stmt
            .query_map(params![source_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                ))
            })
            .map_err(db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_err)?;
        drop(evidence_stmt);

        let mut check_stmt = tx
            .prepare(
                "SELECT id, verdict, provider, model, queries_json, evidence_json, checked_at
                 FROM fact_checks WHERE fact_id = ?1 ORDER BY checked_at",
            )
            .map_err(db_err)?;
        let checks = check_stmt
            .query_map(params![source_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, i64>(6)?,
                ))
            })
            .map_err(db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_err)?;
        drop(check_stmt);

        let mut moved_mentions = Vec::new();
        for (_, note_id, run_id, quote, section, block_id, stance, created_at) in &mentions {
            let existing: Option<String> = tx
                .query_row(
                    "SELECT id FROM note_fact_mentions WHERE fact_id = ?1 AND note_id = ?2 AND quote = ?3",
                    params![target_id, note_id, quote],
                    |row| row.get(0),
                )
                .optional()
                .map_err(db_err)?;
            if let Some(existing_id) = existing {
                tx.execute(
                    "UPDATE note_fact_mentions SET run_id = CASE WHEN ?2 > created_at THEN ?3 ELSE run_id END,
                       created_at = MAX(created_at, ?2) WHERE id = ?1",
                    params![existing_id, created_at, run_id],
                )
                .map_err(db_err)?;
            } else {
                let new_id = format!("mention_{}", Uuid::new_v4().simple());
                tx.execute(
                    "INSERT INTO note_fact_mentions
                      (id, fact_id, note_id, run_id, quote, section, block_id, stance, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    params![new_id, target_id, note_id, run_id, quote, section, block_id, stance, created_at],
                )
                .map_err(db_err)?;
                moved_mentions.push(new_id);
            }
        }
        tx.execute("DELETE FROM note_fact_mentions WHERE fact_id = ?1", params![source_id])
            .map_err(db_err)?;

        let mut moved_evidence = Vec::new();
        for (_, source_ref, relation, quote, created_at, _url, _title, _domain) in &evidence {
            let existing: Option<String> = tx
                .query_row(
                    "SELECT id FROM fact_evidence WHERE fact_id = ?1 AND source_id = ?2 AND quote = ?3",
                    params![target_id, source_ref, quote],
                    |row| row.get(0),
                )
                .optional()
                .map_err(db_err)?;
            if existing.is_none() {
                let new_id = evidence_id_for(target_id, source_ref, quote);
                tx.execute(
                    "INSERT INTO fact_evidence
                      (id, fact_id, source_id, relation, quote, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![new_id, target_id, source_ref, relation, quote, created_at],
                )
                .map_err(db_err)?;
                moved_evidence.push(new_id);
            }
        }
        tx.execute("DELETE FROM fact_evidence WHERE fact_id = ?1", params![source_id])
            .map_err(db_err)?;

        let moved_checks = checks.iter().map(|item| item.0.clone()).collect::<Vec<_>>();
        tx.execute("UPDATE fact_checks SET fact_id = ?2 WHERE fact_id = ?1", params![source_id, target_id])
            .map_err(db_err)?;
        tx.execute(
            "DELETE FROM fact_relations WHERE fact_id = ?1 OR other_fact_id = ?1",
            params![source_id],
        )
        .map_err(db_err)?;

        let snapshot = serde_json::json!({
            "version": 2,
            "target": {
                "text": target_text, "status": target_status, "normalized": target_normalized,
                "queries": target_queries, "createdAt": target_created,
                "updatedAt": target_updated, "lastCheckedAt": target_last
            },
            "source": {
                "text": source_text, "status": source_status, "normalized": source_normalized,
                "queries": source_queries, "createdAt": source_created,
                "updatedAt": source_updated, "lastCheckedAt": source_last
            },
            "mentions": mentions.iter().map(|item| serde_json::json!({
                "id": item.0, "noteId": item.1, "runId": item.2, "quote": item.3,
                "section": item.4, "blockId": item.5, "stance": item.6, "createdAt": item.7
            })).collect::<Vec<_>>(),
            "evidence": evidence.iter().map(|item| serde_json::json!({
                "id": item.0, "sourceId": item.1, "relation": item.2, "quote": item.3,
                "createdAt": item.4, "url": item.5, "title": item.6, "domain": item.7
            })).collect::<Vec<_>>(),
            "checks": checks.iter().map(|item| serde_json::json!({
                "id": item.0, "verdict": item.1, "provider": item.2, "model": item.3,
                "queries": item.4, "evidence": item.5, "checkedAt": item.6
            })).collect::<Vec<_>>(),
            "moved": {
                "mentions": moved_mentions,
                "evidence": moved_evidence,
                "checks": moved_checks
            }
        });

        let status = if target_status == source_status {
            target_status.clone()
        } else if target_status == "candidate" {
            source_status.clone()
        } else if source_status == "candidate" {
            target_status.clone()
        } else if target_status == "verified" && source_status == "verified" {
            "verified".to_string()
        } else {
            "disputed".to_string()
        };
        let last_checked = target_last.max(source_last);
        tx.execute(
            "UPDATE facts SET status = ?2, last_checked_at = ?3, updated_at = ?4 WHERE id = ?1",
            params![target_id, status, last_checked, now],
        )
        .map_err(db_err)?;
        tx.execute("DELETE FROM facts WHERE id = ?1", params![source_id])
            .map_err(db_err)?;
        tx.execute(
            "INSERT INTO fact_merges
              (id, target_fact_id, source_fact_id, target_text, source_text, source_status, snapshot_json, merged_at, undone_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 0)",
            params![
                format!("merge_{}", Uuid::new_v4().simple()),
                target_id,
                source_id,
                target_text,
                source_text,
                source_status,
                serde_json::to_string(&snapshot).map_err(|error| error.to_string())?,
                now
            ],
        )
        .map_err(db_err)?;
        link_related(&tx, target_id, &target_normalized, now)?;
        tx.commit().map_err(db_err)?;
        drop(conn);
        self.get_fact(target_id)?.ok_or_else(|| "合并完成但读取事实失败".to_string())
    }

    pub fn list_merges(&self, limit: usize) -> Result<Vec<FactMerge>, String> {
        let conn = lock(self)?;
        let mut statement = conn
            .prepare(
                "SELECT id, target_fact_id, source_fact_id, target_text, source_text,
                        source_status, merged_at, undone_at, snapshot_json
                 FROM fact_merges ORDER BY merged_at DESC LIMIT ?1",
            )
            .map_err(db_err)?;
        let rows = statement
            .query_map(params![limit.clamp(1, 100) as i64], |row| {
                let snapshot: String = row.get(8)?;
                let parsed = serde_json::from_str::<serde_json::Value>(&snapshot).ok();
                let undoable = parsed
                    .as_ref()
                    .and_then(|value| value.get("version").and_then(|version| version.as_u64()))
                    .unwrap_or(0)
                    >= 2
                    && row.get::<_, i64>(7)? == 0;
                Ok(FactMerge {
                    id: row.get(0)?,
                    target_fact_id: row.get(1)?,
                    source_fact_id: row.get(2)?,
                    target_text: row.get(3)?,
                    source_text: row.get(4)?,
                    source_status: row.get(5)?,
                    merged_at: row.get(6)?,
                    undone_at: row.get(7)?,
                    can_undo: undoable,
                })
            })
            .map_err(db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_err)?;
        Ok(rows)
    }

    pub fn list_conflict_resolutions(&self) -> Result<Vec<FactConflictResolution>, String> {
        let conn = lock(self)?;
        let mut statement = conn
            .prepare(
                "SELECT id, fact_a, fact_b, resolution, note, updated_at
                 FROM fact_conflict_resolutions ORDER BY updated_at DESC",
            )
            .map_err(db_err)?;
        let rows = statement
            .query_map([], |row| {
                Ok(FactConflictResolution {
                    id: row.get(0)?,
                    fact_a: row.get(1)?,
                    fact_b: row.get(2)?,
                    resolution: row.get(3)?,
                    note: row.get(4)?,
                    updated_at: row.get(5)?,
                })
            })
            .map_err(db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_err)?;
        Ok(rows)
    }

    pub fn save_conflict_resolution(
        &self,
        input: &ConflictResolutionInput,
    ) -> Result<FactConflictResolution, String> {
        let resolution = match input.resolution.trim() {
            "different_scope" | "keep_both" | "one_side_wrong" | "needs_research" => input.resolution.trim(),
            _ => return Err("冲突处理方式不合法".into()),
        };
        if input.fact_a.trim().is_empty() || input.fact_b.trim().is_empty() || input.fact_a == input.fact_b {
            return Err("冲突事实不合法".into());
        }
        let (fact_a, fact_b) = if input.fact_a < input.fact_b {
            (input.fact_a.trim().to_string(), input.fact_b.trim().to_string())
        } else {
            (input.fact_b.trim().to_string(), input.fact_a.trim().to_string())
        };
        let now = now_ms();
        let conn = lock(self)?;
        for fact_id in [&fact_a, &fact_b] {
            let exists: Option<String> = conn
                .query_row("SELECT id FROM facts WHERE id = ?1", params![fact_id], |row| row.get(0))
                .optional()
                .map_err(db_err)?;
            if exists.is_none() {
                return Err(format!("冲突事实不存在：{fact_id}"));
            }
        }
        conn.execute(
            "INSERT INTO fact_conflict_resolutions
              (id, fact_a, fact_b, resolution, note, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)
             ON CONFLICT(fact_a, fact_b) DO UPDATE SET
               resolution = excluded.resolution, note = excluded.note, updated_at = excluded.updated_at",
            params![
                format!("conflict_{}", Uuid::new_v4().simple()),
                fact_a,
                fact_b,
                resolution,
                input.note.trim(),
                now
            ],
        )
        .map_err(db_err)?;
        conn.query_row(
            "SELECT id, fact_a, fact_b, resolution, note, updated_at
             FROM fact_conflict_resolutions WHERE fact_a = ?1 AND fact_b = ?2",
            params![fact_a, fact_b],
            |row| {
                Ok(FactConflictResolution {
                    id: row.get(0)?,
                    fact_a: row.get(1)?,
                    fact_b: row.get(2)?,
                    resolution: row.get(3)?,
                    note: row.get(4)?,
                    updated_at: row.get(5)?,
                })
            },
        )
        .map_err(db_err)
    }

    pub fn list_reviews(&self) -> Result<Vec<FactReview>, String> {
        let conn = lock(self)?;
        let mut statement = conn
            .prepare("SELECT fact_id, status, note, updated_at FROM fact_reviews ORDER BY updated_at DESC")
            .map_err(db_err)?;
        let rows = statement
            .query_map([], |row| {
                Ok(FactReview {
                    fact_id: row.get(0)?,
                    status: row.get(1)?,
                    note: row.get(2)?,
                    updated_at: row.get(3)?,
                })
            })
            .map_err(db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_err)?;
        Ok(rows)
    }

    pub fn set_review(&self, fact_id: &str, status: &str, note: &str) -> Result<Option<FactReview>, String> {
        let status = status.trim();
        if !matches!(status, "" | "reviewed" | "needs_split") {
            return Err("事实整理状态不合法".into());
        }
        let now = now_ms();
        let conn = lock(self)?;
        if status.is_empty() {
            conn.execute("DELETE FROM fact_reviews WHERE fact_id = ?1", params![fact_id])
                .map_err(db_err)?;
            return Ok(None);
        }
        let exists: Option<String> = conn
            .query_row("SELECT id FROM facts WHERE id = ?1", params![fact_id], |row| row.get(0))
            .optional()
            .map_err(db_err)?;
        if exists.is_none() {
            return Err("事实不存在".into());
        }
        conn.execute(
            "INSERT INTO fact_reviews (fact_id, status, note, updated_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(fact_id) DO UPDATE SET
               status = excluded.status, note = excluded.note, updated_at = excluded.updated_at",
            params![fact_id, status, note.trim(), now],
        )
        .map_err(db_err)?;
        Ok(Some(FactReview {
            fact_id: fact_id.to_string(),
            status: status.to_string(),
            note: note.trim().to_string(),
            updated_at: now,
        }))
    }

    /// Split one composite fact into atomic child facts.
    ///
    /// Mentions are copied to every child so the note still points at the
    /// sub-claims. Evidence, citations and verification stay on the parent:
    /// copying them would claim that one quote supports every child. Existing
    /// facts with the same normalized text are reused instead of duplicated.
    pub fn split(&self, parent_id: &str, children: &[String]) -> Result<FactSplit, String> {
        if children.len() < 2 {
            return Err("至少需要两条子事实".into());
        }
        if children.len() > 8 {
            return Err("一次最多拆成 8 条子事实".into());
        }
        let mut cleaned = Vec::new();
        let mut seen = HashSet::new();
        for raw in children {
            let text = raw.trim();
            let length = text.chars().count();
            if length < 2 {
                return Err("子事实不能为空".into());
            }
            if length > 300 {
                return Err("单条子事实最多 300 字".into());
            }
            let normalized = normalize(text);
            if normalized.is_empty() {
                return Err("子事实缺少有效文字".into());
            }
            if !seen.insert(normalized) {
                return Err("子事实有重复".into());
            }
            cleaned.push(text.to_string());
        }
        let parent = self.get_fact(parent_id)?.ok_or_else(|| "事实不存在".to_string())?;
        if parent.status == "split" {
            return Err("这条事实已经拆分过".into());
        }
        let parent_normalized = normalize(&parent.text);
        if cleaned.iter().any(|text| normalize(text) == parent_normalized) {
            return Err("子事实不能与原事实完全相同".into());
        }

        let now = now_ms();
        let split_id = format!("split_{}", Uuid::new_v4().simple());
        let mut conn = lock(self)?;
        let tx = conn.transaction().map_err(db_err)?;
        let (parent_text, parent_status, parent_domain, parent_event, parent_historical, parent_claim, parent_updated, parent_last) = tx
            .query_row(
                "SELECT text, status, domain, event_id, historical_status, claim_kind, updated_at, last_checked_at
                 FROM facts WHERE id = ?1",
                params![parent_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, i64>(6)?,
                        row.get::<_, i64>(7)?,
                    ))
                },
            )
            .map_err(db_err)?;
        if parent_status == "split" {
            return Err("这条事实已经拆分过".into());
        }
        let mentions = {
            let mut statement = tx
                .prepare(
                    "SELECT note_id, run_id, quote, section, block_id, stance
                     FROM note_fact_mentions WHERE fact_id = ?1 ORDER BY created_at",
                )
                .map_err(db_err)?;
            statement
                .query_map(params![parent_id], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                    ))
                })
                .map_err(db_err)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(db_err)?
        };
        let review: Option<(String, String, i64)> = tx
            .query_row(
                "SELECT status, note, updated_at FROM fact_reviews WHERE fact_id = ?1",
                params![parent_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()
            .map_err(db_err)?;

        let mut child_snapshots = Vec::new();
        let mut child_ids = Vec::new();
        let mut result_children = Vec::new();
        for text in &cleaned {
            let normalized = normalize(text);
            let existing: Option<(String, String)> = tx
                .query_row(
                    "SELECT id, status FROM facts WHERE normalized = ?1",
                    params![normalized],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()
                .map_err(db_err)?;
            let (child_id, existed) = match existing {
                Some((id, status)) => {
                    if id == parent_id {
                        return Err("子事实不能与原事实完全相同".into());
                    }
                    if status == "split" {
                        return Err("子事实本身已经拆分过".into());
                    }
                    (id, true)
                }
                None => {
                    let id = fact_id_for(text);
                    tx.execute(
                        "INSERT INTO facts
                          (id, text, normalized, status, queries_json, created_at, updated_at, last_checked_at,
                           domain, event_id, historical_status, claim_kind)
                         VALUES (?1, ?2, ?3, 'candidate', '[]', ?4, ?4, 0, ?5, ?6, ?7, ?8)",
                        params![id, text, normalized, now, parent_domain, parent_event, parent_historical, parent_claim],
                    )
                    .map_err(db_err)?;
                    (id, false)
                }
            };
            let mut inserted_mentions = Vec::new();
            for mention in &mentions {
                let mention_id = format!("mention_{}", Uuid::new_v4().simple());
                let changed = tx
                    .execute(
                        "INSERT OR IGNORE INTO note_fact_mentions
                          (id, fact_id, note_id, run_id, quote, section, block_id, stance, created_at)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                        params![mention_id, child_id, mention.0, mention.1, mention.2, mention.3, mention.4, mention.5, now],
                    )
                    .map_err(db_err)?;
                if changed > 0 {
                    inserted_mentions.push(mention_id);
                }
            }
            let evidence_count = count_rows(&tx, "SELECT COUNT(*) FROM fact_evidence WHERE fact_id = ?1", &child_id);
            let citation_count = count_rows(&tx, "SELECT COUNT(*) FROM citations WHERE fact_id = ?1", &child_id);
            let child_text: String = if existed {
                tx.query_row("SELECT text FROM facts WHERE id = ?1", params![child_id], |row| row.get(0))
                    .map_err(db_err)?
            } else {
                text.clone()
            };
            let child_status: String = if existed {
                tx.query_row("SELECT status FROM facts WHERE id = ?1", params![child_id], |row| row.get(0))
                    .map_err(db_err)?
            } else {
                "candidate".into()
            };
            child_snapshots.push(json!({
                "id": child_id,
                "existed": existed,
                "mentions": inserted_mentions,
            }));
            child_ids.push(child_id.clone());
            result_children.push(FactSplitChild {
                id: child_id,
                text: child_text,
                status: child_status,
                existed,
                evidence_count,
                citation_count,
            });
        }

        tx.execute(
            "INSERT INTO fact_reviews (fact_id, status, note, updated_at)
             VALUES (?1, 'reviewed', ?2, ?3)
             ON CONFLICT(fact_id) DO UPDATE SET status = 'reviewed', updated_at = excluded.updated_at",
            params![parent_id, review.as_ref().map(|item| item.1.as_str()).unwrap_or(""), now],
        )
        .map_err(db_err)?;
        tx.execute(
            "UPDATE facts SET status = 'split', updated_at = ?2 WHERE id = ?1",
            params![parent_id, now],
        )
        .map_err(db_err)?;
        let review_snapshot = match &review {
            Some((status, note, updated_at)) => json!({
                "status": status,
                "note": note,
                "updatedAt": updated_at,
            }),
            None => Value::Null,
        };
        let snapshot = json!({
            "parentStatus": parent_status,
            "parentUpdatedAt": parent_updated,
            "parentLastCheckedAt": parent_last,
            "review": review_snapshot,
            "children": child_snapshots,
        });
        tx.execute(
            "INSERT INTO fact_splits
              (id, parent_fact_id, child_ids_json, parent_status, snapshot_json, split_at, undone_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0)",
            params![
                split_id,
                parent_id,
                serde_json::to_string(&child_ids).map_err(|error| error.to_string())?,
                parent_status,
                snapshot.to_string(),
                now
            ],
        )
        .map_err(db_err)?;
        tx.commit().map_err(db_err)?;
        drop(conn);

        Ok(FactSplit {
            id: split_id,
            parent_fact_id: parent_id.to_string(),
            parent_text,
            parent_status,
            children: result_children,
            split_at: now,
            undone_at: 0,
            can_undo: true,
        })
    }

    pub fn split_for_fact(&self, parent_id: &str) -> Result<Option<FactSplit>, String> {
        let conn = lock(self)?;
        let split_id: Option<String> = conn
            .query_row(
                "SELECT id FROM fact_splits
                 WHERE parent_fact_id = ?1 AND undone_at = 0
                 ORDER BY split_at DESC LIMIT 1",
                params![parent_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(db_err)?;
        match split_id {
            Some(id) => load_split(&conn, &id),
            None => Ok(None),
        }
    }

    pub fn undo_split(&self, split_id: &str) -> Result<FactSplit, String> {
        let now = now_ms();
        let mut conn = lock(self)?;
        let row = conn
            .query_row(
                "SELECT parent_fact_id, snapshot_json, undone_at
                 FROM fact_splits WHERE id = ?1",
                params![split_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, i64>(2)?)),
            )
            .optional()
            .map_err(db_err)?;
        let Some((parent_id, snapshot_json, undone_at)) = row else {
            return Err("拆分记录不存在".into());
        };
        if undone_at != 0 {
            return Err("这次拆分已经撤销".into());
        }
        let snapshot: Value = serde_json::from_str(&snapshot_json).map_err(|error| error.to_string())?;
        if !split_can_undo(&conn, &snapshot) {
            return Err("子事实已经有新增的证据、核验、关系或检索候选，不能直接撤销拆分".into());
        }
        let children = split_snapshot_children(&snapshot);
        let parent_status = snapshot
            .get("parentStatus")
            .and_then(Value::as_str)
            .unwrap_or("candidate")
            .to_string();
        let parent_updated = snapshot
            .get("parentUpdatedAt")
            .and_then(Value::as_i64)
            .unwrap_or(now);
        let parent_last = snapshot
            .get("parentLastCheckedAt")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        let tx = conn.transaction().map_err(db_err)?;
        for child in &children {
            let child_id = child.get("id").and_then(Value::as_str).unwrap_or("");
            if child_id.is_empty() {
                return Err("拆分快照损坏".into());
            }
            if let Some(mentions) = child.get("mentions").and_then(Value::as_array) {
                for mention in mentions {
                    if let Some(mention_id) = mention.as_str() {
                        tx.execute("DELETE FROM note_fact_mentions WHERE id = ?1", params![mention_id])
                            .map_err(db_err)?;
                    }
                }
            }
            if !child.get("existed").and_then(Value::as_bool).unwrap_or(false) {
                tx.execute("DELETE FROM facts WHERE id = ?1", params![child_id]).map_err(db_err)?;
            }
        }
        tx.execute(
            "UPDATE facts SET status = ?2, updated_at = ?3, last_checked_at = ?4 WHERE id = ?1",
            params![parent_id, parent_status, parent_updated, parent_last],
        )
        .map_err(db_err)?;
        match snapshot.get("review") {
            Some(value) if !value.is_null() => {
                let status = value.get("status").and_then(Value::as_str).unwrap_or("reviewed");
                let note = value.get("note").and_then(Value::as_str).unwrap_or("");
                let updated_at = value.get("updatedAt").and_then(Value::as_i64).unwrap_or(now);
                tx.execute(
                    "INSERT INTO fact_reviews (fact_id, status, note, updated_at)
                     VALUES (?1, ?2, ?3, ?4)
                     ON CONFLICT(fact_id) DO UPDATE SET
                       status = excluded.status, note = excluded.note, updated_at = excluded.updated_at",
                    params![parent_id, status, note, updated_at],
                )
                .map_err(db_err)?;
            }
            _ => {
                tx.execute("DELETE FROM fact_reviews WHERE fact_id = ?1", params![parent_id])
                    .map_err(db_err)?;
            }
        }
        tx.execute(
            "UPDATE fact_splits SET undone_at = ?2 WHERE id = ?1",
            params![split_id, now],
        )
        .map_err(db_err)?;
        tx.commit().map_err(db_err)?;
        drop(conn);
        let conn = lock(self)?;
        let split = load_split(&conn, split_id)?;
        split.ok_or_else(|| "撤销完成但读取拆分记录失败".to_string())
    }

    pub fn undo_merge(&self, merge_id: &str) -> Result<Fact, String> {
        let now = now_ms();
        let mut conn = lock(self)?;
        let tx = conn.transaction().map_err(db_err)?;
        let row = tx
            .query_row(
                "SELECT target_fact_id, source_fact_id, snapshot_json, undone_at
                 FROM fact_merges WHERE id = ?1",
                params![merge_id],
                |row| Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                )),
            )
            .optional()
            .map_err(db_err)?
            .ok_or_else(|| "合并记录不存在".to_string())?;
        let (target_id, source_id, snapshot_json, undone_at) = row;
        if undone_at != 0 {
            return Err("这条合并已经撤销过了".into());
        }
        let snapshot = serde_json::from_str::<serde_json::Value>(&snapshot_json)
            .map_err(|_| "合并快照无法解析".to_string())?;
        if snapshot.get("version").and_then(|value| value.as_u64()).unwrap_or(0) < 2 {
            return Err("这条合并记录来自旧版本，缺少完整快照，不能安全撤销".into());
        }
        let source_state = snapshot.get("source").ok_or_else(|| "合并快照缺少源事实".to_string())?;
        let target_state = snapshot.get("target").ok_or_else(|| "合并快照缺少保留事实".to_string())?;
        let source_text = source_state.get("text").and_then(|value| value.as_str()).unwrap_or("");
        let source_status = source_state.get("status").and_then(|value| value.as_str()).unwrap_or("candidate");
        let source_queries = source_state.get("queries").and_then(|value| value.as_str()).unwrap_or("[]");
        let source_created = source_state.get("createdAt").and_then(|value| value.as_i64()).unwrap_or(now);
        let source_updated = source_state.get("updatedAt").and_then(|value| value.as_i64()).unwrap_or(now);
        let source_last = source_state.get("lastCheckedAt").and_then(|value| value.as_i64()).unwrap_or(0);
        let target_status = target_state.get("status").and_then(|value| value.as_str()).unwrap_or("candidate");
        let target_updated = target_state.get("updatedAt").and_then(|value| value.as_i64()).unwrap_or(now);
        let target_last = target_state.get("lastCheckedAt").and_then(|value| value.as_i64()).unwrap_or(0);
        if source_text.trim().is_empty() {
            return Err("合并快照里的源事实为空".into());
        }
        let target_exists: Option<String> = tx
            .query_row("SELECT id FROM facts WHERE id = ?1", params![target_id], |row| row.get(0))
            .optional()
            .map_err(db_err)?;
        if target_exists.is_none() {
            return Err("保留事实已不存在，无法撤销".into());
        }
        let source_exists: Option<String> = tx
            .query_row("SELECT id FROM facts WHERE id = ?1", params![source_id], |row| row.get(0))
            .optional()
            .map_err(db_err)?;
        if source_exists.is_some() {
            return Err("源事实已经存在，无法重复撤销".into());
        }

        let moved = snapshot.get("moved").ok_or_else(|| "合并快照缺少移动记录".to_string())?;
        for id in moved.get("mentions").and_then(|value| value.as_array()).into_iter().flatten() {
            if let Some(id) = id.as_str() {
                tx.execute("DELETE FROM note_fact_mentions WHERE id = ?1 AND fact_id = ?2", params![id, target_id])
                    .map_err(db_err)?;
            }
        }
        for id in moved.get("evidence").and_then(|value| value.as_array()).into_iter().flatten() {
            if let Some(id) = id.as_str() {
                tx.execute("DELETE FROM fact_evidence WHERE id = ?1 AND fact_id = ?2", params![id, target_id])
                    .map_err(db_err)?;
            }
        }
        for id in moved.get("checks").and_then(|value| value.as_array()).into_iter().flatten() {
            if let Some(id) = id.as_str() {
                tx.execute("DELETE FROM fact_checks WHERE id = ?1 AND fact_id = ?2", params![id, target_id])
                    .map_err(db_err)?;
            }
        }

        tx.execute(
            "INSERT INTO facts
              (id, text, normalized, status, queries_json, created_at, updated_at, last_checked_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![source_id, source_text, normalize(source_text), source_status, source_queries, source_created, source_updated, source_last],
        )
        .map_err(db_err)?;
        for item in snapshot.get("mentions").and_then(|value| value.as_array()).into_iter().flatten() {
            let note_id = item.get("noteId").and_then(|value| value.as_str()).unwrap_or("");
            let quote = item.get("quote").and_then(|value| value.as_str()).unwrap_or("");
            if note_id.is_empty() || quote.is_empty() {
                continue;
            }
            tx.execute(
                "INSERT OR IGNORE INTO note_fact_mentions
                  (id, fact_id, note_id, run_id, quote, section, block_id, stance, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    format!("mention_{}", Uuid::new_v4().simple()),
                    source_id,
                    note_id,
                    item.get("runId").and_then(|value| value.as_str()).unwrap_or(""),
                    quote,
                    item.get("section").and_then(|value| value.as_str()).unwrap_or(""),
                    item.get("blockId").and_then(|value| value.as_str()).unwrap_or(""),
                    item.get("stance").and_then(|value| value.as_str()).unwrap_or("asserts"),
                    item.get("createdAt").and_then(|value| value.as_i64()).unwrap_or(now)
                ],
            )
            .map_err(db_err)?;
        }
        for item in snapshot.get("evidence").and_then(|value| value.as_array()).into_iter().flatten() {
            let source_ref = item.get("sourceId").and_then(|value| value.as_str()).unwrap_or("");
            if source_ref.is_empty() {
                continue;
            }
            let quote = item.get("quote").and_then(|value| value.as_str()).unwrap_or("");
            tx.execute(
                "INSERT OR IGNORE INTO fact_sources (id, url, title, domain, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
                params![
                    source_ref,
                    item.get("url").and_then(|value| value.as_str()).unwrap_or(""),
                    item.get("title").and_then(|value| value.as_str()).unwrap_or(""),
                    item.get("domain").and_then(|value| value.as_str()).unwrap_or(""),
                    item.get("createdAt").and_then(|value| value.as_i64()).unwrap_or(now)
                ],
            )
            .map_err(db_err)?;
            tx.execute(
                "INSERT OR IGNORE INTO fact_evidence
                  (id, fact_id, source_id, relation, quote, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    evidence_id_for(&source_id, source_ref, quote),
                    source_id,
                    source_ref,
                    item.get("relation").and_then(|value| value.as_str()).unwrap_or("mention"),
                    quote,
                    item.get("createdAt").and_then(|value| value.as_i64()).unwrap_or(now)
                ],
            )
            .map_err(db_err)?;
        }
        for item in snapshot.get("checks").and_then(|value| value.as_array()).into_iter().flatten() {
            tx.execute(
                "INSERT INTO fact_checks
                  (id, fact_id, verdict, provider, model, queries_json, evidence_json, checked_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    format!("check_{}", Uuid::new_v4().simple()),
                    source_id,
                    item.get("verdict").and_then(|value| value.as_str()).unwrap_or(""),
                    item.get("provider").and_then(|value| value.as_str()).unwrap_or(""),
                    item.get("model").and_then(|value| value.as_str()).unwrap_or(""),
                    item.get("queries").and_then(|value| value.as_str()).unwrap_or("[]"),
                    item.get("evidence").and_then(|value| value.as_str()).unwrap_or("[]"),
                    item.get("checkedAt").and_then(|value| value.as_i64()).unwrap_or(now)
                ],
            )
            .map_err(db_err)?;
        }
        tx.execute(
            "UPDATE facts SET status = ?2, last_checked_at = ?3, updated_at = ?4 WHERE id = ?1",
            params![target_id, target_status, target_last, target_updated],
        )
        .map_err(db_err)?;
        tx.execute("UPDATE fact_merges SET undone_at = ?2 WHERE id = ?1", params![merge_id, now])
            .map_err(db_err)?;
        tx.execute(
            "DELETE FROM fact_relations WHERE fact_id = ?1 OR other_fact_id = ?1 OR fact_id = ?2 OR other_fact_id = ?2",
            params![target_id, source_id],
        )
        .map_err(db_err)?;
        let target_normalized = target_state.get("normalized").and_then(|value| value.as_str()).unwrap_or("");
        let source_normalized = normalize(source_text);
        link_related(&tx, &target_id, target_normalized, now)?;
        link_related(&tx, &source_id, &source_normalized, now)?;
        tx.commit().map_err(db_err)?;
        drop(conn);
        self.get_fact(&target_id)?.ok_or_else(|| "撤销完成但读取事实失败".to_string())
    }

    pub fn facts_for_note(&self, note_id: &str, run_id: &str) -> Result<Vec<Fact>, String> {
        let conn = lock(self)?;
        let mut statement = conn
            .prepare(
                "SELECT f.id, f.text, f.status, f.created_at, f.updated_at, f.last_checked_at,
                        f.queries_json,
                        (SELECT COUNT(DISTINCT note_id) FROM note_fact_mentions x WHERE x.fact_id = f.id),
                        f.domain, f.event_id, f.historical_status, f.claim_kind
                 FROM facts f
                 JOIN note_fact_mentions m ON m.fact_id = f.id
                 WHERE m.note_id = ?1 AND (?2 = '' OR m.run_id = ?2)
                 GROUP BY f.id
                 ORDER BY f.updated_at DESC, f.created_at DESC",
            )
            .map_err(db_err)?;
        let rows = statement
            .query_map(params![note_id, run_id], |row| {
                Ok(Fact {
                    id: row.get(0)?,
                    text: row.get(1)?,
                    status: row.get(2)?,
                    created_at: row.get(3)?,
                    updated_at: row.get(4)?,
                    last_checked_at: row.get(5)?,
                    queries: serde_json::from_str::<Vec<String>>(&row.get::<_, String>(6)?).unwrap_or_default(),
                    note_count: row.get(7)?,
                    domain: row.get(8)?,
                    event_id: row.get(9)?,
                    historical_status: row.get(10)?,
                    claim_kind: row.get(11)?,
                    citation_count: 0,
                    mentions: Vec::new(),
                    evidence: Vec::new(),
                    related: Vec::new(),
                })
            })
            .map_err(db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_err)?;
        drop(statement);
        let split_children = active_split_child_ids(&conn)?;
        let mut facts = rows;
        facts.retain(|fact| !split_children.contains(&fact.id));
        for fact in &mut facts {
            let mut mention_stmt = conn
                .prepare(
                    "SELECT note_id, run_id, quote, section, block_id, stance
                     FROM note_fact_mentions WHERE fact_id = ?1 AND note_id = ?2
                       AND (?3 = '' OR run_id = ?3) ORDER BY created_at",
                )
                .map_err(db_err)?;
            fact.mentions = mention_stmt
                .query_map(params![fact.id, note_id, run_id], |row| {
                    Ok(FactMention {
                        note_id: row.get(0)?,
                        run_id: row.get(1)?,
                        quote: row.get(2)?,
                        section: row.get(3)?,
                        block_id: row.get(4)?,
                        stance: row.get(5)?,
                    })
                })
                .map_err(db_err)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(db_err)?;
            let mut evidence_stmt = conn
                .prepare(
                    "SELECT s.url, s.title, s.domain, e.relation, e.quote, e.created_at
                     FROM fact_evidence e JOIN fact_sources s ON s.id = e.source_id
                     WHERE e.fact_id = ?1 ORDER BY e.created_at DESC",
                )
                .map_err(db_err)?;
            fact.evidence = evidence_stmt
                .query_map(params![fact.id], |row| {
                    Ok(FactEvidence {
                        url: row.get(0)?,
                        title: row.get(1)?,
                        domain: row.get(2)?,
                        relation: row.get(3)?,
                        quote: clean_quote(&row.get::<_, String>(4)?),
                        checked_at: row.get(5)?,
                    })
                })
                .map_err(db_err)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(db_err)?;
            fact.related = related_for(&conn, &fact.id)?;
        }
        Ok(facts)
    }

    pub fn list_facts(&self, status: &str, query: &str, limit: usize) -> Result<Vec<Fact>, String> {
        let conn = lock(self)?;
        let mut statement = conn
            .prepare(
                "SELECT f.id, f.text, f.status, f.created_at, f.updated_at, f.last_checked_at,
                        f.queries_json,
                        (SELECT COUNT(DISTINCT note_id) FROM note_fact_mentions x WHERE x.fact_id = f.id),
                        f.domain, f.event_id, f.historical_status, f.claim_kind
                 FROM facts f
                 WHERE (?1 = '' OR f.status = ?1)
                   AND (?2 = '' OR f.text LIKE '%' || ?2 || '%')
                 ORDER BY f.updated_at DESC, f.created_at DESC
                 LIMIT ?3",
            )
            .map_err(db_err)?;
        let rows = statement
            .query_map(params![status, query.trim(), limit.clamp(1, 300) as i64], |row| {
                Ok(Fact {
                    id: row.get(0)?,
                    text: row.get(1)?,
                    status: row.get(2)?,
                    created_at: row.get(3)?,
                    updated_at: row.get(4)?,
                    last_checked_at: row.get(5)?,
                    queries: serde_json::from_str::<Vec<String>>(&row.get::<_, String>(6)?).unwrap_or_default(),
                    note_count: row.get(7)?,
                    domain: row.get(8)?,
                    event_id: row.get(9)?,
                    historical_status: row.get(10)?,
                    claim_kind: row.get(11)?,
                    citation_count: 0,
                    mentions: Vec::new(),
                    evidence: Vec::new(),
                    related: Vec::new(),
                })
            })
            .map_err(db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_err)?;
        drop(statement);
        let mut facts = rows;
        for fact in &mut facts {
            let mut mention_stmt = conn
                .prepare(
                    "SELECT note_id, run_id, quote, section, block_id, stance
                     FROM note_fact_mentions WHERE fact_id = ?1 ORDER BY created_at",
                )
                .map_err(db_err)?;
            fact.mentions = mention_stmt
                .query_map(params![fact.id], |row| {
                    Ok(FactMention {
                        note_id: row.get(0)?,
                        run_id: row.get(1)?,
                        quote: row.get(2)?,
                        section: row.get(3)?,
                        block_id: row.get(4)?,
                        stance: row.get(5)?,
                    })
                })
                .map_err(db_err)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(db_err)?;
            let mut evidence_stmt = conn
                .prepare(
                    "SELECT s.url, s.title, s.domain, e.relation, e.quote, e.created_at
                     FROM fact_evidence e JOIN fact_sources s ON s.id = e.source_id
                     WHERE e.fact_id = ?1 ORDER BY e.created_at DESC",
                )
                .map_err(db_err)?;
            fact.evidence = evidence_stmt
                .query_map(params![fact.id], |row| {
                    Ok(FactEvidence {
                        url: row.get(0)?,
                        title: row.get(1)?,
                        domain: row.get(2)?,
                        relation: row.get(3)?,
                        quote: clean_quote(&row.get::<_, String>(4)?),
                        checked_at: row.get(5)?,
                    })
                })
                .map_err(db_err)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(db_err)?;
            fact.related = related_for(&conn, &fact.id)?;
        }
        Ok(facts)
    }

    pub fn candidate_facts_for_claim(&self, claim: &str, limit: usize) -> Result<Vec<FactCandidate>, String> {
        let normalized = normalize(claim);
        if normalized.is_empty() {
            return Ok(Vec::new());
        }
        let conn = lock(self)?;
        let mut statement = conn
            .prepare("SELECT id, text, status, normalized FROM facts")
            .map_err(db_err)?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })
            .map_err(db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_err)?;
        let mut candidates = rows
            .into_iter()
            .filter_map(|(id, text, status, other)| {
                let (relation, confidence) = relation_for(&normalized, &other)?;
                if confidence < 0.2 {
                    return None;
                }
                Some(FactCandidate {
                    id,
                    text,
                    status,
                    relation: relation.to_string(),
                    confidence,
                })
            })
            .collect::<Vec<_>>();
        candidates.sort_by(|left, right| right.confidence.partial_cmp(&left.confidence).unwrap_or(std::cmp::Ordering::Equal));
        candidates.truncate(limit.clamp(1, 12));
        Ok(candidates)
    }

    pub fn record_mention_for_fact(
        &self,
        fact_id: &str,
        note_id: &str,
        run_id: &str,
        claim: &str,
        source_quote: &str,
    ) -> Result<(), String> {
        self.record_mention_for_fact_with_domain(fact_id, note_id, run_id, claim, source_quote, "general")
    }

    pub fn record_mention_for_fact_with_domain(
        &self,
        fact_id: &str,
        note_id: &str,
        run_id: &str,
        claim: &str,
        source_quote: &str,
        domain: &str,
    ) -> Result<(), String> {
        let domain = if domain == "history" { "history" } else { "general" };
        let now = now_ms();
        let quote = if source_quote.trim().is_empty() { claim.trim() } else { source_quote.trim() };
        let mut conn = lock(self)?;
        let exists: Option<String> = conn
            .query_row("SELECT id FROM facts WHERE id = ?1", params![fact_id], |row| row.get(0))
            .optional()
            .map_err(db_err)?;
        if exists.is_none() {
            return Err("要关联的事实不存在".into());
        }
        conn.execute(
            "INSERT INTO note_fact_mentions
              (id, fact_id, note_id, run_id, quote, section, block_id, stance, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, '', '', 'asserts', ?6)
             ON CONFLICT(fact_id, note_id, quote) DO UPDATE SET run_id = excluded.run_id",
            params![format!("mention_{}", Uuid::new_v4().simple()), fact_id, note_id, run_id, quote, now],
        )
        .map_err(db_err)?;
        conn.execute(
            "UPDATE facts SET domain = 'history' WHERE id = ?1 AND domain = 'general' AND ?2 = 'history'",
            params![fact_id, domain],
        )
        .map_err(db_err)?;
        conn.execute(
            "UPDATE facts SET updated_at = ?2 WHERE id = ?1 AND updated_at < ?2",
            params![fact_id, now],
        )
        .map_err(db_err)?;
        Ok(())
    }

    pub fn checks_for_fact(&self, fact_id: &str, limit: usize) -> Result<Vec<FactCheck>, String> {
        let conn = lock(self)?;
        let mut statement = conn
            .prepare(
                "SELECT id, verdict, provider, model, queries_json, evidence_json, checked_at
                 FROM fact_checks WHERE fact_id = ?1 ORDER BY checked_at DESC LIMIT ?2",
            )
            .map_err(db_err)?;
        let rows = statement
            .query_map(params![fact_id, limit.clamp(1, 100) as i64], |row| {
                let queries: Vec<String> = serde_json::from_str::<Vec<String>>(&row.get::<_, String>(4)?).unwrap_or_default();
                let evidence: Vec<serde_json::Value> = serde_json::from_str(&row.get::<_, String>(5)?).unwrap_or_default();
                Ok(FactCheck {
                    id: row.get(0)?,
                    verdict: row.get(1)?,
                    provider: row.get(2)?,
                    model: row.get(3)?,
                    queries,
                    evidence_count: evidence.len(),
                    checked_at: row.get(6)?,
                })
            })
            .map_err(db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_err)?;
        Ok(rows)
    }

    /// 人工纠正领域路由。只允许 history / general；不改事实正文、证据或状态。
    pub fn set_domain(&self, id: &str, domain: &str) -> Result<Fact, String> {
        let domain = match domain.trim() {
            "history" => "history",
            "general" => "general",
            _ => return Err("事实领域只能是 history 或 general".into()),
        };
        let now = now_ms();
        let conn = lock(self)?;
        let changed = conn
            .execute(
                "UPDATE facts SET domain = ?2, updated_at = ?3 WHERE id = ?1",
                params![id, domain, now],
            )
            .map_err(db_err)?;
        if changed == 0 {
            return Err("事实不存在".into());
        }
        drop(conn);
        self.get_fact(id)?.ok_or_else(|| "事实更新后读取失败".to_string())
    }

    pub fn get_fact(&self, id: &str) -> Result<Option<Fact>, String> {
        let facts = self.list_facts("", "", 300)?;
        Ok(facts.into_iter().find(|fact| fact.id == id))
    }

    pub fn fact_count_for_note(&self, note_id: &str) -> Result<usize, String> {
        let conn = lock(self)?;
        let split_children = active_split_child_ids(&conn)?;
        let mut statement = conn
            .prepare("SELECT DISTINCT fact_id FROM note_fact_mentions WHERE note_id = ?1")
            .map_err(db_err)?;
        let ids = statement
            .query_map(params![note_id], |row| row.get::<_, String>(0))
            .map_err(db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_err)?;
        Ok(ids.into_iter().filter(|id| !split_children.contains(id)).count())
    }

    /// Import old `production_jobs.result_json.trace[]` into facts. This only
    /// runs at startup and only when the fact does not already exist.
    fn rebuild_relations(&self) -> Result<(), String> {
        let mut conn = lock(self)?;
        let tx = conn.transaction().map_err(db_err)?;
        tx.execute("DELETE FROM fact_relations", []).map_err(db_err)?;
        let mut statement = tx.prepare("SELECT id, normalized FROM facts").map_err(db_err)?;
        let rows = statement
            .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))
            .map_err(db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_err)?;
        drop(statement);
        let now = now_ms();
        for (id, normalized) in rows {
            link_related(&tx, &id, &normalized, now)?;
        }
        tx.commit().map_err(db_err)?;
        Ok(())
    }

    fn seed_existing_traces(&self) -> Result<(), String> {
        let conn = lock(self)?;
        let table_exists: Option<String> = conn
            .query_row(
                "SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'production_jobs'",
                [],
                |row| row.get(0),
            )
            .optional()
            .map_err(db_err)?;
        if table_exists.is_none() {
            return Ok(());
        }
        let mut statement = conn
            .prepare(
                "SELECT j.note_id, j.id, j.result_json FROM production_jobs j
                 WHERE j.result_json != '' AND j.note_id != ''
                   AND j.updated_at = (
                     SELECT MAX(j2.updated_at) FROM production_jobs j2
                     WHERE j2.note_id = j.note_id AND j2.result_json != ''
                   )",
            )
            .map_err(db_err)?;
        let rows = statement
            .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?)))
            .map_err(db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_err)?;
        drop(statement);
        for (note_id, run_id, raw) in rows {
            if note_id.trim().is_empty() {
                continue;
            }
            let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw) else { continue };
            let Some(items) = value.get("trace").and_then(serde_json::Value::as_array) else { continue };
            for item in items {
                let claim = item.get("claim").and_then(serde_json::Value::as_str).unwrap_or_default();
                if claim.trim().is_empty() {
                    continue;
                }
                let id = fact_id_for(claim);
                let now = now_ms();
                let _ = conn.execute(
                    "INSERT OR IGNORE INTO facts (id, text, normalized, status, created_at, updated_at, last_checked_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?5, ?5)",
                    params![id, claim.trim(), normalize(claim), status_for_verdict(
                        item.get("verdict").and_then(serde_json::Value::as_str).unwrap_or_default()
                    ), now],
                );
                let _ = conn.execute(
                    "INSERT INTO note_fact_mentions
                      (id, fact_id, note_id, run_id, quote, section, block_id, stance, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, '', '', 'asserts', ?6)
                     ON CONFLICT(fact_id, note_id, quote) DO UPDATE SET run_id = excluded.run_id",
                    params![format!("mention_{}", Uuid::new_v4().simple()), id, note_id, run_id, claim.trim(), now],
                );
                if let Some(source) = item.get("source").and_then(serde_json::Value::as_str).filter(|value| !value.is_empty()) {
                    let source_id = source_id_for(source);
                    let title = item.get("claim").and_then(serde_json::Value::as_str).unwrap_or("");
                    let _ = conn.execute(
                        "INSERT OR IGNORE INTO fact_sources (id, url, title, domain, created_at, updated_at)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
                        params![source_id, source, title, domain_of(source), now],
                    );
                    let quote = clean_quote(item.get("quote").and_then(serde_json::Value::as_str).unwrap_or(""));
                    let _ = conn.execute(
                        "INSERT OR IGNORE INTO fact_evidence
                          (id, fact_id, source_id, relation, quote, created_at)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                        params![
                            evidence_id_for(&id, &source_id, &quote),
                            id,
                            source_id,
                            {
                                let verdict = item.get("verdict").and_then(serde_json::Value::as_str).unwrap_or("");
                                let relation = item.get("relation").and_then(serde_json::Value::as_str).unwrap_or("");
                                if matches!(relation, "supports" | "contradicts" | "context") {
                                    relation
                                } else if verdict.contains("可印证") || verdict.contains("非权威") {
                                    "supports"
                                } else if verdict.contains("争议") {
                                    "contradicts"
                                } else {
                                    "context"
                                }
                            },
                            quote,
                            now,
                        ],
                    );
                }
            }
        }
        let mut statement = conn.prepare("SELECT id, normalized FROM facts").map_err(db_err)?;
        let facts = statement
            .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))
            .map_err(db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_err)?;
        drop(statement);
        for (id, normalized) in facts {
            let _ = link_related(&conn, &id, &normalized, now_ms());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_domain_validates_and_persists() {
        let dir = std::env::temp_dir().join(format!("yebian-facts-set-domain-{}", Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = FactStore::open(&dir.join("facts.sqlite")).unwrap();
        let id = store.record_candidate("note-a", "run-a", "一条可以改归类的事实。", "", "", &[]).unwrap();
        assert_eq!(store.get_fact(&id).unwrap().unwrap().domain, "general");
        let fact = store.set_domain(&id, "history").unwrap();
        assert_eq!(fact.domain, "history");
        assert!(store.set_domain(&id, "science").is_err());
        let fact = store.set_domain(&id, "general").unwrap();
        assert_eq!(fact.domain, "general");
    }

    #[test]
    fn split_reuses_existing_child_and_keeps_evidence_on_parent() {
        let dir = std::env::temp_dir().join(format!("yebian-facts-split-{}", Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = FactStore::open(&dir.join("facts.sqlite")).unwrap();
        let parent = store
            .record_candidate(
                "note-a",
                "run-a",
                "小布什的父亲是前美国总统，祖父是摩根兄弟合伙人，曾祖父是钢铁和铁路大亨。",
                "",
                "",
                &[],
            )
            .unwrap();
        let existing = store
            .record_candidate("note-b", "run-b", "小布什的父亲是前美国总统。", "", "", &[])
            .unwrap();
        store
            .record_verification(
                &parent,
                "外部可印证",
                "deepseek",
                "deepseek-v4-flash",
                &[],
                &[FactEvidenceInput {
                    source_url: "https://example.com/bush".into(),
                    title: "bush".into(),
                    quote: "小布什的父亲是前美国总统。".into(),
                    relation: "supports".into(),
                }],
            )
            .unwrap();
        store.set_review(&parent, "needs_split", "三个断言").unwrap();

        let split = store
            .split(
                &parent,
                &[
                    "小布什的父亲是前美国总统。".into(),
                    "小布什的祖父是摩根兄弟合伙人。".into(),
                ],
            )
            .unwrap();
        assert_eq!(split.children.len(), 2);
        let reused = split.children.iter().find(|child| child.existed).expect("reused child");
        assert_eq!(reused.id, existing);
        assert_eq!(reused.evidence_count, 0);
        assert_eq!(store.get_fact(&parent).unwrap().unwrap().status, "split");
        assert_eq!(store.get_fact(&parent).unwrap().unwrap().evidence.len(), 1);
        let active = store.split_for_fact(&parent).unwrap().expect("active split");
        assert!(active.can_undo);

        let note_facts = store.facts_for_note("note-a", "run-a").unwrap();
        assert_eq!(note_facts.len(), 1, "reading layer keeps the parent composite, hides split children");
        assert_eq!(note_facts[0].id, parent);
        assert_eq!(note_facts[0].status, "split");
        assert_eq!(store.fact_count_for_note("note-a").unwrap(), 1);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn undo_split_removes_new_children_and_restores_parent() {
        let dir = std::env::temp_dir().join(format!("yebian-facts-split-undo-{}", Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = FactStore::open(&dir.join("facts.sqlite")).unwrap();
        let parent = store
            .record_candidate("note-a", "run-a", "父亲是总统，祖父是合伙人。", "", "", &[])
            .unwrap();
        let existing = store
            .record_candidate("note-b", "run-b", "父亲是总统。", "", "", &[])
            .unwrap();
        store.set_review(&parent, "needs_split", "两个断言").unwrap();
        let split = store
            .split(&parent, &["父亲是总统。".into(), "祖父是合伙人。".into()])
            .unwrap();
        let created = split.children.iter().find(|child| !child.existed).expect("new child").id.clone();

        let undone = store.undo_split(&split.id).unwrap();
        assert!(undone.undone_at > 0);
        assert!(store.get_fact(&created).unwrap().is_none());
        assert!(store.get_fact(&existing).unwrap().is_some());
        let restored = store.get_fact(&parent).unwrap().unwrap();
        assert_eq!(restored.status, "candidate");
        assert_eq!(store.list_reviews().unwrap()[0].status, "needs_split");
        assert!(store.split_for_fact(&parent).unwrap().is_none());
        let note_facts = store.facts_for_note("note-a", "run-a").unwrap();
        assert_eq!(note_facts.len(), 1);
        assert_eq!(note_facts[0].id, parent);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn split_rejects_invalid_children_and_double_split() {
        let dir = std::env::temp_dir().join(format!("yebian-facts-split-invalid-{}", Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = FactStore::open(&dir.join("facts.sqlite")).unwrap();
        let parent = store
            .record_candidate("note-a", "run-a", "父亲是总统，祖父是合伙人。", "", "", &[])
            .unwrap();
        assert!(store.split(&parent, &["只有一条".into()]).is_err());
        assert!(store
            .split(&parent, &["父亲是总统。".into(), "父亲是总统。".into()])
            .is_err());
        assert!(store
            .split(&parent, &["父亲是总统。".into(), "父亲是总统，祖父是合伙人。".into()])
            .is_err());
        let eight = (1..=9).map(|index| format!("子事实第 {index} 条。")).collect::<Vec<_>>();
        assert!(store.split(&parent, &eight).is_err());
        let split = store
            .split(&parent, &["父亲是总统。".into(), "祖父是合伙人。".into()])
            .unwrap();
        assert!(store
            .split(&parent, &["另一个子事实。".into(), "再一个子事实。".into()])
            .is_err());
        assert!(store.undo_split(&split.id).is_ok());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn undo_split_refuses_after_a_new_child_gets_evidence() {
        let dir = std::env::temp_dir().join(format!("yebian-facts-split-refuse-{}", Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = FactStore::open(&dir.join("facts.sqlite")).unwrap();
        let parent = store
            .record_candidate("note-a", "run-a", "父亲是总统，祖父是合伙人。", "", "", &[])
            .unwrap();
        let split = store
            .split(&parent, &["父亲是总统。".into(), "祖父是合伙人。".into()])
            .unwrap();
        let child = split.children[0].id.clone();
        store
            .record_verification(
                &child,
                "外部可印证",
                "deepseek",
                "deepseek-v4-flash",
                &[],
                &[FactEvidenceInput {
                    source_url: "https://example.com/child".into(),
                    title: "child".into(),
                    quote: "父亲是总统。".into(),
                    relation: "supports".into(),
                }],
            )
            .unwrap();
        let active = store.split_for_fact(&parent).unwrap().expect("split");
        assert!(!active.can_undo);
        assert!(store.undo_split(&split.id).is_err());
        assert!(store.get_fact(&child).unwrap().is_some());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn conflict_resolution_and_review_round_trip() {
        let dir = std::env::temp_dir().join(format!("yebian-facts-curation-{}", Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = FactStore::open(&dir.join("facts.sqlite")).unwrap();
        let first = store.record_candidate("note-a", "run-a", "2026年选举花费120亿美元。", "", "", &[]).unwrap();
        let second = store.record_candidate("note-b", "run-b", "2026年选举花费130亿美元。", "", "", &[]).unwrap();

        let saved = store
            .save_conflict_resolution(&ConflictResolutionInput {
                fact_a: second.clone(),
                fact_b: first.clone(),
                resolution: "different_scope".into(),
                note: "统计口径不同".into(),
            })
            .unwrap();
        assert_eq!(saved.resolution, "different_scope");
        assert_eq!(saved.note, "统计口径不同");
        let list = store.list_conflict_resolutions().unwrap();
        assert_eq!(list.len(), 1);
        let (expected_a, expected_b) = if first < second {
            (first.clone(), second.clone())
        } else {
            (second.clone(), first.clone())
        };
        assert_eq!(saved.fact_a, expected_a);
        assert_eq!(saved.fact_b, expected_b);
        assert_eq!(list[0].fact_a, expected_a);
        assert!(store
            .save_conflict_resolution(&ConflictResolutionInput {
                fact_a: first.clone(),
                fact_b: second.clone(),
                resolution: "invalid".into(),
                note: String::new(),
            })
            .is_err());

        let review = store.set_review(&first, "needs_split", "含两个数字").unwrap().unwrap();
        assert_eq!(review.status, "needs_split");
        assert_eq!(store.list_reviews().unwrap().len(), 1);
        assert!(store.set_review(&first, "", "").unwrap().is_none());
        assert!(store.list_reviews().unwrap().is_empty());
    }

    #[test]
    fn history_domain_persists_and_is_not_downgraded() {
        let dir = std::env::temp_dir().join(format!("yebian-facts-domain-{}", Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = FactStore::open(&dir.join("facts.sqlite")).unwrap();
        let claim = "严嵩无他才略，唯一意媚上，窃权罔利。";
        let id = store
            .record_candidate_with_domain("note-a", "run-a", claim, "", "", &[], "history")
            .unwrap();
        let fact = store.get_fact(&id).unwrap().unwrap();
        assert_eq!(fact.domain, "history");

        store
            .record_candidate_with_domain("note-a", "run-b", claim, "", "", &[], "general")
            .unwrap();
        let fact = store.get_fact(&id).unwrap().unwrap();
        assert_eq!(fact.domain, "history");
    }

    #[test]
    fn history_mention_upgrades_general_fact() {
        let dir = std::env::temp_dir().join(format!("yebian-facts-domain-mention-{}", Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = FactStore::open(&dir.join("facts.sqlite")).unwrap();
        let claim = "周瑜判断曹军不过十五六万。";
        let id = store.record_candidate("note-a", "run-a", claim, "", "", &[]).unwrap();
        assert_eq!(store.get_fact(&id).unwrap().unwrap().domain, "general");
        store
            .record_mention_for_fact_with_domain(&id, "note-b", "run-b", claim, "", "history")
            .unwrap();
        assert_eq!(store.get_fact(&id).unwrap().unwrap().domain, "history");
    }

    #[test]
    fn evidence_quotes_strip_html_markup() {
        let dir = std::env::temp_dir().join(format!("yebian-facts-quote-{}", Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = FactStore::open(&dir.join("facts.sqlite")).unwrap();
        let id = store.record_candidate("note-a", "run-a", "1.8 million children received subsidies.", "", "", &[]).unwrap();
        store.record_verification(
            &id,
            "外部可印证",
            "deepseek",
            "deepseek-v4-flash",
            &[],
            &[FactEvidenceInput {
                source_url: "https://example.com/report".into(),
                title: "report".into(),
                quote: "1.8 million<sup>9</sup> children &amp; families".into(),
                relation: "supports".into(),
            }],
        ).unwrap();
        let fact = store.get_fact(&id).unwrap().expect("fact");
        assert_eq!(fact.evidence[0].quote, "1.8 million children & families");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_matched_claim_attaches_to_the_existing_fact_without_creating_another() {
        let dir = std::env::temp_dir().join(format!("yebian-facts-match-{}", Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = FactStore::open(&dir.join("facts.sqlite")).unwrap();
        let fact = store
            .record_candidate("note-a", "run-a", "美国每年为低收入家庭儿童提供的托育补贴约120亿美元。", "约120亿美元", "", &[])
            .unwrap();
        let candidates = store.candidate_facts_for_claim("美国每年为低收入家庭孩子提供的托育补贴约为120亿美元。", 5).unwrap();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].id, fact);
        assert_eq!(candidates[0].relation, "same_as");
        store
            .record_mention_for_fact(&fact, "note-b", "run-b", "美国每年为低收入家庭孩子提供的托育补贴约为120亿美元。", "约为120亿美元")
            .unwrap();
        let facts = store.facts_for_note("note-b", "run-b").unwrap();
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].id, fact);
        assert_eq!(facts[0].mentions.len(), 1);
        assert_eq!(store.fact_count_for_note("note-a").unwrap(), 1);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn reopening_the_store_does_not_reseed_or_wipe_existing_facts() {
        let dir = std::env::temp_dir().join(format!("yebian-facts-reopen-{}", Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("facts.sqlite");
        let id = {
            let store = FactStore::open(&path).unwrap();
            store.record_candidate("note-a", "run-a", "重新打开不应清空的事实。", "", "", &[]).unwrap()
        };
        let reopened = FactStore::open(&path).unwrap();
        assert!(reopened.get_fact(&id).unwrap().is_some());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn exact_claims_share_a_fact_and_keep_mentions() {
        let dir = std::env::temp_dir().join(format!("yebian-facts-{}", Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = FactStore::open(&dir.join("facts.sqlite")).unwrap();
        let first = store.record_candidate("note-a", "run-a", "2026 年选举花费 120 亿美元。", "2026 年选举花费 120 亿美元。", "", &[]).unwrap();
        let second = store.record_candidate("note-b", "run-b", "2026年选举花费120亿美元", "2026年选举花费120亿美元", "", &[]).unwrap();
        assert_eq!(first, second);
        assert_eq!(store.fact_count_for_note("note-a").unwrap(), 1);
        let facts = store.facts_for_note("note-b", "").unwrap();
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].status, "candidate");
        assert_eq!(facts[0].mentions.len(), 1);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn similar_claims_link_both_ways_but_shared_numbers_do_not() {
        let dir = std::env::temp_dir().join(format!("yebian-facts-related-{}", Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = FactStore::open(&dir.join("facts.sqlite")).unwrap();
        let bush_war = store
            .record_candidate("note-a", "run-a", "小布什政府将美国带入阿富汗战争和伊拉克战争。", "", "", &[])
            .unwrap();
        let bush_iraq = store
            .record_candidate(
                "note-b",
                "run-b",
                "小布什政府以伊拉克拥有大规模杀伤性武器的虚假情报为依据，将美国卷入伊拉克战争。",
                "",
                "",
                &[],
            )
            .unwrap();
        let election_cost = store
            .record_candidate("note-c", "run-c", "2026年美国中期选举耗资超过120亿美元。", "", "", &[])
            .unwrap();
        let child_subsidy = store
            .record_candidate("note-d", "run-d", "美国每年为低收入家庭儿童提供的托育补贴约120亿美元。", "", "", &[])
            .unwrap();

        let related_to_bush = store.get_fact(&bush_war).unwrap().expect("bush fact");
        assert_eq!(related_to_bush.related.len(), 1);
        assert_eq!(related_to_bush.related[0].other_id, bush_iraq);
        assert_eq!(related_to_bush.related[0].relation, "related");
        assert!(related_to_bush.related[0].confidence >= RELATED_THRESHOLD);
        let reverse = store.get_fact(&bush_iraq).unwrap().expect("iraq fact");
        assert_eq!(reverse.related.len(), 1);
        assert_eq!(reverse.related[0].other_id, bush_war);

        let cost = store.get_fact(&election_cost).unwrap().expect("cost fact");
        assert!(cost.related.is_empty(), "shared 120亿美元 must not imply related: {:?}", cost.related);
        let subsidy = store.get_fact(&child_subsidy).unwrap().expect("subsidy fact");
        assert!(subsidy.related.is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_verbatim_claim_moves_its_mention_to_the_latest_run() {
        let dir = std::env::temp_dir().join(format!("yebian-facts-rerun-{}", Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = FactStore::open(&dir.join("facts.sqlite")).unwrap();
        let claim = "美国全国有400万条含铅水管。";
        store.record_candidate("note-a", "run-old", claim, "美国全国有400万条含铅水管。", "", &[]).unwrap();
        store.record_candidate("note-a", "run-new", claim, "美国全国有400万条含铅水管。", "", &[]).unwrap();
        assert!(store.facts_for_note("note-a", "run-old").unwrap().is_empty());
        let latest = store.facts_for_note("note-a", "run-new").unwrap();
        assert_eq!(latest.len(), 1);
        assert_eq!(latest[0].mentions.len(), 1);
        assert_eq!(store.fact_count_for_note("note-a").unwrap(), 1);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn numeric_difference_is_a_conflict_candidate_and_blocks_merge() {
        let dir = std::env::temp_dir().join(format!("yebian-facts-conflict-{}", Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = FactStore::open(&dir.join("facts.sqlite")).unwrap();
        let four = store
            .record_candidate("note-a", "run-a", "美国全国有400万条含铅水管。", "", "", &[])
            .unwrap();
        let three = store
            .record_candidate("note-b", "run-b", "美国全国有300万条含铅水管。", "", "", &[])
            .unwrap();
        let fact = store.get_fact(&four).unwrap().expect("fact");
        assert_eq!(fact.related.len(), 1);
        assert_eq!(fact.related[0].relation, "contradicts");
        assert!(fact.related[0].confidence >= CONFLICT_THRESHOLD);
        let error = store.merge(&four, &three).unwrap_err();
        assert!(error.contains("同一事实"), "{error}");
        assert!(store.get_fact(&four).unwrap().is_some());
        assert!(store.get_fact(&three).unwrap().is_some());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn merge_moves_evidence_mentions_checks_and_writes_audit() {
        let dir = std::env::temp_dir().join(format!("yebian-facts-merge-{}", Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = FactStore::open(&dir.join("facts.sqlite")).unwrap();
        let target = store
            .record_candidate("note-a", "run-a", "美国每年为低收入家庭儿童提供的托育补贴约120亿美元。", "约120亿美元", "", &[])
            .unwrap();
        let source = store
            .record_candidate("note-b", "run-b", "美国每年为低收入家庭儿童提供的托育补贴约为120亿美元。", "约为120亿美元", "", &[])
            .unwrap();
        store.record_verification(
            &source,
            "外部可印证",
            "deepseek",
            "deepseek-v4-flash",
            &["US child care subsidy 12 billion".into()],
            &[FactEvidenceInput {
                source_url: "https://example.com/childcare".into(),
                title: "child care".into(),
                quote: "about 12 billion a year".into(),
                relation: "supports".into(),
            }],
        ).unwrap();
        let merged = store.merge(&target, &source).unwrap();
        assert_eq!(merged.mentions.len(), 2);
        assert_eq!(merged.evidence.len(), 1);
        assert_eq!(merged.status, "verified");
        assert!(store.get_fact(&source).unwrap().is_none());
        let conn = lock(&store).unwrap();
        let checks: i64 = conn.query_row("SELECT COUNT(*) FROM fact_checks WHERE fact_id = ?1", params![target], |row| row.get(0)).unwrap();
        let audits: i64 = conn.query_row("SELECT COUNT(*) FROM fact_merges WHERE target_fact_id = ?1 AND source_fact_id = ?2", params![target, source], |row| row.get(0)).unwrap();
        let snapshot: String = conn.query_row("SELECT snapshot_json FROM fact_merges WHERE source_fact_id = ?1", params![source], |row| row.get(0)).unwrap();
        assert_eq!(checks, 1);
        assert_eq!(audits, 1);
        assert!(snapshot.contains("note-b"));
        drop(conn);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn undo_merge_restores_source_and_target_state() {
        let dir = std::env::temp_dir().join(format!("yebian-facts-undo-{}", Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = FactStore::open(&dir.join("facts.sqlite")).unwrap();
        let target = store
            .record_candidate("note-a", "run-a", "美国每年为低收入家庭儿童提供的托育补贴约120亿美元。", "", "", &[])
            .unwrap();
        let source = store
            .record_candidate("note-b", "run-b", "美国每年为低收入家庭儿童提供的托育补贴约为120亿美元。", "", "", &[])
            .unwrap();
        store.record_verification(
            &source,
            "外部可印证",
            "deepseek",
            "deepseek-v4-flash",
            &["US child care subsidy 12 billion".into()],
            &[FactEvidenceInput {
                source_url: "https://example.com/childcare".into(),
                title: "child care".into(),
                quote: "about 12 billion a year".into(),
                relation: "supports".into(),
            }],
        ).unwrap();
        store.merge(&target, &source).unwrap();
        let merges = store.list_merges(10).unwrap();
        assert_eq!(merges.len(), 1);
        assert!(merges[0].can_undo);
        store.undo_merge(&merges[0].id).unwrap();
        let restored = store.get_fact(&source).unwrap().expect("source restored");
        assert_eq!(restored.status, "verified");
        assert_eq!(restored.mentions.len(), 1);
        assert_eq!(restored.evidence.len(), 1);
        let target_fact = store.get_fact(&target).unwrap().expect("target remains");
        assert_eq!(target_fact.status, "candidate");
        assert_eq!(target_fact.mentions.len(), 1);
        assert!(target_fact.evidence.is_empty());
        let after = store.list_merges(10).unwrap();
        assert_eq!(after[0].undone_at > 0, true);
        assert!(!after[0].can_undo);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn merge_rejects_low_similarity_and_keeps_both_facts() {
        let dir = std::env::temp_dir().join(format!("yebian-facts-merge-guard-{}", Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = FactStore::open(&dir.join("facts.sqlite")).unwrap();
        let target = store
            .record_candidate("note-a", "run-a", "美国每年为低收入家庭儿童提供的托育补贴约120亿美元。", "", "", &[])
            .unwrap();
        let other = store
            .record_candidate("note-b", "run-b", "2026年美国中期选举耗资超过120亿美元。", "", "", &[])
            .unwrap();
        let error = store.merge(&target, &other).unwrap_err();
        assert!(error.contains("相似度"), "{error}");
        assert!(store.get_fact(&target).unwrap().is_some());
        assert!(store.get_fact(&other).unwrap().is_some());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_later_fact_links_to_an_existing_fact_even_with_a_larger_id() {
        let dir = std::env::temp_dir().join(format!("yebian-facts-order-{}", Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = FactStore::open(&dir.join("facts.sqlite")).unwrap();
        let older = "美国每年为低收入家庭儿童提供的托育补贴约120亿美元。";
        let newer = "美国每年为低收入家庭儿童提供的托育补贴约为120亿美元。";
        let (first, second) = if fact_id_for(older) < fact_id_for(newer) { (older, newer) } else { (newer, older) };
        let first_id = store.record_candidate("note-a", "run-a", first, "", "", &[]).unwrap();
        let second_id = store.record_candidate("note-b", "run-b", second, "", "", &[]).unwrap();
        assert!(fact_id_for(first) < fact_id_for(second), "fixture must create the larger-id fact second");
        let related = store.get_fact(&second_id).unwrap().expect("second fact").related;
        assert!(related.iter().any(|item| item.other_id == first_id), "{related:?}");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn verification_updates_status_and_evidence() {
        let dir = std::env::temp_dir().join(format!("yebian-facts-verify-{}", Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = FactStore::open(&dir.join("facts.sqlite")).unwrap();
        let id = store.record_candidate("note-a", "run-a", "2026 年选举花费 120 亿美元。", "2026年选举花费120亿美元", "", &[]).unwrap();
        store.record_verification(
            &id,
            "外部可印证",
            "deepseek",
            "deepseek-v4-flash",
            &["2026 选举 120 亿美元".into()],
            &[FactEvidenceInput {
                source_url: "https://example.com/report".into(),
                title: "report".into(),
                quote: "the election cost 12 billion".into(),
                relation: "supports".into(),
            }],
        ).unwrap();
        let facts = store.facts_for_note("note-a", "run-a").unwrap();
        assert_eq!(facts[0].status, "verified");
        assert_eq!(facts[0].evidence.len(), 1);
        assert_eq!(facts[0].evidence[0].domain, "example.com");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn supporting_evidence_wins_over_a_disputed_verdict() {
        let supports = FactEvidenceInput {
            source_url: "https://example.com/a".into(),
            title: "a".into(),
            quote: "quote".into(),
            relation: "supports".into(),
        };
        let contradicts = FactEvidenceInput {
            source_url: "https://example.com/b".into(),
            title: "b".into(),
            quote: "other".into(),
            relation: "contradicts".into(),
        };
        assert_eq!(status_for_verification("存在争议", &[supports.clone()]), "verified");
        assert_eq!(status_for_verification("外部可印证", &[contradicts.clone()]), "disputed");
        assert_eq!(status_for_verification("仅非权威来源", &[supports.clone()]), "candidate");
        assert_eq!(status_for_verification("存在争议", &[]), "candidate");
        assert_eq!(status_for_verification("未找到外部出处", &[]), "unsupported");
    }
}
