//! Scan lifecycle commands.

use std::sync::Arc;

use tauri::State;

use crate::core::scanner_engine::scan_large_files;
use crate::models::scan_result::{LargeFilesResult, ScanOptions, ScanResult};
use crate::services::scan_service::{ActiveScanInfo, ScanService};
use crate::utils::error::AppResult;

#[tauri::command]
pub async fn start_scan(
    service: State<'_, Arc<ScanService>>,
    options: ScanOptions,
) -> AppResult<ScanResult> {
    service.inner().start_scan(options).await
}

#[tauri::command]
pub fn cancel_scan(service: State<'_, Arc<ScanService>>, scan_id: String) -> bool {
    service.inner().cancel(&scan_id)
}

#[tauri::command]
pub fn pause_scan(service: State<'_, Arc<ScanService>>, scan_id: String) -> bool {
    service.inner().set_paused(&scan_id, true)
}

#[tauri::command]
pub fn resume_scan(service: State<'_, Arc<ScanService>>, scan_id: String) -> bool {
    service.inner().set_paused(&scan_id, false)
}

#[tauri::command]
pub fn get_active_scan(service: State<'_, Arc<ScanService>>) -> Option<ActiveScanInfo> {
    service.registry.info()
}

/// Large-files scan runs on a blocking thread so the UI stays responsive.
#[tauri::command]
pub async fn scan_large_files_cmd(
    options: ScanOptions,
    limit: Option<usize>,
) -> AppResult<LargeFilesResult> {
    let control = crate::core::scanner_engine::ScanControl::new();
    let limit = super::limits::large_files(limit);
    tokio::task::spawn_blocking(move || {
        scan_large_files(
            &options.paths,
            &options.excluded_dir_names,
            options.include_system_dirs,
            limit,
            &control,
        )
    })
    .await
    .map_err(|error| crate::utils::error::AppError::other(format!("task failed: {error}")))?
}

/// Estimate how many files live in the given roots using a cheap traversal
/// (no hashing). Used by the UI to show the scan scope before starting.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderEstimate {
    pub files: u64,
    pub bytes: u64,
}

#[tauri::command]
pub async fn estimate_folder(paths: Vec<String>) -> AppResult<FolderEstimate> {
    tokio::task::spawn_blocking(move || {
        let mut files = 0u64;
        let mut bytes = 0u64;
        for root in paths {
            for entry in walkdir::WalkDir::new(&root)
                .follow_links(false)
                .into_iter()
                .filter_map(Result::ok)
            {
                if entry.file_type().is_file() {
                    if let Ok(metadata) = entry.metadata() {
                        files += 1;
                        bytes += metadata.len();
                        if files >= 5_000_000 {
                            return Ok(FolderEstimate { files, bytes });
                        }
                    }
                }
            }
        }
        Ok(FolderEstimate { files, bytes })
    })
    .await
    .map_err(|error| crate::utils::error::AppError::other(format!("task failed: {error}")))?
}
