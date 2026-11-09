//! The relay half of Yebian sync.
//!
//! It never parses `payload`: an op is opaque bytes from the server's point of
//! view. That keeps the door open for end-to-end encryption later. The only
//! guarantees it adds are: a monotonic `seq` per store, op-id dedupe, token
//! auth and content-addressed blobs.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use axum::extract::{Path as AxumPath, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{get, post, put};
use axum::{Json, Router};
use rand::RngCore;
use rusqlite::{Connection, OptionalExtension};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

pub struct ServerStore {
    db: Mutex<Connection>,
    blobs: PathBuf,
}

#[derive(Clone)]
pub struct AppState {
    pub store: std::sync::Arc<ServerStore>,
    pub admin_token: String,
}

impl ServerStore {
    pub fn open(db: &Path, blobs: &Path) -> Result<Self, String> {
        if let Some(parent) = db.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        std::fs::create_dir_all(blobs).map_err(|error| error.to_string())?;
        let conn = Connection::open(db).map_err(|error| error.to_string())?;
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA busy_timeout = 30000;
             CREATE TABLE IF NOT EXISTS vaults (
               id TEXT PRIMARY KEY,
               name TEXT NOT NULL DEFAULT '',
               token_hash TEXT NOT NULL UNIQUE,
               created_at INTEGER NOT NULL
             );
             CREATE TABLE IF NOT EXISTS ops (
               seq INTEGER PRIMARY KEY AUTOINCREMENT,
               vault_id TEXT NOT NULL,
               op_id TEXT NOT NULL,
               data TEXT NOT NULL,
               received_at INTEGER NOT NULL,
               UNIQUE(vault_id, op_id)
             );
             CREATE INDEX IF NOT EXISTS idx_ops_vault_seq ON ops(vault_id, seq);
             CREATE TABLE IF NOT EXISTS blobs (
               vault_id TEXT NOT NULL,
               hash TEXT NOT NULL,
               size INTEGER NOT NULL,
               PRIMARY KEY(vault_id, hash)
             );",
        )
        .map_err(|error| error.to_string())?;
        Ok(Self { db: Mutex::new(conn), blobs: blobs.to_path_buf() })
    }

    pub fn create_vault(&self, name: &str) -> Result<(String, String), String> {
        let id = uuid::Uuid::new_v4().to_string();
        let mut raw = [0u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut raw);
        let token = hex::encode(raw);
        let hash = sha256_hex(token.as_bytes());
        let conn = self.db.lock().map_err(|error| error.to_string())?;
        conn.execute(
            "INSERT INTO vaults(id, name, token_hash, created_at) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![id, name, hash, now_ms()],
        )
        .map_err(|error| error.to_string())?;
        Ok((id, token))
    }

    pub fn authenticate(&self, vault_id: &str, token: &str) -> Result<bool, String> {
        if !valid_vault_id(vault_id) || token.is_empty() {
            return Ok(false);
        }
        let conn = self.db.lock().map_err(|error| error.to_string())?;
        let stored: Option<String> = conn
            .query_row("SELECT token_hash FROM vaults WHERE id = ?1", [vault_id], |row| row.get(0))
            .optional()
            .map_err(|error| error.to_string())?;
        Ok(stored.as_deref() == Some(sha256_hex(token.as_bytes()).as_str()))
    }

    pub fn push(&self, vault_id: &str, ops: &[Value]) -> Result<Vec<String>, String> {
        let conn = self.db.lock().map_err(|error| error.to_string())?;
        let mut accepted = Vec::new();
        for op in ops {
            let op_id = op.get("op_id").and_then(Value::as_str).unwrap_or("");
            if op_id.is_empty() {
                continue;
            }
            let inserted = conn
                .execute(
                    "INSERT OR IGNORE INTO ops(vault_id, op_id, data, received_at) VALUES (?1, ?2, ?3, ?4)",
                    rusqlite::params![vault_id, op_id, op.to_string(), now_ms()],
                )
                .map_err(|error| error.to_string())?;
            if inserted > 0 {
                accepted.push(op_id.to_string());
            }
        }
        Ok(accepted)
    }

    pub fn pull(
        &self,
        vault_id: &str,
        cursor: i64,
        limit: usize,
    ) -> Result<(Vec<Value>, i64, bool), String> {
        let limit = limit.clamp(1, 1000);
        let conn = self.db.lock().map_err(|error| error.to_string())?;
        let mut statement = conn
            .prepare("SELECT seq, data FROM ops WHERE vault_id = ?1 AND seq > ?2 ORDER BY seq LIMIT ?3")
            .map_err(|error| error.to_string())?;
        let rows = statement
            .query_map(
                rusqlite::params![vault_id, cursor, (limit + 1) as i64],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
            )
            .map_err(|error| error.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?;
        let has_more = rows.len() > limit;
        let selected = &rows[..rows.len().min(limit)];
        let next = selected.last().map(|(seq, _)| *seq).unwrap_or(cursor);
        let mut out = Vec::with_capacity(selected.len());
        for (seq, data) in selected {
            let op: Value = serde_json::from_str(data).map_err(|error| error.to_string())?;
            out.push(json!({ "seq": seq, "op": op }));
        }
        Ok((out, next, has_more))
    }

    pub fn put_blob(&self, vault_id: &str, hash: &str, bytes: &[u8]) -> Result<(), String> {
        if !valid_hash(hash) {
            return Err("bad blob hash".to_string());
        }
        if sha256_hex(bytes) != hash {
            return Err("blob hash mismatch".to_string());
        }
        let dir = self.blobs.join(vault_id);
        std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
        let path = dir.join(hash);
        let temporary = dir.join(format!("{hash}.tmp"));
        std::fs::write(&temporary, bytes).map_err(|error| error.to_string())?;
        std::fs::rename(&temporary, &path).map_err(|error| error.to_string())?;
        let conn = self.db.lock().map_err(|error| error.to_string())?;
        conn.execute(
            "INSERT INTO blobs(vault_id, hash, size) VALUES (?1, ?2, ?3)
             ON CONFLICT(vault_id, hash) DO NOTHING",
            rusqlite::params![vault_id, hash, bytes.len() as i64],
        )
        .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub fn get_blob(&self, vault_id: &str, hash: &str) -> Result<Option<Vec<u8>>, String> {
        if !valid_hash(hash) {
            return Ok(None);
        }
        let path = self.blobs.join(vault_id).join(hash);
        match std::fs::read(path) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.to_string()),
        }
    }
}

fn valid_vault_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value.chars().all(|character| character.is_ascii_alphanumeric() || character == '-')
}

fn valid_hash(value: &str) -> bool {
    value.len() == 64 && value.chars().all(|character| character.is_ascii_hexdigit())
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

fn authorize(state: &AppState, headers: &HeaderMap, vault: &str) -> Result<(), (StatusCode, String)> {
    let token = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .unwrap_or("");
    match state.store.authenticate(vault, token) {
        Ok(true) => Ok(()),
        Ok(false) => Err((StatusCode::UNAUTHORIZED, "invalid vault token".to_string())),
        Err(error) => Err((StatusCode::INTERNAL_SERVER_ERROR, error)),
    }
}

async fn health() -> impl IntoResponse {
    Json(json!({ "ok": true }))
}

async fn push_ops(
    State(state): State<AppState>,
    AxumPath(vault): AxumPath<String>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<Value>, (StatusCode, String)> {
    authorize(&state, &headers, &vault)?;
    let ops = body.get("ops").and_then(Value::as_array).cloned().unwrap_or_default();
    let accepted = state
        .store
        .push(&vault, &ops)
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error))?;
    Ok(Json(json!({ "accepted": accepted })))
}

async fn pull_ops(
    State(state): State<AppState>,
    AxumPath(vault): AxumPath<String>,
    headers: HeaderMap,
    axum::extract::Query(query): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, (StatusCode, String)> {
    authorize(&state, &headers, &vault)?;
    let cursor = query.get("cursor").and_then(|value| value.parse::<i64>().ok()).unwrap_or(0);
    let limit = query.get("limit").and_then(|value| value.parse::<usize>().ok()).unwrap_or(200);
    let (ops, next, has_more) = state
        .store
        .pull(&vault, cursor, limit)
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error))?;
    Ok(Json(json!({ "ops": ops, "cursor": next, "has_more": has_more })))
}

async fn put_blob(
    State(state): State<AppState>,
    AxumPath((vault, hash)): AxumPath<(String, String)>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<Value>, (StatusCode, String)> {
    authorize(&state, &headers, &vault)?;
    if body.len() > 64 * 1024 * 1024 {
        return Err((StatusCode::PAYLOAD_TOO_LARGE, "blob too large".to_string()));
    }
    state
        .store
        .put_blob(&vault, &hash, &body)
        .map_err(|error| (StatusCode::BAD_REQUEST, error))?;
    Ok(Json(json!({ "ok": true })))
}

async fn get_blob(
    State(state): State<AppState>,
    AxumPath((vault, hash)): AxumPath<(String, String)>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    authorize(&state, &headers, &vault)?;
    match state
        .store
        .get_blob(&vault, &hash)
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error))?
    {
        Some(bytes) => Ok(([(axum::http::header::CONTENT_TYPE, "application/octet-stream")], bytes)),
        None => Err((StatusCode::NOT_FOUND, "blob not found".to_string())),
    }
}

#[derive(Deserialize)]
struct CreateVaultBody {
    name: Option<String>,
}

async fn admin_create_vault(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<CreateVaultBody>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let token = headers
        .get("x-admin-token")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");
    if state.admin_token.is_empty() || token != state.admin_token {
        return Err((StatusCode::UNAUTHORIZED, "invalid admin token".to_string()));
    }
    let (vault_id, token) = state
        .store
        .create_vault(body.name.as_deref().unwrap_or(""))
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error))?;
    Ok(Json(json!({ "vault_id": vault_id, "token": token })))
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/v1/health", get(health))
        .route("/v1/vaults/{vault}/ops", post(push_ops).get(pull_ops))
        .route("/v1/vaults/{vault}/blobs/{hash}", put(put_blob).get(get_blob))
        .route("/v1/admin/vaults", post(admin_create_vault))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store(label: &str) -> ServerStore {
        let root = std::env::temp_dir().join(format!("yebian-sync-server-{label}-{}", uuid::Uuid::new_v4()));
        ServerStore::open(&root.join("server.sqlite"), &root.join("blobs")).unwrap()
    }

    #[test]
    fn vault_tokens_are_hashed_and_enforced() {
        let store = store("auth");
        let (vault, token) = store.create_vault("test").unwrap();
        assert!(store.authenticate(&vault, &token).unwrap());
        assert!(!store.authenticate(&vault, "wrong").unwrap());
        assert!(!store.authenticate("other", &token).unwrap());
    }

    #[test]
    fn push_dedupes_and_pull_uses_a_cursor() {
        let store = store("ops");
        let (vault, _) = store.create_vault("test").unwrap();
        let op = json!({ "op_id": "op-1", "entity": "table:annotations", "pk": "a", "kind": "upsert", "hlc": "1", "payload": {} });
        assert_eq!(store.push(&vault, &[op.clone()]).unwrap(), vec!["op-1".to_string()]);
        assert!(store.push(&vault, &[op]).unwrap().is_empty());
        let (first, cursor, more) = store.pull(&vault, 0, 10).unwrap();
        assert_eq!(first.len(), 1);
        assert!(!more);
        assert!(store.pull(&vault, cursor, 10).unwrap().0.is_empty());
    }

    #[test]
    fn blobs_are_content_addressed() {
        let store = store("blobs");
        let (vault, _) = store.create_vault("test").unwrap();
        let bytes = b"# note\n";
        let hash = sha256_hex(bytes);
        store.put_blob(&vault, &hash, bytes).unwrap();
        assert_eq!(store.get_blob(&vault, &hash).unwrap().unwrap(), bytes);
        assert!(store.put_blob(&vault, &hash, b"tampered").is_err());
        assert!(store.get_blob(&vault, &"0".repeat(64)).unwrap().is_none());
    }

    #[test]
    fn invalid_ids_and_hashes_are_rejected() {
        assert!(!valid_vault_id("../escape"));
        assert!(!valid_vault_id(""));
        assert!(valid_vault_id("vault-1"));
        assert!(!valid_hash("abc"));
        assert!(valid_hash(&"a".repeat(64)));
    }
}
