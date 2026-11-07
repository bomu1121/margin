mod artifact;
mod db;
mod indexer;
mod links;
mod outbox;
mod production;
mod ops;
mod vault;
mod window;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use db::{Annotation, AppStats, IndexStats, NoteDetail, NoteMeta, Store};
use indexer::{IndexJob, IndexService, IndexStatus};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use std::sync::mpsc::Sender;
use tauri::{AppHandle, Emitter, Manager, State};

static BOOT: OnceLock<Instant> = OnceLock::new();

struct AppState {
    store: Arc<Store>,
    production: Arc<production::ProductionService>,
    index: Arc<IndexService>,
    vault: Mutex<Option<String>>,
    watcher: Mutex<Option<RecommendedWatcher>>,
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
) -> Result<Annotation, String> {
    state.store.add_annotation(&note_id, &block_id, &quote, &body, &kind)
}

#[tauri::command(async)]
fn delete_annotation(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state.store.delete_annotation(&id)
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
    state.production.list()
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
fn production_data_usage(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let root = vault_root(&state)?;
    state.production.data_usage(&root)
}

#[tauri::command(async)]
fn production_clear_temp(state: State<'_, AppState>) -> Result<u64, String> {
    let root = vault_root(&state)?;
    state.production.clear_temp(&root)
}

#[tauri::command(async)]
fn production_clear_finished(state: State<'_, AppState>) -> Result<usize, String> {
    state.production.clear_finished()
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
            app.manage(AppState {
                store: store.clone(),
                production,
                index: index.clone(),
                vault: Mutex::new(vault_path.clone()),
                watcher: Mutex::new(None),
            });

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

            // The window-state plugin restores geometry during its own setup;
            // re-check once more after the window has been shown.
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(300));
                let handle_for_main = handle.clone();
                let _ = handle.run_on_main_thread(move || {
                    window::clamp_main_window(&handle_for_main);
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
            list_folders,
            create_note,
            create_folder,
            rename_entry,
            move_entry,
            delete_entry,
            restore_entry,
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
            production_bili_folder_videos,
            production_bili_watch_later,
            production_bili_history,
            production_bili_preview,
            production_data_usage,
            production_clear_temp,
            production_clear_finished,
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
