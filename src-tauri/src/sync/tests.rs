use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::Connection;
use uuid::Uuid;

use super::engine::SyncEngine;
use super::transport::MemoryTransport;

struct Device {
    db: PathBuf,
    vault: PathBuf,
    engine: SyncEngine,
}

impl Device {
    fn new(root: &Path, name: &str) -> Self {
        let dir = root.join(name);
        std::fs::create_dir_all(&dir).unwrap();
        let vault = dir.join("vault");
        std::fs::create_dir_all(&vault).unwrap();
        let db = dir.join("yebian.sqlite");
        init_db(&db);
        let engine = SyncEngine::open(&db).unwrap();
        engine.configure("http://sync.test", "token", "vault-1").unwrap();
        Self { db, vault, engine }
    }

    fn sync(&self, transport: &MemoryTransport) -> super::engine::SyncReport {
        self.engine.sync_once(transport, &self.vault).unwrap()
    }

    fn annotations(&self) -> Vec<(String, String)> {
        let conn = Connection::open(&self.db).unwrap();
        let mut statement = conn.prepare("SELECT id, body FROM annotations ORDER BY id").unwrap();
        let rows = statement
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        rows
    }

    fn facts(&self) -> Vec<(String, String)> {
        let conn = Connection::open(&self.db).unwrap();
        let mut statement = conn.prepare("SELECT id, text FROM facts ORDER BY id").unwrap();
        let rows = statement
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        rows
    }

    fn fact_body(&self, id: &str) -> Option<String> {
        let conn = Connection::open(&self.db).unwrap();
        conn.query_row("SELECT text FROM facts WHERE id = ?1", [id], |row| row.get(0)).ok()
    }

    fn conflicts(&self) -> Vec<(String, String, String)> {
        let conn = Connection::open(&self.db).unwrap();
        let mut statement = conn
            .prepare("SELECT entity, local_json, remote_json FROM sync_conflicts ORDER BY created_at")
            .unwrap();
        let rows = statement
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        rows
    }

    fn setting(&self, key: &str) -> Option<String> {
        let conn = Connection::open(&self.db).unwrap();
        conn.query_row("SELECT value FROM settings WHERE key = ?1", [key], |row| row.get(0))
            .ok()
    }
}

fn init_db(path: &Path) {
    let conn = Connection::open(path).unwrap();
    conn.execute_batch(
        "
        PRAGMA journal_mode = WAL;
        CREATE TABLE annotations (
          id TEXT PRIMARY KEY,
          note_id TEXT NOT NULL,
          block_id TEXT NOT NULL,
          quote TEXT NOT NULL,
          body TEXT NOT NULL DEFAULT '',
          kind TEXT NOT NULL DEFAULT '标记',
          created_at INTEGER NOT NULL
        );
        CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
        CREATE TABLE facts (
          id TEXT PRIMARY KEY,
          text TEXT NOT NULL,
          normalized TEXT NOT NULL UNIQUE,
          status TEXT NOT NULL DEFAULT 'candidate',
          queries_json TEXT NOT NULL DEFAULT '[]',
          created_at INTEGER NOT NULL,
          updated_at INTEGER NOT NULL,
          last_checked_at INTEGER NOT NULL DEFAULT 0
        );
        ",
    )
    .unwrap();
}

fn add_annotation(db: &Path, id: &str, body: &str) {
    let conn = Connection::open(db).unwrap();
    conn.execute(
        "INSERT INTO annotations(id, note_id, block_id, quote, body, kind, created_at)
         VALUES (?1, 'note-1', 'block-1', 'quote', ?2, '标记', 1)",
        rusqlite::params![id, body],
    )
    .unwrap();
}

fn update_annotation(db: &Path, id: &str, body: &str) {
    let conn = Connection::open(db).unwrap();
    conn.execute("UPDATE annotations SET body = ?1 WHERE id = ?2", rusqlite::params![body, id])
        .unwrap();
}

fn delete_annotation(db: &Path, id: &str) {
    let conn = Connection::open(db).unwrap();
    conn.execute("DELETE FROM annotations WHERE id = ?1", [id]).unwrap();
}

fn set_setting(db: &Path, key: &str, value: &str) {
    let conn = Connection::open(db).unwrap();
    conn.execute(
        "INSERT INTO settings(key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        rusqlite::params![key, value],
    )
    .unwrap();
}

fn add_fact(db: &Path, id: &str, text: &str, normalized: &str, updated_at: i64) {
    let conn = Connection::open(db).unwrap();
    conn.execute(
        "INSERT INTO facts(id, text, normalized, status, queries_json, created_at, updated_at, last_checked_at)
         VALUES (?1, ?2, ?3, 'candidate', '[]', ?4, ?4, 0)",
        rusqlite::params![id, text, normalized, updated_at],
    )
    .unwrap();
}

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as i64
}

fn set_mtime(path: &Path, ms: i64) {
    use std::fs::FileTimes;
    let modified = UNIX_EPOCH + Duration::from_millis(ms.max(0) as u64);
    let file = std::fs::OpenOptions::new().write(true).open(path).unwrap();
    file.set_times(FileTimes::new().set_modified(modified)).unwrap();
}

fn conflict_files(vault: &Path) -> Vec<(String, String)> {
    let root = vault.join(".yebian").join("conflicts");
    let mut out = Vec::new();
    for entry in walkdir::WalkDir::new(&root).into_iter().filter_map(Result::ok) {
        if !entry.file_type().is_file() {
            continue;
        }
        let rel = entry
            .path()
            .strip_prefix(&root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        let content = std::fs::read_to_string(entry.path()).unwrap();
        out.push((rel, content));
    }
    out
}

fn temp_root(label: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("yebian-sync-{label}-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    root
}

#[test]
fn records_converge_after_offline_edits_on_both_sides() {
    let root = temp_root("records");
    let a = Device::new(&root, "a");
    let b = Device::new(&root, "b");
    let transport = MemoryTransport::new();

    add_annotation(&a.db, "a1", "A original");
    a.sync(&transport);
    b.sync(&transport);
    assert_eq!(b.annotations(), vec![("a1".into(), "A original".into())]);

    // Independent adds while offline.
    add_annotation(&a.db, "a2", "from A");
    add_annotation(&b.db, "a3", "from B");
    a.sync(&transport);
    b.sync(&transport);
    a.sync(&transport);
    assert_eq!(a.annotations().len(), 3);
    assert_eq!(a.annotations(), b.annotations());

    // The same row edited on both sides: no manual step, one deterministic win.
    update_annotation(&a.db, "a1", "A edit");
    update_annotation(&b.db, "a1", "B edit");
    a.sync(&transport);
    b.sync(&transport);
    a.sync(&transport);
    b.sync(&transport);
    assert_eq!(a.annotations(), b.annotations());
    let winner = &a.annotations()[0].1;
    assert!(winner == "A edit" || winner == "B edit", "unexpected winner {winner}");

    // Delete on one side, edit on the other: converge without losing the row.
    delete_annotation(&a.db, "a2");
    update_annotation(&b.db, "a2", "B kept editing");
    a.sync(&transport);
    b.sync(&transport);
    a.sync(&transport);
    b.sync(&transport);
    assert_eq!(a.annotations(), b.annotations());
}

#[test]
fn settings_use_lww_and_secrets_stay_local() {
    let root = temp_root("settings");
    let a = Device::new(&root, "a");
    let b = Device::new(&root, "b");
    let transport = MemoryTransport::new();

    set_setting(&a.db, "production.retries", "3");
    set_setting(&a.db, "production.ai.api_key.deepseek", "sk-secret");
    set_setting(&a.db, "vault_path", "D:/somewhere");
    a.sync(&transport);
    b.sync(&transport);
    assert_eq!(b.setting("production.retries").as_deref(), Some("3"));
    assert_eq!(b.setting("production.ai.api_key.deepseek"), None);
    assert_eq!(b.setting("vault_path"), None);

    set_setting(&b.db, "production.retries", "9");
    b.sync(&transport);
    a.sync(&transport);
    assert_eq!(a.setting("production.retries").as_deref(), Some("9"));
}

#[test]
fn files_and_deletes_converge() {
    let root = temp_root("files");
    let a = Device::new(&root, "a");
    let b = Device::new(&root, "b");
    let transport = MemoryTransport::new();

    std::fs::create_dir_all(a.vault.join("notes")).unwrap();
    std::fs::write(a.vault.join("notes/one.md"), "# one\n\nfirst").unwrap();
    a.sync(&transport);
    b.sync(&transport);
    assert_eq!(
        std::fs::read_to_string(b.vault.join("notes/one.md")).unwrap(),
        "# one\n\nfirst"
    );

    // Concurrent edits of the same path: both sides end on the same bytes and
    // the losing version is kept in sync_conflicts, not silently dropped.
    std::fs::write(a.vault.join("notes/one.md"), "# one\n\nA version").unwrap();
    std::fs::write(b.vault.join("notes/one.md"), "# one\n\nB version").unwrap();
    a.sync(&transport);
    b.sync(&transport);
    a.sync(&transport);
    b.sync(&transport);
    let left = std::fs::read_to_string(a.vault.join("notes/one.md")).unwrap();
    let right = std::fs::read_to_string(b.vault.join("notes/one.md")).unwrap();
    assert_eq!(left, right);

    std::fs::remove_file(a.vault.join("notes/one.md")).unwrap();
    a.sync(&transport);
    b.sync(&transport);
    assert!(!b.vault.join("notes/one.md").exists());
}

#[test]
fn device_identity_and_cursor_survive_reopen() {
    let root = temp_root("reopen");
    let a = Device::new(&root, "a");
    let before = a.engine.state().unwrap();
    drop(a.engine);
    let reopened = SyncEngine::open(&a.db).unwrap();
    let after = reopened.state().unwrap();
    assert_eq!(before.device_id, after.device_id);
    assert!(!after.device_id.is_empty());
}

#[test]
fn configuring_twice_keeps_one_vault() {
    let root = temp_root("configure");
    let device = Device::new(&root, "a");
    device.engine.configure("http://one.test/", "t1", "v1").unwrap();
    device.engine.configure("http://two.test", "t2", "v2").unwrap();
    let state = device.engine.state().unwrap();
    assert_eq!(state.server_url, "http://two.test");
    assert_eq!(state.vault_id, "v2");
}

#[test]
fn two_devices_converge_over_real_http() {
    use std::sync::Arc;

    use super::transport::HttpTransport;
    use yebian_sync_server::{AppState, ServerStore};

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let server_root = temp_root("http-server");
    let store = Arc::new(
        ServerStore::open(&server_root.join("server.sqlite"), &server_root.join("blobs")).unwrap(),
    );
    let (vault_id, token) = store.create_vault("test").unwrap();
    let listener = runtime.block_on(async {
        tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap()
    });
    let address = listener.local_addr().unwrap();
    runtime.spawn(async move {
        let state = AppState { store, admin_token: String::new() };
        let _ = axum::serve(listener, yebian_sync_server::router(state)).await;
    });

    let base = format!("http://{address}");
    let root = temp_root("http-clients");
    let a = Device::new(&root, "a");
    let b = Device::new(&root, "b");
    a.engine.configure(&base, &token, &vault_id).unwrap();
    b.engine.configure(&base, &token, &vault_id).unwrap();
    let transport_a = HttpTransport::new(&base, &token).unwrap();
    let transport_b = HttpTransport::new(&base, &token).unwrap();

    add_annotation(&a.db, "h1", "over http");
    a.engine.sync_once(&transport_a, &a.vault).unwrap();
    b.engine.sync_once(&transport_b, &b.vault).unwrap();
    assert_eq!(b.annotations(), vec![("h1".to_string(), "over http".to_string())]);

    std::fs::write(a.vault.join("http.md"), "# http\n").unwrap();
    a.engine.sync_once(&transport_a, &a.vault).unwrap();
    b.engine.sync_once(&transport_b, &b.vault).unwrap();
    assert_eq!(std::fs::read_to_string(b.vault.join("http.md")).unwrap(), "# http\n");
}

#[test]
fn local_writes_bump_the_dirty_counter() {
    let root = temp_root("dirty");
    let a = Device::new(&root, "a");
    let before = a.engine.dirty_seq().unwrap();
    add_annotation(&a.db, "d1", "auto sync trigger");
    let after = a.engine.dirty_seq().unwrap();
    assert!(after > before, "expected a trigger bump: {before} -> {after}");

    set_setting(&a.db, "production.retries", "4");
    let after_setting = a.engine.dirty_seq().unwrap();
    assert!(after_setting > after, "settings writes should also bump");
}

#[test]
fn first_sync_keeps_newer_local_row_and_archives_remote() {
    let root = temp_root("first-local-newer");
    let a = Device::new(&root, "a");
    let b = Device::new(&root, "b");
    let transport = MemoryTransport::new();

    add_fact(&a.db, "f1", "from A", "norm-a", 1_000);
    a.sync(&transport);

    add_fact(&b.db, "f1", "from B", "norm-b", 2_000);
    let report = b.sync(&transport);
    assert_eq!(report.conflicts, 1);
    assert_eq!(b.fact_body("f1").as_deref(), Some("from B"));
    let conflicts = b.conflicts();
    assert_eq!(conflicts.len(), 1);
    assert!(conflicts[0].1.contains("from B"), "local version should be archived");
    assert!(conflicts[0].2.contains("from A"), "remote version should be archived");

    // The same conflict must not be recorded again on the next cycle.
    b.sync(&transport);
    assert_eq!(b.conflicts().len(), 1);

    a.sync(&transport);
    assert_eq!(a.fact_body("f1").as_deref(), Some("from B"));
}

#[test]
fn first_sync_takes_newer_server_row_and_archives_local() {
    let root = temp_root("first-remote-newer");
    let a = Device::new(&root, "a");
    let b = Device::new(&root, "b");
    let transport = MemoryTransport::new();

    add_fact(&a.db, "f1", "from A", "norm-a", 2_000);
    a.sync(&transport);

    add_fact(&b.db, "f1", "from B", "norm-b", 1_000);
    let report = b.sync(&transport);
    assert_eq!(report.conflicts, 1);
    assert_eq!(b.fact_body("f1").as_deref(), Some("from A"));
    let conflicts = b.conflicts();
    assert!(conflicts[0].1.contains("from B"));
    assert!(conflicts[0].2.contains("from A"));

    b.sync(&transport);
    assert_eq!(b.conflicts().len(), 1);
}

#[test]
fn first_sync_unions_rows_and_files_from_both_sides() {
    let root = temp_root("first-union");
    let a = Device::new(&root, "a");
    let b = Device::new(&root, "b");
    let transport = MemoryTransport::new();

    add_fact(&a.db, "f1", "from A", "norm-a", 1_000);
    std::fs::write(a.vault.join("a.md"), "# from A\n").unwrap();
    a.sync(&transport);

    add_fact(&b.db, "f2", "from B", "norm-b", 1_000);
    std::fs::write(b.vault.join("b.md"), "# from B\n").unwrap();
    let report = b.sync(&transport);
    assert_eq!(report.conflicts, 0);

    assert_eq!(b.facts(), vec![("f1".into(), "from A".into()), ("f2".into(), "from B".into())]);
    assert_eq!(std::fs::read_to_string(b.vault.join("a.md")).unwrap(), "# from A\n");
    assert_eq!(std::fs::read_to_string(b.vault.join("b.md")).unwrap(), "# from B\n");

    a.sync(&transport);
    assert_eq!(a.facts(), b.facts());
    assert_eq!(std::fs::read_to_string(a.vault.join("b.md")).unwrap(), "# from B\n");
}

#[test]
fn first_sync_file_conflicts_keep_the_newer_mtime_and_archive_the_loser() {
    let root = temp_root("first-file");
    let a = Device::new(&root, "a");
    let transport = MemoryTransport::new();

    std::fs::create_dir_all(a.vault.join("notes")).unwrap();
    std::fs::write(a.vault.join("notes/x.md"), "A version").unwrap();
    a.sync(&transport);

    // The local file is newer than the remote op: keep it and archive the
    // remote bytes under .yebian/conflicts/.
    let b = Device::new(&root, "b");
    std::fs::create_dir_all(b.vault.join("notes")).unwrap();
    std::fs::write(b.vault.join("notes/x.md"), "B version").unwrap();
    set_mtime(&b.vault.join("notes/x.md"), now_ms() + 60_000);
    let report = b.sync(&transport);
    assert_eq!(report.conflicts, 1);
    assert_eq!(std::fs::read_to_string(b.vault.join("notes/x.md")).unwrap(), "B version");
    assert!(conflict_files(&b.vault)
        .iter()
        .any(|(rel, content)| rel.ends_with("notes/x.md") && content == "A version"));

    a.sync(&transport);
    assert_eq!(std::fs::read_to_string(a.vault.join("notes/x.md")).unwrap(), "B version");

    // A fresh device with an older local file loses to the server version; its
    // own bytes are preserved in .yebian/conflicts/.
    std::fs::write(a.vault.join("notes/y.md"), "A second").unwrap();
    a.sync(&transport);
    let c = Device::new(&root, "c");
    std::fs::create_dir_all(c.vault.join("notes")).unwrap();
    std::fs::write(c.vault.join("notes/y.md"), "C second").unwrap();
    set_mtime(&c.vault.join("notes/y.md"), now_ms() - 600_000);
    let report = c.sync(&transport);
    assert_eq!(report.conflicts, 1);
    assert_eq!(std::fs::read_to_string(c.vault.join("notes/y.md")).unwrap(), "A second");
    assert!(conflict_files(&c.vault)
        .iter()
        .any(|(rel, content)| rel.ends_with("notes/y.md") && content == "C second"));
}

#[test]
fn empty_token_on_save_keeps_the_stored_one() {
    let root = temp_root("token");
    let device = Device::new(&root, "a");
    device.engine.configure("http://new.test", "", "vault-1").unwrap();
    let (url, token) = device.engine.credentials().unwrap();
    assert_eq!(url, "http://new.test");
    assert_eq!(token, "token");
}
