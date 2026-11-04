mod db;
mod vault;
mod window;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use db::{Annotation, AppStats, IndexStats, NoteDetail, NoteMeta, Store};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use tauri::{AppHandle, Emitter, Manager, State};

static BOOT: OnceLock<Instant> = OnceLock::new();

struct AppState {
    store: Arc<Store>,
    vault: Mutex<Option<String>>,
    watcher: Mutex<Option<RecommendedWatcher>>,
}

#[tauri::command]
fn get_vault(state: State<'_, AppState>) -> Option<String> {
    state.vault.lock().ok().and_then(|vault| vault.clone())
}

#[tauri::command]
fn set_vault(app: AppHandle, state: State<'_, AppState>, path: String) -> Result<IndexStats, String> {
    let path = vault::display_path(Path::new(&path));
    let path_buf = PathBuf::from(&path);
    if !path_buf.is_dir() {
        return Err(format!("不是目录：{path}"));
    }
    let stats = state.store.index_vault(&path_buf, true)?;
    state.store.set_vault(&path)?;
    {
        let mut vault = state.vault.lock().map_err(|_| "vault lock poisoned")?;
        *vault = Some(path.clone());
    }
    if let Err(error) = start_watcher(&app, state.store.clone(), path_buf, &state.watcher) {
        log::warn!("file watcher unavailable: {error}");
    }
    Ok(stats)
}

#[tauri::command]
fn reindex(state: State<'_, AppState>) -> Result<IndexStats, String> {
    let vault = state
        .vault
        .lock()
        .map_err(|_| "vault lock poisoned")?
        .clone()
        .ok_or_else(|| "还没有选择笔记库".to_string())?;
    state.store.index_vault(Path::new(&vault), true)
}

#[tauri::command]
fn app_ready() {
    if let Some(start) = BOOT.get() {
        log::info!("app ready in {}ms", start.elapsed().as_millis());
    }
}

#[tauri::command]
fn list_notes(state: State<'_, AppState>) -> Result<Vec<NoteMeta>, String> {
    state.store.list_notes()
}

#[tauri::command]
fn get_note(state: State<'_, AppState>, id: String) -> Result<Option<NoteDetail>, String> {
    state.store.get_note(&id)
}

#[tauri::command]
fn search_notes(state: State<'_, AppState>, query: String) -> Result<Vec<NoteMeta>, String> {
    state.store.search_notes(&query)
}

#[tauri::command]
fn list_annotations(state: State<'_, AppState>, note_id: String) -> Result<Vec<Annotation>, String> {
    state.store.list_annotations(&note_id)
}

#[tauri::command]
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

#[tauri::command]
fn delete_annotation(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state.store.delete_annotation(&id)
}

#[tauri::command]
fn get_stats(state: State<'_, AppState>) -> Result<AppStats, String> {
    let vault = state
        .vault
        .lock()
        .map_err(|_| "vault lock poisoned")?
        .clone();
    state.store.stats(vault)
}

fn start_watcher(
    app: &AppHandle,
    store: Arc<Store>,
    vault: PathBuf,
    holder: &Mutex<Option<RecommendedWatcher>>,
) -> Result<(), String> {
    let (tx, rx) = std::sync::mpsc::channel::<()>();
    let mut watcher = notify::recommended_watcher(move |result: notify::Result<notify::Event>| {
        if let Ok(event) = result {
            let relevant = event.paths.iter().any(|path| {
                path.extension()
                    .map(|ext| ext.eq_ignore_ascii_case("md"))
                    .unwrap_or(false)
            });
            if relevant {
                let _ = tx.send(());
            }
        }
    })
    .map_err(|error| error.to_string())?;

    watcher
        .watch(&vault, RecursiveMode::Recursive)
        .map_err(|error| error.to_string())?;

    let app = app.clone();
    let store = store.clone();
    let vault = vault.clone();
    std::thread::spawn(move || loop {
        if rx.recv().is_err() {
            break;
        }
        loop {
            match rx.recv_timeout(Duration::from_millis(450)) {
                Ok(()) => continue,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => break,
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return,
            }
        }
        match store.index_vault(&vault, false) {
            Ok(stats) => {
                log::info!("watcher reindexed {} notes", stats.notes);
                let _ = app.emit("vault-changed", &stats);
            }
            Err(error) => log::error!("watcher reindex failed: {error}"),
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
            let store = Arc::new(Store::open(&db_path).map_err(std::io::Error::other)?);

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
                match store.index_vault(Path::new(path), false) {
                    Ok(stats) => log::info!(
                        "startup index: {} notes in {}ms",
                        stats.notes,
                        stats.elapsed_ms
                    ),
                    Err(error) => log::error!("startup index failed: {error}"),
                }
                if store.get_vault().unwrap_or(None).as_deref() != Some(path.as_str()) {
                    let _ = store.set_vault(path);
                }
            }

            app.manage(AppState {
                store: store.clone(),
                vault: Mutex::new(vault_path.clone()),
                watcher: Mutex::new(None),
            });

            if let Some(path) = &vault_path {
                if let Err(error) = start_watcher(
                    app.handle(),
                    store,
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
            search_notes,
            list_annotations,
            add_annotation,
            delete_annotation,
            get_stats,
            app_ready
        ])
        .run(tauri::generate_context!())
        .expect("error while running Yebian");
}
