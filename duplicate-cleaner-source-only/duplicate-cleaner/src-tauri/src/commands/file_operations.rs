//! Deletion and OS integration commands.

use std::sync::Arc;

use tauri::State;

use crate::database::repository;
use crate::database::Database;
use crate::models::settings::AppSettings;
use crate::services::deletion_service::{
    DeletionPlan, DeletionReport, DeletionRequest, DeletionService,
};
use crate::utils::error::{AppError, AppResult};

#[tauri::command]
pub fn preview_deletion(
    db: State<'_, Arc<Database>>,
    request: DeletionRequest,
) -> AppResult<DeletionPlan> {
    let service = DeletionService::new(db.inner().clone());
    Ok(service.plan(&request))
}

#[tauri::command]
pub async fn delete_files(
    db: State<'_, Arc<Database>>,
    request: DeletionRequest,
) -> AppResult<DeletionReport> {
    let service = DeletionService::new(db.inner().clone());
    tokio::task::spawn_blocking(move || service.delete(&request))
        .await
        .map_err(|error| AppError::other(format!("task failed: {error}")))?
}

#[tauri::command]
pub fn open_in_explorer(path: String) -> AppResult<()> {
    let as_path = std::path::Path::new(&path);
    if !as_path.exists() {
        return Err(AppError::invalid(format!("path does not exist: {path}")));
    }
    // Open the file itself with its default application.
    open::that(as_path).map_err(|error| AppError::other(format!("could not open file: {error}")))
}

#[tauri::command]
pub fn reveal_in_folder(path: String) -> AppResult<()> {
    let as_path = std::path::Path::new(&path);
    if !as_path.exists() {
        return Err(AppError::invalid(format!("path does not exist: {path}")));
    }
    #[cfg(windows)]
    {
        std::process::Command::new("explorer")
            .arg(format!(
                "/select,{}",
                crate::utils::file_utils::display_path(as_path)
            ))
            .spawn()
            .map_err(|error| AppError::other(format!("could not open explorer: {error}")))?;
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let parent = as_path.parent().unwrap_or(as_path);
        open::that(parent)
            .map_err(|error| AppError::other(format!("could not open folder: {error}")))
    }
}

#[tauri::command]
pub fn get_settings(db: State<'_, Arc<Database>>) -> AppResult<AppSettings> {
    repository::load_settings(&db)
}

#[tauri::command]
pub fn save_settings(
    db: State<'_, Arc<Database>>,
    settings: AppSettings,
) -> AppResult<AppSettings> {
    repository::save_settings(&db, &settings)?;
    repository::load_settings(&db)
}

#[tauri::command]
pub fn clear_hash_cache(db: State<'_, Arc<Database>>) -> AppResult<u64> {
    repository::clear_hash_cache(&db)
}

#[tauri::command]
pub fn list_deletions(
    db: State<'_, Arc<Database>>,
    limit: Option<u32>,
) -> AppResult<Vec<repository::DeletionLogEntry>> {
    repository::list_deletions(&db, super::limits::deletions(limit))
}

#[tauri::command]
pub fn clear_deletion_history(db: State<'_, Arc<Database>>) -> AppResult<()> {
    repository::clear_deletions(&db)
}
