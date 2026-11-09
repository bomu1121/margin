use std::path::PathBuf;
use std::sync::Arc;

use yebian_sync_server::{router, AppState, ServerStore};

fn main() {
    let mut args = std::env::args().skip(1).collect::<Vec<_>>();
    let command = args.first().cloned().unwrap_or_default();

    let mut db = PathBuf::from("yebian-sync.sqlite");
    let mut blobs = PathBuf::from("blobs");
    let mut bind = "127.0.0.1:8787".to_string();
    let mut admin_token = std::env::var("SYNC_ADMIN_TOKEN").unwrap_or_default();
    let mut name = "default".to_string();

    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--db" if index + 1 < args.len() => {
                db = PathBuf::from(&args[index + 1]);
                index += 2;
            }
            "--blobs" if index + 1 < args.len() => {
                blobs = PathBuf::from(&args[index + 1]);
                index += 2;
            }
            "--bind" if index + 1 < args.len() => {
                bind = args[index + 1].clone();
                index += 2;
            }
            "--admin-token" if index + 1 < args.len() => {
                admin_token = args[index + 1].clone();
                index += 2;
            }
            value if command == "create-vault" && value != "create-vault" => {
                name = value.to_string();
                index += 1;
            }
            _ => index += 1,
        }
    }
    args.clear();

    let store = match ServerStore::open(&db, &blobs) {
        Ok(store) => Arc::new(store),
        Err(error) => {
            eprintln!("open store failed: {error}");
            std::process::exit(1);
        }
    };

    if command == "create-vault" {
        match store.create_vault(&name) {
            Ok((vault_id, token)) => {
                println!("vault_id: {vault_id}");
                println!("token: {token}");
                println!("server: put these into the client's 同步设置");
            }
            Err(error) => {
                eprintln!("create vault failed: {error}");
                std::process::exit(1);
            }
        }
        return;
    }

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    runtime.block_on(async move {
        let state = AppState { store, admin_token };
        let listener = match tokio::net::TcpListener::bind(&bind).await {
            Ok(listener) => listener,
            Err(error) => {
                eprintln!("bind {bind} failed: {error}");
                std::process::exit(1);
            }
        };
        println!("yebian-sync-server listening on http://{bind}");
        if let Err(error) = axum::serve(listener, router(state)).await {
            eprintln!("server stopped: {error}");
        }
    });
}
