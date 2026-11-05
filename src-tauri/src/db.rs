use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Instant;

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use uuid::Uuid;

use crate::vault::{self, ParsedNote};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteMeta {
    pub id: String,
    pub path: String,
    pub rel_path: String,
    pub title: String,
    pub up: String,
    pub date: String,
    pub domain: String,
    pub tags: Vec<String>,
    pub people: Vec<String>,
    pub events: Vec<String>,
    pub periods: Vec<String>,
    pub run_id: String,
    pub mtime: i64,
    pub chars: usize,
    pub annotation_count: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteDetail {
    pub meta: NoteMeta,
    pub body: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Annotation {
    pub id: String,
    pub note_id: String,
    pub block_id: String,
    pub quote: String,
    pub body: String,
    pub kind: String,
    pub created_at: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexStats {
    pub notes: usize,
    pub added: usize,
    pub updated: usize,
    pub removed: usize,
    pub skipped: usize,
    pub unchanged: usize,
    pub elapsed_ms: u128,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppStats {
    pub notes: usize,
    pub annotations: usize,
    pub vault: Option<String>,
}

pub struct Store {
    conn: Mutex<Option<Connection>>,
    user_path: PathBuf,
    index_path: PathBuf,
}

/// Bump when the FTS layout changes. Existing index files are rebuilt from
/// the vault on startup instead of being patched in place.
pub(crate) const FTS_SCHEMA: &str = "contentless-detail-none-v1";

const NOTE_SELECT: &str = "
  SELECT n.id, n.path, n.rel_path, n.title, n.up, n.date, n.domain,
         n.tags, n.people, n.events, n.periods, n.run_id, n.mtime, n.chars,
         (SELECT COUNT(*) FROM user.annotations a WHERE a.note_id = n.id) AS annotation_count
  FROM notes n
";

impl Store {
    pub fn open(path: &Path) -> Result<Self, String> {
        let user_path = path.to_path_buf();
        let index_path = path.with_file_name("yebian-index.sqlite");
        let backup_path = index_path.with_file_name("yebian-index.old");

        // The user database owns annotations/settings. Older versions also
        // carried notes/FTS here; those tables are left untouched but ignored.
        let user = Connection::open(&user_path).map_err(err)?;
        user.execute_batch(
            "
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;
            PRAGMA busy_timeout = 30000;
            PRAGMA temp_store = MEMORY;

            CREATE TABLE IF NOT EXISTS settings (
              key TEXT PRIMARY KEY,
              value TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS annotations (
              id TEXT PRIMARY KEY,
              note_id TEXT NOT NULL,
              block_id TEXT NOT NULL,
              quote TEXT NOT NULL,
              body TEXT NOT NULL DEFAULT '',
              kind TEXT NOT NULL DEFAULT '标记',
              created_at INTEGER NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_annotations_note ON annotations(note_id);
            ",
        )
        .map_err(err)?;
        drop(user);

        // Crash recovery around the file-level index swap.
        if !index_path.exists() && backup_path.exists() {
            fs::rename(&backup_path, &index_path).map_err(|error| error.to_string())?;
        }
        if index_path.exists() && backup_path.exists() {
            let _ = fs::remove_file(&backup_path);
        }

        let conn = open_index_database(&index_path, &user_path)?;
        Ok(Self {
            conn: Mutex::new(Some(conn)),
            user_path,
            index_path,
        })
    }

    pub fn index_path(&self) -> &Path {
        &self.index_path
    }

    /// Replace the index database with a fully built staging file. The reader
    /// connection is closed for the duration of the rename (milliseconds), then
    /// reopened against the new file; annotations/settings never move.
    pub fn swap_index(&self, staging: &Path) -> Result<(), String> {
        let mut guard = self.conn.lock().map_err(lock_err)?;
        if let Some(conn) = guard.take() {
            let _ = conn.execute("DETACH DATABASE user", []);
            drop(conn);
        }

        let live = &self.index_path;
        let backup = live.with_file_name("yebian-index.old");
        let _ = fs::remove_file(&backup);
        for suffix in ["-wal", "-shm"] {
            let _ = fs::remove_file(format!("{}{suffix}", live.display()));
        }

        let had_live = live.exists();
        if had_live {
            fs::rename(live, &backup).map_err(|error| error.to_string())?;
        }
        if let Err(error) = fs::rename(staging, live) {
            if had_live {
                let _ = fs::rename(&backup, live);
            }
            *guard = Some(open_index_database(live, &self.user_path)?);
            return Err(error.to_string());
        }
        match open_index_database(live, &self.user_path) {
            Ok(conn) => {
                *guard = Some(conn);
                let _ = fs::remove_file(&backup);
                Ok(())
            }
            Err(error) => {
                if had_live {
                    let _ = fs::remove_file(live);
                    let _ = fs::rename(&backup, live);
                }
                *guard = Some(open_index_database(live, &self.user_path)?);
                Err(error)
            }
        }
    }

    pub fn index_vault(&self, root: &Path, force: bool) -> Result<IndexStats, String> {
        let started = Instant::now();
        let paths = vault::walk_notes(root);
        let mut guard = self.conn.lock().map_err(lock_err)?;
        let conn = guard.as_mut().ok_or_else(|| "索引正在切换".to_string())?;
        let tx = conn.transaction().map_err(err)?;

        let mut existing: HashMap<String, i64> = HashMap::new();
        {
            let mut stmt = tx
                .prepare("SELECT rel_path, mtime FROM notes")
                .map_err(err)?;
            let rows = stmt
                .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)))
                .map_err(err)?;
            for row in rows {
                let (rel_path, mtime) = row.map_err(err)?;
                existing.insert(rel_path, mtime);
            }
        }

        // An interrupted write, or the one-time FTS layout migration above, can
        // leave the two tables out of sync. Rebuild everything in that case.
        let note_count: i64 = tx
            .query_row("SELECT COUNT(*) FROM notes", [], |row| row.get(0))
            .map_err(err)?;
        let fts_count: i64 = tx
            .query_row("SELECT COUNT(*) FROM notes_fts", [], |row| row.get(0))
            .map_err(err)?;
        // mtime is only meaningful within the same vault. If the root changed
        // (e.g. the folder was moved), every stored absolute path is stale.
        let root_key = root.to_string_lossy().to_string();
        let indexed_root: Option<String> = tx
            .query_row(
                "SELECT value FROM settings WHERE key = 'index_root'",
                [],
                |row| row.get(0),
            )
            .optional()
            .map_err(err)?;
        let root_changed = indexed_root.as_deref() != Some(root_key.as_str());
        let rebuild_all = force || root_changed || (note_count > 0 && fts_count != note_count);

        let mut seen = HashSet::new();
        let mut parsed: Vec<ParsedNote> = Vec::new();
        let mut parsed_rels: HashSet<String> = HashSet::new();
        let mut added = 0usize;
        let mut updated = 0usize;
        let mut skipped = 0usize;
        let mut unchanged = 0usize;

        for path in paths {
            let rel = vault::rel_path(&path, root);
            if !seen.insert(rel.clone()) {
                continue;
            }
            let mtime = vault::mtime_ms(&path);
            if !rebuild_all && existing.get(&rel) == Some(&mtime) {
                unchanged += 1;
                continue;
            }
            let Some(note) = vault::parse_file_at(&path, root, mtime) else {
                skipped += 1;
                continue;
            };
            if existing.contains_key(&rel) {
                updated += 1;
            } else {
                added += 1;
            }
            parsed_rels.insert(rel);
            parsed.push(note);
        }

        // Ids owned by rows this pass is not rewriting. A duplicate front-matter
        // id must not silently take one of them over.
        let mut used_ids: HashSet<String> = {
            let mut stmt = tx.prepare("SELECT id, rel_path FROM notes").map_err(err)?;
            let rows = stmt
                .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))
                .map_err(err)?;
            let mut out = HashSet::new();
            for row in rows {
                let (id, rel) = row.map_err(err)?;
                if seen.contains(&rel) && !parsed_rels.contains(&rel) {
                    out.insert(id);
                }
            }
            out
        };

        // A file moved outside the app keeps its content but loses its
        // path-derived id. Pair vanished and appeared notes by body hash, and
        // only when the pairing is unambiguous in both directions.
        let missing: Vec<String> = existing
            .keys()
            .filter(|rel| !seen.contains(*rel))
            .cloned()
            .collect();
        let mut orphan_hashes: HashMap<String, Vec<String>> = HashMap::new();
        for rel in &missing {
            let row: Option<(String, String)> = tx
                .query_row(
                    "SELECT id, body FROM notes WHERE rel_path = ?1",
                    params![rel],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()
                .map_err(err)?;
            if let Some((id, body)) = row {
                orphan_hashes.entry(vault::body_hash(&body)).or_default().push(id);
            }
        }

        if !orphan_hashes.is_empty() {
            let mut added_hash_counts: HashMap<String, usize> = HashMap::new();
            for note in &parsed {
                if note.front_id.is_none() && !existing.contains_key(&note.rel_path) {
                    *added_hash_counts
                        .entry(vault::body_hash(&note.body))
                        .or_default() += 1;
                }
            }
            let mut claimed: HashSet<String> = HashSet::new();
            for note in parsed.iter_mut() {
                if note.front_id.is_some() || existing.contains_key(&note.rel_path) {
                    continue;
                }
                let hash = vault::body_hash(&note.body);
                let Some(candidates) = orphan_hashes.get(&hash) else {
                    continue;
                };
                if candidates.len() != 1
                    || claimed.contains(&candidates[0])
                    || added_hash_counts.get(&hash).copied().unwrap_or(0) != 1
                {
                    continue;
                }
                let id = candidates[0].clone();
                if vault::ensure_id(&root.join(&note.rel_path), &id).unwrap_or(false) {
                    note.id = id.clone();
                    note.front_id = Some(id.clone());
                    claimed.insert(id);
                }
            }
        }

        // Drop the old row first so an id that hops between files (swap,
        // duplicate) cannot trip the primary key or the unique rel_path index.
        let mut migrations: Vec<(String, String)> = Vec::new();
        for note in &parsed {
            let prior: Option<String> = tx
                .query_row(
                    "SELECT id FROM notes WHERE rel_path = ?1",
                    params![note.rel_path],
                    |row| row.get(0),
                )
                .optional()
                .map_err(err)?;
            if let Some(old) = prior {
                if old != note.id {
                    migrations.push((old.clone(), note.id.clone()));
                }
                delete_note_row(&tx, &old)?;
            }
        }
        // Route annotations through a temporary id: an X->Y / Y->X swap would
        // otherwise chain and merge two histories.
        for (old, new) in &migrations {
            let holder = format!("migrate-{}", Uuid::new_v4().simple());
            tx.execute(
                "UPDATE user.annotations SET note_id = ?1 WHERE note_id = ?2",
                params![holder, old],
            )
            .map_err(err)?;
            tx.execute(
                "UPDATE user.annotations SET note_id = ?1 WHERE note_id = ?2",
                params![new, holder],
            )
            .map_err(err)?;
        }

        for note in &mut parsed {
            if used_ids.contains(&note.id) {
                let fresh = Uuid::new_v4().to_string();
                if note.front_id.is_some() {
                    let _ = vault::write_id(&root.join(&note.rel_path), &fresh);
                }
                note.id = fresh;
            }
            used_ids.insert(note.id.clone());
            upsert_note(&tx, note)?;
        }

        let mut removed = 0usize;
        for rel in &missing {
            let rowid: Option<i64> = tx
                .query_row(
                    "SELECT rowid FROM notes WHERE rel_path = ?1",
                    params![rel],
                    |row| row.get(0),
                )
                .optional()
                .map_err(err)?;
            if let Some(rowid) = rowid {
                tx.execute("DELETE FROM notes_fts WHERE rowid = ?1", params![rowid])
                    .map_err(err)?;
                tx.execute("DELETE FROM notes WHERE rowid = ?1", params![rowid])
                    .map_err(err)?;
                removed += 1;
            }
        }
        tx.execute(
            "INSERT INTO settings (key, value) VALUES ('index_root', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![root_key],
        )
        .map_err(err)?;
        tx.commit().map_err(err)?;
        let notes: i64 = conn
            .query_row("SELECT COUNT(*) FROM notes", [], |row| row.get(0))
            .map_err(err)?;

        let stats = IndexStats {
            notes: notes.max(0) as usize,
            added,
            updated,
            removed,
            skipped,
            unchanged,
            elapsed_ms: started.elapsed().as_millis(),
        };
        log::info!(
            "indexed {} notes (added {}, updated {}, removed {}, skipped {}, unchanged {}) in {}ms",
            stats.notes,
            stats.added,
            stats.updated,
            stats.removed,
            stats.skipped,
            stats.unchanged,
            stats.elapsed_ms
        );
        Ok(stats)
    }

    /// Notes whose body or title mentions any of the needles. The link
    /// rewriter uses this to avoid reading the whole vault on every move.
    pub fn notes_containing(&self, needles: &[String]) -> Result<Vec<String>, String> {
        if needles.is_empty() {
            return Ok(Vec::new());
        }
        let guard = self.conn.lock().map_err(lock_err)?;
        let conn = guard.as_ref().ok_or_else(|| "索引正在切换".to_string())?;
        let mut stmt = conn
            .prepare("SELECT rel_path FROM notes WHERE body LIKE ?1 OR title LIKE ?1")
            .map_err(err)?;
        let mut out = HashSet::new();
        for needle in needles {
            let escaped = needle.replace('%', "").replace('_', "");
            let rows = stmt
                .query_map(params![format!("%{escaped}%")], |row| row.get::<_, String>(0))
                .map_err(err)?;
            for row in rows {
                out.insert(row.map_err(err)?);
            }
        }
        Ok(out.into_iter().collect())
    }

    pub fn list_notes(&self) -> Result<Vec<NoteMeta>, String> {
        let guard = self.conn.lock().map_err(lock_err)?;
        let conn = guard.as_ref().ok_or_else(|| "索引正在切换".to_string())?;
        let sql = format!("{} ORDER BY n.date DESC, n.title ASC", NOTE_SELECT);
        let mut stmt = conn.prepare(&sql).map_err(err)?;
        let rows = stmt.query_map([], row_to_meta).map_err(err)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(err)
    }

    pub fn get_note(&self, id: &str) -> Result<Option<NoteDetail>, String> {
        let guard = self.conn.lock().map_err(lock_err)?;
        let conn = guard.as_ref().ok_or_else(|| "索引正在切换".to_string())?;
        let sql = format!("{} WHERE n.id = ?1", NOTE_SELECT);
        let note = conn
            .query_row(&sql, params![id], row_to_meta)
            .optional()
            .map_err(err)?;
        let Some(meta) = note else {
            return Ok(None);
        };
        let body: String = conn
            .query_row("SELECT body FROM notes WHERE id = ?1", params![id], |row| row.get(0))
            .map_err(err)?;
        Ok(Some(NoteDetail { meta, body }))
    }

    pub fn search_notes(&self, query: &str) -> Result<Vec<NoteMeta>, String> {
        let trimmed = query.trim();
        if trimmed.is_empty() {
            return Ok(Vec::new());
        }
        let tokens = tokenize_query(trimmed);
        let guard = self.conn.lock().map_err(lock_err)?;
        let conn = guard.as_ref().ok_or_else(|| "索引正在切换".to_string())?;

        let mut rowids: Vec<i64> = Vec::new();
        if !tokens.is_empty() {
            let and_query = tokens
                .iter()
                .map(|token| format!("\"{}\"", token.replace('"', "")))
                .collect::<Vec<_>>()
                .join(" AND ");
            rowids = fts_rowids(&conn, &and_query)?;
            if rowids.is_empty() && tokens.len() > 1 {
                let or_query = tokens
                    .iter()
                    .map(|token| format!("\"{}\"", token.replace('"', "")))
                    .collect::<Vec<_>>()
                    .join(" OR ");
                rowids = fts_rowids(&conn, &or_query)?;
            }
        }

        if rowids.is_empty() {
            let like = format!("%{}%", trimmed);
            let mut stmt = conn
                .prepare(
                    "SELECT n.rowid FROM notes n
                     WHERE n.title LIKE ?1 OR n.body LIKE ?1
                     ORDER BY n.date DESC LIMIT 40",
                )
                .map_err(err)?;
            let rows = stmt.query_map(params![like], |row| row.get::<_, i64>(0)).map_err(err)?;
            rowids = rows.collect::<Result<Vec<_>, _>>().map_err(err)?;
        }

        let sql = format!("{} WHERE n.rowid = ?1", NOTE_SELECT);
        let mut out = Vec::new();
        for rowid in rowids {
            let note = conn
                .query_row(&sql, params![rowid], row_to_meta)
                .optional()
                .map_err(err)?;
            if let Some(note) = note {
                out.push(note);
            }
        }
        Ok(out)
    }

    pub fn list_annotations(&self, note_id: &str) -> Result<Vec<Annotation>, String> {
        let guard = self.conn.lock().map_err(lock_err)?;
        let conn = guard.as_ref().ok_or_else(|| "索引正在切换".to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT id, note_id, block_id, quote, body, kind, created_at
                 FROM user.annotations WHERE note_id = ?1 ORDER BY created_at ASC",
            )
            .map_err(err)?;
        let rows = stmt
            .query_map(params![note_id], |row| {
                Ok(Annotation {
                    id: row.get(0)?,
                    note_id: row.get(1)?,
                    block_id: row.get(2)?,
                    quote: row.get(3)?,
                    body: row.get(4)?,
                    kind: row.get(5)?,
                    created_at: row.get(6)?,
                })
            })
            .map_err(err)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(err)
    }

    pub fn add_annotation(
        &self,
        note_id: &str,
        block_id: &str,
        quote: &str,
        body: &str,
        kind: &str,
    ) -> Result<Annotation, String> {
        let guard = self.conn.lock().map_err(lock_err)?;
        let conn = guard.as_ref().ok_or_else(|| "索引正在切换".to_string())?;
        let exists: Option<String> = conn
            .query_row("SELECT id FROM notes WHERE id = ?1", params![note_id], |row| row.get(0))
            .optional()
            .map_err(err)?;
        if exists.is_none() {
            return Err("note not found".into());
        }
        let annotation = Annotation {
            id: format!("a{}", Uuid::new_v4().simple()),
            note_id: note_id.to_string(),
            block_id: block_id.to_string(),
            quote: quote.to_string(),
            body: body.to_string(),
            kind: if kind.is_empty() { "标记".into() } else { kind.to_string() },
            created_at: chrono::Utc::now().timestamp_millis(),
        };
        conn.execute(
            "INSERT INTO user.annotations (id, note_id, block_id, quote, body, kind, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                annotation.id,
                annotation.note_id,
                annotation.block_id,
                annotation.quote,
                annotation.body,
                annotation.kind,
                annotation.created_at
            ],
        )
        .map_err(err)?;
        Ok(annotation)
    }

    pub fn delete_annotation(&self, id: &str) -> Result<(), String> {
        let guard = self.conn.lock().map_err(lock_err)?;
        let conn = guard.as_ref().ok_or_else(|| "索引正在切换".to_string())?;
        conn.execute("DELETE FROM user.annotations WHERE id = ?1", params![id])
            .map_err(err)?;
        Ok(())
    }

    pub fn get_setting(&self, key: &str) -> Result<Option<String>, String> {
        let guard = self.conn.lock().map_err(lock_err)?;
        let conn = guard.as_ref().ok_or_else(|| "索引正在切换".to_string())?;
        conn.query_row("SELECT value FROM user.settings WHERE key = ?1", params![key], |row| row.get(0))
            .optional()
            .map_err(err)
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<(), String> {
        let guard = self.conn.lock().map_err(lock_err)?;
        let conn = guard.as_ref().ok_or_else(|| "索引正在切换".to_string())?;
        conn.execute(
            "INSERT INTO user.settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )
        .map_err(err)?;
        Ok(())
    }

    pub fn get_vault(&self) -> Result<Option<String>, String> {
        self.get_setting("vault_path")
    }

    pub fn set_vault(&self, path: &str) -> Result<(), String> {
        self.set_setting("vault_path", path)
    }

    pub fn stats(&self, vault: Option<String>) -> Result<AppStats, String> {
        let guard = self.conn.lock().map_err(lock_err)?;
        let conn = guard.as_ref().ok_or_else(|| "索引正在切换".to_string())?;
        let notes: i64 = conn
            .query_row("SELECT COUNT(*) FROM notes", [], |row| row.get(0))
            .map_err(err)?;
        let annotations: i64 = conn
            .query_row("SELECT COUNT(*) FROM user.annotations", [], |row| row.get(0))
            .map_err(err)?;
        Ok(AppStats {
            notes: notes.max(0) as usize,
            annotations: annotations.max(0) as usize,
            vault,
        })
    }
}

pub(crate) fn open_index_database(index_path: &Path, user_path: &Path) -> Result<Connection, String> {
    let conn = Connection::open(index_path).map_err(err)?;
    let fts_existed: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'notes_fts')",
            [],
            |row| row.get(0),
        )
        .map_err(err)?;
    conn.execute_batch(
        "
        PRAGMA journal_mode = WAL;
        PRAGMA synchronous = NORMAL;
        PRAGMA busy_timeout = 30000;
        PRAGMA temp_store = MEMORY;

        CREATE TABLE IF NOT EXISTS notes (
          id TEXT PRIMARY KEY,
          path TEXT NOT NULL,
          rel_path TEXT NOT NULL,
          title TEXT NOT NULL,
          up TEXT NOT NULL DEFAULT '',
          date TEXT NOT NULL DEFAULT '',
          domain TEXT NOT NULL DEFAULT '未分类',
          tags TEXT NOT NULL DEFAULT '[]',
          people TEXT NOT NULL DEFAULT '[]',
          events TEXT NOT NULL DEFAULT '[]',
          periods TEXT NOT NULL DEFAULT '[]',
          body TEXT NOT NULL DEFAULT '',
          run_id TEXT NOT NULL DEFAULT '',
          mtime INTEGER NOT NULL DEFAULT 0,
          chars INTEGER NOT NULL DEFAULT 0,
          mtime_ns INTEGER NOT NULL DEFAULT 0,
          size INTEGER NOT NULL DEFAULT 0,
          content_hash TEXT NOT NULL DEFAULT ''
        );

        CREATE TABLE IF NOT EXISTS index_meta (
          key TEXT PRIMARY KEY,
          value TEXT NOT NULL
        );

        CREATE VIRTUAL TABLE IF NOT EXISTS notes_fts USING fts5(
          id UNINDEXED,
          title,
          tokens,
          tokenize = 'unicode61',
          detail = none,
          content = '',
          contentless_delete = 1
        );

        CREATE INDEX IF NOT EXISTS idx_notes_date ON notes(date DESC);
        CREATE UNIQUE INDEX IF NOT EXISTS idx_notes_rel ON notes(rel_path);
        ",
    )
    .map_err(err)?;
    if !fts_existed {
        conn.execute(
            "INSERT INTO index_meta (key, value) VALUES ('fts_schema', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![FTS_SCHEMA],
        )
        .map_err(err)?;
    }
    conn.execute(
        "ATTACH DATABASE ?1 AS user",
        params![user_path.to_string_lossy()],
    )
    .map_err(err)?;
    Ok(conn)
}

fn delete_note_row(tx: &rusqlite::Transaction<'_>, id: &str) -> Result<(), String> {
    let rowid: Option<i64> = tx
        .query_row("SELECT rowid FROM notes WHERE id = ?1", params![id], |row| row.get(0))
        .optional()
        .map_err(err)?;
    if let Some(rowid) = rowid {
        tx.execute("DELETE FROM notes_fts WHERE rowid = ?1", params![rowid])
            .map_err(err)?;
    }
    tx.execute("DELETE FROM notes WHERE id = ?1", params![id])
        .map_err(err)?;
    Ok(())
}

pub(crate) fn upsert_note(tx: &rusqlite::Transaction<'_>, note: &ParsedNote) -> Result<(), String> {
    let tags = serde_json::to_string(&note.tags).unwrap_or_else(|_| "[]".into());
    let people = serde_json::to_string(&note.people).unwrap_or_else(|_| "[]".into());
    let events = serde_json::to_string(&note.events).unwrap_or_else(|_| "[]".into());
    let periods = serde_json::to_string(&note.periods).unwrap_or_else(|_| "[]".into());

    tx.execute(
        "INSERT INTO notes (id, path, rel_path, title, up, date, domain, tags, people, events, periods, body, run_id, mtime, chars, mtime_ns, size, content_hash)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)
         ON CONFLICT(id) DO UPDATE SET
           path = excluded.path,
           rel_path = excluded.rel_path,
           title = excluded.title,
           up = excluded.up,
           date = excluded.date,
           domain = excluded.domain,
           tags = excluded.tags,
           people = excluded.people,
           events = excluded.events,
           periods = excluded.periods,
           body = excluded.body,
           run_id = excluded.run_id,
           mtime = excluded.mtime,
           chars = excluded.chars,
           mtime_ns = excluded.mtime_ns,
           size = excluded.size,
           content_hash = excluded.content_hash",
        params![
            note.id,
            note.path,
            note.rel_path,
            note.title,
            note.up,
            note.date,
            note.domain,
            tags,
            people,
            events,
            periods,
            note.body,
            note.run_id,
            note.mtime,
            note.chars as i64,
            note.mtime_ns,
            note.size,
            note.content_hash
        ],
    )
    .map_err(err)?;

    let search_text = format!(
        "{} {} {} {} {} {}",
        note.title,
        note.tags.join(" "),
        note.people.join(" "),
        note.events.join(" "),
        note.periods.join(" "),
        note.body
    );
    let tokens = tokenize_text(&search_text);
    let rowid: i64 = tx
        .query_row(
            "SELECT rowid FROM notes WHERE id = ?1",
            params![note.id],
            |row| row.get(0),
        )
        .map_err(err)?;
    tx.execute("DELETE FROM notes_fts WHERE rowid = ?1", params![rowid])
        .map_err(err)?;
    tx.execute(
        "INSERT INTO notes_fts (rowid, id, title, tokens) VALUES (?1, ?2, ?3, ?4)",
        params![rowid, note.id, note.title, tokens],
    )
    .map_err(err)?;
    Ok(())
}

fn fts_rowids(conn: &Connection, query: &str) -> Result<Vec<i64>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT rowid FROM main.notes_fts
             WHERE notes_fts MATCH ?1
             ORDER BY bm25(notes_fts)
             LIMIT 40",
        )
        .map_err(err)?;
    let rows = stmt.query_map(params![query], |row| row.get::<_, i64>(0)).map_err(err)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(err)
}

fn row_to_meta(row: &rusqlite::Row<'_>) -> rusqlite::Result<NoteMeta> {
    let tags: String = row.get("tags")?;
    let people: String = row.get("people")?;
    let events: String = row.get("events")?;
    let periods: String = row.get("periods")?;
    Ok(NoteMeta {
        id: row.get("id")?,
        path: row.get("path")?,
        rel_path: row.get("rel_path")?,
        title: row.get("title")?,
        up: row.get("up")?,
        date: row.get("date")?,
        domain: row.get("domain")?,
        tags: serde_json::from_str(&tags).unwrap_or_default(),
        people: serde_json::from_str(&people).unwrap_or_default(),
        events: serde_json::from_str(&events).unwrap_or_default(),
        periods: serde_json::from_str(&periods).unwrap_or_default(),
        run_id: row.get("run_id")?,
        mtime: row.get("mtime")?,
        chars: row.get::<_, i64>("chars")?.max(0) as usize,
        annotation_count: row.get("annotation_count")?,
    })
}

fn is_cjk(ch: char) -> bool {
    ('\u{3400}'..='\u{4DBF}').contains(&ch)
        || ('\u{4E00}'..='\u{9FFF}').contains(&ch)
        || ('\u{F900}'..='\u{FAFF}').contains(&ch)
}

/// Dependency-free CJK search tokenizer.
///
/// FTS5's unicode61 treats a whole Chinese sentence as one token, so a query
/// like "养老" never matches "养老金". We index overlapping bigrams instead:
/// "养老金" -> "养老 老金". For Latin text we keep whole lowercased words.
/// This is a deliberately small, offline tokenizer; it trades a little
/// precision for zero dictionary/assets and predictable behavior.
fn tokenize_text(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len() * 2);
    let mut index = 0;
    while index < chars.len() {
        let ch = chars[index];
        if is_cjk(ch) {
            let start = index;
            while index < chars.len() && is_cjk(chars[index]) {
                index += 1;
            }
            let run = &chars[start..index];
            if run.len() == 1 {
                if !out.is_empty() {
                    out.push(' ');
                }
                out.push(run[0]);
            } else {
                for offset in 0..run.len() - 1 {
                    if !out.is_empty() {
                        out.push(' ');
                    }
                    out.push(run[offset]);
                    out.push(run[offset + 1]);
                }
            }
        } else if ch.is_alphanumeric() {
            let start = index;
            while index < chars.len()
                && !is_cjk(chars[index])
                && (chars[index].is_alphanumeric()
                    || chars[index] == '_'
                    || chars[index] == '-'
                    || chars[index] == '.')
            {
                index += 1;
            }
            if !out.is_empty() {
                out.push(' ');
            }
            for lower in chars[start..index]
                .iter()
                .flat_map(|value| value.to_lowercase())
            {
                out.push(lower);
            }
        } else {
            index += 1;
        }
    }
    out
}

fn tokenize_query(query: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for token in tokenize_text(query).split_whitespace() {
        if seen.insert(token.to_string()) {
            out.push(token.to_string());
        }
    }
    out
}

fn err(error: rusqlite::Error) -> String {
    error.to_string()
}

fn lock_err<T>(_: T) -> String {
    "database lock poisoned".to_string()
}

#[cfg(test)]
mod bench {
    use super::*;
    use std::fs;
    use std::path::Path;
    use std::time::Instant;

    fn corpus(root: &Path, count: usize) {
        // Use the real sample note body (~1.4k Chinese chars) instead of a
        // short synthetic string, so tokenization and FTS volume match the app.
        let sample = include_str!("../../sample-vault/shebao.md");
        let body = sample.splitn(3, "---").nth(2).unwrap_or(sample).trim();
        for index in 0..count {
            let id = format!("{index:05}");
            let content = format!(
                "---\n标题: 测试笔记 {id}\n作者: 测试UP\n创建日期: 2026-10-01\n\
                 tags:\n- 领域/财经\n- 类型/视频笔记\n- 主题/养老金\n\
                 人物:\n- 测试人物\n事件:\n- 测试事件\n时期:\n- 当代\n---\n\n\
                 # 测试笔记 {id}\n\n## 速览\n\n- 这是第 {id} 条测试判断，养老金与财政。\n\n\
                 ## 正文\n\n### 第 {id} 节\n\n{body}\n\n第 {id} 条唯一内容：养老金与财政。\n"
            );
            fs::write(root.join(format!("note-{id}.md")), content).expect("write note");
        }
    }

    fn percentile(values: &mut [u128], p: usize) -> u128 {
        values.sort_unstable();
        if values.is_empty() {
            return 0;
        }
        let index = ((values.len() - 1) * p) / 100;
        values[index]
    }

    fn time_search(store: &Store, query: &str, iterations: usize) -> (u128, u128, usize) {
        let mut samples = Vec::new();
        let mut hits = 0;
        for _ in 0..iterations {
            let started = Instant::now();
            hits = store.search_notes(query).expect("search").len();
            samples.push(started.elapsed().as_micros());
        }
        (percentile(&mut samples, 50), percentile(&mut samples, 95), hits)
    }

    #[test]
    #[ignore = "run explicitly: cargo test --release bench_index -- --ignored --nocapture"]
    fn bench_index_search() {
        for count in [1_000usize, 10_000] {
            let root = std::env::temp_dir().join(format!(
                "yebian-bench-{}-{}",
                count,
                std::process::id()
            ));
            let _ = fs::remove_dir_all(&root);
            fs::create_dir_all(&root).expect("create corpus");

            let generate_started = Instant::now();
            corpus(&root, count);
            let generate_ms = generate_started.elapsed().as_millis();

            let db_path = root.join("bench.sqlite");
            let store = Store::open(&db_path).expect("open store");

            let first = store.index_vault(&root, false).expect("first index");
            let second = store.index_vault(&root, false).expect("second index");
            let forced = store.index_vault(&root, true).expect("forced index");

            let (p50, p95, hits) = time_search(&store, "养老金", 30);

            let list_started = Instant::now();
            let notes = store.list_notes().expect("list");
            let list_ms = list_started.elapsed().as_millis();

            let note_id = notes.first().map(|note| note.id.clone()).unwrap_or_default();
            let open_started = Instant::now();
            let detail = store.get_note(&note_id).expect("get note");
            let open_ms = open_started.elapsed().as_micros();

            let (miss_p50, miss_p95, _) = time_search(&store, "不存在的词组", 10);

            let mut write_samples = Vec::new();
            for _ in 0..100 {
                let started = Instant::now();
                let annotation = store
                    .add_annotation(&note_id, "bench-block", "bench quote", "bench body", "标记")
                    .expect("add annotation");
                write_samples.push(started.elapsed().as_micros());
                store
                    .delete_annotation(&annotation.id)
                    .expect("delete annotation");
            }
            let write_p50 = percentile(&mut write_samples, 50);
            let write_p95 = percentile(&mut write_samples, 95);

            println!(
                "BENCH notes={count} generate_ms={generate_ms} first_index_ms={} second_index_ms={} \
                 second_unchanged={} forced_index_ms={} search_p50_us={p50} search_p95_us={p95} \
                 search_hits={hits} list_ms={list_ms} open_us={open_ms} like_p50_us={miss_p50} \
                 like_p95_us={miss_p95} anno_write_p50_us={write_p50} anno_write_p95_us={write_p95} \
                 body_chars={}",
                first.elapsed_ms,
                second.elapsed_ms,
                second.unchanged,
                forced.elapsed_ms,
                detail.map(|item| item.meta.chars).unwrap_or(0)
            );

            drop(store);
            let _ = fs::remove_dir_all(&root);
        }
    }
}
