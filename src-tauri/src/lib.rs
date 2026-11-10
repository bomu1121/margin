mod artifact;
mod db;
mod indexer;
mod links;
mod outbox;
mod production;
mod ops;
mod sync;
mod vault;
mod window;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use db::{Annotation, AnnotationSegment, AppStats, IndexStats, NoteDetail, NoteMeta, Store};
use indexer::{IndexJob, IndexService, IndexStatus};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use std::sync::mpsc::Sender;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

static BOOT: OnceLock<Instant> = OnceLock::new();

struct AppState {
    store: Arc<Store>,
    production: Arc<production::ProductionService>,
    index: Arc<IndexService>,
    sync: Arc<sync::SyncEngine>,
    vault: Mutex<Option<String>>,
    watcher: Mutex<Option<RecommendedWatcher>>,
    /// Keep the tray handle alive for the process lifetime; dropping the last
    /// TrayIcon removes the icon from the notification area.
    tray: Mutex<Option<tauri::tray::TrayIcon>>,
}

#[tauri::command]
fn get_vault(state: State<'_, AppState>) -> Option<String> {
    state.vault.lock().ok().and_then(|vault| vault.clone())
}

/// A tiny marker lets ScribeFlow know this vault wants yebian artifact
/// sidecars. It is metadata only; no note content is touched.
fn ensure_vault_marker(root: &Path) {
    let marker = root.join(".yebian-vault");
    let body = r#"{"schema":"yebian-vault-v2","outbox":".yebian/outbox","artifacts":".yebian/artifacts"}"#;
    if std::fs::read_to_string(&marker).ok().as_deref() == Some(body) {
        return;
    }
    if let Err(error) = std::fs::write(&marker, body) {
        log::warn!("could not write vault marker {}: {error}", marker.display());
    }
}

#[tauri::command(async)]
fn set_vault(app: AppHandle, state: State<'_, AppState>, path: String) -> Result<IndexStats, String> {
    let path = vault::display_path(Path::new(&path));
    let path_buf = PathBuf::from(&path);
    if !path_buf.is_dir() {
        return Err(format!("不是目录：{path}"));
    }
    ensure_vault_marker(&path_buf);
    let stats = state.index.rebuild_wait(&path_buf)?;
    state.store.set_vault(&path)?;
    {
        let mut vault = state.vault.lock().map_err(|_| "vault lock poisoned")?;
        *vault = Some(path.clone());
    }
    if let Err(error) = start_watcher(state.index.sender(), path_buf, &state.watcher) {
        log::warn!("file watcher unavailable: {error}");
    }
    let _ = app.emit("vault-changed", &stats);
    Ok(stats)
}

#[tauri::command(async)]
fn reindex(state: State<'_, AppState>) -> Result<IndexStats, String> {
    let vault = state
        .vault
        .lock()
        .map_err(|_| "vault lock poisoned")?
        .clone()
        .ok_or_else(|| "还没有选择笔记库".to_string())?;
    state.index.rebuild_wait(Path::new(&vault))
}

#[tauri::command]
fn app_ready() {
    if let Some(start) = BOOT.get() {
        log::info!("app ready in {}ms", start.elapsed().as_millis());
    }
}

#[tauri::command(async)]
fn list_notes(state: State<'_, AppState>) -> Result<Vec<NoteMeta>, String> {
    state.store.list_notes()
}

#[tauri::command(async)]
fn get_note(state: State<'_, AppState>, id: String) -> Result<Option<NoteDetail>, String> {
    let mut detail = state.store.get_note(&id)?;
    if let Some(item) = &mut detail {
        if let Ok(root) = vault_root(&state) {
            item.artifact = artifact::for_note(&root, Path::new(&item.meta.path), &item.meta);
        }
    }
    Ok(detail)
}

#[tauri::command(async)]
fn list_artifacts(state: State<'_, AppState>) -> Result<Vec<artifact::StudioArtifact>, String> {
    let root = vault_root(&state)?;
    let notes = state.store.list_notes()?;
    Ok(artifact::scan(&root, &notes))
}

#[tauri::command(async)]
fn drain_outbox(state: State<'_, AppState>) -> Result<outbox::OutboxReport, String> {
    let root = vault_root(&state)?;
    Ok(outbox::drain(&root))
}

fn url_encode(value: &str) -> String {
    let mut out = String::new();
    for byte in value.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~' | b'/' | b':') {
            out.push(*byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

#[tauri::command(async)]
fn studio_info() -> artifact::StudioInfo {
    artifact::studio_info()
}

fn scribeflow_new_url(base: &str, vault: &Path) -> String {
    format!(
        "{}/?new=1&yebianVault={}",
        base.trim_end_matches('/'),
        url_encode(&vault.to_string_lossy())
    )
}

#[tauri::command(async)]
fn open_scribeflow(state: State<'_, AppState>, new_project: Option<bool>) -> Result<(), String> {
    // Deliberately fixed to the local ScribeFlow endpoint: this command must
    // not become a general "open any URL" primitive.
    let base = artifact::scribeflow_url();
    let url = if new_project.unwrap_or(false) {
        let vault = vault_root(&state)?;
        scribeflow_new_url(&base, &vault)
    } else {
        base
    };
    let url = url.as_str();
    #[cfg(target_os = "windows")]
    {
        // 不要走 `cmd /C start`：`Command` 只在参数含空格时才加引号，而带 `&` 的
        // deep link 会被 cmd 当成命令分隔符截断（实测地址栏只剩 ?new=1）。
        // rundll32 直接交给 ShellExecute，参数原样保留。
        std::process::Command::new("rundll32.exe")
            .args(["url.dll,FileProtocolHandler", url])
            .spawn()
            .map_err(|error| error.to_string())?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(url)
            .spawn()
            .map_err(|error| error.to_string())?;
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::process::Command::new("xdg-open")
            .arg(url)
            .spawn()
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[tauri::command(async)]
fn search_notes(state: State<'_, AppState>, query: String) -> Result<Vec<NoteMeta>, String> {
    state.store.search_notes(&query)
}

#[tauri::command(async)]
fn list_annotations(state: State<'_, AppState>, note_id: String) -> Result<Vec<Annotation>, String> {
    state.store.list_annotations(&note_id)
}

#[tauri::command(async)]
fn add_annotation(
    state: State<'_, AppState>,
    note_id: String,
    block_id: String,
    quote: String,
    body: String,
    kind: String,
    segments: Option<Vec<AnnotationSegment>>,
    color: Option<String>,
) -> Result<Annotation, String> {
    match segments.filter(|items| !items.is_empty()) {
        Some(items) => state.store.add_annotation_with_segments(
            &note_id,
            &block_id,
            &quote,
            &body,
            &kind,
            items,
            color.as_deref().unwrap_or("gold"),
        ),
        None => state.store.add_annotation(&note_id, &block_id, &quote, &body, &kind),
    }
}

#[tauri::command(async)]
fn delete_annotation(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state.store.delete_annotation(&id)
}

#[tauri::command(async)]
fn relocate_annotations(
    state: State<'_, AppState>,
    note_id: String,
    mappings: Vec<db::AnnotationRelocation>,
) -> Result<usize, String> {
    state.store.relocate_annotations(&note_id, &mappings)
}

fn vault_root(state: &State<'_, AppState>) -> Result<PathBuf, String> {
    state
        .vault
        .lock()
        .map_err(|_| "vault lock poisoned")?
        .clone()
        .map(PathBuf::from)
        .ok_or_else(|| "还没有选择笔记库".to_string())
}

#[tauri::command(async)]
fn list_folders(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    let root = vault_root(&state)?;
    Ok(vault::walk_folders(&root))
}

#[tauri::command(async)]
fn create_note(
    state: State<'_, AppState>,
    parent: String,
    title: String,
) -> Result<ops::CreatedNote, String> {
    let root = vault_root(&state)?;
    let created = ops::create_note(&root, &parent, &title)?;
    let abs = root.join(created.rel_path.replace('/', std::path::MAIN_SEPARATOR_STR));
    if let Err(error) = state.index.delta_wait(&root, vec![abs]) {
        log::warn!("index after create failed: {error}");
    }
    Ok(created)
}

#[tauri::command(async)]
fn create_folder(state: State<'_, AppState>, parent: String, name: String) -> Result<String, String> {
    let root = vault_root(&state)?;
    ops::create_folder(&root, &parent, &name)
}

#[tauri::command(async)]
fn rename_entry(
    state: State<'_, AppState>,
    rel_path: String,
    new_name: String,
) -> Result<ops::MoveReport, String> {
    let root = vault_root(&state)?;
    let report = ops::rename_entry(&root, &state.store, &rel_path, &new_name)?;
    if let Err(error) = state.index.move_wait(&root, report.moved.clone()) {
        log::warn!("index after rename failed: {error}");
    }
    let rewritten: Vec<PathBuf> = report
        .rewritten
        .iter()
        .map(|rel| root.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR)))
        .collect();
    if !rewritten.is_empty() {
        if let Err(error) = state.index.delta_wait(&root, rewritten) {
            log::warn!("index after link rewrite failed: {error}");
        }
    }
    Ok(report)
}

#[tauri::command(async)]
fn move_entry(
    state: State<'_, AppState>,
    rel_path: String,
    new_parent: String,
) -> Result<ops::MoveReport, String> {
    let root = vault_root(&state)?;
    let report = ops::move_entry(&root, &state.store, &rel_path, &new_parent)?;
    if let Err(error) = state.index.move_wait(&root, report.moved.clone()) {
        log::warn!("index after move failed: {error}");
    }
    let rewritten: Vec<PathBuf> = report
        .rewritten
        .iter()
        .map(|rel| root.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR)))
        .collect();
    if !rewritten.is_empty() {
        if let Err(error) = state.index.delta_wait(&root, rewritten) {
            log::warn!("index after link rewrite failed: {error}");
        }
    }
    Ok(report)
}

#[tauri::command(async)]
fn delete_entry(state: State<'_, AppState>, rel_path: String) -> Result<ops::DeletedEntry, String> {
    let root = vault_root(&state)?;
    let target = root.join(rel_path.replace('/', std::path::MAIN_SEPARATOR_STR));
    let affected: Vec<String> = if target.is_dir() {
        vault::walk_notes(&target)
            .iter()
            .map(|path| vault::rel_path(path, &root))
            .collect()
    } else {
        vec![rel_path.clone()]
    };
    let deleted = ops::delete_entry(&root, &rel_path)?;
    if let Err(error) = state.index.remove_wait(&root, affected) {
        log::warn!("index after delete failed: {error}");
    }
    Ok(deleted)
}

#[tauri::command(async)]
fn restore_entry(
    state: State<'_, AppState>,
    trash_path: String,
    rel_path: String,
) -> Result<String, String> {
    let root = vault_root(&state)?;
    let restored = ops::restore_entry(&root, &trash_path, &rel_path)?;
    let target = root.join(restored.replace('/', std::path::MAIN_SEPARATOR_STR));
    let restored_paths: Vec<PathBuf> = if target.is_dir() {
        vault::walk_notes(&target)
    } else {
        vec![target]
    };
    if let Err(error) = state.index.delta_wait(&root, restored_paths) {
        log::warn!("index after restore failed: {error}");
    }
    Ok(restored)
}

#[tauri::command]
fn get_index_status(state: State<'_, AppState>) -> IndexStatus {
    state.index.status()
}

/// Background sync. Local writes bump `sync_dirty.seq`, so a change is pushed
/// within a couple of seconds without any user action. The configured interval
/// is only the remote pull cadence.
fn spawn_auto_sync(engine: Arc<sync::SyncEngine>, store: Arc<Store>) {
    std::thread::spawn(move || {
        let mut last_dirty = -1i64;
        loop {
            std::thread::sleep(Duration::from_secs(2));
            let Ok(state) = engine.state() else { continue };
            if !state.configured || !state.auto_enabled {
                continue;
            }
            let Ok(dirty) = engine.dirty_seq() else { continue };
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_millis() as i64)
                .unwrap_or(0);
            let interval_due = state.last_sync_at == 0
                || now - state.last_sync_at >= state.auto_minutes * 60 * 1000;
            let dirty_due = dirty != last_dirty;
            if !dirty_due && !interval_due {
                continue;
            }
            let Ok(Some(vault)) = store.get_vault() else { continue };
            let Ok((server, token)) = engine.credentials() else { continue };
            let Ok(transport) = sync::HttpTransport::new(&server, &token) else { continue };
            if let Err(error) = engine.sync_once(&transport, Path::new(&vault)) {
                log::warn!("auto sync failed: {error}");
            }
            last_dirty = dirty;
        }
    });
}

#[tauri::command]
fn sync_status(state: State<'_, AppState>) -> Result<sync::SyncState, String> {
    state.sync.state()
}

#[tauri::command]
fn sync_configure(
    state: State<'_, AppState>,
    server_url: String,
    token: String,
    vault_id: String,
) -> Result<(), String> {
    state.sync.configure(&server_url, &token, &vault_id)
}

#[tauri::command]
fn sync_set_auto(state: State<'_, AppState>, enabled: bool, minutes: i64) -> Result<(), String> {
    state.sync.set_auto(enabled, minutes)
}

#[tauri::command]
async fn sync_now(state: State<'_, AppState>) -> Result<sync::SyncReport, String> {
    let engine = state.sync.clone();
    let root = vault_root(&state)?;
    let current = engine.state()?;
    if !current.configured {
        return Err("同步未配置".to_string());
    }
    let (server, token) = engine.credentials()?;
    let transport = sync::HttpTransport::new(&server, &token)?;
    tauri::async_runtime::spawn_blocking(move || engine.sync_once(&transport, &root))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command(async)]
fn get_stats(state: State<'_, AppState>) -> Result<AppStats, String> {
    let vault = state
        .vault
        .lock()
        .map_err(|_| "vault lock poisoned")?
        .clone();
    state.store.stats(vault)
}

fn start_watcher(
    sender: Sender<IndexJob>,
    vault: PathBuf,
    holder: &Mutex<Option<RecommendedWatcher>>,
) -> Result<(), String> {
    let (tx, rx) = std::sync::mpsc::channel::<PathBuf>();
    let mut watcher = notify::recommended_watcher(move |result: notify::Result<notify::Event>| {
        if let Ok(event) = result {
            for path in event.paths {
                let _ = tx.send(path);
            }
        }
    })
    .map_err(|error| error.to_string())?;

    watcher
        .watch(&vault, RecursiveMode::Recursive)
        .map_err(|error| error.to_string())?;

    let vault = vault.clone();
    std::thread::spawn(move || loop {
        let first = match rx.recv() {
            Ok(path) => path,
            Err(_) => break,
        };
        let mut paths = vec![first];
        loop {
            match rx.recv_timeout(Duration::from_millis(450)) {
                Ok(path) => paths.push(path),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => break,
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return,
            }
        }
        paths.sort();
        paths.dedup();
        if sender
            .send(IndexJob::Delta {
                root: vault.clone(),
                paths,
                reply: None,
            })
            .is_err()
        {
            break;
        }
    });

    *holder.lock().map_err(|_| "watcher lock poisoned")? = Some(watcher);
    Ok(())
}

fn find_sample_vault(app: &tauri::App) -> Option<String> {
    let mut candidates = Vec::new();
    if let Ok(resource_dir) = app.path().resource_dir() {
        candidates.push(resource_dir.join("sample-vault"));
    }
    candidates.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../sample-vault"));
    candidates
        .into_iter()
        .find(|path| path.is_dir())
        .map(|path| vault::display_path(&path))
}



#[tauri::command(async)]
fn open_external_url(url: String) -> Result<(), String> {
    let lower = url.trim().to_ascii_lowercase();
    if !(lower.starts_with("http://") || lower.starts_with("https://")) {
        return Err("只允许打开 http/https 链接".into());
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("rundll32.exe")
            .args(["url.dll,FileProtocolHandler", url.trim()])
            .spawn()
            .map_err(|error| error.to_string())?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open").arg(url.trim()).spawn().map_err(|error| error.to_string())?;
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::process::Command::new("xdg-open").arg(url.trim()).spawn().map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[tauri::command]
fn production_providers(state: State<'_, AppState>) -> Vec<production::ProviderPreset> {
    state.production.providers()
}

#[tauri::command]
async fn production_models(
    state: State<'_, AppState>,
    provider: String,
    base_url: String,
    api_key: String,
) -> Result<production::ModelList, String> {
    state.production.models(&provider, &base_url, &api_key).await
}

#[tauri::command]
async fn production_price(
    state: State<'_, AppState>,
    provider: String,
    model: String,
) -> Result<Option<production::PriceQuote>, String> {
    Ok(state.production.price(&provider, &model).await)
}

#[tauri::command]
async fn production_balance(
    state: State<'_, AppState>,
    provider: String,
    base_url: String,
    api_key: String,
) -> Result<production::BalanceResult, String> {
    state.production.balance(&provider, &base_url, &api_key).await
}

#[tauri::command]
async fn production_usage_summary(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    state.production.usage_summary()
}

#[tauri::command]
fn production_templates(state: State<'_, AppState>) -> Vec<production::TemplateInfo> {
    state.production.templates()
}

#[tauri::command]
fn production_settings(state: State<'_, AppState>) -> production::ProductionSettings {
    state.production.settings()
}

#[tauri::command(async)]
fn production_save_settings(
    state: State<'_, AppState>,
    input: production::SaveSettings,
) -> Result<production::ProductionSettings, String> {
    state.production.save_settings(&input)
}

#[tauri::command(async)]
fn production_list(state: State<'_, AppState>) -> Result<Vec<production::Job>, String> {
    let root = vault_root(&state)?;
    state.production.list(&root)
}

#[tauri::command(async)]
fn production_get(state: State<'_, AppState>, id: String) -> Result<Option<production::JobDetail>, String> {
    state.production.detail(&id)
}

#[tauri::command]
async fn production_test_model(
    state: State<'_, AppState>,
    provider: String,
    base_url: String,
    model: String,
    api_key: String,
) -> Result<production::TestModelResult, String> {
    state.production.test_model(&provider, &base_url, &model, &api_key).await
}

#[tauri::command]
fn production_versions(state: State<'_, AppState>, note_id: String) -> Result<Vec<production::VersionEntry>, String> {
    let root = vault_root(&state)?;
    state.production.versions(&root, &note_id)
}

#[tauri::command]
fn production_version_content(state: State<'_, AppState>, note_id: String, version_id: String) -> Result<String, String> {
    let root = vault_root(&state)?;
    state.production.version_content(&root, &note_id, &version_id)
}

#[tauri::command(async)]
fn production_restore_version(state: State<'_, AppState>, note_id: String, version_id: String) -> Result<String, String> {
    let root = vault_root(&state)?;
    state.production.restore_version(&root, &note_id, &version_id)
}

#[tauri::command(async)]
fn production_conflict_check(
    state: State<'_, AppState>,
    request: production::StartJobRequest,
) -> Result<Option<production::ConflictInfo>, String> {
    let root = vault_root(&state)?;
    state.production.conflict_check(&root, &request)
}

#[tauri::command(async)]
fn production_start(
    state: State<'_, AppState>,
    request: production::StartJobRequest,
) -> Result<production::Job, String> {
    let root = vault_root(&state)?;
    state.production.start(&root, request)
}

#[tauri::command(async)]
fn production_cancel(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state.production.cancel(&id)
}

#[tauri::command(async)]
fn production_resume_info(
    state: State<'_, AppState>,
    id: String,
) -> Result<production::ResumeInfo, String> {
    let root = vault_root(&state)?;
    state.production.resume_info(&root, &id)
}

#[tauri::command(async)]
fn production_resume(state: State<'_, AppState>, id: String) -> Result<production::Job, String> {
    let root = vault_root(&state)?;
    state.production.resume(&root, &id)
}

#[tauri::command(async)]
fn production_retry(state: State<'_, AppState>, id: String) -> Result<production::Job, String> {
    let root = vault_root(&state)?;
    state.production.retry(&root, &id)
}

#[tauri::command(async)]
fn production_rename(state: State<'_, AppState>, id: String, title: String) -> Result<production::Job, String> {
    state.production.rename(&id, &title)
}

#[tauri::command(async)]
fn production_delete(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let root = state.vault.lock().ok().and_then(|vault| vault.clone()).map(std::path::PathBuf::from);
    state.production.delete(root.as_deref(), &id)
}

#[tauri::command(async)]
fn production_prompts(state: State<'_, AppState>) -> Result<Vec<production::PromptEntry>, String> {
    state.production.prompts()
}

#[tauri::command(async)]
fn production_save_prompt(
    state: State<'_, AppState>,
    prompt: production::PromptEntry,
) -> Result<production::PromptEntry, String> {
    state.production.save_prompt(prompt)
}

#[tauri::command(async)]
fn production_delete_prompt(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state.production.delete_prompt(&id)
}

#[tauri::command]
fn production_prompt_versions(
    state: State<'_, AppState>,
    prompt_id: String,
) -> Result<Vec<production::PromptVersion>, String> {
    state.production.prompt_versions(&prompt_id)
}

#[tauri::command]
fn production_prompt_version(
    state: State<'_, AppState>,
    prompt_id: String,
    version: i64,
) -> Result<production::PromptVersion, String> {
    state.production.prompt_version(&prompt_id, version)
}

#[tauri::command]
fn production_restore_prompt_version(
    state: State<'_, AppState>,
    prompt_id: String,
    version: i64,
) -> Result<production::PromptEntry, String> {
    state.production.restore_prompt_version(&prompt_id, version)
}

#[tauri::command]
fn production_model_prices(state: State<'_, AppState>) -> Result<Vec<production::ModelPrice>, String> {
    state.production.model_prices()
}

#[tauri::command]
fn production_set_model_price(
    state: State<'_, AppState>,
    provider: String,
    model: String,
    input_cny: f64,
    output_cny: f64,
) -> Result<production::ModelPrice, String> {
    state.production.set_model_price(&provider, &model, input_cny, output_cny)
}

#[tauri::command]
fn production_delete_model_price(
    state: State<'_, AppState>,
    provider: String,
    model: String,
) -> Result<(), String> {
    state.production.delete_model_price(&provider, &model)
}

#[tauri::command]
async fn production_bili_login_start(state: State<'_, AppState>) -> Result<production::BiliLoginStart, String> {
    state.production.bili_login_start().await
}

#[tauri::command]
async fn production_bili_login_poll(
    state: State<'_, AppState>,
    key: String,
) -> Result<production::BiliLoginPoll, String> {
    state.production.bili_login_poll(&key).await
}

#[tauri::command(async)]
fn production_bili_logout(state: State<'_, AppState>) -> Result<(), String> {
    state.production.bili_logout()
}

#[tauri::command]
fn production_bili_account(state: State<'_, AppState>) -> Option<production::BiliAccount> {
    state.production.bili_account()
}

#[tauri::command]
async fn production_bili_favorites(state: State<'_, AppState>) -> Result<Vec<production::BiliFolder>, String> {
    state.production.bili_favorites().await
}

#[tauri::command]
async fn production_bili_followings(
    state: State<'_, AppState>,
    page: Option<u32>,
) -> Result<production::BiliFollowingPage, String> {
    state.production.bili_followings(page.unwrap_or(1)).await
}

#[tauri::command]
async fn production_bili_folder_videos(
    state: State<'_, AppState>,
    media_id: i64,
    page: Option<u32>,
) -> Result<production::BiliVideoPage, String> {
    state.production.bili_folder_videos(media_id, page.unwrap_or(1)).await
}

#[tauri::command]
async fn production_bili_watch_later(state: State<'_, AppState>) -> Result<Vec<production::BiliVideoRef>, String> {
    state.production.bili_watch_later().await
}


#[tauri::command]
async fn production_bili_history(state: State<'_, AppState>) -> Result<Vec<production::BiliVideoRef>, String> {
    state.production.bili_history().await
}

#[tauri::command]
async fn production_bili_preview(
    state: State<'_, AppState>,
    url: String,
) -> Result<production::BiliVideoPreview, String> {
    state.production.bili_preview(&url).await
}

#[tauri::command(async)]
async fn production_up_lookup(query: String) -> Result<production::BiliUpProfile, String> {
    let mid = production::subscriptions::parse_mid(&query)?;
    production::clients::fetch_bili_up_profile(mid).await
}

#[tauri::command(async)]
fn production_subscriptions(
    state: State<'_, AppState>,
) -> Result<Vec<production::Subscription>, String> {
    let root = vault_root(&state)?;
    state.production.store().list_subscriptions(&root.to_string_lossy())
}

#[tauri::command]
async fn production_subscription_save(
    state: State<'_, AppState>,
    input: production::SubscriptionInput,
) -> Result<production::Subscription, String> {
    let root = vault_root(&state)?;
    let service = state.production.clone();
    drop(state);
    if input.id.trim().is_empty() {
        return production::subscriptions::create(&service, &root, input).await;
    }
    let existing = service
        .store()
        .get_subscription(&input.id)?
        .ok_or_else(|| "订阅不存在".to_string())?;
    if !existing.vault_path.is_empty() && existing.vault_path != root.to_string_lossy() {
        return Err("这个订阅属于另一个笔记库".into());
    }
    let mut input = input;
    if input.mid <= 0 {
        input.mid = existing.mid;
    }
    if input.name.trim().is_empty() {
        input.name = existing.name;
    }
    if input.avatar.trim().is_empty() {
        input.avatar = existing.avatar;
    }
    if input.template_id.trim().is_empty() {
        input.template_id = existing.template_id;
    }
    if input.prompt.trim().is_empty() {
        input.prompt = existing.prompt;
    }
    service
        .store()
        .save_subscription(&root.to_string_lossy(), &input)
}

#[tauri::command(async)]
fn production_subscription_delete(
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    let root = vault_root(&state)?;
    state.production.store().delete_subscription(&root.to_string_lossy(), &id)
}

#[tauri::command(async)]
fn production_subscription_seen(
    state: State<'_, AppState>,
    id: String,
) -> Result<Vec<production::SeenVideo>, String> {
    let root = vault_root(&state)?;
    let sub = state
        .production
        .store()
        .get_subscription(&id)?
        .ok_or_else(|| "订阅不存在".to_string())?;
    if !sub.vault_path.is_empty() && sub.vault_path != root.to_string_lossy() {
        return Err("这个订阅属于另一个笔记库".into());
    }
    state.production.store().recent_seen(&root.to_string_lossy(), sub.mid, 50)
}

#[tauri::command]
async fn production_subscription_check(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<production::SubscriptionCheckResult, String> {
    let root = vault_root(&state)?;
    let service = state.production.clone();
    drop(state);
    production::subscriptions::check_now(&app, &service, &root, &id).await
}

#[tauri::command(async)]
fn production_auto_settings(state: State<'_, AppState>) -> production::AutoSettings {
    state.production.auto_settings()
}

#[tauri::command(async)]
fn production_save_auto_settings(
    state: State<'_, AppState>,
    input: production::SaveAutoSettings,
) -> Result<production::AutoSettings, String> {
    state.production.save_auto_settings(&input)
}

#[tauri::command(async)]
fn production_facts(
    state: State<'_, AppState>,
    status: Option<String>,
    query: Option<String>,
    limit: Option<usize>,
) -> Result<Vec<production::Fact>, String> {
    state.production.list_facts(
        status.as_deref().unwrap_or(""),
        query.as_deref().unwrap_or(""),
        limit.unwrap_or(100),
    )
}

#[tauri::command]
async fn production_history_search(query: String) -> Result<production::SourceSearchResult, String> {
    Ok(production::source_search::search(&query).await)
}

#[tauri::command(async)]
fn production_sources(
    state: State<'_, AppState>,
    query: Option<String>,
    source_type: Option<String>,
    limit: Option<usize>,
) -> Result<Vec<production::Source>, String> {
    state.production.history().list_sources(
        query.as_deref().unwrap_or(""),
        source_type.as_deref().unwrap_or(""),
        limit.unwrap_or(100),
    )
}

#[tauri::command(async)]
fn production_source(
    state: State<'_, AppState>,
    id: String,
) -> Result<production::SourceDetail, String> {
    state
        .production
        .history()
        .get_source(&id)?
        .ok_or_else(|| "史料不存在".to_string())
}

#[tauri::command(async)]
fn production_source_save(
    state: State<'_, AppState>,
    input: production::SourceInput,
) -> Result<production::Source, String> {
    state.production.history().save_source(input)
}

#[tauri::command(async)]
fn production_source_passage_save(
    state: State<'_, AppState>,
    input: production::PassageInput,
) -> Result<production::SourcePassage, String> {
    state.production.history().save_passage(input)
}

#[tauri::command(async)]
fn production_source_passage_delete(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state.production.history().delete_passage(&id)
}

#[tauri::command(async)]
fn production_source_delete(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state.production.history().delete_source(&id)
}

#[tauri::command(async)]
fn production_fact_citations(
    state: State<'_, AppState>,
    fact_id: String,
) -> Result<Vec<production::FactCitation>, String> {
    state.production.history().fact_citations(&fact_id)
}

#[tauri::command(async)]
fn production_citation_save(
    state: State<'_, AppState>,
    input: production::CitationInput,
) -> Result<production::FactCitation, String> {
    state.production.history().save_citation(input)
}

#[tauri::command(async)]
fn production_citation_delete(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state.production.history().delete_citation(&id)
}

#[tauri::command(async)]
fn production_events(
    state: State<'_, AppState>,
    query: Option<String>,
    limit: Option<usize>,
) -> Result<Vec<production::HistoryEvent>, String> {
    state.production.history().list_events(query.as_deref().unwrap_or(""), limit.unwrap_or(100))
}

#[tauri::command(async)]
fn production_event(state: State<'_, AppState>, id: String) -> Result<production::EventDetail, String> {
    state
        .production
        .history()
        .get_event(&id)?
        .ok_or_else(|| "事件不存在".to_string())
}

#[tauri::command(async)]
fn production_event_save(
    state: State<'_, AppState>,
    input: production::EventInput,
) -> Result<production::HistoryEvent, String> {
    state.production.history().save_event(input)
}

#[tauri::command(async)]
fn production_fact_event_set(
    state: State<'_, AppState>,
    fact_id: String,
    event_id: String,
) -> Result<(), String> {
    state.production.history().set_fact_event(&fact_id, &event_id)
}

#[tauri::command(async)]
fn production_fact_history_set(
    state: State<'_, AppState>,
    fact_id: String,
    historical_status: String,
    claim_kind: Option<String>,
) -> Result<(), String> {
    state
        .production
        .history()
        .set_fact_history(&fact_id, &historical_status, claim_kind.as_deref().unwrap_or(""))
}

#[tauri::command(async)]
fn production_fact(
    state: State<'_, AppState>,
    id: String,
) -> Result<production::Fact, String> {
    state.production.get_fact_required(&id)
}

#[tauri::command(async)]
fn production_fact_domain_set(
    state: State<'_, AppState>,
    fact_id: String,
    domain: String,
) -> Result<production::Fact, String> {
    state.production.set_fact_domain(&fact_id, &domain)
}

#[tauri::command]
async fn production_fact_history_ingest(
    state: State<'_, AppState>,
    id: String,
    force: Option<bool>,
) -> Result<production::HistoryIngestReport, String> {
    let service = state.production.clone();
    drop(state);
    service.ingest_fact_history(&id, force.unwrap_or(false)).await
}

#[tauri::command(async)]
fn production_fact_history_candidates(
    state: State<'_, AppState>,
    fact_id: String,
) -> Result<Vec<production::HistoryCandidate>, String> {
    state.production.history().candidates_for_fact(&fact_id)
}

#[tauri::command(async)]
fn production_history_candidates_all(
    state: State<'_, AppState>,
    status: Option<String>,
    limit: Option<usize>,
) -> Result<Vec<production::HistoryCandidate>, String> {
    state
        .production
        .history()
        .list_candidates(status.as_deref().unwrap_or("candidate"), limit.unwrap_or(500))
}

#[tauri::command]
async fn production_fact_history_candidate_import(
    state: State<'_, AppState>,
    candidate_id: String,
    text: String,
) -> Result<production::HistoryCandidate, String> {
    let service = state.production.clone();
    drop(state);
    service.import_history_candidate(&candidate_id, &text).await
}

#[tauri::command(async)]
fn production_fact_checks(
    state: State<'_, AppState>,
    id: String,
    limit: Option<usize>,
) -> Result<Vec<production::FactCheck>, String> {
    state.production.fact_checks(&id, limit.unwrap_or(30))
}

#[tauri::command(async)]
fn production_merge_fact(
    state: State<'_, AppState>,
    target_id: String,
    source_id: String,
) -> Result<production::Fact, String> {
    state.production.merge_fact(&target_id, &source_id)
}

#[tauri::command(async)]
fn production_fact_merges(
    state: State<'_, AppState>,
    limit: Option<usize>,
) -> Result<Vec<production::FactMerge>, String> {
    state.production.list_fact_merges(limit.unwrap_or(20))
}

#[tauri::command(async)]
fn production_fact_split(
    state: State<'_, AppState>,
    id: String,
    children: Vec<String>,
) -> Result<production::FactSplit, String> {
    state.production.split_fact(&id, &children)
}

#[tauri::command(async)]
fn production_fact_split_info(
    state: State<'_, AppState>,
    id: String,
) -> Result<Option<production::FactSplit>, String> {
    state.production.fact_split(&id)
}

#[tauri::command(async)]
fn production_fact_split_undo(
    state: State<'_, AppState>,
    split_id: String,
) -> Result<production::FactSplit, String> {
    state.production.undo_fact_split(&split_id)
}

#[tauri::command(async)]
fn production_undo_fact_merge(
    state: State<'_, AppState>,
    id: String,
) -> Result<production::Fact, String> {
    state.production.undo_fact_merge(&id)
}

#[tauri::command(async)]
fn production_fact_conflict_resolutions(
    state: State<'_, AppState>,
) -> Result<Vec<production::FactConflictResolution>, String> {
    state.production.conflict_resolutions()
}

#[tauri::command(async)]
fn production_fact_conflict_resolve(
    state: State<'_, AppState>,
    input: production::ConflictResolutionInput,
) -> Result<production::FactConflictResolution, String> {
    state.production.save_conflict_resolution(&input)
}

#[tauri::command(async)]
fn production_fact_reviews(state: State<'_, AppState>) -> Result<Vec<production::FactReview>, String> {
    state.production.fact_reviews()
}

#[tauri::command(async)]
fn production_fact_review_set(
    state: State<'_, AppState>,
    fact_id: String,
    status: String,
    note: Option<String>,
) -> Result<Option<production::FactReview>, String> {
    state.production.set_fact_review(&fact_id, &status, note.as_deref().unwrap_or(""))
}

#[tauri::command]
async fn production_verify_fact(
    state: State<'_, AppState>,
    id: String,
) -> Result<production::Fact, String> {
    let service = state.production.clone();
    drop(state);
    service.verify_fact(&id).await
}

#[tauri::command(async)]
fn production_note_facts(
    state: State<'_, AppState>,
    note_id: String,
    run_id: Option<String>,
) -> Result<Vec<production::Fact>, String> {
    state.production.facts_for_note(&note_id, run_id.as_deref().unwrap_or(""))
}

#[tauri::command(async)]
fn production_data_usage(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let root = vault_root(&state)?;
    state.production.data_usage(&root)
}

#[tauri::command(async)]
fn production_clear_temp(state: State<'_, AppState>) -> Result<production::ClearTempResult, String> {
    let root = vault_root(&state)?;
    state.production.clear_temp(&root)
}

#[tauri::command(async)]
fn production_clear_finished(state: State<'_, AppState>) -> Result<usize, String> {
    let root = vault_root(&state)?;
    state.production.clear_finished(&root)
}

#[tauri::command(async)]
fn production_clear_resumable(state: State<'_, AppState>) -> Result<usize, String> {
    let root = vault_root(&state)?;
    state.production.clear_resumable(&root)
}

/// 进程内调度器：只在这个桌面应用运行时工作，不注册 Windows 服务。
/// 每 60 秒看一次订阅表；真正到期时串行检查，检查之间至少间隔 2 秒。
fn start_subscription_monitor(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        // 先让窗口、索引和 vault 完成初始化，避免启动瞬间和用户操作抢资源。
        tokio::time::sleep(Duration::from_secs(20)).await;
        loop {
            let (root, service) = {
                let state = app.state::<AppState>();
                let root = state
                    .vault
                    .lock()
                    .ok()
                    .and_then(|vault| vault.clone())
                    .map(PathBuf::from);
                (root, state.production.clone())
            };
            if let Some(root) = root {
                if let Err(error) = production::subscriptions::run_due(&app, &service, &root).await {
                    log::warn!("subscription monitor: {error}");
                }
            }
            tokio::time::sleep(Duration::from_secs(60)).await;
        }
    });
}

fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

/// 托盘是“关掉窗口后自动检查还能继续”的前提；不注册系统服务。
fn install_tray(app: &tauri::App) -> tauri::Result<tauri::tray::TrayIcon> {
    let show = MenuItem::with_id(app, "show", "打开页边笔记", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &quit])?;
    let mut builder = TrayIconBuilder::new()
        .tooltip("页边笔记 · UP 更新自动制作")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_main_window(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    BOOT.get_or_init(Instant::now);

    tauri::Builder::default()
        // The single-instance plugin must be registered first so a second
        // launch is routed to the existing instance.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--hidden"]),
        ))
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(
            tauri_plugin_log::Builder::default()
                .level(log::LevelFilter::Info)
                .targets([
                    tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Stdout),
                    tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::LogDir {
                        file_name: Some("yebian".into()),
                    }),
                ])
                .build(),
        )
        .plugin(tauri_plugin_dialog::init())
        .on_window_event(|window, event| {
            if window.label() != "main" {
                return;
            }
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let close_to_tray = window
                    .app_handle()
                    .state::<AppState>()
                    .production
                    .auto_settings()
                    .close_to_tray;
                if close_to_tray {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .setup(|app| {
            let data_dir = app
                .path()
                .app_data_dir()
                .map_err(|error| std::io::Error::other(error.to_string()))?;
            std::fs::create_dir_all(&data_dir)?;
            let db_path = data_dir.join("yebian.sqlite");
            let index_path = data_dir.join("yebian-index.sqlite");
            let store = Arc::new(Store::open(&db_path).map_err(std::io::Error::other)?);
            let sink_handle = app.handle().clone();
            let status_sink: indexer::StatusSink = Arc::new(move |status| {
                let _ = sink_handle.emit("index-status", status);
            });
            let index = Arc::new(
                IndexService::open(index_path, Some(status_sink), Some(store.clone()))
                    .map_err(std::io::Error::other)?,
            );

            let stored_vault = store
                .get_vault()
                .unwrap_or(None)
                .map(|path| vault::display_path(Path::new(&path)));
            if let Some(path) = &stored_vault {
                if !Path::new(path).is_dir() {
                    log::warn!("saved vault is missing, falling back: {path}");
                }
            }
            let saved_vault = stored_vault.filter(|path| Path::new(path).is_dir());
            let sample_vault = find_sample_vault(app);
            let vault_path = saved_vault.or(sample_vault);

            if let Some(path) = &vault_path {
                ensure_vault_marker(Path::new(path));
                index.start(Path::new(path));
                if store.get_vault().unwrap_or(None).as_deref() != Some(path.as_str()) {
                    let _ = store.set_vault(path);
                }
            }

            let production = Arc::new(
                production::ProductionService::open(app.handle().clone(), &db_path, index.clone())
                    .map_err(std::io::Error::other)?,
            );
            if let Some(path) = &vault_path {
                let _ = production.bind_legacy_vault(Path::new(path));
            }
            let sync_engine = Arc::new(
                sync::SyncEngine::open(&db_path).map_err(std::io::Error::other)?,
            );
            app.manage(AppState {
                store: store.clone(),
                production,
                index: index.clone(),
                sync: sync_engine.clone(),
                vault: Mutex::new(vault_path.clone()),
                watcher: Mutex::new(None),
                tray: Mutex::new(None),
            });
            spawn_auto_sync(sync_engine.clone(), store.clone());
            match install_tray(app) {
                Ok(tray) => {
                    if let Ok(mut slot) = app.state::<AppState>().tray.lock() {
                        *slot = Some(tray);
                    }
                }
                Err(error) => log::warn!("tray unavailable: {error}"),
            }
            start_subscription_monitor(app.handle().clone());

            if let Some(path) = &vault_path {
                if let Err(error) = start_watcher(
                    index.sender(),
                    PathBuf::from(path),
                    &app.state::<AppState>().watcher,
                ) {
                    log::warn!("startup watcher unavailable: {error}");
                }
            }

            window::clamp_main_window(app.handle());
            window::install_scale_guard(app.handle());

            let start_hidden = std::env::args().any(|arg| arg == "--hidden")
                && app.state::<AppState>().production.auto_settings().close_to_tray;

            // The window-state plugin restores geometry during its own setup;
            // re-check once more after the window has been shown.
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(300));
                let handle_for_main = handle.clone();
                let _ = handle.run_on_main_thread(move || {
                    window::clamp_main_window(&handle_for_main);
                    if start_hidden {
                        if let Some(window) = handle_for_main.get_webview_window("main") {
                            let _ = window.hide();
                        }
                    }
                });
            });

            Ok(())
        })

        .invoke_handler(tauri::generate_handler![
            get_vault,
            set_vault,
            reindex,
            list_notes,
            get_note,
            list_artifacts,
            drain_outbox,
            studio_info,
            open_scribeflow,
            search_notes,
            list_annotations,
            add_annotation,
            delete_annotation,
            relocate_annotations,
            list_folders,
            create_note,
            create_folder,
            rename_entry,
            move_entry,
            delete_entry,
            restore_entry,
            sync_status,
            sync_configure,
            sync_set_auto,
            sync_now,
            get_stats,
            get_index_status,
            open_external_url,
            production_providers,
            production_models,
            production_price,
            production_balance,
            production_usage_summary,
            production_templates,
            production_settings,
            production_save_settings,
            production_list,
            production_get,
            production_test_model,
            production_versions,
            production_version_content,
            production_restore_version,
            production_conflict_check,
            production_start,
            production_cancel,
            production_resume_info,
            production_resume,
            production_retry,
            production_rename,
            production_delete,
            production_prompts,
            production_save_prompt,
            production_delete_prompt,
            production_prompt_versions,
            production_prompt_version,
            production_restore_prompt_version,
            production_model_prices,
            production_set_model_price,
            production_delete_model_price,
            production_bili_login_start,
            production_bili_login_poll,
            production_bili_logout,
            production_bili_account,
            production_bili_favorites,
            production_bili_followings,
            production_bili_folder_videos,
            production_bili_watch_later,
            production_bili_history,
            production_bili_preview,
            production_up_lookup,
            production_subscriptions,
            production_subscription_save,
            production_subscription_delete,
            production_subscription_seen,
            production_subscription_check,
            production_auto_settings,
            production_save_auto_settings,
            production_note_facts,
            production_facts,
            production_verify_fact,
            production_merge_fact,
            production_fact,
            production_fact_domain_set,
            production_fact_history_ingest,
            production_fact_history_candidates,
            production_history_candidates_all,
            production_fact_history_candidate_import,
            production_fact_checks,
            production_history_search,
            production_sources,
            production_source,
            production_source_save,
            production_source_passage_save,
            production_source_passage_delete,
            production_source_delete,
            production_fact_citations,
            production_citation_save,
            production_citation_delete,
            production_events,
            production_event,
            production_event_save,
            production_fact_event_set,
            production_fact_history_set,
            production_fact_merges,
            production_undo_fact_merge,
            production_fact_split,
            production_fact_split_info,
            production_fact_split_undo,
            production_fact_conflict_resolutions,
            production_fact_conflict_resolve,
            production_fact_reviews,
            production_fact_review_set,
            production_data_usage,
            production_clear_temp,
            production_clear_finished,
            production_clear_resumable,
            app_ready
        ])
        .run(tauri::generate_context!())
        .expect("error while running Yebian");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_project_url_keeps_both_query_params() {
        let url = scribeflow_new_url("http://127.0.0.1:5173/", Path::new("D:/笔记"));
        assert!(url.starts_with("http://127.0.0.1:5173/?new=1&yebianVault="));
        assert_eq!(url.matches('&').count(), 1);
        assert!(url.contains("D:/"));
        assert!(url.contains('%'));
    }
}
