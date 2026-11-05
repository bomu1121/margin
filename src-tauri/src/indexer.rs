// Incremental index service.
//
// One writer thread owns the live index connection; readers keep using the
// Store connection. WAL lets those readers see the previous snapshot while a
// batch/swap transaction is running, so list/search no longer wait for a full
// index. The watcher and file operations enqueue small deltas instead of
// calling index_vault for the whole vault.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use rusqlite::{params, params_from_iter, Connection, OptionalExtension};
use serde::Serialize;

use crate::db::{self, IndexStats, Store};
use crate::vault::{self, ParsedNote};

const BATCH: usize = 200;
const STAGING_BATCH: usize = 100_000;

#[derive(Debug)]
pub enum IndexJob {
    Startup {
        root: PathBuf,
    },
    Delta {
        root: PathBuf,
        paths: Vec<PathBuf>,
        reply: Option<Sender<Result<IndexStats, String>>>,
    },
    Rebuild {
        root: PathBuf,
        reply: Option<Sender<Result<IndexStats, String>>>,
    },
    Remove {
        root: PathBuf,
        rel_paths: Vec<String>,
        reply: Option<Sender<Result<IndexStats, String>>>,
    },
    Move {
        root: PathBuf,
        pairs: Vec<(String, String)>,
        reply: Option<Sender<Result<IndexStats, String>>>,
    },
    Shutdown,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct IndexStatus {
    pub state: String,
    pub done: usize,
    pub total: usize,
    pub current: String,
    pub generation: i64,
    pub error: Option<String>,
    pub root: Option<String>,
}

pub type StatusSink = Arc<dyn Fn(&IndexStatus) + Send + Sync>;

pub struct IndexService {
    tx: Sender<IndexJob>,
    status: Arc<Mutex<IndexStatus>>,
    root: Arc<Mutex<Option<String>>>,
}

impl IndexService {
    pub fn open(
        index_path: PathBuf,
        sink: Option<StatusSink>,
        store: Option<Arc<Store>>,
    ) -> Result<Self, String> {
        let (tx, rx) = mpsc::channel();
        let status = Arc::new(Mutex::new(IndexStatus {
            state: "idle".into(),
            ..IndexStatus::default()
        }));
        let root = Arc::new(Mutex::new(None));
        let has_sink = sink.is_some();
        let thread_status = status.clone();
        let thread_root = root.clone();
        thread::Builder::new()
            .name("yebian-index".into())
            .spawn(move || actor_loop(rx, index_path, thread_status, thread_root, sink, store))
            .map_err(|error| error.to_string())?;
        if has_sink {
            // Correctness backstop: watcher events are hints and can be lost
            // (network drives, overflow, downtime). A full stat pass every
            // 15 minutes catches drift without writing unchanged notes.
            let timer_tx = tx.clone();
            let timer_root = root.clone();
            thread::Builder::new()
                .name("yebian-index-reconcile".into())
                .spawn(move || loop {
                    thread::sleep(Duration::from_secs(900));
                    let root = timer_root.lock().ok().and_then(|root| root.clone());
                    let Some(root) = root else { continue };
                    if timer_tx
                        .send(IndexJob::Startup {
                            root: PathBuf::from(root),
                        })
                        .is_err()
                    {
                        break;
                    }
                })
                .ok();
        }
        Ok(Self { tx, status, root })
    }

    pub fn sender(&self) -> Sender<IndexJob> {
        self.tx.clone()
    }

    pub fn start(&self, root: &Path) {
        let key = root_key(root);
        if let Ok(mut current) = self.root.lock() {
            *current = Some(key);
        }
        let _ = self.tx.send(IndexJob::Startup {
            root: root.to_path_buf(),
        });
    }

    pub fn shutdown(&self) {
        let _ = self.tx.send(IndexJob::Shutdown);
    }

    pub fn status(&self) -> IndexStatus {
        self.status
            .lock()
            .map(|status| status.clone())
            .unwrap_or_default()
    }

    pub fn delta_wait(&self, root: &Path, paths: Vec<PathBuf>) -> Result<IndexStats, String> {
        let (reply, rx) = mpsc::channel();
        self.tx
            .send(IndexJob::Delta {
                root: root.to_path_buf(),
                paths,
                reply: Some(reply),
            })
            .map_err(|error| error.to_string())?;
        wait_reply(rx)
    }

    pub fn rebuild_wait(&self, root: &Path) -> Result<IndexStats, String> {
        let (reply, rx) = mpsc::channel();
        self.tx
            .send(IndexJob::Rebuild {
                root: root.to_path_buf(),
                reply: Some(reply),
            })
            .map_err(|error| error.to_string())?;
        wait_reply(rx)
    }

    pub fn remove_wait(&self, root: &Path, rel_paths: Vec<String>) -> Result<IndexStats, String> {
        let (reply, rx) = mpsc::channel();
        self.tx
            .send(IndexJob::Remove {
                root: root.to_path_buf(),
                rel_paths,
                reply: Some(reply),
            })
            .map_err(|error| error.to_string())?;
        wait_reply(rx)
    }

    pub fn move_wait(
        &self,
        root: &Path,
        pairs: Vec<(String, String)>,
    ) -> Result<IndexStats, String> {
        let (reply, rx) = mpsc::channel();
        self.tx
            .send(IndexJob::Move {
                root: root.to_path_buf(),
                pairs,
                reply: Some(reply),
            })
            .map_err(|error| error.to_string())?;
        wait_reply(rx)
    }
}

fn wait_reply(rx: Receiver<Result<IndexStats, String>>) -> Result<IndexStats, String> {
    rx.recv_timeout(Duration::from_secs(180))
        .map_err(|_| "索引任务超时".to_string())?
}

struct StatusEmitter {
    status: Arc<Mutex<IndexStatus>>,
    sink: Option<StatusSink>,
    last_emit: Instant,
    last_state: String,
}

impl StatusEmitter {
    fn new(status: Arc<Mutex<IndexStatus>>, sink: Option<StatusSink>) -> Self {
        Self {
            status,
            sink,
            last_emit: Instant::now() - Duration::from_secs(1),
            last_state: String::new(),
        }
    }

    fn status(&self) -> IndexStatus {
        self.status.lock().map(|s| s.clone()).unwrap_or_default()
    }

    #[allow(clippy::too_many_arguments)]
    fn set(
        &mut self,
        state: &str,
        done: usize,
        total: usize,
        current: &str,
        error: Option<String>,
        root: Option<String>,
        generation: i64,
        force: bool,
    ) {
        let snapshot = {
            let Ok(mut guard) = self.status.lock() else {
                return;
            };
            guard.state = state.to_string();
            guard.done = done;
            guard.total = total;
            guard.current = current.to_string();
            guard.error = error;
            guard.root = root;
            guard.generation = generation;
            guard.clone()
        };
        let should_emit =
            force || state != self.last_state || self.last_emit.elapsed() >= Duration::from_millis(100);
        if !should_emit {
            return;
        }
        self.last_state = state.to_string();
        self.last_emit = Instant::now();
        if let Some(sink) = &self.sink {
            sink(&snapshot);
        }
    }
}

fn actor_loop(
    rx: Receiver<IndexJob>,
    index_path: PathBuf,
    status: Arc<Mutex<IndexStatus>>,
    root_state: Arc<Mutex<Option<String>>>,
    sink: Option<StatusSink>,
    store: Option<Arc<Store>>,
) {
    cleanup_staging(&index_path);
    let mut conn = match open_writer(&index_path) {
        Ok(conn) => Some(conn),
        Err(error) => {
            if let Ok(mut guard) = status.lock() {
                guard.state = "error".into();
                guard.error = Some(error.to_string());
            }
            return;
        }
    };
    let mut emitter = StatusEmitter::new(status, sink);

    while let Ok(job) = rx.recv() {
        match job {
            IndexJob::Startup { root } => {
                let started = Instant::now();
                let result = startup_job(
                    &mut conn,
                    &index_path,
                    &store,
                    &root,
                    &mut emitter,
                    started,
                );
                finish(&result, &mut emitter, &root_state, &root);
            }
            IndexJob::Delta {
                root,
                paths,
                reply,
            } => {
                let started = Instant::now();
                let current = root_state.lock().ok().and_then(|root| root.clone());
                let result = match conn.as_mut() {
                    Some(live) if current.as_deref() == Some(root_key(&root).as_str()) => {
                        delta_job(live, &root, Some(paths), &mut emitter, started)
                    }
                    _ => match conn.as_ref() {
                        Some(live) => empty_stats(live),
                        None => Err("索引正在切换".to_string()),
                    },
                };
                finish(&result, &mut emitter, &root_state, &root);
                if let Some(reply) = reply {
                    let _ = reply.send(result);
                }
            }
            IndexJob::Rebuild { root, reply } => {
                let started = Instant::now();
                let result = rebuild_job(
                    &mut conn,
                    &index_path,
                    &store,
                    &root,
                    &mut emitter,
                    started,
                );
                finish(&result, &mut emitter, &root_state, &root);
                if let Some(reply) = reply {
                    let _ = reply.send(result);
                }
            }
            IndexJob::Remove {
                root,
                rel_paths,
                reply,
            } => {
                let started = Instant::now();
                let result = match conn.as_mut() {
                    Some(live) => remove_job(live, &rel_paths, &mut emitter, started),
                    None => Err("索引正在切换".to_string()),
                };
                finish(&result, &mut emitter, &root_state, &root);
                if let Some(reply) = reply {
                    let _ = reply.send(result);
                }
            }
            IndexJob::Move {
                root,
                pairs,
                reply,
            } => {
                let started = Instant::now();
                let result = match conn.as_mut() {
                    Some(live) => move_job(live, &root, &pairs, &mut emitter, started),
                    None => Err("索引正在切换".to_string()),
                };
                finish(&result, &mut emitter, &root_state, &root);
                if let Some(reply) = reply {
                    let _ = reply.send(result);
                }
            }
            IndexJob::Shutdown => break,
        }
    }
}

fn finish(
    result: &Result<IndexStats, String>,
    emitter: &mut StatusEmitter,
    root_state: &Arc<Mutex<Option<String>>>,
    root: &Path,
) {
    let key = root_key(root);
    if result.is_ok() {
        if let Ok(mut current) = root_state.lock() {
            *current = Some(key.clone());
        }
    }
    let status = emitter.status();
    match result {
        Ok(_) => emitter.set(
            "idle",
            status.total,
            status.total,
            "",
            None,
            Some(key),
            status.generation,
            true,
        ),
        Err(error) => emitter.set(
            "error",
            status.done,
            status.total,
            "",
            Some(error.clone()),
            Some(key),
            status.generation,
            true,
        ),
    }
}

fn startup_job(
    conn: &mut Option<Connection>,
    index_path: &Path,
    store: &Option<Arc<Store>>,
    root: &Path,
    emitter: &mut StatusEmitter,
    started: Instant,
) -> Result<IndexStats, String> {
    let key = root_key(root);
    let live = conn.as_ref().ok_or_else(|| "索引正在切换".to_string())?;
    let meta_root = meta_get(live, "root")?;
    let note_count: i64 = live
        .query_row("SELECT COUNT(*) FROM notes", [], |row| row.get(0))
        .map_err(db_err)?;
    if meta_root.as_deref() != Some(key.as_str()) {
        if note_count == 0 {
            let live = conn.as_mut().ok_or_else(|| "索引正在切换".to_string())?;
            return delta_job(live, root, None, emitter, started);
        }
        return rebuild_job(conn, index_path, store, root, emitter, started);
    }
    let live = conn.as_mut().ok_or_else(|| "索引正在切换".to_string())?;
    delta_job(live, root, None, emitter, started)
}

fn delta_job(
    conn: &mut Connection,
    root: &Path,
    paths: Option<Vec<PathBuf>>,
    emitter: &mut StatusEmitter,
    started: Instant,
) -> Result<IndexStats, String> {
    match paths {
        Some(paths) => {
            let needs_full = paths.len() > 2000
                || paths.iter().any(|path| {
                    path.is_dir()
                        || !path
                            .extension()
                            .map(|ext| ext.eq_ignore_ascii_case("md"))
                            .unwrap_or(false)
                });
            if needs_full {
                full_scan_delta(conn, root, emitter, started)
            } else {
                apply_paths(conn, root, &paths, emitter, started, 0)
            }
        }
        None => full_scan_delta(conn, root, emitter, started),
    }
}

fn full_scan_delta(
    conn: &mut Connection,
    root: &Path,
    emitter: &mut StatusEmitter,
    started: Instant,
) -> Result<IndexStats, String> {
    let files = vault::walk_notes(root);
    let existing = meta_map(conn)?;
    let total = files.len();
    let mut seen: HashSet<String> = HashSet::new();
    let mut changed: Vec<PathBuf> = Vec::new();
    let mut unchanged = 0usize;
    emitter.set("scanning", 0, total, "", None, Some(root_key(root)), 0, true);
    for (index, path) in files.iter().enumerate() {
        let rel = vault::rel_path(path, root);
        if !seen.insert(rel.clone()) {
            continue;
        }
        let mtime_ns = vault::mtime_ns(path);
        let size = vault::file_size(path);
        let fresh = match existing.get(&rel) {
            Some((old_mtime, old_size, old_hash)) => {
                *old_mtime == mtime_ns && *old_size == size && !old_hash.is_empty()
            }
            None => false,
        };
        if fresh {
            unchanged += 1;
        } else {
            changed.push(path.clone());
        }
        if index % 200 == 0 || index + 1 == total {
            emitter.set(
                "scanning",
                index + 1,
                total,
                &rel,
                None,
                Some(root_key(root)),
                0,
                false,
            );
        }
    }
    for rel in existing.keys() {
        if !seen.contains(rel) {
            changed.push(root.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR)));
        }
    }
    if changed.is_empty() {
        let generation = bump_generation(conn)?;
        set_complete(conn, root, generation)?;
        emitter.set(
            "idle",
            total,
            total,
            "",
            None,
            Some(root_key(root)),
            generation,
            true,
        );
        return stats(conn, 0, 0, 0, 0, unchanged, started);
    }
    apply_paths(conn, root, &changed, emitter, started, unchanged)
}

fn apply_paths(
    conn: &mut Connection,
    root: &Path,
    paths: &[PathBuf],
    emitter: &mut StatusEmitter,
    started: Instant,
    unchanged: usize,
) -> Result<IndexStats, String> {
    let root_key = root_key(root);
    let mut rels: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for path in paths {
        let rel = vault::rel_path(path, root);
        if !is_markdown(&rel) || !seen.insert(rel.clone()) {
            continue;
        }
        rels.push(rel);
    }
    let total = rels.len();
    if total == 0 {
        return stats(conn, 0, 0, 0, 0, unchanged, started);
    }
    emitter.set(
        "indexing",
        0,
        total,
        "",
        None,
        Some(root_key.clone()),
        0,
        true,
    );

    let rows = load_rows(conn, &rels)?;
    let mut used_ids: HashSet<String> = conn
        .prepare("SELECT id FROM notes")
        .map_err(db_err)?
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(db_err)?
        .collect::<Result<HashSet<_>, _>>()
        .map_err(db_err)?;

    let mut parsed: Vec<ParsedNote> = Vec::new();
    let mut parsed_rels: HashSet<String> = HashSet::new();
    let mut skipped = 0usize;
    for (index, rel) in rels.iter().enumerate() {
        let abs = root.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
        if abs.is_file() {
            match vault::parse_file_at(&abs, root, vault::mtime_ms(&abs)) {
                Some(note) => {
                    parsed_rels.insert(note.rel_path.clone());
                    parsed.push(note);
                }
                None => skipped += 1,
            }
        }
        if index % 200 == 0 || index + 1 == total {
            emitter.set(
                "indexing",
                index + 1,
                total,
                rel,
                None,
                Some(root_key.clone()),
                0,
                false,
            );
        }
    }

    let mut removed: Vec<String> = rows
        .keys()
        .filter(|rel| !parsed_rels.contains(*rel))
        .cloned()
        .collect();
    removed.sort();

    let mut removed_by_hash: HashMap<String, Vec<String>> = HashMap::new();
    for rel in &removed {
        if let Some(row) = rows.get(rel) {
            removed_by_hash
                .entry(vault::body_hash(&row.body))
                .or_default()
                .push(row.id.clone());
            used_ids.remove(&row.id);
        }
    }

    let mut added = 0usize;
    let mut updated = 0usize;
    for note in &mut parsed {
        let rel = note.rel_path.clone();
        if let Some(row) = rows.get(&rel) {
            note.id = row.id.clone();
            used_ids.insert(note.id.clone());
            updated += 1;
            continue;
        }
        let mut chosen: Option<String> = None;
        if let Some(front) = &note.front_id {
            if !used_ids.contains(front) {
                chosen = Some(front.clone());
            }
        }
        if chosen.is_none() {
            if let Some(candidates) = removed_by_hash.get(&note.content_hash) {
                if candidates.len() == 1 && !used_ids.contains(&candidates[0]) {
                    chosen = Some(candidates[0].clone());
                    let abs = root.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
                    let _ = vault::ensure_id(&abs, &candidates[0]);
                }
            }
        }
        let id = chosen.unwrap_or_else(|| vault::note_id(&rel));
        let id = if used_ids.contains(&id) {
            let fresh = uuid::Uuid::new_v4().to_string();
            let abs = root.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
            if note.front_id.is_some() {
                let _ = vault::write_id(&abs, &fresh);
            } else {
                let _ = vault::ensure_id(&abs, &fresh);
            }
            fresh
        } else {
            id
        };
        note.id = id.clone();
        used_ids.insert(id);
        added += 1;
    }

    // Delete rows that no longer have a file, then write the current batch.
    // Deleting by rel_path in one statement avoids the old per-row FTS loop.
    let mut removed_count = 0usize;
    for chunk in removed.chunks(400) {
        let placeholders = vec!["?"; chunk.len()].join(",");
        let sql = format!("SELECT rowid FROM notes WHERE rel_path IN ({placeholders})");
        let rowids: Vec<i64> = conn
            .prepare(&sql)
            .map_err(db_err)?
            .query_map(params_from_iter(chunk.iter()), |row| row.get(0))
            .map_err(db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_err)?;
        if !rowids.is_empty() {
            let tx = conn.transaction().map_err(db_err)?;
            for rowid in &rowids {
                tx.execute("DELETE FROM notes_fts WHERE rowid = ?1", params![rowid])
                    .map_err(db_err)?;
            }
            tx.execute(
                &format!("DELETE FROM notes WHERE rel_path IN ({placeholders})"),
                params_from_iter(chunk.iter()),
            )
            .map_err(db_err)?;
            tx.commit().map_err(db_err)?;
            removed_count += rowids.len();
        }
    }

    let mut done = 0usize;
    for chunk in parsed.chunks(BATCH) {
        let tx = conn.transaction().map_err(db_err)?;
        for note in chunk {
            db::upsert_note(&tx, note)?;
        }
        tx.commit().map_err(db_err)?;
        done += chunk.len();
        emitter.set(
            "indexing",
            done,
            parsed.len(),
            chunk.last().map(|note| note.rel_path.as_str()).unwrap_or(""),
            None,
            Some(root_key.clone()),
            0,
            false,
        );
    }

    let generation = bump_generation(conn)?;
    set_complete(conn, root, generation)?;
    emitter.set(
        "idle",
        parsed.len(),
        parsed.len(),
        "",
        None,
        Some(root_key),
        generation,
        true,
    );
    stats(conn, added, updated, removed_count, skipped, unchanged, started)
}

fn remove_job(
    conn: &mut Connection,
    rel_paths: &[String],
    emitter: &mut StatusEmitter,
    started: Instant,
) -> Result<IndexStats, String> {
    let rels: Vec<String> = rel_paths
        .iter()
        .filter(|rel| is_markdown(rel))
        .cloned()
        .collect();
    let total = rels.len();
    let mut removed = 0usize;
    emitter.set("indexing", 0, total, "", None, None, 0, true);
    for (index, chunk) in rels.chunks(400).enumerate() {
        let placeholders = vec!["?"; chunk.len()].join(",");
        let sql = format!("SELECT rowid FROM notes WHERE rel_path IN ({placeholders})");
        let rowids: Vec<i64> = conn
            .prepare(&sql)
            .map_err(db_err)?
            .query_map(params_from_iter(chunk.iter()), |row| row.get(0))
            .map_err(db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_err)?;
        if !rowids.is_empty() {
            let tx = conn.transaction().map_err(db_err)?;
            for rowid in &rowids {
                tx.execute("DELETE FROM notes_fts WHERE rowid = ?1", params![rowid])
                    .map_err(db_err)?;
            }
            tx.execute(
                &format!("DELETE FROM notes WHERE rel_path IN ({placeholders})"),
                params_from_iter(chunk.iter()),
            )
            .map_err(db_err)?;
            tx.commit().map_err(db_err)?;
            removed += rowids.len();
        }
        emitter.set(
            "indexing",
            std::cmp::min((index + 1) * 400, total),
            total,
            "",
            None,
            None,
            0,
            false,
        );
    }
    let generation = bump_generation(conn)?;
    if removed > 1000 {
        let _ = conn.execute("INSERT INTO notes_fts(notes_fts) VALUES('optimize')", []);
        compact(conn);
    }
    emitter.set("idle", total, total, "", None, None, generation, true);
    stats(conn, 0, 0, removed, 0, 0, started)
}

fn move_job(
    conn: &mut Connection,
    root: &Path,
    pairs: &[(String, String)],
    emitter: &mut StatusEmitter,
    started: Instant,
) -> Result<IndexStats, String> {
    if pairs.is_empty() {
        return stats(conn, 0, 0, 0, 0, 0, started);
    }
    emitter.set("indexing", 0, pairs.len(), "", None, None, 0, true);
    let tx = conn.transaction().map_err(db_err)?;
    let mut moved: Vec<(String, String)> = Vec::new();
    for (old, new) in pairs {
        let id: Option<String> = tx
            .query_row(
                "SELECT id FROM notes WHERE rel_path = ?1",
                params![old],
                |row| row.get(0),
            )
            .optional()
            .map_err(db_err)?;
        if let Some(id) = id {
            // Park the row under a temporary path first: folder moves can
            // contain old paths that are also new paths for other rows.
            tx.execute(
                "UPDATE notes SET rel_path = ?1 WHERE id = ?2",
                params![format!("__move__/{id}"), id],
            )
            .map_err(db_err)?;
            moved.push((id, new.clone()));
        }
    }
    for (id, new) in &moved {
        tx.execute(
            "UPDATE notes SET rel_path = ?1, path = ?2 WHERE id = ?3",
            params![
                new,
                vault::display_path(&root.join(new.replace('/', std::path::MAIN_SEPARATOR_STR))),
                id
            ],
        )
        .map_err(db_err)?;
    }
    tx.commit().map_err(db_err)?;

    let new_paths: Vec<PathBuf> = pairs
        .iter()
        .map(|(_, new)| root.join(new.replace('/', std::path::MAIN_SEPARATOR_STR)))
        .collect();
    let result = apply_paths(conn, root, &new_paths, emitter, started, 0)?;
    Ok(result)
}

fn rebuild_job(
    conn: &mut Option<Connection>,
    index_path: &Path,
    store: &Option<Arc<Store>>,
    root: &Path,
    emitter: &mut StatusEmitter,
    started: Instant,
) -> Result<IndexStats, String> {
    let root_key = root_key(root);
    let staging_path = staging_path_for(index_path);
    cleanup_staging_path(&staging_path);
    let mut staging = Connection::open(&staging_path).map_err(db_err)?;
    let _ = staging.execute_batch(
        "PRAGMA journal_mode = OFF;
         PRAGMA synchronous = OFF;
         PRAGMA temp_store = MEMORY;
         CREATE TABLE notes (
           id TEXT PRIMARY KEY, path TEXT NOT NULL, rel_path TEXT NOT NULL,
           title TEXT NOT NULL, up TEXT NOT NULL DEFAULT '', date TEXT NOT NULL DEFAULT '',
           domain TEXT NOT NULL DEFAULT '未分类', tags TEXT NOT NULL DEFAULT '[]',
           people TEXT NOT NULL DEFAULT '[]', events TEXT NOT NULL DEFAULT '[]',
           periods TEXT NOT NULL DEFAULT '[]', body TEXT NOT NULL DEFAULT '',
           run_id TEXT NOT NULL DEFAULT '', mtime INTEGER NOT NULL DEFAULT 0,
           chars INTEGER NOT NULL DEFAULT 0, mtime_ns INTEGER NOT NULL DEFAULT 0,
           size INTEGER NOT NULL DEFAULT 0, content_hash TEXT NOT NULL DEFAULT ''
         );
         CREATE UNIQUE INDEX stage_rel ON notes(rel_path);
         CREATE VIRTUAL TABLE notes_fts USING fts5(id UNINDEXED, title, tokens, tokenize = 'unicode61');",
    );

    let live_ids: HashMap<String, String> = {
        let live = conn.as_ref().ok_or_else(|| "索引正在切换".to_string())?;
        live.prepare("SELECT rel_path, id FROM notes")
            .map_err(db_err)?
            .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))
            .map_err(db_err)?
            .collect::<Result<_, _>>()
            .map_err(db_err)?
    };
    let build_started = Instant::now();
    let files = vault::walk_notes(root);
    let seen: HashSet<String> = files
        .iter()
        .map(|path| vault::rel_path(path, root))
        .collect();
    let mut live_by_hash: HashMap<String, Vec<String>> = HashMap::new();
    {
        let live = conn.as_ref().ok_or_else(|| "索引正在切换".to_string())?;
        let mut stmt = live
            .prepare("SELECT id, rel_path, body FROM notes")
            .map_err(db_err)?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(db_err)?;
        for row in rows {
            let (id, rel, body) = row.map_err(db_err)?;
            // Only rows whose file disappeared can donate an id to a new path.
            if seen.contains(&rel) {
                continue;
            }
            live_by_hash
                .entry(vault::body_hash(&body))
                .or_default()
                .push(id);
        }
    }

    let total = files.len();
    emitter.set("indexing", 0, total, "", None, Some(root_key.clone()), 0, true);
    let mut used_ids: HashSet<String> = live_ids
        .iter()
        .filter(|(rel, _)| seen.contains(*rel))
        .map(|(_, id)| id.clone())
        .collect();
    let mut parsed: Vec<ParsedNote> = Vec::new();
    for (index, path) in files.iter().enumerate() {
        let rel = vault::rel_path(path, root);
        let Some(mut note) = vault::parse_file_at(path, root, vault::mtime_ms(path)) else {
            continue;
        };
        let existing = live_ids.get(&rel).cloned();
        let mut chosen = existing.clone();
        if chosen.is_none() {
            if let Some(front) = &note.front_id {
                if !used_ids.contains(front) {
                    chosen = Some(front.clone());
                }
            }
        }
        if chosen.is_none() {
            if let Some(candidates) = live_by_hash.get(&note.content_hash) {
                if candidates.len() == 1 && !used_ids.contains(&candidates[0]) {
                    chosen = Some(candidates[0].clone());
                    let _ = vault::ensure_id(path, &candidates[0]);
                }
            }
        }
        let id = chosen.unwrap_or_else(|| vault::note_id(&rel));
        let id = if existing.is_none() && used_ids.contains(&id) {
            let fresh = uuid::Uuid::new_v4().to_string();
            let _ = vault::write_id(path, &fresh);
            fresh
        } else {
            id
        };
        note.id = id.clone();
        used_ids.insert(id);
        parsed.push(note);
        if index % 200 == 0 || index + 1 == total {
            emitter.set(
                "indexing",
                index + 1,
                total,
                &rel,
                None,
                Some(root_key.clone()),
                0,
                false,
            );
        }
    }

    let build_ms = build_started.elapsed().as_millis();
    let stage_started = Instant::now();
    // Parsing is done, but writing 50k rows + FTS terms can take ten seconds.
    // Keep the sidebar honest about which phase is running instead of leaving
    // it at "50,000 / 50,000" until the swap.
    let mut written = 0usize;
    emitter.set("writing", 0, total, "", None, Some(root_key.clone()), 0, true);
    for chunk in parsed.chunks(STAGING_BATCH) {
        let tx = staging.transaction().map_err(db_err)?;
        for note in chunk {
            db::upsert_note(&tx, note)?;
            written += 1;
            if written % 500 == 0 {
                emitter.set("writing", written, total, "", None, Some(root_key.clone()), 0, false);
            }
        }
        tx.commit().map_err(db_err)?;
        emitter.set("writing", written, total, "", None, Some(root_key.clone()), 0, false);
    }
    emitter.set("committing", total, total, "", None, Some(root_key.clone()), 0, true);
    let optimize_started = Instant::now();
    if let Err(error) = staging.execute("INSERT INTO notes_fts(notes_fts) VALUES('optimize')", []) {
        log::warn!("staging fts optimize failed: {error}");
    }
    log::info!("staging fts optimize {}ms", optimize_started.elapsed().as_millis());
    let stage_ms = stage_started.elapsed().as_millis();
    let swap_started = Instant::now();
    drop(staging);

    // Close the writer before touching the file; Store::swap_index closes
    // the read connection too, renames the staging file over the live index,
    // and reopens both sides. No 10k-row table copy.
    let old_writer = conn.take();
    drop(old_writer);
    if let Some(store) = store {
        if let Err(error) = store.swap_index(&staging_path) {
            *conn = Some(open_writer(index_path)?);
            return Err(error);
        }
    } else {
        replace_file(index_path, &staging_path)?;
    }
    *conn = Some(open_writer(index_path)?);
    let live = conn.as_mut().ok_or_else(|| "索引正在切换".to_string())?;
    let generation = bump_generation(live)?;
    set_complete(live, root, generation)?;
    let swap_ms = swap_started.elapsed().as_millis();

    let post_started = Instant::now();
    let live = conn.as_mut().ok_or_else(|| "索引正在切换".to_string())?;
    let _ = full_scan_delta(live, root, emitter, started);
    log::info!(
        "rebuild {} notes: parse {}ms, staging {}ms, swap {}ms, post {}ms",
        total,
        build_ms,
        stage_ms,
        swap_ms,
        post_started.elapsed().as_millis()
    );

    let generation = meta_get(live, "generation")?
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(0);
    emitter.set(
        "idle",
        total,
        total,
        "",
        None,
        Some(root_key),
        generation,
        true,
    );
    let mut result = stats(live, total, 0, 0, 0, 0, started)?;
    result.added = total;
    Ok(result)
}

fn compact(conn: &Connection) {
    let started = Instant::now();
    let _ = conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()));
    let free: i64 = conn
        .query_row("PRAGMA freelist_count", [], |row| row.get(0))
        .unwrap_or(0);
    if free > 2000 {
        if let Err(error) = conn.execute("VACUUM", []) {
            log::warn!("vacuum after rebuild skipped: {error}");
        }
    }
    log::info!(
        "index compact: free pages {free}, {}ms",
        started.elapsed().as_millis()
    );
}

fn stats(
    conn: &Connection,
    added: usize,
    updated: usize,
    removed: usize,
    skipped: usize,
    unchanged: usize,
    started: Instant,
) -> Result<IndexStats, String> {
    let notes: i64 = conn
        .query_row("SELECT COUNT(*) FROM notes", [], |row| row.get(0))
        .map_err(db_err)?;
    Ok(IndexStats {
        notes: notes.max(0) as usize,
        added,
        updated,
        removed,
        skipped,
        unchanged,
        elapsed_ms: started.elapsed().as_millis(),
    })
}

fn empty_stats(conn: &Connection) -> Result<IndexStats, String> {
    stats(conn, 0, 0, 0, 0, 0, Instant::now())
}

struct RowInfo {
    id: String,
    body: String,
}

fn load_rows(conn: &Connection, rels: &[String]) -> Result<HashMap<String, RowInfo>, String> {
    let mut out = HashMap::new();
    for chunk in rels.chunks(400) {
        let placeholders = vec!["?"; chunk.len()].join(",");
        let sql = format!(
            "SELECT id, rel_path, body FROM notes WHERE rel_path IN ({placeholders})"
        );
        let mut stmt = conn.prepare(&sql).map_err(db_err)?;
        let rows = stmt
            .query_map(params_from_iter(chunk.iter()), |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(db_err)?;
        for row in rows {
            let (id, rel_path, body) = row.map_err(db_err)?;
            out.insert(rel_path, RowInfo { id, body });
        }
    }
    Ok(out)
}

fn meta_map(conn: &Connection) -> Result<HashMap<String, (i64, i64, String)>, String> {
    let mut stmt = conn
        .prepare("SELECT rel_path, mtime_ns, size, content_hash FROM notes")
        .map_err(db_err)?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, String>(3)?,
            ))
        })
        .map_err(db_err)?;
    let mut out = HashMap::new();
    for row in rows {
        let (rel, mtime, size, hash) = row.map_err(db_err)?;
        out.insert(rel, (mtime, size, hash));
    }
    Ok(out)
}

fn meta_get(conn: &Connection, key: &str) -> Result<Option<String>, String> {
    conn.query_row(
        "SELECT value FROM index_meta WHERE key = ?1",
        params![key],
        |row| row.get(0),
    )
    .optional()
    .map_err(db_err)
}

fn meta_set(conn: &Connection, key: &str, value: &str) -> Result<(), String> {
    conn.execute(
        "INSERT INTO index_meta (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )
    .map_err(db_err)?;
    Ok(())
}

fn bump_generation(conn: &Connection) -> Result<i64, String> {
    let generation = meta_get(conn, "generation")?
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(0)
        + 1;
    meta_set(conn, "generation", &generation.to_string())?;
    Ok(generation)
}

fn set_complete(conn: &Connection, root: &Path, generation: i64) -> Result<(), String> {
    meta_set(conn, "root", &root_key(root))?;
    meta_set(conn, "state", "complete")?;
    meta_set(conn, "generation", &generation.to_string())?;
    meta_set(
        conn,
        "completed_at",
        &chrono::Utc::now().timestamp_millis().to_string(),
    )?;
    Ok(())
}

fn root_key(root: &Path) -> String {
    vault::display_path(root)
}

fn is_markdown(rel: &str) -> bool {
    let normalized = rel.replace('\\', "/");
    normalized.to_ascii_lowercase().ends_with(".md") && !normalized.starts_with(".trash/")
}

fn staging_path_for(index_path: &Path) -> PathBuf {
    index_path.with_file_name("yebian-index.staging.sqlite")
}

fn open_writer(index_path: &Path) -> Result<Connection, String> {
    let conn = Connection::open(index_path).map_err(db_err)?;
    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA synchronous = NORMAL;
         PRAGMA busy_timeout = 30000;
         PRAGMA temp_store = MEMORY;",
    )
    .map_err(db_err)?;
    Ok(conn)
}

fn replace_file(live: &Path, staging: &Path) -> Result<(), String> {
    let backup = live.with_file_name("yebian-index.old");
    let _ = fs::remove_file(&backup);
    let had_live = live.exists();
    if had_live {
        fs::rename(live, &backup).map_err(|error| error.to_string())?;
    }
    if let Err(error) = fs::rename(staging, live) {
        if had_live {
            let _ = fs::rename(&backup, live);
        }
        return Err(error.to_string());
    }
    let _ = fs::remove_file(&backup);
    Ok(())
}

fn cleanup_staging(index_path: &Path) {
    cleanup_staging_path(&staging_path_for(index_path));
}

fn cleanup_staging_path(path: &Path) {
    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_file(format!("{}-wal", path.display()));
    let _ = std::fs::remove_file(format!("{}-shm", path.display()));
}

fn db_err(error: rusqlite::Error) -> String {
    error.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Store;
    use std::fs;

    struct Fixture {
        root: PathBuf,
        store: Arc<Store>,
        index: IndexService,
    }

    impl Fixture {
        fn new(name: &str) -> Self {
            let base = std::env::temp_dir().join(format!(
                "yebian-index-test-{name}-{}",
                uuid::Uuid::new_v4().simple()
            ));
            let root = base.join("vault");
            fs::create_dir_all(&root).expect("create vault");
            let db = base.join("index.sqlite");
            let store = Arc::new(Store::open(&db).expect("open store"));
            let index = IndexService::open(
                store.index_path().to_path_buf(),
                None,
                Some(store.clone()),
            )
            .expect("open index");
            Self { root, store, index }
        }

        fn note(&self, rel: &str, title: &str, body: &str) -> PathBuf {
            let path = self.root.join(rel);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).expect("mkdir");
            }
            fs::write(
                &path,
                format!("---\n标题: {title}\n创建日期: 2026-11-05\n---\n\n# {title}\n\n{body}\n"),
            )
            .expect("write note");
            path
        }

        fn rebuild(&self) -> IndexStats {
            self.index.rebuild_wait(&self.root).expect("rebuild")
        }

        fn delta(&self, paths: Vec<PathBuf>) -> IndexStats {
            self.index.delta_wait(&self.root, paths).expect("delta")
        }

        fn titles(&self) -> Vec<String> {
            self.store
                .list_notes()
                .expect("list")
                .into_iter()
                .map(|note| note.title)
                .collect()
        }
    }

    #[test]
    fn rebuild_preserves_id_and_annotation() {
        let fx = Fixture::new("rebuild");
        fx.note("one.md", "一", "第一段 养老金");
        let stats = fx.rebuild();
        assert_eq!(stats.notes, 1);
        let notes = fx.store.list_notes().expect("list");
        let id = notes[0].id.clone();
        fx.store
            .add_annotation(&id, "block", "quote", "body", "标记")
            .expect("add annotation");

        let stats = fx.rebuild();
        assert_eq!(stats.notes, 1);
        let notes = fx.store.list_notes().expect("list");
        assert_eq!(notes[0].id, id);
        assert_eq!(notes[0].annotation_count, 1);
        assert_eq!(fx.store.list_annotations(&id).expect("annotations").len(), 1);
        fx.index.shutdown();
    }

    #[test]
    fn delta_adds_updates_and_removes() {
        let fx = Fixture::new("delta");
        fx.rebuild();
        let path = fx.note("two.md", "二", "新增内容");
        fx.delta(vec![path]);
        assert_eq!(fx.titles(), vec!["二"]);

        fx.note("two.md", "二改", "更新后的内容");
        fx.delta(vec![fx.root.join("two.md")]);
        assert_eq!(fx.titles(), vec!["二改"]);

        fs::remove_file(fx.root.join("two.md")).expect("remove");
        fx.index
            .remove_wait(&fx.root, vec!["two.md".into()])
            .expect("remove index");
        assert!(fx.titles().is_empty());
        fx.index.shutdown();
    }

    #[test]
    fn external_rename_keeps_id_and_annotation() {
        let fx = Fixture::new("rename");
        let old = fx.note("old.md", "旧", "正文");
        fx.rebuild();
        let id = fx.store.list_notes().expect("list")[0].id.clone();
        fx.store
            .add_annotation(&id, "block", "quote", "body", "标记")
            .expect("add annotation");

        let new = fx.root.join("new.md");
        fs::rename(&old, &new).expect("rename");
        fx.delta(vec![old, new]);
        let notes = fx.store.list_notes().expect("list");
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].id, id);
        assert_eq!(notes[0].annotation_count, 1);
        fx.index.shutdown();
    }

    #[test]
    fn rebuild_switch_replaces_notes() {
        let fx = Fixture::new("switch");
        fx.note("a.md", "A", "A body");
        fx.rebuild();
        assert_eq!(fx.titles(), vec!["A"]);

        let other = fx.root.parent().expect("base").join("other");
        fs::create_dir_all(&other).expect("create other");
        fs::write(
            other.join("b.md"),
            "---\n标题: B\n创建日期: 2026-11-05\n---\n\n# B\n\nB body\n",
        )
        .expect("write B");
        fx.index.rebuild_wait(&other).expect("switch");
        assert_eq!(fx.titles(), vec!["B"]);
        fx.index.shutdown();
    }
}
