//! Duplicate results, similar photos, previews and recent scans.

use std::sync::Arc;

use tauri::State;

use crate::core::{disk_analyzer, hasher};
use crate::database::repository;
use crate::database::Database;
use crate::models::disk::DiskMapResult;
use crate::models::scan_result::{ScanRecord, ScanResult};
use crate::models::similar::SimilarPhotosResult;
use crate::services::scan_service::ScanService;
use crate::utils::error::{AppError, AppResult};

#[tauri::command]
pub fn get_scan_result(db: State<'_, Arc<Database>>, scan_id: String) -> AppResult<ScanResult> {
    repository::load_scan_result(&db, &scan_id)
}

#[tauri::command]
pub fn list_recent_scans(
    db: State<'_, Arc<Database>>,
    limit: Option<u32>,
) -> AppResult<Vec<ScanRecord>> {
    repository::list_scans(&db, super::limits::recent_scans(limit))
}

#[tauri::command]
pub fn delete_scan(db: State<'_, Arc<Database>>, scan_id: String) -> AppResult<()> {
    repository::delete_scan(&db, &scan_id)
}

#[tauri::command]
pub async fn find_similar_photos(
    service: State<'_, Arc<ScanService>>,
    paths: Vec<String>,
    threshold: Option<u32>,
    excluded_paths: Option<Vec<String>>,
) -> AppResult<SimilarPhotosResult> {
    service
        .inner()
        .start_similar_scan(paths, threshold, excluded_paths.unwrap_or_default())
        .await
}

#[tauri::command]
pub async fn analyze_disk_usage(root: String) -> AppResult<DiskMapResult> {
    let control = crate::core::scanner_engine::ScanControl::new();
    tokio::task::spawn_blocking(move || disk_analyzer::analyze_disk(&root, &control))
        .await
        .map_err(|error| AppError::other(format!("task failed: {error}")))?
}

/// Base64 JPEG preview of an image file (used by Similar Photos detail view).
#[tauri::command]
pub async fn get_image_preview(path: String, max_dim: Option<u32>) -> AppResult<String> {
    let dim = super::limits::preview_dim(max_dim);
    tokio::task::spawn_blocking(move || {
        hasher::thumbnail_data_uri(std::path::Path::new(&path), dim)
    })
    .await
    .map_err(|error| AppError::other(format!("task failed: {error}")))?
}
