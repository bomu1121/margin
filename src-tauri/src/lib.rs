mod artifact;
mod db;
mod indexer;
mod links;
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
    index: Arc<IndexService>,
    vault: Mutex<Option<String>>,
    watcher: Mutex<Option<RecommendedWatcher>>,
}

#[tauri::command]
fn get_vault(state: State<'_, AppState>) -> Option<String> {
    state.vault.lock().ok().and_then(|vault| vault.clone())
}

#[tauri::command(async)]
fn set_vault(app: AppHandle, state: State<'_, AppState>, path: String) -> Result<IndexStats, String> {
    let path = vault::display_path(Path::new(&path));
    let path_buf = PathBuf::from(&path);
    if !path_buf.is_dir() {
        return Err(format!("不是目录：{path}"));
    }
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
fn studio_info() -> artifact::StudioInfo {
    artifact::studio_info()
}

#[tauri::command(async)]
fn open_scribeflow() -> Result<(), String> {
    // Deliberately fixed to the local ScribeFlow endpoint: this command must
    // not become a general "open any URL" primitive.
    let url = artifact::scribeflow_url();
    let url = url.as_str();
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", url])
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
                index.start(Path::new(path));
                if store.get_vault().unwrap_or(None).as_deref() != Some(path.as_str()) {
                    let _ = store.set_vault(path);
                }
            }

            app.manage(AppState {
                store: store.clone(),
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
            app_ready
        ])
        .run(tauri::generate_context!())
        .expect("error while running Yebian");
}
