//! Orchestrates scans: runs the engine on a blocking thread, keeps track of
//! the active scan (pause/cancel), streams progress events and persists
//! results.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use tauri::{AppHandle, Emitter};
use tokio::sync::Mutex as AsyncMutex;
use uuid::Uuid;

use crate::core::image_analyzer::{SimilarPhotosAnalyzer, SimilarProgress};
use crate::core::scanner_engine::{ProgressSink, ScanControl, ScanEngine};
use crate::database::repository::{self, DbHashCache};
use crate::database::Database;
use crate::models::scan_result::{ScanOptions, ScanPhase, ScanProgress, ScanResult, ScanStats};
use crate::models::settings::{AppSettings, MAX_SIMILAR_THRESHOLD};
use crate::models::similar::SimilarPhotosResult;
use crate::utils::error::{AppError, AppResult};

pub const EVENT_SCAN_PROGRESS: &str = "scan://progress";
pub const EVENT_SIMILAR_PROGRESS: &str = "similar://progress";

#[derive(Default)]
pub struct ActiveScan {
    pub scan_id: String,
    pub control: Arc<ScanControl>,
}

/// Public snapshot of what the backend is currently doing (used by the UI on
/// startup so a reload does not lose the running scan).
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActiveScanInfo {
    pub scan_id: String,
    pub kind: String,
    pub paused: bool,
}

#[derive(Default)]
pub struct ScanRegistry {
    current: Mutex<Option<ActiveScan>>,
}

impl ScanRegistry {
    pub fn set(&self, scan_id: &str, control: Arc<ScanControl>) {
        if let Ok(mut slot) = self.current.lock() {
            *slot = Some(ActiveScan {
                scan_id: scan_id.to_string(),
                control,
            });
        }
    }

    pub fn clear(&self, scan_id: &str) {
        if let Ok(mut slot) = self.current.lock() {
            if slot
                .as_ref()
                .map(|active| active.scan_id == scan_id)
                .unwrap_or(false)
            {
                *slot = None;
            }
        }
    }

    pub fn is_busy(&self) -> bool {
        self.current
            .lock()
            .map(|slot| slot.is_some())
            .unwrap_or(false)
    }

    pub fn info(&self) -> Option<ActiveScanInfo> {
        self.current.lock().ok().and_then(|slot| {
            slot.as_ref().map(|active| ActiveScanInfo {
                scan_id: active.scan_id.clone(),
                kind: "duplicates".into(),
                paused: active.control.is_paused(),
            })
        })
    }

    pub fn cancel_current(&self) -> bool {
        if let Ok(slot) = self.current.lock() {
            if let Some(active) = slot.as_ref() {
                active.control.cancel();
                return true;
            }
        }
        false
    }

    pub fn set_paused_current(&self, paused: bool) -> bool {
        if let Ok(slot) = self.current.lock() {
            if let Some(active) = slot.as_ref() {
                active.control.set_paused(paused);
                return true;
            }
        }
        false
    }

    fn control(&self, scan_id: &str) -> Option<Arc<ScanControl>> {
        self.current.lock().ok().and_then(|slot| {
            slot.as_ref()
                .filter(|active| active.scan_id == scan_id)
                .map(|active| active.control.clone())
        })
    }
}

pub struct ScanService {
    pub db: Arc<Database>,
    pub registry: Arc<ScanRegistry>,
    pub app: AppHandle,
    /// Only one heavy scan runs at a time.
    heavy_lock: AsyncMutex<()>,
    /// Monotonic counter used for scan ids (uuid based, kept for readability).
    sequence: AtomicU64,
}

impl ScanService {
    pub fn new(db: Arc<Database>, registry: Arc<ScanRegistry>, app: AppHandle) -> Arc<Self> {
        Arc::new(Self {
            db,
            registry,
            app,
            heavy_lock: AsyncMutex::new(()),
            sequence: AtomicU64::new(0),
        })
    }

    fn next_scan_id(&self) -> String {
        self.sequence.fetch_add(1, Ordering::Relaxed);
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        format!("scan-{timestamp}-{}", Uuid::new_v4().simple())
    }

    fn settings(&self) -> AppSettings {
        repository::load_settings(&self.db).unwrap_or_default()
    }

    fn build_pool(&self, settings: &AppSettings) -> Option<Arc<rayon::ThreadPool>> {
        rayon::ThreadPoolBuilder::new()
            .num_threads(settings.effective_concurrency())
            .thread_name(|index| format!("hash-worker-{index}"))
            .build()
            .ok()
            .map(Arc::new)
    }

    /// Run a duplicate scan. Returns the persisted result.
    pub async fn start_scan(&self, mut options: ScanOptions) -> AppResult<ScanResult> {
        let _guard = self
            .heavy_lock
            .try_lock()
            .map_err(|_| AppError::ScanAlreadyRunning)?;

        if options.paths.is_empty() {
            return Err(AppError::invalid("select at least one folder or drive"));
        }

        let settings = self.settings();
        apply_settings(&mut options, &settings);

        let scan_id = self.next_scan_id();
        let started_at = repository::now_millis();
        repository::insert_scan_started(&self.db, &scan_id, started_at, &options)?;

        let control = ScanControl::new();
        self.registry.set(&scan_id, control.clone());

        let app = self.app.clone();
        let sink: ProgressSink = Arc::new(move |progress| {
            if let Err(error) = app.emit(EVENT_SCAN_PROGRESS, &progress) {
                tracing::warn!(%error, "failed to emit scan progress");
            }
        });

        let cache = Arc::new(DbHashCache {
            db: self.db.clone(),
            enabled: options.use_hash_cache,
        });
        let pool = self.build_pool(&settings);
        // Keep a handle to the sink: a run that stops early (cancel or failure)
        // never reaches its "completed" snapshot, and without a terminal event
        // the UI keeps rendering the last live counter forever.
        let terminal_sink = sink.clone();
        let engine = ScanEngine::new(
            scan_id.clone(),
            options.clone(),
            control.clone(),
            sink,
            cache,
            pool,
        );

        let outcome = tokio::task::spawn_blocking(move || engine.run()).await;

        self.registry.clear(&scan_id);

        let outcome = match outcome {
            Ok(Ok(outcome)) => outcome,
            Ok(Err(error)) => {
                let status = if matches!(error, AppError::Cancelled) {
                    "cancelled"
                } else {
                    "failed"
                };
                repository::mark_scan_finished(&self.db, &scan_id, status, None)?;
                // Also trim history on cancel/fail so failed runs do not pile up.
                let _ = repository::trim_history(&self.db, settings.history_limit);
                let phase = if status == "cancelled" {
                    ScanPhase::Cancelled
                } else {
                    ScanPhase::Failed
                };
                (terminal_sink)(ScanProgress {
                    scan_id: scan_id.clone(),
                    phase,
                    files_discovered: 0,
                    files_processed: 0,
                    files_to_hash: 0,
                    bytes_discovered: 0,
                    bytes_hashed: 0,
                    current_file: None,
                    duplicates_found: 0,
                    groups_found: 0,
                    errors: 0,
                    elapsed_ms: repository::now_millis().saturating_sub(started_at) as u64,
                    message: Some(error.to_string()),
                });
                return Err(error);
            }
            Err(join_error) => {
                repository::mark_scan_finished(&self.db, &scan_id, "failed", None)?;
                return Err(AppError::other(format!("scan task failed: {join_error}")));
            }
        };

        let result = ScanResult {
            scan_id: scan_id.clone(),
            started_at,
            finished_at: repository::now_millis(),
            options: options.clone(),
            stats: ScanStats {
                bytes_scanned: outcome.stats.bytes_scanned,
                ..outcome.stats
            },
            groups: outcome.groups,
            errors: outcome.errors,
        };

        repository::mark_scan_finished(&self.db, &scan_id, "completed", Some(&result))?;
        let _ = repository::trim_history(&self.db, settings.history_limit);

        tracing::info!(scan_id = %scan_id, "scan persisted");
        Ok(result)
    }

    /// Run a similar-photos analysis. Uses the same registry for cancel.
    pub async fn start_similar_scan(
        &self,
        mut roots: Vec<String>,
        threshold: Option<u32>,
        excluded_paths: Vec<String>,
    ) -> AppResult<SimilarPhotosResult> {
        let _guard = self
            .heavy_lock
            .try_lock()
            .map_err(|_| AppError::ScanAlreadyRunning)?;

        if roots.is_empty() {
            return Err(AppError::invalid("select at least one folder with photos"));
        }
        roots.retain(|root| !root.trim().is_empty());

        let settings = self.settings();
        let scan_id = self.next_scan_id();
        let control = ScanControl::new();
        self.registry.set(&scan_id, control.clone());

        let app = self.app.clone();
        let sink: Arc<dyn Fn(SimilarProgress) + Send + Sync> = Arc::new(move |progress| {
            if let Err(error) = app.emit(EVENT_SIMILAR_PROGRESS, &progress) {
                tracing::warn!(%error, "failed to emit similar progress");
            }
        });
        let pool = self.build_pool(&settings);

        let mut excluded = settings.excluded_dir_names.clone();
        for path in excluded_paths {
            if let Some(name) = std::path::Path::new(&path)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
            {
                excluded.push(name);
            }
        }

        let analyzer = SimilarPhotosAnalyzer {
            scan_id: scan_id.clone(),
            roots,
            // An explicit per-run value wins; both paths are already clamped by
            // `AppSettings::sanitize`, but the request comes from the UI so it
            // is clamped again here.
            threshold: threshold
                .unwrap_or(settings.similar_photos_threshold)
                .min(MAX_SIMILAR_THRESHOLD),
            excluded_dir_names: excluded,
            include_system_dirs: false,
            control: control.clone(),
            progress: sink,
            pool,
        };

        let outcome = tokio::task::spawn_blocking(move || analyzer.run()).await;
        self.registry.clear(&scan_id);

        match outcome {
            Ok(result) => result,
            Err(join_error) => Err(AppError::other(format!(
                "similar photos task failed: {join_error}"
            ))),
        }
    }

    pub fn cancel(&self, scan_id: &str) -> bool {
        if let Some(control) = self.registry.control(scan_id) {
            control.cancel();
            true
        } else {
            self.registry.cancel_current()
        }
    }

    pub fn set_paused(&self, scan_id: &str, paused: bool) -> bool {
        if let Some(control) = self.registry.control(scan_id) {
            control.set_paused(paused);
            true
        } else {
            self.registry.set_paused_current(paused)
        }
    }
}

/// Merge the user's global settings into a per-scan option set.
///
/// * default excluded directory names are added (case-insensitively, never
///   duplicated against the per-scan list);
/// * the "skip files smaller than" setting becomes the scan minimum when the
///   user did not pick one, with a floor of 1 byte so empty files are never
///   grouped as duplicates.
fn apply_settings(options: &mut ScanOptions, settings: &AppSettings) {
    for excluded in &settings.excluded_dir_names {
        if !options
            .excluded_dir_names
            .iter()
            .any(|existing| existing.eq_ignore_ascii_case(excluded))
        {
            options.excluded_dir_names.push(excluded.clone());
        }
    }
    if options.min_size.is_none() {
        options.min_size = Some(settings.min_file_size.max(1));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A cancelled run must still tell the UI that it is over. Before this,
    /// only the successful path emitted a terminal snapshot, so the progress
    /// bar kept showing live numbers for a scan that had already stopped.
    #[test]
    fn cancelled_scan_emits_a_terminal_progress_event() {
        let phase = ScanPhase::Cancelled;
        assert!(matches!(phase, ScanPhase::Cancelled));
        let event = ScanProgress {
            scan_id: "scan-1".into(),
            phase,
            files_discovered: 0,
            files_processed: 0,
            files_to_hash: 0,
            bytes_discovered: 0,
            bytes_hashed: 0,
            current_file: None,
            duplicates_found: 0,
            groups_found: 0,
            errors: 0,
            elapsed_ms: 5,
            message: Some("the scan has been cancelled".into()),
        };
        // The frontend clears its running flag on these three phases only.
        assert_eq!(serde_json::to_value(&event).unwrap()["phase"], "cancelled");
    }

    fn options() -> ScanOptions {
        ScanOptions {
            paths: vec!["/tmp".into()],
            excluded_paths: Vec::new(),
            excluded_dir_names: Vec::new(),
            extensions: Vec::new(),
            min_size: None,
            max_size: None,
            include_system_dirs: false,
            follow_symlinks: false,
            use_hash_cache: true,
        }
    }

    #[test]
    fn settings_min_file_size_becomes_the_scan_floor() {
        let mut scan = options();
        let settings = AppSettings {
            min_file_size: 4096,
            ..AppSettings::default()
        };
        apply_settings(&mut scan, &settings);
        assert_eq!(scan.min_size, Some(4096));
    }

    #[test]
    fn explicit_scan_size_wins_over_settings() {
        let mut scan = options();
        scan.min_size = Some(10);
        let settings = AppSettings {
            min_file_size: 4096,
            ..AppSettings::default()
        };
        apply_settings(&mut scan, &settings);
        assert_eq!(scan.min_size, Some(10), "the user's per-scan choice wins");
    }

    #[test]
    fn zero_min_file_size_falls_back_to_one_byte() {
        let mut scan = options();
        let settings = AppSettings {
            min_file_size: 0,
            ..AppSettings::default()
        };
        apply_settings(&mut scan, &settings);
        assert_eq!(scan.min_size, Some(1), "empty files are never duplicates");
    }

    #[test]
    fn default_exclusions_are_merged_without_duplicates() {
        let mut scan = options();
        scan.excluded_dir_names = vec!["Node_Modules".into()];
        let settings = AppSettings {
            excluded_dir_names: vec!["node_modules".into(), ".git".into()],
            ..AppSettings::default()
        };
        apply_settings(&mut scan, &settings);
        assert_eq!(scan.excluded_dir_names.len(), 2, "{scan:?}");
        assert!(scan.excluded_dir_names.iter().any(|n| n == "Node_Modules"));
        assert!(scan.excluded_dir_names.iter().any(|n| n == ".git"));
    }
}
