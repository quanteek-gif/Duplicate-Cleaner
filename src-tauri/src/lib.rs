//! Duplicate Cleaner — Tauri 2 application entry point (library part).
//!
//! All business logic lives in Rust; the webview only renders the UI and calls
//! the commands exposed below.

pub mod commands;
pub mod core;
pub mod database;
pub mod models;
pub mod services;
pub mod utils;

use std::sync::Arc;

use tauri::Manager;

use crate::database::Database;
use crate::services::scan_service::{ScanRegistry, ScanService};
use crate::utils::error::{AppError, AppResult};
use crate::utils::logger;

/// Application-wide state shared with every command.
pub struct AppState {
    pub db: Arc<Database>,
    pub scans: Arc<ScanService>,
    pub registry: Arc<ScanRegistry>,
}

/// Resolve the per-user data directory (e.g. `%APPDATA%\com.duplicatecleaner.desktop`).
fn data_dir(app: &tauri::AppHandle) -> AppResult<std::path::PathBuf> {
    app.path()
        .app_data_dir()
        .map_err(|error| AppError::other(format!("cannot resolve app data dir: {error}")))
}

fn log_dir(app: &tauri::AppHandle) -> AppResult<std::path::PathBuf> {
    app.path()
        .app_log_dir()
        .map_err(|error| AppError::other(format!("cannot resolve log dir: {error}")))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let handle = app.handle().clone();

            let log_directory = log_dir(&handle)?;
            let log_guard = logger::init(log_directory)?;
            // Keep the non-blocking writer alive for the whole runtime.
            app.manage(logger::LoggerGuardHolder::new(Some(log_guard)));

            let db_path = data_dir(&handle)?.join("duplicate-cleaner.sqlite3");
            tracing::info!(?db_path, "opening database");
            let db = Database::open(&db_path)?;
            let registry = Arc::new(ScanRegistry::default());
            let scans = ScanService::new(db.clone(), registry.clone(), handle.clone());

            // Commands take `State<Arc<Database>>` and `State<Arc<ScanService>>`,
            // so each shared value is registered under its own `Arc` type.
            app.manage(db.clone());
            app.manage(scans.clone());
            app.manage(registry.clone());
            app.manage(AppState {
                db: db.clone(),
                scans,
                registry,
            });

            // Trim stale history/cache in the background (never blocks startup).
            let cleanup_db = db.clone();
            tauri::async_runtime::spawn(async move {
                if let Ok(settings) = database::repository::load_settings(&cleanup_db) {
                    let _ = database::repository::trim_history(&cleanup_db, settings.history_limit);
                    let _ = database::repository::evict_stale_hash_cache(&cleanup_db, 90);
                }
            });

            tracing::info!("Duplicate Cleaner started");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // scanning
            commands::scanner::start_scan,
            commands::scanner::cancel_scan,
            commands::scanner::pause_scan,
            commands::scanner::resume_scan,
            commands::scanner::get_active_scan,
            commands::scanner::scan_large_files_cmd,
            commands::scanner::estimate_folder,
            // duplicates & previews
            commands::duplicates::get_scan_result,
            commands::duplicates::list_recent_scans,
            commands::duplicates::delete_scan,
            commands::duplicates::find_similar_photos,
            commands::duplicates::analyze_disk_usage,
            commands::duplicates::get_image_preview,
            // deletion & OS integration
            commands::file_operations::preview_deletion,
            commands::file_operations::delete_files,
            commands::file_operations::open_in_explorer,
            commands::file_operations::reveal_in_folder,
            commands::file_operations::get_settings,
            commands::file_operations::save_settings,
            commands::file_operations::clear_hash_cache,
            commands::file_operations::list_deletions,
            commands::file_operations::clear_deletion_history,
            // statistics
            commands::statistics::get_dashboard_stats,
            commands::statistics::get_drives,
            commands::statistics::get_quick_folders,
        ])
        .run(tauri::generate_context!())
        .unwrap_or_else(|error| {
            tracing::error!(%error, "Duplicate Cleaner stopped");
            std::process::exit(1);
        });
}
