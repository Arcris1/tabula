pub mod ddl;
pub mod tunnel;
pub mod changeset;
pub mod commands;
pub mod config;
pub mod credentials;
pub mod drivers;
pub mod error;
pub mod history;
pub mod impexp;
pub mod known_hosts;
pub mod queries;
pub mod registry;
pub mod sessions;

use config::ConfigStore;
use history::HistoryStore;
use known_hosts::KnownHostsStore;
use queries::QueryRegistry;
use registry::ConnectionRegistry;
use sessions::SessionRegistry;
use tunnel::TunnelRegistry;
use tauri::Manager;
use tokio::sync::Mutex;

pub struct AppState {
    pub store: Mutex<ConfigStore>,
    pub registry: ConnectionRegistry,
    pub queries: QueryRegistry,
    pub history: Mutex<HistoryStore>,
    pub tunnels: TunnelRegistry,
    pub sessions: SessionRegistry,
    pub known_hosts: Mutex<KnownHostsStore>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // capture tiberius server messages (PRINT etc.) — see drivers::mssql::TiberiusMessageLayer
    {
        use tracing_subscriber::prelude::*;
        let _ = tracing_subscriber::registry()
            .with(crate::drivers::mssql::TiberiusMessageLayer)
            .try_init();
    }
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        // must be registered AFTER fs so it can save/restore the fs scope
        .plugin(tauri_plugin_persisted_scope::init())
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            app.manage(AppState {
                store: Mutex::new(ConfigStore::new(dir.join("connections.json"))),
                registry: ConnectionRegistry::default(),
                queries: QueryRegistry::default(),
                history: Mutex::new(HistoryStore::new(dir.join("history.json"))),
                tunnels: TunnelRegistry::default(),
                sessions: SessionRegistry::default(),
                known_hosts: Mutex::new(KnownHostsStore::new(dir.join("known_hosts.json"))),
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::Destroyed = event {
                let label = window.label().to_string();
                let app = window.app_handle().clone();
                tauri::async_runtime::spawn(async move {
                    let state = app.state::<AppState>();
                    // sessions hold pooled connections: release them first
                    for s in state.sessions.drain_by_prefix(&format!("{label}:")).await {
                        crate::commands::discard_session(s);
                    }
                    state.registry.remove_by_window(&label).await;
                    state.tunnels.remove_by_window(&label).await;
                });
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_connections,
            commands::save_connection,
            commands::delete_connection,
            commands::test_connection,
            commands::ping_connection,
            commands::connect,
            commands::reconnect,
            commands::disconnect,
            commands::list_tables,
            commands::list_functions,
            commands::function_definition,
            commands::export_table,
            commands::import_table,
            commands::fetch_rows,
            commands::scan_keys,
            commands::get_redis_value,
            commands::build_key_tree,
            commands::write_text_file,
            commands::remember_file,
            commands::schema_columns,
            commands::run_query,
            commands::cancel_query,
            commands::list_history,
            commands::add_history,
            commands::table_primary_key,
            commands::preview_changeset,
            commands::apply_changeset,
            commands::set_redis_value,
            commands::delete_redis_key,
            commands::set_redis_ttl,
            commands::redis_select_db,
            commands::table_structure,
            commands::preview_ddl,
            commands::apply_ddl,
            commands::open_new_window,
            commands::close_session,
            commands::forget_host_key,
            commands::list_databases,
            commands::create_database,
            commands::drop_database,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
