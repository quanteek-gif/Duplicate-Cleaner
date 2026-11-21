//! Dashboard statistics and drive information.

use std::sync::Arc;

use sysinfo::Disks;
use tauri::State;

use crate::database::repository;
use crate::database::Database;
use crate::models::disk::DriveInfo;
use crate::models::scan_result::DashboardStats;
use crate::utils::error::{AppError, AppResult};

#[tauri::command]
pub fn get_dashboard_stats(db: State<'_, Arc<Database>>) -> AppResult<DashboardStats> {
    repository::dashboard_stats(&db)
}

#[tauri::command]
pub async fn get_drives() -> AppResult<Vec<DriveInfo>> {
    tokio::task::spawn_blocking(|| {
        let disks = Disks::new_with_refreshed_list()
            .map_err(|error| AppError::other(format!("cannot enumerate drives: {error}")))?;
        let mut drives: Vec<DriveInfo> = disks
            .iter()
            .map(|disk| {
                let total = disk.total_space();
                let available = disk.available_space();
                DriveInfo {
                    name: disk.name().to_string_lossy().to_string(),
                    mount_point: disk.mount_point().to_string_lossy().to_string(),
                    file_system: disk.file_system().to_string_lossy().to_string(),
                    total_bytes: total,
                    available_bytes: available,
                    used_bytes: total.saturating_sub(available),
                    is_removable: disk.is_removable(),
                    is_read_only: disk.is_read_only(),
                }
            })
            .collect();
        drives.sort_by(|a, b| a.mount_point.cmp(&b.mount_point));
        Ok::<Vec<DriveInfo>, AppError>(drives)
    })
    .await
    .map_err(|error| AppError::other(format!("task failed: {error}")))?
}

/// Home directory and well-known folders proposed as quick scan targets.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuickFolder {
    pub label: String,
    pub path: String,
}

#[tauri::command]
pub fn get_quick_folders() -> Vec<QuickFolder> {
    let mut folders = Vec::new();
    let candidates: [(&str, Option<std::path::PathBuf>); 6] = [
        ("Downloads", dirs::download_dir()),
        ("Documents", dirs::document_dir()),
        ("Pictures", dirs::picture_dir()),
        ("Videos", dirs::video_dir()),
        ("Desktop", dirs::desktop_dir()),
        ("Music", dirs::audio_dir()),
    ];
    for (label, path) in candidates {
        if let Some(path) = path {
            if path.exists() {
                folders.push(QuickFolder {
                    label: label.to_string(),
                    path: path.to_string_lossy().to_string(),
                });
            }
        }
    }
    folders
}
