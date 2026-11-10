//! The client side of Yebian sync.
//!
//! Local SQLite stays the source of truth for the app. This engine diffs the
//! synced tables against a shadow of the last synced state, pushes the changes
//! as ops, pulls remote ops and materialises them with per-row last-writer-wins.
//! Vault files use content-addressed blobs and a path manifest.
//!
//! See `docs/SYNC_DESIGN.md` for the contract and the honest limitations.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

use rusqlite::types::Value as SqlValue;
use rusqlite::{params_from_iter, Connection, OptionalExtension};
use serde::Serialize;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::hlc::{Hlc, HlcClock};
use super::registry::{syncable_setting, TableKind, TableSpec, SYNCED_TABLES, VAULT_PLACEHOLDER};
use super::transport::{Op, Transport, OP_DELETE, OP_UPSERT};

const PULL_BATCH: usize = 200;
const MAX_PULL_BATCHES: usize = 500;
const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;
const FILE_ENTITY: &str = "file";

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncState {
    pub configured: bool,
    pub vault_id: String,
    pub device_id: String,
    pub cursor: i64,
    pub server_url: String,
    pub has_token: bool,
    pub auto_enabled: bool,
    pub auto_minutes: i64,
    pub last_sync_at: i64,
    pub last_error: String,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncReport {
    pub pushed: usize,
    pub pulled: usize,
    pub applied: usize,
    pub skipped: usize,
    pub conflicts: usize,
    pub blobs_uploaded: usize,
    pub blobs_downloaded: usize,
}

struct Shadow {
    row_hash: String,
    hlc: String,
    deleted: bool,
}

#[derive(Clone)]
struct Snapshot {
    entity: String,
    pk: String,
    row: Value,
    hash: String,
}

struct Pending {
    op: Op,
    entity: String,
    pk: String,
    row_hash: String,
    deleted: bool,
}

pub struct SyncEngine {
    db_path: PathBuf,
    running: std::sync::atomic::AtomicBool,
}

impl SyncEngine {
    pub fn open(db_path: &Path) -> Result<Self, String> {
        let conn = open_db(db_path)?;
        ensure_schema(&conn)?;
        Ok(Self { db_path: db_path.to_path_buf(), running: std::sync::atomic::AtomicBool::new(false) })
    }

    pub fn state(&self) -> Result<SyncState, String> {
        let conn = open_db(&self.db_path)?;
        let (vault_id, device_id, cursor, server_url, token, auto_enabled, auto_minutes, last_sync_at, last_error) =
            conn.query_row(
                "SELECT vault_id, device_id, cursor, server_url, token, auto_enabled, auto_minutes, last_sync_at, last_error
                 FROM sync_state WHERE id = 1",
                [],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, i64>(5)? != 0,
                        row.get::<_, i64>(6)?,
                        row.get::<_, i64>(7)?,
                        row.get::<_, String>(8)?,
                    ))
                },
            )
            .map_err(|error| error.to_string())?;
        Ok(SyncState {
            configured: !vault_id.is_empty() && !server_url.is_empty() && !token.is_empty(),
            vault_id,
            device_id,
            cursor,
            server_url,
            has_token: !token.is_empty(),
            auto_enabled,
            auto_minutes,
            last_sync_at,
            last_error,
        })
    }

    pub fn configure(&self, server_url: &str, token: &str, vault_id: &str) -> Result<(), String> {
        let conn = open_db(&self.db_path)?;
        conn.execute(
            // An empty token means "keep the stored one": the settings page
            // never echoes the token back, so saving the form must not wipe it.
            "UPDATE sync_state SET server_url = ?1,
               token = CASE WHEN ?2 = '' THEN token ELSE ?2 END,
               vault_id = ?3, last_error = '', auto_enabled = 1 WHERE id = 1",
            rusqlite::params![server_url.trim().trim_end_matches('/'), token.trim(), vault_id.trim()],
        )
        .map_err(|error| error.to_string())?;
        bump_dirty(&conn)?;
        Ok(())
    }

    pub fn set_auto(&self, enabled: bool, minutes: i64) -> Result<(), String> {
        let conn = open_db(&self.db_path)?;
        conn.execute(
            "UPDATE sync_state SET auto_enabled = ?1, auto_minutes = ?2 WHERE id = 1",
            rusqlite::params![if enabled { 1 } else { 0 }, minutes.clamp(1, 24 * 60)],
        )
        .map_err(|error| error.to_string())?;
        if enabled {
            bump_dirty(&conn)?;
        }
        Ok(())
    }

    /// Monotonic counter bumped by triggers on every synced table. The
    /// background worker compares it to know whether a local push is due.
    pub fn dirty_seq(&self) -> Result<i64, String> {
        let conn = open_db(&self.db_path)?;
        conn.query_row("SELECT seq FROM sync_dirty WHERE id = 1", [], |row| row.get(0))
            .map_err(|error| error.to_string())
    }

    pub fn bump_dirty(&self) -> Result<(), String> {
        let conn = open_db(&self.db_path)?;
        bump_dirty(&conn)
    }

    /// Server URL and token, for the background sync thread. The token is never
    /// sent to the frontend.
    pub fn credentials(&self) -> Result<(String, String), String> {
        let conn = open_db(&self.db_path)?;
        conn.query_row(
            "SELECT server_url, token FROM sync_state WHERE id = 1",
            [],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .map_err(|error| error.to_string())
    }

    pub fn mark_error(&self, message: &str) {
        if let Ok(conn) = open_db(&self.db_path) {
            let _ = conn.execute("UPDATE sync_state SET last_error = ?1 WHERE id = 1", [message]);
        }
    }

    /// Run one full push/pull cycle. Safe to call concurrently: a second call
    /// returns without doing work.
    pub fn sync_once<T: Transport>(
        &self,
        transport: &T,
        vault_root: &Path,
    ) -> Result<SyncReport, String> {
        use std::sync::atomic::Ordering;
        if self.running.swap(true, Ordering::SeqCst) {
            return Err("同步已在进行中".to_string());
        }
        let result = self.sync_inner(transport, vault_root);
        self.running.store(false, Ordering::SeqCst);
        if let Err(error) = &result {
            self.mark_error(error);
        }
        result
    }

    fn sync_inner<T: Transport>(&self, transport: &T, vault_root: &Path) -> Result<SyncReport, String> {
        let conn = open_db(&self.db_path)?;
        let state = self.state()?;
        if !state.configured {
            return Err("同步未配置".to_string());
        }
        let vault_path = vault_root.to_string_lossy().to_string();
        let last_hlc = load_last_hlc(&conn)?;
        let clock = HlcClock::new(state.device_id.clone(), last_hlc.as_ref());

        let mut report = SyncReport::default();
        let first_sync = state.cursor == 0 && shadow_is_empty(&conn)?;

        // 0. First sync on this device: pull and merge the server state before
        //    pushing anything. A local copy that has never seen the server must
        //    not silently overwrite a newer remote row.
        if first_sync {
            pull_remote(
                &conn,
                transport,
                &state.vault_id,
                &state.device_id,
                &vault_path,
                vault_root,
                &clock,
                0,
                FirstSync::Yes,
                &mut report,
            )?;
        }

        // 1. Records: diff local tables against the synced shadow.
        let record_ops = build_record_ops(&conn, &clock, &vault_path)?;

        // 2. Files: diff the vault manifest and collect blobs to upload.
        let (file_ops, uploads) = build_file_ops(&conn, &clock, vault_root, &vault_path)?;

        // 3. Push blobs, then ops. Local shadows are only committed after a
        //    successful push, so a network failure retries next cycle.
        for (hash, bytes) in &uploads {
            transport.put_blob(&state.vault_id, hash, bytes)?;
            mark_blob_uploaded(&conn, hash, bytes.len())?;
            report.blobs_uploaded += 1;
        }
        let mut all_ops: Vec<Op> = Vec::new();
        let mut pending: Vec<Pending> = Vec::new();
        for pending_op in record_ops.into_iter().chain(file_ops.into_iter()) {
            all_ops.push(pending_op.op.clone());
            pending.push(pending_op);
        }
        if !all_ops.is_empty() {
            transport.push_ops(&state.vault_id, &all_ops)?;
            for entry in &pending {
                put_shadow(&conn, &entry.entity, &entry.pk, &entry.row_hash, &entry.op.hlc, entry.deleted)?;
            }
            report.pushed = all_ops.len();
        }

        // 4. Pull and apply remote ops. After a first-sync merge this is the
        //    normal LWW pass; it also catches ops that arrived while pushing.
        let cursor: i64 = conn
            .query_row("SELECT cursor FROM sync_state WHERE id = 1", [], |row| row.get(0))
            .map_err(|error| error.to_string())?;
        pull_remote(
            &conn,
            transport,
            &state.vault_id,
            &state.device_id,
            &vault_path,
            vault_root,
            &clock,
            cursor,
            FirstSync::No,
            &mut report,
        )?;

        // 5. Persist the clock and the result.
        let last = clock.last().encode();
        let now = now_ms();
        conn.execute(
            "UPDATE sync_state SET last_hlc = ?1, last_sync_at = ?2, last_error = '' WHERE id = 1",
            rusqlite::params![last, now],
        )
        .map_err(|error| error.to_string())?;
        Ok(report)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FirstSync {
    Yes,
    No,
}

/// Pull remote ops from `start_cursor` and apply them, advancing the cursor
/// after every successful batch.
#[allow(clippy::too_many_arguments)]
fn pull_remote<T: Transport>(
    conn: &Connection,
    transport: &T,
    vault_id: &str,
    device_id: &str,
    vault_path: &str,
    vault_root: &Path,
    clock: &HlcClock,
    start_cursor: i64,
    first_sync: FirstSync,
    report: &mut SyncReport,
) -> Result<(), String> {
    let mut cursor = start_cursor;
    for _ in 0..MAX_PULL_BATCHES {
        let batch = transport.pull_ops(vault_id, cursor, PULL_BATCH)?;
        if batch.ops.is_empty() && !batch.has_more {
            break;
        }
        for remote in &batch.ops {
            report.pulled += 1;
            if remote.op.device_id == device_id {
                report.skipped += 1;
                continue;
            }
            if let Some(hlc) = Hlc::parse(&remote.op.hlc) {
                clock.observe(&hlc);
            }
            let outcome = match first_sync {
                FirstSync::Yes => {
                    apply_remote_first(conn, transport, vault_id, vault_path, vault_root, &remote.op)
                }
                FirstSync::No => {
                    apply_remote(conn, transport, vault_id, vault_path, vault_root, &remote.op)
                }
            }
            .map_err(|error| format!("应用远端变更失败: {error}"))?;
            if outcome.applied {
                report.applied += 1;
            } else {
                report.skipped += 1;
            }
            if outcome.conflict {
                report.conflicts += 1;
            }
            if outcome.blob_downloaded {
                report.blobs_downloaded += 1;
            }
        }
        cursor = batch.cursor;
        conn.execute("UPDATE sync_state SET cursor = ?1 WHERE id = 1", [cursor])
            .map_err(|error| error.to_string())?;
        if !batch.has_more {
            break;
        }
    }
    Ok(())
}

fn shadow_is_empty(conn: &Connection) -> Result<bool, String> {
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM sync_shadow", [], |row| row.get(0))
        .map_err(|error| error.to_string())?;
    Ok(count == 0)
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

fn open_db(path: &Path) -> Result<Connection, String> {
    let conn = Connection::open(path).map_err(|error| error.to_string())?;
    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA synchronous = NORMAL;
         PRAGMA busy_timeout = 30000;",
    )
    .map_err(|error| error.to_string())?;
    Ok(conn)
}

fn ensure_schema(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS sync_state (
          id INTEGER PRIMARY KEY CHECK(id = 1),
          vault_id TEXT NOT NULL DEFAULT '',
          device_id TEXT NOT NULL,
          cursor INTEGER NOT NULL DEFAULT 0,
          server_url TEXT NOT NULL DEFAULT '',
          token TEXT NOT NULL DEFAULT '',
          last_hlc TEXT NOT NULL DEFAULT '',
          auto_enabled INTEGER NOT NULL DEFAULT 1,
          auto_minutes INTEGER NOT NULL DEFAULT 1,
          last_sync_at INTEGER NOT NULL DEFAULT 0,
          last_error TEXT NOT NULL DEFAULT ''
        );

        CREATE TABLE IF NOT EXISTS sync_shadow (
          entity TEXT NOT NULL,
          pk TEXT NOT NULL,
          row_hash TEXT NOT NULL,
          hlc TEXT NOT NULL,
          deleted INTEGER NOT NULL DEFAULT 0,
          PRIMARY KEY(entity, pk)
        );

        CREATE TABLE IF NOT EXISTS sync_conflicts (
          id TEXT PRIMARY KEY,
          entity TEXT NOT NULL,
          pk TEXT NOT NULL,
          local_json TEXT NOT NULL DEFAULT '',
          remote_json TEXT NOT NULL DEFAULT '',
          remote_blob TEXT NOT NULL DEFAULT '',
          created_at INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_sync_conflicts_created
          ON sync_conflicts(created_at DESC);

        CREATE TABLE IF NOT EXISTS sync_blobs (
          hash TEXT PRIMARY KEY,
          size INTEGER NOT NULL,
          uploaded_at INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS sync_dirty (
          id INTEGER PRIMARY KEY CHECK(id = 1),
          seq INTEGER NOT NULL DEFAULT 0
        );
        INSERT OR IGNORE INTO sync_dirty(id, seq) VALUES (1, 0);

        INSERT OR IGNORE INTO sync_state(id, device_id) VALUES (1, '');
        ",
    )
    .map_err(|error| error.to_string())?;
    let empty: i64 = conn
        .query_row("SELECT COUNT(*) FROM sync_state WHERE device_id = ''", [], |row| row.get(0))
        .map_err(|error| error.to_string())?;
    if empty > 0 {
        conn.execute(
            "UPDATE sync_state SET device_id = ?1 WHERE id = 1",
            [Uuid::new_v4().to_string()],
        )
        .map_err(|error| error.to_string())?;
    }
    // Any write to a synced table bumps `sync_dirty.seq`; the background
    // worker uses it to push local edits within a couple of seconds instead of
    // waiting for the next poll. Remote applies bump it too, which only costs
    // one extra no-op cycle.
    for spec in SYNCED_TABLES {
        if !table_exists(conn, spec.table)? {
            continue;
        }
        for (suffix, event) in [("ai", "INSERT"), ("au", "UPDATE"), ("ad", "DELETE")] {
            let name = format!("sync_dirty_{}_{}", spec.table, suffix);
            let sql = format!(
                "CREATE TRIGGER IF NOT EXISTS \"{name}\" AFTER {event} ON \"{}\" \
                 BEGIN UPDATE sync_dirty SET seq = seq + 1 WHERE id = 1; END;",
                spec.table
            );
            conn.execute_batch(&sql).map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

fn bump_dirty(conn: &Connection) -> Result<(), String> {
    conn.execute("UPDATE sync_dirty SET seq = seq + 1 WHERE id = 1", [])
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn load_last_hlc(conn: &Connection) -> Result<Option<Hlc>, String> {
    let value: String = conn
        .query_row("SELECT last_hlc FROM sync_state WHERE id = 1", [], |row| row.get(0))
        .map_err(|error| error.to_string())?;
    Ok(if value.is_empty() { None } else { Hlc::parse(&value) })
}

fn hash_json(value: &Value) -> String {
    let bytes = serde_json::to_vec(value).unwrap_or_default();
    hash_bytes(&bytes)
}

fn hash_bytes(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn entity_for(table: &str) -> String {
    format!("table:{table}")
}

fn spec_for(table: &str) -> Option<&'static TableSpec> {
    SYNCED_TABLES.iter().find(|spec| spec.table == table)
}

fn spec_by_entity(entity: &str) -> Option<&'static TableSpec> {
    entity.strip_prefix("table:").and_then(spec_for)
}

// ---------------------------------------------------------------------------
// Records
// ---------------------------------------------------------------------------

fn build_record_ops(
    conn: &Connection,
    clock: &HlcClock,
    vault_path: &str,
) -> Result<Vec<Pending>, String> {
    let shadow = load_shadow(conn)?;
    let mut local: HashMap<(String, String), Snapshot> = HashMap::new();
    for spec in SYNCED_TABLES {
        for snapshot in read_table(conn, spec, vault_path)? {
            local.insert((snapshot.entity.clone(), snapshot.pk.clone()), snapshot);
        }
    }

    let mut out = Vec::new();
    let mut seen: HashSet<(String, String)> = HashSet::new();
    for (key, snapshot) in &local {
        seen.insert(key.clone());
        let previous = shadow.get(key);
        let changed = match previous {
            None => true,
            Some(previous) => previous.deleted || previous.row_hash != snapshot.hash,
        };
        if !changed {
            continue;
        }
        let hlc = clock.now().encode();
        out.push(Pending {
            op: Op {
                op_id: Uuid::new_v4().to_string(),
                entity: snapshot.entity.clone(),
                pk: snapshot.pk.clone(),
                kind: OP_UPSERT.to_string(),
                hlc: hlc.clone(),
                device_id: clock.device().to_string(),
                codec: "json".to_string(),
                payload: snapshot.row.clone(),
            },
            entity: snapshot.entity.clone(),
            pk: snapshot.pk.clone(),
            row_hash: snapshot.hash.clone(),
            deleted: false,
        });
    }

    for (key, previous) in &shadow {
        if previous.deleted || seen.contains(key) {
            continue;
        }
        let hlc = clock.now().encode();
        let Some(table) = key.0.strip_prefix("table:") else { continue };
        if spec_for(table).is_none() {
            continue;
        }
        out.push(Pending {
            op: Op {
                op_id: Uuid::new_v4().to_string(),
                entity: key.0.clone(),
                pk: key.1.clone(),
                kind: OP_DELETE.to_string(),
                hlc: hlc.clone(),
                device_id: clock.device().to_string(),
                codec: "json".to_string(),
                payload: Value::Null,
            },
            entity: key.0.clone(),
            pk: key.1.clone(),
            row_hash: String::new(),
            deleted: true,
        });
    }
    Ok(out)
}

fn read_table(
    conn: &Connection,
    spec: &TableSpec,
    vault_path: &str,
) -> Result<Vec<Snapshot>, String> {
    if !table_exists(conn, spec.table)? {
        return Ok(Vec::new());
    }
    let sql = format!("SELECT * FROM \"{}\"", spec.table);
    let mut statement = conn.prepare(&sql).map_err(|error| error.to_string())?;
    let columns: Vec<String> = statement.column_names().map(|name| name.to_string()).collect();
    let mut rows = statement.query([]).map_err(|error| error.to_string())?;
    let mut out = Vec::new();
    while let Some(row) = rows.next().map_err(|error| error.to_string())? {
        let mut object = Map::new();
        for (index, column) in columns.iter().enumerate() {
            object.insert(column.clone(), read_value(row, index)?);
        }
        if spec.kind == TableKind::Settings {
            let key = object.get("key").and_then(Value::as_str).unwrap_or("");
            if !syncable_setting(key) {
                continue;
            }
        }
        if spec.kind == TableKind::VaultScoped {
            let scope = object.get("vault_path").and_then(Value::as_str).unwrap_or("");
            if !scope.is_empty() && scope != vault_path {
                continue;
            }
            object.insert("vault_path".into(), Value::String(VAULT_PLACEHOLDER.into()));
        }
        let row = Value::Object(object);
        let pk = primary_key(spec, &row)?;
        let hash = hash_json(&row);
        out.push(Snapshot { entity: entity_for(spec.table), pk, row, hash });
    }
    Ok(out)
}

fn read_value(row: &rusqlite::Row<'_>, index: usize) -> Result<Value, String> {
    use rusqlite::types::ValueRef;
    Ok(match row.get_ref(index).map_err(|error| error.to_string())? {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(value) => Value::Number(value.into()),
        ValueRef::Real(value) => serde_json::Number::from_f64(value)
            .map(Value::Number)
            .unwrap_or(Value::Null),
        ValueRef::Text(bytes) => Value::String(String::from_utf8_lossy(bytes).into_owned()),
        ValueRef::Blob(bytes) => Value::String(format!("$blob:{}", hex::encode(bytes))),
    })
}

fn primary_key(spec: &TableSpec, row: &Value) -> Result<String, String> {
    let mut parts = Vec::with_capacity(spec.pk.len());
    for column in spec.pk {
        let value = row.get(*column).ok_or_else(|| format!("缺主键列 {column}"))?;
        parts.push(value_to_key(value));
    }
    Ok(parts.join("\u{1f}"))
}

fn value_to_key(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

fn load_shadow(conn: &Connection) -> Result<HashMap<(String, String), Shadow>, String> {
    let mut statement = conn
        .prepare("SELECT entity, pk, row_hash, hlc, deleted FROM sync_shadow")
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                (row.get::<_, String>(0)?, row.get::<_, String>(1)?),
                Shadow {
                    row_hash: row.get(2)?,
                    hlc: row.get(3)?,
                    deleted: row.get::<_, i64>(4)? != 0,
                },
            ))
        })
        .map_err(|error| error.to_string())?;
    let mut out = HashMap::new();
    for entry in rows {
        let (key, shadow) = entry.map_err(|error| error.to_string())?;
        out.insert(key, shadow);
    }
    Ok(out)
}

fn get_shadow(conn: &Connection, entity: &str, pk: &str) -> Result<Option<Shadow>, String> {
    conn.query_row(
        "SELECT row_hash, hlc, deleted FROM sync_shadow WHERE entity = ?1 AND pk = ?2",
        rusqlite::params![entity, pk],
        |row| {
            Ok(Shadow {
                row_hash: row.get(0)?,
                hlc: row.get(1)?,
                deleted: row.get::<_, i64>(2)? != 0,
            })
        },
    )
    .optional()
    .map_err(|error| error.to_string())
}

fn put_shadow(
    conn: &Connection,
    entity: &str,
    pk: &str,
    row_hash: &str,
    hlc: &str,
    deleted: bool,
) -> Result<(), String> {
    conn.execute(
        "INSERT INTO sync_shadow(entity, pk, row_hash, hlc, deleted) VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(entity, pk) DO UPDATE SET row_hash = excluded.row_hash,
           hlc = excluded.hlc, deleted = excluded.deleted",
        rusqlite::params![entity, pk, row_hash, hlc, if deleted { 1 } else { 0 }],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

#[derive(Default)]
struct ApplyOutcome {
    applied: bool,
    conflict: bool,
    blob_downloaded: bool,
}

fn apply_remote<T: Transport>(
    conn: &Connection,
    transport: &T,
    vault_id: &str,
    vault_path: &str,
    vault_root: &Path,
    op: &Op,
) -> Result<ApplyOutcome, String> {
    if op.entity == FILE_ENTITY {
        return apply_file(conn, transport, vault_id, vault_root, op);
    }
    let Some(spec) = spec_by_entity(&op.entity) else {
        return Ok(ApplyOutcome { applied: false, conflict: false, blob_downloaded: false });
    };
    if !table_exists(conn, spec.table)? {
        return Ok(ApplyOutcome { applied: false, conflict: false, blob_downloaded: false });
    }
    let shadow = get_shadow(conn, &op.entity, &op.pk)?;
    if let Some(shadow) = &shadow {
        if shadow.hlc >= op.hlc {
            return Ok(ApplyOutcome { applied: false, conflict: false, blob_downloaded: false });
        }
    }
    let current = read_row_by_pk(conn, spec, &op.pk, vault_path)?;
    let current_hash = current.as_ref().map(hash_json);

    if op.is_delete() {
        let local_changed = match (&shadow, &current_hash) {
            (Some(shadow), Some(hash)) => !shadow.deleted && shadow.row_hash != *hash,
            (None, Some(_)) => true,
            _ => false,
        };
        if local_changed {
            record_conflict(conn, op, current.as_ref(), "")?;
            return Ok(ApplyOutcome { applied: false, conflict: true, blob_downloaded: false });
        }
        delete_row(conn, spec, &op.pk, vault_path)?;
        put_shadow(conn, &op.entity, &op.pk, "", &op.hlc, true)?;
        return Ok(ApplyOutcome { applied: true, conflict: false, blob_downloaded: false });
    }

    let payload = normalize_payload(spec, &op.payload, vault_path);
    let remote_hash = hash_json(&payload);
    let local_changed = match (&shadow, &current_hash) {
        (Some(shadow), Some(hash)) => !shadow.deleted && shadow.row_hash != *hash,
        (None, Some(hash)) => *hash != remote_hash,
        _ => false,
    };
    if local_changed {
        record_conflict(conn, op, current.as_ref(), "")?;
        return Ok(ApplyOutcome { applied: false, conflict: true, blob_downloaded: false });
    }
    upsert_row(conn, spec, &payload)?;
    put_shadow(conn, &op.entity, &op.pk, &remote_hash, &op.hlc, false)?;
    Ok(ApplyOutcome { applied: true, conflict: false, blob_downloaded: false })
}

/// First-sync merge for a record row: union by primary key, then pick a winner
/// when both sides hold the same key. `updated_at` decides when both sides have
/// it; otherwise the server wins. The losing version always goes to
/// `sync_conflicts`.
fn apply_remote_first<T: Transport>(
    conn: &Connection,
    transport: &T,
    vault_id: &str,
    vault_path: &str,
    vault_root: &Path,
    op: &Op,
) -> Result<ApplyOutcome, String> {
    if op.entity == FILE_ENTITY {
        return apply_file_first(conn, transport, vault_id, vault_root, op);
    }
    let Some(spec) = spec_by_entity(&op.entity) else {
        return Ok(ApplyOutcome::default());
    };
    if !table_exists(conn, spec.table)? {
        return Ok(ApplyOutcome::default());
    }
    let shadow = get_shadow(conn, &op.entity, &op.pk)?;
    if let Some(shadow) = &shadow {
        if shadow.hlc >= op.hlc {
            return Ok(ApplyOutcome::default());
        }
    }
    let current = read_row_by_pk(conn, spec, &op.pk, vault_path)?;

    if op.is_delete() {
        let Some(local) = current.as_ref() else {
            put_shadow(conn, &op.entity, &op.pk, "", &op.hlc, true)?;
            return Ok(ApplyOutcome { applied: true, ..Default::default() });
        };
        // A row this pass already materialised from an earlier remote op is
        // not a local edit: apply the delete instead of archiving a conflict.
        if shadow.as_ref().is_some_and(|shadow| {
            !shadow.deleted && shadow.row_hash == hash_json(local)
        }) {
            delete_row(conn, spec, &op.pk, vault_path)?;
            put_shadow(conn, &op.entity, &op.pk, "", &op.hlc, true)?;
            return Ok(ApplyOutcome { applied: true, ..Default::default() });
        }
        // The server deleted a row this device still has: keep the local row,
        // archive the delete, and let the push phase resurrect it.
        record_conflict_values(conn, &op.entity, &op.pk, Some(local), Some(&Value::Null), "")?;
        put_shadow(conn, &op.entity, &op.pk, "", &op.hlc, true)?;
        return Ok(ApplyOutcome { conflict: true, ..Default::default() });
    }

    let payload = normalize_payload(spec, &op.payload, vault_path);
    let remote_hash = hash_json(&payload);
    let Some(local) = current.as_ref() else {
        upsert_row(conn, spec, &payload)?;
        put_shadow(conn, &op.entity, &op.pk, &remote_hash, &op.hlc, false)?;
        return Ok(ApplyOutcome { applied: true, ..Default::default() });
    };
    let local_hash = hash_json(local);
    if local_hash == remote_hash {
        put_shadow(conn, &op.entity, &op.pk, &remote_hash, &op.hlc, false)?;
        return Ok(ApplyOutcome { applied: true, ..Default::default() });
    }
    // Same rule as above: content produced by an earlier remote op in this
    // first-sync pass is not a conflicting local edit.
    if shadow.as_ref().is_some_and(|shadow| !shadow.deleted && shadow.row_hash == local_hash) {
        upsert_row(conn, spec, &payload)?;
        put_shadow(conn, &op.entity, &op.pk, &remote_hash, &op.hlc, false)?;
        return Ok(ApplyOutcome { applied: true, ..Default::default() });
    }

    // Both sides hold this key with different content. Archive the loser either
    // way, then materialise only the winner.
    record_conflict_values(conn, &op.entity, &op.pk, Some(local), Some(&payload), "")?;
    if first_sync_local_wins(local, &payload) {
        // Shadow the remote hash so the push phase sees the local row as a
        // change and re-publishes it with a fresh HLC.
        put_shadow(conn, &op.entity, &op.pk, &remote_hash, &op.hlc, false)?;
        Ok(ApplyOutcome { conflict: true, ..Default::default() })
    } else {
        upsert_row(conn, spec, &payload)?;
        put_shadow(conn, &op.entity, &op.pk, &remote_hash, &op.hlc, false)?;
        Ok(ApplyOutcome { applied: true, conflict: true, ..Default::default() })
    }
}

fn first_sync_local_wins(local: &Value, remote: &Value) -> bool {
    match (
        local.get("updated_at").and_then(Value::as_i64),
        remote.get("updated_at").and_then(Value::as_i64),
    ) {
        (Some(local_time), Some(remote_time)) => local_time > remote_time,
        // No comparable timestamp: treat the server as authoritative. Both
        // versions are still archived in sync_conflicts.
        _ => false,
    }
}

fn normalize_payload(spec: &TableSpec, payload: &Value, vault_path: &str) -> Value {
    let mut object = payload.as_object().cloned().unwrap_or_default();
    if spec.kind == TableKind::VaultScoped && object.contains_key("vault_path") {
        object.insert("vault_path".into(), Value::String(vault_path.to_string()));
    }
    Value::Object(object)
}
fn localize_pk(spec: &TableSpec, pk: &str, vault_path: &str) -> String {
    if spec.kind != TableKind::VaultScoped {
        return pk.to_string();
    }
    let mut parts: Vec<String> = pk.split('\u{1f}').map(|part| part.to_string()).collect();
    if let Some(index) = spec.pk.iter().position(|column| *column == "vault_path") {
        if index < parts.len() {
            parts[index] = vault_path.to_string();
        }
    }
    parts.join("\u{1f}")
}

fn read_row_by_pk(
    conn: &Connection,
    spec: &TableSpec,
    pk: &str,
    vault_path: &str,
) -> Result<Option<Value>, String> {
    let local_pk = localize_pk(spec, pk, vault_path);
    let parts: Vec<&str> = local_pk.split('\u{1f}').collect();
    if parts.len() != spec.pk.len() {
        return Ok(None);
    }
    let sql = format!(
        "SELECT * FROM \"{}\" WHERE {}",
        spec.table,
        spec.pk.iter().map(|column| format!("\"{column}\" = ?")).collect::<Vec<_>>().join(" AND ")
    );
    let mut statement = conn.prepare(&sql).map_err(|error| error.to_string())?;
    let columns: Vec<String> = statement.column_names().map(|name| name.to_string()).collect();
    let mut rows = statement
        .query(params_from_iter(parts.iter()))
        .map_err(|error| error.to_string())?;
    let Some(row) = rows.next().map_err(|error| error.to_string())? else {
        return Ok(None);
    };
    let mut object = Map::new();
    for (index, column) in columns.iter().enumerate() {
        object.insert(column.clone(), read_value(row, index)?);
    }
    if spec.kind == TableKind::VaultScoped {
        object.insert("vault_path".into(), Value::String(VAULT_PLACEHOLDER.into()));
    }
    Ok(Some(Value::Object(object)))
}

fn table_exists(conn: &Connection, table: &str) -> Result<bool, String> {
    conn.query_row(
        "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
        [table],
        |_| Ok(()),
    )
    .optional()
    .map(|row| row.is_some())
    .map_err(|error| error.to_string())
}

fn table_columns(conn: &Connection, table: &str) -> Result<HashSet<String>, String> {
    let mut statement = conn
        .prepare(&format!("PRAGMA table_info(\"{table}\")"))
        .map_err(|error| error.to_string())?;
    let names = statement
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|error| error.to_string())?
        .collect::<Result<HashSet<_>, _>>()
        .map_err(|error| error.to_string())?;
    Ok(names)
}

fn upsert_row(conn: &Connection, spec: &TableSpec, payload: &Value) -> Result<(), String> {
    if spec.kind == TableKind::Settings {
        let key = payload.get("key").and_then(Value::as_str).unwrap_or("");
        if !syncable_setting(key) {
            return Ok(());
        }
    }
    let object = payload
        .as_object()
        .ok_or_else(|| format!("{} 的远端行不是对象", spec.table))?;
    let allowed = table_columns(conn, spec.table)?;
    let mut columns = Vec::new();
    let mut values = Vec::new();
    let mut update_columns = Vec::new();
    for (column, value) in object {
        if !allowed.contains(column) {
            continue;
        }
        let quoted = quote_ident(column);
        columns.push(quoted.clone());
        values.push(json_to_sql(value));
        if !spec.pk.contains(&column.as_str()) {
            update_columns.push(quoted);
        }
    }
    if columns.is_empty() {
        return Ok(());
    }
    let placeholders = vec!["?"; columns.len()].join(", ");
    let conflict = spec.pk.iter().map(|column| quote_ident(column)).collect::<Vec<_>>().join(", ");
    // A plain INSERT OR REPLACE would delete dependent rows and can trip
    // unrelated UNIQUE columns; an upsert keeps the row identity stable.
    let tail = if update_columns.is_empty() {
        "DO NOTHING".to_string()
    } else {
        format!(
            "DO UPDATE SET {}",
            update_columns
                .iter()
                .map(|column| format!("{column} = excluded.{column}"))
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    let sql = format!(
        "INSERT INTO \"{}\" ({}) VALUES ({}) ON CONFLICT({}) {}",
        spec.table,
        columns.join(", "),
        placeholders,
        conflict,
        tail
    );
    conn.execute(&sql, params_from_iter(values))
        .map_err(|error| format!("写入 {} 失败: {error}", spec.table))?;
    Ok(())
}

fn delete_row(
    conn: &Connection,
    spec: &TableSpec,
    pk: &str,
    vault_path: &str,
) -> Result<(), String> {
    let local_pk = localize_pk(spec, pk, vault_path);
    let parts: Vec<&str> = local_pk.split('\u{1f}').collect();
    if parts.len() != spec.pk.len() {
        return Ok(());
    }
    let sql = format!(
        "DELETE FROM \"{}\" WHERE {}",
        spec.table,
        spec.pk.iter().map(|column| format!("\"{column}\" = ?")).collect::<Vec<_>>().join(" AND ")
    );
    conn.execute(&sql, params_from_iter(parts.iter()))
        .map_err(|error| format!("删除 {} 失败: {error}", spec.table))?;
    Ok(())
}

fn quote_ident(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}

fn json_to_sql(value: &Value) -> SqlValue {
    match value {
        Value::Null => SqlValue::Null,
        Value::Bool(flag) => SqlValue::Integer(*flag as i64),
        Value::Number(number) => {
            if let Some(value) = number.as_i64() {
                SqlValue::Integer(value)
            } else if let Some(value) = number.as_f64() {
                SqlValue::Real(value)
            } else {
                SqlValue::Null
            }
        }
        Value::String(text) => {
            if let Some(hex_value) = text.strip_prefix("$blob:") {
                SqlValue::Blob(hex::decode(hex_value).unwrap_or_default())
            } else {
                SqlValue::Text(text.clone())
            }
        }
        other => SqlValue::Text(other.to_string()),
    }
}

fn record_conflict(
    conn: &Connection,
    op: &Op,
    local: Option<&Value>,
    remote_blob: &str,
) -> Result<(), String> {
    record_conflict_values(conn, &op.entity, &op.pk, local, Some(&op.payload), remote_blob)
}

fn record_conflict_values(
    conn: &Connection,
    entity: &str,
    pk: &str,
    local: Option<&Value>,
    remote: Option<&Value>,
    remote_blob: &str,
) -> Result<(), String> {
    conn.execute(
        "INSERT INTO sync_conflicts(id, entity, pk, local_json, remote_json, remote_blob, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![
            Uuid::new_v4().to_string(),
            entity,
            pk,
            local.map(|value| value.to_string()).unwrap_or_default(),
            remote.map(|value| value.to_string()).unwrap_or_default(),
            remote_blob,
            now_ms(),
        ],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Files
// ---------------------------------------------------------------------------

const SKIP_DIRS: &[&str] = &[".obsidian", ".trash", ".yebian", ".git", "node_modules", "target"];

struct FileEntry {
    rel: String,
    hash: String,
    size: u64,
}

fn walk_vault(root: &Path) -> Result<Vec<FileEntry>, String> {
    let mut out = Vec::new();
    for entry in walkdir::WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| {
            if !entry.file_type().is_dir() {
                return true;
            }
            let name = entry.file_name().to_string_lossy();
            !SKIP_DIRS.contains(&name.as_ref())
        })
    {
        let Ok(entry) = entry else { continue };
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        let name = entry.file_name().to_string_lossy();
        if name.starts_with('.') {
            continue;
        }
        let extension = path.extension().and_then(|value| value.to_str()).unwrap_or("");
        if !extension.eq_ignore_ascii_case("md") && !extension.eq_ignore_ascii_case("json") {
            continue;
        }
        let Ok(metadata) = entry.metadata() else { continue };
        if metadata.len() > MAX_FILE_BYTES {
            continue;
        }
        let Ok(relative) = path.strip_prefix(root) else { continue };
        let rel = relative.to_string_lossy().replace('\\', "/");
        let bytes = fs::read(path).map_err(|error| format!("读取 {rel} 失败: {error}"))?;
        out.push(FileEntry { rel, hash: hash_bytes(&bytes), size: metadata.len() });
    }
    out.sort_by(|left, right| left.rel.cmp(&right.rel));
    Ok(out)
}

fn build_file_ops(
    conn: &Connection,
    clock: &HlcClock,
    root: &Path,
    _vault_path: &str,
) -> Result<(Vec<Pending>, Vec<(String, Vec<u8>)>), String> {
    let shadow = load_shadow(conn)?;
    let local = walk_vault(root)?;
    let mut uploads: HashMap<String, Vec<u8>> = HashMap::new();
    let mut seen = HashSet::new();
    let mut out = Vec::new();

    for entry in &local {
        let key = (FILE_ENTITY.to_string(), entry.rel.clone());
        seen.insert(key.clone());
        let changed = match shadow.get(&key) {
            None => true,
            Some(previous) => previous.deleted || previous.row_hash != entry.hash,
        };
        if !changed {
            continue;
        }
        let uploaded = conn
            .query_row("SELECT 1 FROM sync_blobs WHERE hash = ?1", [&entry.hash], |_| Ok(()))
            .optional()
            .map_err(|error| error.to_string())?
            .is_some();
        if !uploaded {
            let bytes = fs::read(root.join(&entry.rel))
                .map_err(|error| format!("读取 {} 失败: {error}", entry.rel))?;
            uploads.insert(entry.hash.clone(), bytes);
        }
        let hlc = clock.now().encode();
        out.push(Pending {
            op: Op {
                op_id: Uuid::new_v4().to_string(),
                entity: FILE_ENTITY.to_string(),
                pk: entry.rel.clone(),
                kind: OP_UPSERT.to_string(),
                hlc: hlc.clone(),
                device_id: clock.device().to_string(),
                codec: "json".to_string(),
                payload: json!({ "path": entry.rel, "hash": entry.hash, "size": entry.size }),
            },
            entity: FILE_ENTITY.to_string(),
            pk: entry.rel.clone(),
            row_hash: entry.hash.clone(),
            deleted: false,
        });
    }

    for (key, previous) in &shadow {
        if key.0 != FILE_ENTITY || previous.deleted || seen.contains(key) {
            continue;
        }
        let hlc = clock.now().encode();
        out.push(Pending {
            op: Op {
                op_id: Uuid::new_v4().to_string(),
                entity: FILE_ENTITY.to_string(),
                pk: key.1.clone(),
                kind: OP_DELETE.to_string(),
                hlc: hlc.clone(),
                device_id: clock.device().to_string(),
                codec: "json".to_string(),
                payload: json!({ "path": key.1 }),
            },
            entity: FILE_ENTITY.to_string(),
            pk: key.1.clone(),
            row_hash: String::new(),
            deleted: true,
        });
    }

    Ok((out, uploads.into_iter().collect()))
}

fn apply_file<T: Transport>(
    conn: &Connection,
    transport: &T,
    vault_id: &str,
    root: &Path,
    op: &Op,
) -> Result<ApplyOutcome, String> {
    let rel = op
        .payload
        .get("path")
        .and_then(Value::as_str)
        .or(Some(op.pk.as_str()))
        .unwrap_or_default();
    let path = safe_relative(rel).ok_or_else(|| format!("拒绝不安全路径: {rel}"))?;
    let target = root.join(&path);
    let shadow = get_shadow(conn, FILE_ENTITY, rel)?;
    if let Some(shadow) = &shadow {
        if shadow.hlc >= op.hlc {
            return Ok(ApplyOutcome { applied: false, conflict: false, blob_downloaded: false });
        }
    }
    let local_hash = if target.is_file() {
        Some(hash_bytes(&fs::read(&target).map_err(|error| error.to_string())?))
    } else {
        None
    };

    if op.is_delete() {
        let local_changed = match (&shadow, &local_hash) {
            (Some(shadow), Some(hash)) => !shadow.deleted && shadow.row_hash != *hash,
            (None, Some(_)) => true,
            _ => false,
        };
        if local_changed {
            record_conflict(conn, op, None, "")?;
            return Ok(ApplyOutcome { applied: false, conflict: true, blob_downloaded: false });
        }
        if target.is_file() {
            fs::remove_file(&target).map_err(|error| error.to_string())?;
        }
        put_shadow(conn, FILE_ENTITY, rel, "", &op.hlc, true)?;
        return Ok(ApplyOutcome { applied: true, conflict: false, blob_downloaded: false });
    }

    let hash = op
        .payload
        .get("hash")
        .and_then(Value::as_str)
        .ok_or_else(|| "文件 op 缺 hash".to_string())?
        .to_string();
    let local_changed = match (&shadow, &local_hash) {
        (Some(shadow), Some(local)) => !shadow.deleted && shadow.row_hash != *local,
        (None, Some(local)) => *local != hash,
        _ => false,
    };
    if local_changed {
        record_conflict(conn, op, None, &hash)?;
        return Ok(ApplyOutcome { applied: false, conflict: true, blob_downloaded: false });
    }
    if local_hash.as_deref() != Some(hash.as_str()) {
        let bytes = download_blob(transport, vault_id, &hash)?;
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let temporary = target.with_extension(format!("{}.sync-tmp", hash));
        fs::write(&temporary, &bytes).map_err(|error| error.to_string())?;
        fs::rename(&temporary, &target).map_err(|error| error.to_string())?;
        put_shadow(conn, FILE_ENTITY, rel, &hash, &op.hlc, false)?;
        return Ok(ApplyOutcome { applied: true, conflict: false, blob_downloaded: true });
    }
    put_shadow(conn, FILE_ENTITY, rel, &hash, &op.hlc, false)?;
    Ok(ApplyOutcome { applied: true, conflict: false, blob_downloaded: false })
}

/// First-sync merge for a vault file. Local mtime is compared with the remote
/// op's physical HLC time; the loser is copied into `.yebian/conflicts/` so the
/// user can still get it back without a manual merge step.
fn apply_file_first<T: Transport>(
    conn: &Connection,
    transport: &T,
    vault_id: &str,
    root: &Path,
    op: &Op,
) -> Result<ApplyOutcome, String> {
    let rel = op
        .payload
        .get("path")
        .and_then(Value::as_str)
        .or(Some(op.pk.as_str()))
        .unwrap_or_default();
    let path = safe_relative(rel).ok_or_else(|| format!("拒绝不安全路径: {rel}"))?;
    let target = root.join(&path);
    let shadow = get_shadow(conn, FILE_ENTITY, rel)?;
    if let Some(shadow) = &shadow {
        if shadow.hlc >= op.hlc {
            return Ok(ApplyOutcome::default());
        }
    }
    let local_bytes = if target.is_file() {
        Some(fs::read(&target).map_err(|error| error.to_string())?)
    } else {
        None
    };
    let local_hash = local_bytes.as_ref().map(|bytes| hash_bytes(bytes));
    // Content produced by an earlier remote op in this first-sync pass is not
    // a conflicting local edit.
    let from_sync = shadow
        .as_ref()
        .is_some_and(|shadow| !shadow.deleted && Some(shadow.row_hash.as_str()) == local_hash.as_deref());

    if op.is_delete() {
        let Some(local_hash) = local_hash else {
            put_shadow(conn, FILE_ENTITY, rel, "", &op.hlc, true)?;
            return Ok(ApplyOutcome { applied: true, ..Default::default() });
        };
        if from_sync {
            if target.is_file() {
                fs::remove_file(&target).map_err(|error| error.to_string())?;
            }
            put_shadow(conn, FILE_ENTITY, rel, "", &op.hlc, true)?;
            return Ok(ApplyOutcome { applied: true, ..Default::default() });
        }
        let meta = file_meta(&local_hash, local_bytes.as_ref().map(Vec::len).unwrap_or(0), file_mtime_ms(&target));
        record_conflict_values(conn, FILE_ENTITY, rel, Some(&meta), Some(&Value::Null), "")?;
        put_shadow(conn, FILE_ENTITY, rel, "", &op.hlc, true)?;
        return Ok(ApplyOutcome { conflict: true, ..Default::default() });
    }

    let hash = op
        .payload
        .get("hash")
        .and_then(Value::as_str)
        .ok_or_else(|| "文件 op 缺 hash".to_string())?
        .to_string();
    let Some(local_hash) = local_hash else {
        write_remote_file(transport, vault_id, root, &path, &hash)?;
        put_shadow(conn, FILE_ENTITY, rel, &hash, &op.hlc, false)?;
        return Ok(ApplyOutcome { applied: true, blob_downloaded: true, ..Default::default() });
    };
    if local_hash == hash {
        put_shadow(conn, FILE_ENTITY, rel, &hash, &op.hlc, false)?;
        return Ok(ApplyOutcome { applied: true, ..Default::default() });
    }
    if from_sync {
        write_remote_file(transport, vault_id, root, &path, &hash)?;
        put_shadow(conn, FILE_ENTITY, rel, &hash, &op.hlc, false)?;
        return Ok(ApplyOutcome { applied: true, blob_downloaded: true, ..Default::default() });
    }

    let local_meta = file_meta(&local_hash, local_bytes.as_ref().map(Vec::len).unwrap_or(0), file_mtime_ms(&target));
    record_conflict_values(conn, FILE_ENTITY, rel, Some(&local_meta), Some(&op.payload), &hash)?;
    let remote_ms = Hlc::parse(&op.hlc).map(|hlc| hlc.ms).unwrap_or(0);
    let local_wins = file_mtime_ms(&target).unwrap_or(0) > remote_ms;
    if local_wins {
        // Keep the local bytes, archive the remote version, and re-publish the
        // local file from the push phase.
        let remote_bytes = download_blob(transport, vault_id, &hash)?;
        archive_conflict_file(root, &path, &remote_bytes)?;
        put_shadow(conn, FILE_ENTITY, rel, &hash, &op.hlc, false)?;
        Ok(ApplyOutcome { conflict: true, blob_downloaded: true, ..Default::default() })
    } else {
        archive_conflict_file(root, &path, local_bytes.as_deref().unwrap_or_default())?;
        write_remote_file(transport, vault_id, root, &path, &hash)?;
        put_shadow(conn, FILE_ENTITY, rel, &hash, &op.hlc, false)?;
        Ok(ApplyOutcome { applied: true, conflict: true, blob_downloaded: true, ..Default::default() })
    }
}

fn file_meta(hash: &str, size: usize, mtime: Option<u64>) -> Value {
    json!({ "hash": hash, "size": size as u64, "mtime": mtime })
}

fn file_mtime_ms(path: &Path) -> Option<u64> {
    path.metadata()
        .ok()?
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_millis() as u64)
}

fn archive_conflict_file(root: &Path, path: &Path, bytes: &[u8]) -> Result<(), String> {
    let target = root
        .join(".yebian")
        .join("conflicts")
        .join(now_ms().to_string())
        .join(path);
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    fs::write(&target, bytes).map_err(|error| format!("写入冲突副本失败: {error}"))
}

fn download_blob<T: Transport>(transport: &T, vault_id: &str, hash: &str) -> Result<Vec<u8>, String> {
    let bytes = transport.get_blob(vault_id, hash)?;
    if hash_bytes(&bytes) != hash {
        return Err(format!("blob {hash} 校验失败"));
    }
    Ok(bytes)
}

fn write_remote_file<T: Transport>(
    transport: &T,
    vault_id: &str,
    root: &Path,
    path: &Path,
    hash: &str,
) -> Result<(), String> {
    let bytes = download_blob(transport, vault_id, hash)?;
    let target = root.join(path);
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let temporary = target.with_extension(format!("{hash}.sync-tmp"));
    fs::write(&temporary, &bytes).map_err(|error| error.to_string())?;
    fs::rename(&temporary, &target).map_err(|error| error.to_string())?;
    Ok(())
}

fn safe_relative(rel: &str) -> Option<PathBuf> {
    if rel.is_empty() {
        return None;
    }
    let path = Path::new(rel);
    if path.is_absolute() {
        return None;
    }
    let mut clean = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => clean.push(part),
            Component::CurDir => {}
            _ => return None,
        }
    }
    if clean.as_os_str().is_empty() {
        None
    } else {
        Some(clean)
    }
}

fn mark_blob_uploaded(conn: &Connection, hash: &str, size: usize) -> Result<(), String> {
    conn.execute(
        "INSERT INTO sync_blobs(hash, size, uploaded_at) VALUES (?1, ?2, ?3)
         ON CONFLICT(hash) DO NOTHING",
        rusqlite::params![hash, size as i64, now_ms()],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}
