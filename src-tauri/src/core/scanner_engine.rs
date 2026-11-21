//! The duplicate scanning engine.
//!
//! Pipeline (never loads whole files into memory):
//! 1. **Discover** — recursive traversal, filtering, collecting `(path, size)`.
//! 2. **Size groups** — only files sharing a size can be duplicates.
//! 3. **Quick hash** — the first 64 KiB of every size-collision candidate.
//! 4. **Full SHA-256** — streamed in 1 MiB blocks with a bounded worker pool.
//! 5. **Grouping** — files with the same digest form duplicate groups.
//!
//! The engine is synchronous; the service layer runs it on a blocking thread
//! and forwards progress to the UI. Cancellation and pause are cooperative.

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rayon::prelude::*;
use walkdir::WalkDir;

use crate::core::hasher::{self, QuickHash};
use crate::models::disk::category_for_extension;
use crate::models::duplicate_group::DuplicateGroup;
use crate::models::file::FileEntry;
use crate::models::scan_result::{
    CategoryTotal, LargeFile, LargeFilesResult, ScanErrorEntry, ScanOptions, ScanPhase,
    ScanProgress, ScanStats,
};
use crate::utils::error::{AppError, AppResult};
use crate::utils::file_utils;

/// Directories that are never scanned unless the user explicitly opts in.
const SYSTEM_DIR_NAMES: &[&str] = &[
    "$recycle.bin",
    "system volume information",
    "recovery",
    "windows",
    "winsxs",
    "program files",
    "program files (x86)",
    "programdata",
    "boot",
    "$windows.~bt",
    "$windows.~ws",
    "config.msi",
    "msocache",
    "perflogs",
];

/// `true` when a directory name belongs to a protected system location.
pub fn is_system_dir_name(name: &str) -> bool {
    SYSTEM_DIR_NAMES.contains(&name.to_lowercase().as_str())
}

/// `true` when the path *is* one of the top-level system folders on any volume
/// (`C:\Windows`, `D:\Program Files`, `\\nas\share\Windows`, …).
///
/// Used to reject a scan root that would otherwise open the whole system tree
/// without the explicit "include system directories" opt-in.
pub fn is_system_location(path: &Path) -> bool {
    let normalized = crate::utils::file_utils::normalize_for_security(&path.to_string_lossy());
    let body = crate::utils::file_utils::rooted_segments(&normalized);
    body.len() == 1 && is_system_dir_name(body[0])
}

/// Maximum number of individual errors reported to the UI (counts stay exact).
const MAX_REPORTED_ERRORS: usize = 500;

/// Emit progress at most every 150 ms to keep the IPC channel calm.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(150);

/// Cooperative scan control: cancellation and pause.
#[derive(Debug, Default)]
pub struct ScanControl {
    cancelled: AtomicBool,
    paused: AtomicBool,
}

impl ScanControl {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        self.paused.store(false, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    pub fn set_paused(&self, paused: bool) {
        self.paused.store(paused, Ordering::SeqCst);
    }

    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::SeqCst)
    }

    /// Block while paused; returns `Err(Cancelled)` if the scan was cancelled
    /// while waiting.
    pub fn checkpoint(&self) -> AppResult<()> {
        while self.is_paused() {
            if self.is_cancelled() {
                return Err(AppError::Cancelled);
            }
            std::thread::sleep(Duration::from_millis(40));
        }
        if self.is_cancelled() {
            Err(AppError::Cancelled)
        } else {
            Ok(())
        }
    }
}

/// Cross-scan hash cache (backed by SQLite in the service layer).
pub trait HashCacheProvider: Send + Sync {
    fn lookup(&self, path: &str, size: u64, modified_ms: Option<i64>) -> Option<String>;
    fn store(&self, path: &str, size: u64, modified_ms: Option<i64>, sha256: &str);
}

/// Adapter that disables the cache.
pub struct NoopHashCache;
impl HashCacheProvider for NoopHashCache {
    fn lookup(&self, _path: &str, _size: u64, _modified_ms: Option<i64>) -> Option<String> {
        None
    }
    fn store(&self, _path: &str, _size: u64, _modified_ms: Option<i64>, _sha256: &str) {}
}

pub type ProgressSink = Arc<dyn Fn(ScanProgress) + Send + Sync>;

/// `(size, 64 KiB fingerprint)` bucket key.
type SizeFingerprint = (u64, u64);
/// Buckets of candidate indexes sharing the same size and fingerprint.
type QuickGroups = Vec<(SizeFingerprint, Vec<usize>)>;

/// Final output of an engine run.
pub struct EngineOutcome {
    pub stats: ScanStats,
    pub groups: Vec<DuplicateGroup>,
    pub errors: Vec<ScanErrorEntry>,
}

/// Normalised path used for exclusion comparisons.
fn normalize_for_compare(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/").to_lowercase();
    if text.len() > 1 {
        text.trim_end_matches('/').to_string()
    } else {
        text
    }
}

// ---------------------------------------------------------------------------
// Shared run state (progress + counters), thread-safe
// ---------------------------------------------------------------------------

struct RunState {
    scan_id: String,
    started: Instant,
    sink: ProgressSink,
    last_emit: Mutex<Instant>,
    processed: AtomicU64,
    files_to_hash: AtomicU64,
    bytes_hashed: AtomicU64,
    files_hashed: AtomicU64,
    errors: AtomicU64,
    current: Mutex<Option<String>>,
    error_log: Mutex<Vec<ScanErrorEntry>>,
    discovered_files: AtomicU64,
    discovered_bytes: AtomicU64,
    groups_found: AtomicU64,
    duplicates_found: AtomicU64,
}

impl RunState {
    fn new(scan_id: String, sink: ProgressSink) -> Self {
        Self {
            scan_id,
            started: Instant::now(),
            sink,
            last_emit: Mutex::new(Instant::now() - PROGRESS_INTERVAL),
            processed: AtomicU64::new(0),
            files_to_hash: AtomicU64::new(0),
            bytes_hashed: AtomicU64::new(0),
            files_hashed: AtomicU64::new(0),
            errors: AtomicU64::new(0),
            current: Mutex::new(None),
            error_log: Mutex::new(Vec::new()),
            discovered_files: AtomicU64::new(0),
            discovered_bytes: AtomicU64::new(0),
            groups_found: AtomicU64::new(0),
            duplicates_found: AtomicU64::new(0),
        }
    }

    fn snapshot(&self, phase: ScanPhase, message: Option<String>) -> ScanProgress {
        ScanProgress {
            scan_id: self.scan_id.clone(),
            phase,
            files_discovered: self.discovered_files.load(Ordering::Relaxed),
            files_processed: self.processed.load(Ordering::Relaxed),
            files_to_hash: self.files_to_hash.load(Ordering::Relaxed),
            bytes_discovered: self.discovered_bytes.load(Ordering::Relaxed),
            bytes_hashed: self.bytes_hashed.load(Ordering::Relaxed),
            current_file: self.current.lock().ok().and_then(|slot| slot.clone()),
            duplicates_found: self.duplicates_found.load(Ordering::Relaxed),
            groups_found: self.groups_found.load(Ordering::Relaxed),
            errors: self.errors.load(Ordering::Relaxed),
            elapsed_ms: self.started.elapsed().as_millis() as u64,
            message,
        }
    }

    /// Emit unconditionally (used for phase transitions and the final report).
    fn emit(&self, phase: ScanPhase, message: Option<String>) {
        if let Ok(mut last) = self.last_emit.lock() {
            *last = Instant::now();
        }
        (self.sink)(self.snapshot(phase, message));
    }

    /// Emit at most once per [`PROGRESS_INTERVAL`].
    fn maybe_emit(&self, phase: ScanPhase) {
        let ready = self
            .last_emit
            .lock()
            .map(|mut last| {
                if last.elapsed() >= PROGRESS_INTERVAL {
                    *last = Instant::now();
                    true
                } else {
                    false
                }
            })
            .unwrap_or(true);
        if ready {
            (self.sink)(self.snapshot(phase, None));
        }
    }

    fn set_current(&self, path: &Path) {
        if let Ok(mut slot) = self.current.lock() {
            *slot = path
                .file_name()
                .map(|name| name.to_string_lossy().to_string());
        }
    }

    fn record_error(&self, path: &Path, message: String) {
        self.errors.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut entries) = self.error_log.lock() {
            if entries.len() < MAX_REPORTED_ERRORS {
                entries.push(ScanErrorEntry {
                    path: file_utils::display_path(path),
                    message,
                });
            }
        }
    }

    fn take_error_log(&self) -> Vec<ScanErrorEntry> {
        self.error_log
            .lock()
            .map(|entries| entries.clone())
            .unwrap_or_default()
    }
}

// ---------------------------------------------------------------------------
// Engine
// ---------------------------------------------------------------------------

pub struct ScanEngine {
    pub scan_id: String,
    pub options: ScanOptions,
    pub control: Arc<ScanControl>,
    progress_sink: ProgressSink,
    hash_cache: Arc<dyn HashCacheProvider>,
    pool: Option<Arc<rayon::ThreadPool>>,
}

impl ScanEngine {
    pub fn new(
        scan_id: impl Into<String>,
        options: ScanOptions,
        control: Arc<ScanControl>,
        progress_sink: ProgressSink,
        hash_cache: Arc<dyn HashCacheProvider>,
        pool: Option<Arc<rayon::ThreadPool>>,
    ) -> Self {
        Self {
            scan_id: scan_id.into(),
            options,
            control,
            progress_sink,
            hash_cache,
            pool,
        }
    }

    fn install_pool<R: Send>(&self, f: impl FnOnce() -> R + Send) -> R {
        match &self.pool {
            Some(pool) => pool.install(f),
            None => f(),
        }
    }

    fn worker_count(&self) -> usize {
        self.pool
            .as_ref()
            .map(|pool| pool.current_num_threads())
            .unwrap_or_else(|| {
                std::thread::available_parallelism()
                    .map(|n| n.get())
                    .unwrap_or(4)
            })
    }

    /// Number of candidates submitted to rayon at once. Bounds the number of
    /// simultaneously open files and keeps pause/cancel responsive.
    fn chunk_size(&self) -> usize {
        (self.worker_count() * 8).max(16)
    }

    /// Run the full pipeline. Returns [`AppError::Cancelled`] when the user
    /// cancels (partial results are discarded by the service layer).
    pub fn run(&self) -> AppResult<EngineOutcome> {
        self.validate_options()?;
        let state = RunState::new(self.scan_id.clone(), self.progress_sink.clone());
        let started = Instant::now();

        // --- Phase 1: discovery -------------------------------------
        let files = self.discover(&state)?;
        let mut stats = ScanStats::empty();
        stats.files_scanned = files.len() as u64;
        stats.bytes_scanned = files.iter().map(|f| f.size).sum();
        state
            .discovered_files
            .store(files.len() as u64, Ordering::Relaxed);
        state
            .discovered_bytes
            .store(stats.bytes_scanned, Ordering::Relaxed);

        // --- Phase 2: size buckets ----------------------------------
        let mut by_size: HashMap<u64, Vec<usize>> = HashMap::new();
        for (index, file) in files.iter().enumerate() {
            if file.size > 0 {
                by_size.entry(file.size).or_default().push(index);
            }
        }
        let candidate_indexes: Vec<usize> = by_size
            .values()
            .filter(|indexes| indexes.len() > 1)
            .flat_map(|indexes| indexes.iter().copied())
            .collect();

        state.emit(ScanPhase::Sizing, None);

        if candidate_indexes.is_empty() {
            let mut done = state.snapshot(
                ScanPhase::Completed,
                Some("No size collisions found".into()),
            );
            done.files_discovered = files.len() as u64;
            (self.progress_sink)(done);
            return Ok(EngineOutcome {
                stats,
                groups: Vec::new(),
                errors: Vec::new(),
            });
        }

        state
            .files_to_hash
            .store(candidate_indexes.len() as u64, Ordering::Relaxed);

        // --- Phase 3: quick fingerprint filter ----------------------
        let (quick_groups, precomputed) =
            self.fingerprint_groups(&files, &candidate_indexes, &state)?;

        // --- Phase 4: full SHA-256 ----------------------------------
        let hashed = self.hash_candidates(&files, quick_groups, precomputed, &state)?;

        // --- Phase 5: grouping --------------------------------------
        let mut groups: Vec<DuplicateGroup> = hashed
            .into_iter()
            .filter(|(_, members)| members.len() > 1)
            .map(|(hash, members)| {
                let entries: Vec<FileEntry> = members
                    .into_iter()
                    .map(|index| files[index].clone())
                    .collect();
                DuplicateGroup::new(hash, entries)
            })
            .collect();
        groups.sort_by_key(|group| std::cmp::Reverse(group.wasted_bytes));

        stats.files_hashed = state.files_hashed.load(Ordering::Relaxed);
        stats.duplicate_groups = groups.len() as u64;
        stats.duplicate_files = groups.iter().map(|g| g.count() as u64).sum();
        stats.wasted_bytes = groups.iter().map(|g| g.wasted_bytes).sum();
        stats.errors = state.errors.load(Ordering::Relaxed);

        state
            .groups_found
            .store(stats.duplicate_groups, Ordering::Relaxed);
        state
            .duplicates_found
            .store(stats.duplicate_files, Ordering::Relaxed);

        let message = if groups.is_empty() {
            "No duplicates were found".to_string()
        } else {
            format!("{} duplicate groups found", stats.duplicate_groups)
        };
        state.emit(ScanPhase::Completed, Some(message));

        tracing::info!(
            elapsed_ms = started.elapsed().as_millis() as u64,
            files = stats.files_scanned,
            hashed = stats.files_hashed,
            groups = stats.duplicate_groups,
            wasted = stats.wasted_bytes,
            errors = stats.errors,
            "scan finished"
        );

        Ok(EngineOutcome {
            stats,
            groups,
            errors: state.take_error_log(),
        })
    }

    // -----------------------------------------------------------------
    // Discovery
    // -----------------------------------------------------------------

    fn validate_options(&self) -> AppResult<()> {
        if self.options.paths.is_empty() {
            return Err(AppError::invalid(
                "select at least one folder or drive to scan",
            ));
        }
        for root in &self.options.paths {
            if !Path::new(root).exists() {
                return Err(AppError::invalid(format!("path does not exist: {root}")));
            }
        }
        if let (Some(min), Some(max)) = (self.options.min_size, self.options.max_size) {
            if min > max {
                return Err(AppError::invalid(
                    "minimum size is greater than maximum size",
                ));
            }
        }
        for root in &self.options.paths {
            let root_path = Path::new(root);
            let last = root_path
                .file_name()
                .map(|n| n.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            // A bare drive root (`D:\`) has no file name, so the name check is
            // not enough — the system-folder check must run on any volume.
            let is_system_root = is_system_dir_name(&last) || is_system_location(root_path);
            if is_system_root && !self.options.include_system_dirs {
                return Err(AppError::AccessDenied(format!(
                    "scanning system directory '{root}' requires explicit permission — enable \"Include system directories\""
                )));
            }
        }
        Ok(())
    }

    fn should_skip_dir(&self, path: &Path, name: &str) -> bool {
        let lower = name.to_lowercase();

        if is_system_dir_name(&lower) && !self.options.include_system_dirs {
            return true;
        }
        if self
            .options
            .excluded_dir_names
            .iter()
            .any(|excluded| excluded.trim().to_lowercase() == lower)
        {
            return true;
        }
        if !self.options.excluded_paths.is_empty() {
            let normalized = normalize_for_compare(path);
            for excluded in &self.options.excluded_paths {
                if excluded.trim().is_empty() {
                    continue;
                }
                let excluded_norm = normalize_for_compare(Path::new(excluded));
                if normalized == excluded_norm
                    || normalized.starts_with(&format!("{excluded_norm}/"))
                {
                    return true;
                }
            }
        }
        false
    }

    fn matches_filters(&self, size: u64, extension: &str) -> bool {
        if let Some(min) = self.options.min_size {
            if size < min {
                return false;
            }
        }
        if let Some(max) = self.options.max_size {
            if size > max {
                return false;
            }
        }
        if !self.options.extensions.is_empty() {
            let wanted = self
                .options
                .extensions
                .iter()
                .any(|ext| ext.trim_start_matches('.').eq_ignore_ascii_case(extension));
            if !wanted {
                return false;
            }
        }
        true
    }

    fn discover(&self, state: &RunState) -> AppResult<Vec<FileEntry>> {
        let mut files: Vec<FileEntry> = Vec::new();
        let mut bytes_discovered: u64 = 0;

        for root in &self.options.paths {
            state.emit(ScanPhase::Discovering, Some(format!("Scanning {root}")));
            tracing::info!(root, "discovering files");

            let walker = WalkDir::new(root)
                .follow_links(self.options.follow_symlinks)
                .into_iter()
                .filter_entry(|entry| {
                    if entry.file_type().is_dir() {
                        let name = entry.file_name().to_string_lossy();
                        !self.should_skip_dir(entry.path(), &name)
                    } else {
                        true
                    }
                });

            for entry_result in walker {
                self.control.checkpoint()?;

                let entry = match entry_result {
                    Ok(entry) => entry,
                    Err(error) => {
                        let path = error.path().map(Path::to_path_buf).unwrap_or_default();
                        state.record_error(&path, error.to_string());
                        continue;
                    }
                };

                if entry.depth() == 0 {
                    continue;
                }
                let file_type = entry.file_type();
                if file_type.is_symlink() && !self.options.follow_symlinks {
                    continue;
                }
                if !file_type.is_file() {
                    continue;
                }

                let metadata = match entry.metadata() {
                    Ok(metadata) => metadata,
                    Err(error) => {
                        state.record_error(entry.path(), error.to_string());
                        continue;
                    }
                };

                let extension = file_utils::extension_of(entry.path());
                if !self.matches_filters(metadata.len(), &extension) {
                    continue;
                }

                let file = FileEntry::from_metadata(entry.path(), &metadata);
                bytes_discovered = bytes_discovered.saturating_add(file.size);
                files.push(file);

                state
                    .discovered_files
                    .store(files.len() as u64, Ordering::Relaxed);
                state
                    .discovered_bytes
                    .store(bytes_discovered, Ordering::Relaxed);
                state.set_current(entry.path());
                state.maybe_emit(ScanPhase::Discovering);
            }
        }

        state.emit(ScanPhase::Discovering, None);
        tracing::info!(
            files = files.len(),
            errors = state.errors.load(Ordering::Relaxed),
            bytes = bytes_discovered,
            "discovery finished"
        );
        Ok(files)
    }

    // -----------------------------------------------------------------
    // Hashing phases
    // -----------------------------------------------------------------

    /// First pass: 64 KiB fingerprints. Returns `(size, fingerprint)` buckets
    /// that contain at least two files, plus digests already computed for
    /// files smaller than the quick-hash window (never re-read).
    fn fingerprint_groups(
        &self,
        files: &[FileEntry],
        candidate_indexes: &[usize],
        state: &RunState,
    ) -> AppResult<(QuickGroups, HashMap<usize, String>)> {
        let mut buckets: HashMap<SizeFingerprint, Vec<usize>> = HashMap::new();
        let mut precomputed: HashMap<usize, String> = HashMap::new();
        let chunk_size = self.chunk_size();

        for chunk in candidate_indexes.chunks(chunk_size) {
            self.control.checkpoint()?;

            let partial: Vec<(SizeFingerprint, usize, Option<String>)> = self.install_pool(|| {
                chunk
                    .par_iter()
                    .map(|index| {
                        let file = &files[*index];
                        state.set_current(Path::new(&file.path));
                        match hasher::quick_hash(Path::new(&file.path)) {
                            Ok(QuickHash {
                                fingerprint,
                                full_sha256,
                            }) => {
                                if let Some(digest) = &full_sha256 {
                                    // The whole file fit into the quick window:
                                    // remember the digest, never read it again.
                                    state.files_hashed.fetch_add(1, Ordering::Relaxed);
                                    state.bytes_hashed.fetch_add(file.size, Ordering::Relaxed);
                                    self.hash_cache.store(
                                        &file.path,
                                        file.size,
                                        file.modified_ms,
                                        digest,
                                    );
                                }
                                ((file.size, fingerprint), *index, full_sha256)
                            }
                            Err(error) => {
                                state.record_error(Path::new(&file.path), error.to_string());
                                ((file.size, 0), usize::MAX, None)
                            }
                        }
                    })
                    .collect()
            });

            for (key, index, digest) in partial {
                if index == usize::MAX {
                    continue;
                }
                buckets.entry(key).or_default().push(index);
                if let Some(digest) = digest {
                    precomputed.insert(index, digest);
                }
            }

            state
                .processed
                .fetch_add(chunk.len() as u64, Ordering::Relaxed);
            state.maybe_emit(ScanPhase::Hashing);
        }

        Ok((
            buckets
                .into_iter()
                .filter(|(_, members)| members.len() > 1)
                .collect(),
            precomputed,
        ))
    }

    /// Final pass: full streaming SHA-256 for fingerprint survivors, with the
    /// cross-scan cache consulted first.
    fn hash_candidates(
        &self,
        files: &[FileEntry],
        quick_groups: QuickGroups,
        precomputed: HashMap<usize, String>,
        state: &RunState,
    ) -> AppResult<HashMap<String, Vec<usize>>> {
        let total: u64 = quick_groups.iter().map(|(_, m)| m.len() as u64).sum();
        state.files_to_hash.store(total, Ordering::Relaxed);
        state.processed.store(0, Ordering::Relaxed);

        let mut hashes: HashMap<String, Vec<usize>> = HashMap::new();
        let chunk_size = self.chunk_size();

        for (_, members) in quick_groups {
            for chunk in members.chunks(chunk_size) {
                self.control.checkpoint()?;

                let partial: Vec<(usize, Option<String>)> = self.install_pool(|| {
                    chunk
                        .par_iter()
                        .map(|index| {
                            let file = &files[*index];
                            state.set_current(Path::new(&file.path));

                            // 0. Digest already computed during fingerprinting.
                            if let Some(digest) = precomputed.get(index) {
                                return (*index, Some(digest.clone()));
                            }

                            // 1. Cross-scan cache: same path, size and mtime.
                            if self.options.use_hash_cache {
                                if let Some(cached) =
                                    self.hash_cache
                                        .lookup(&file.path, file.size, file.modified_ms)
                                {
                                    return (*index, Some(cached));
                                }
                            }

                            // 2. Full streaming hash.
                            match hasher::hash_file(Path::new(&file.path)) {
                                Ok((digest, read)) => {
                                    state.files_hashed.fetch_add(1, Ordering::Relaxed);
                                    state.bytes_hashed.fetch_add(read, Ordering::Relaxed);
                                    self.hash_cache.store(
                                        &file.path,
                                        file.size,
                                        file.modified_ms,
                                        &digest,
                                    );
                                    (*index, Some(digest))
                                }
                                Err(error) => {
                                    state.record_error(Path::new(&file.path), error.to_string());
                                    (*index, None)
                                }
                            }
                        })
                        .collect()
                });

                for (index, digest) in partial {
                    if let Some(digest) = digest {
                        hashes.entry(digest).or_default().push(index);
                    }
                }

                state
                    .processed
                    .fetch_add(chunk.len() as u64, Ordering::Relaxed);
                state.maybe_emit(ScanPhase::Hashing);
            }
        }
        Ok(hashes)
    }
}

// ---------------------------------------------------------------------------
// Large files scan
// ---------------------------------------------------------------------------

/// Find the largest files under the given roots (top `limit`, default 100).
pub fn scan_large_files(
    roots: &[String],
    excluded_names: &[String],
    include_system_dirs: bool,
    limit: usize,
    control: &ScanControl,
) -> AppResult<LargeFilesResult> {
    let started = Instant::now();
    if roots.is_empty() {
        return Err(AppError::invalid("select at least one folder to scan"));
    }

    let mut top: Vec<LargeFile> = Vec::new();
    let mut scanned: u64 = 0;
    let mut errors: u64 = 0;
    let mut totals: HashMap<&'static str, (u64, u64)> = HashMap::new();

    for root in roots {
        if !Path::new(root).exists() {
            return Err(AppError::invalid(format!("path does not exist: {root}")));
        }
        let walker = WalkDir::new(root)
            .follow_links(false)
            .into_iter()
            .filter_entry(|entry| {
                if entry.file_type().is_dir() {
                    let name = entry.file_name().to_string_lossy().to_lowercase();
                    if is_system_dir_name(&name) && !include_system_dirs {
                        return false;
                    }
                    return !excluded_names
                        .iter()
                        .any(|excluded| excluded.trim().to_lowercase() == name);
                }
                true
            });

        for result in walker {
            control.checkpoint()?;
            let entry = match result {
                Ok(entry) => entry,
                Err(_) => {
                    errors += 1;
                    continue;
                }
            };
            let file_type = entry.file_type();
            if !file_type.is_file() || file_type.is_symlink() {
                continue;
            }
            let metadata = match entry.metadata() {
                Ok(metadata) => metadata,
                Err(_) => {
                    errors += 1;
                    continue;
                }
            };
            scanned += 1;
            let extension = file_utils::extension_of(entry.path());
            let category = category_for_extension(&extension);
            let bucket = totals.entry(category).or_insert((0, 0));
            bucket.0 += metadata.len();
            bucket.1 += 1;

            let size = metadata.len();
            if top.len() >= limit {
                if let Some(smallest) = top.last() {
                    if size <= smallest.size {
                        continue;
                    }
                }
            }

            let file = LargeFile {
                path: file_utils::display_path(entry.path()),
                name: entry.file_name().to_string_lossy().to_string(),
                size,
                extension,
                modified_ms: file_utils::system_time_to_millis(metadata.modified().ok()),
                category: category.to_string(),
            };
            top.push(file);
            top.sort_by_key(|file| std::cmp::Reverse(file.size));
            top.truncate(limit);
        }
    }

    let mut category_totals: Vec<CategoryTotal> = totals
        .into_iter()
        .map(|(category, (bytes, count))| CategoryTotal {
            category: category.to_string(),
            bytes,
            count,
        })
        .collect();
    category_totals.sort_by_key(|total| std::cmp::Reverse(total.bytes));

    Ok(LargeFilesResult {
        scanned_files: scanned,
        files: top,
        category_totals,
        elapsed_ms: started.elapsed().as_millis() as u64,
        errors,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::sync::atomic::AtomicUsize;

    fn write(path: &Path, content: &[u8]) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        let mut file = std::fs::File::create(path).unwrap();
        file.write_all(content).unwrap();
    }

    fn test_options(root: &Path) -> ScanOptions {
        ScanOptions {
            paths: vec![root.to_string_lossy().to_string()],
            excluded_paths: vec![],
            excluded_dir_names: vec!["node_modules".into()],
            extensions: vec![],
            min_size: Some(1),
            max_size: None,
            follow_symlinks: false,
            include_system_dirs: true,
            use_hash_cache: false,
        }
    }

    fn run_engine(options: ScanOptions) -> (EngineOutcome, usize) {
        let emits = Arc::new(AtomicUsize::new(0));
        let counter = emits.clone();
        let sink: ProgressSink = Arc::new(move |_progress| {
            counter.fetch_add(1, Ordering::Relaxed);
        });
        let engine = ScanEngine::new(
            "test-scan",
            options,
            ScanControl::new(),
            sink,
            Arc::new(NoopHashCache),
            None,
        );
        let outcome = engine.run().expect("scan should succeed");
        (outcome, emits.load(Ordering::Relaxed))
    }

    fn build_fixture() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write(&root.join("a/one.txt"), b"duplicate content");
        write(&root.join("a/two.txt"), b"duplicate content");
        write(&root.join("b/copy/three.txt"), b"duplicate content");
        write(&root.join("b/unique.txt"), b"unique content here");
        write(&root.join("b/photo1.bin"), vec![1u8; 5000].as_slice());
        write(&root.join("c/photo2.bin"), vec![1u8; 5000].as_slice());
        write(&root.join("c/empty1.txt"), b"");
        write(&root.join("c/empty2.txt"), b"");
        write(&root.join("node_modules/ignored.txt"), b"duplicate content");
        dir
    }

    #[test]
    fn finds_duplicates_recursively() {
        let dir = build_fixture();
        let (outcome, _) = run_engine(test_options(dir.path()));

        // empty files are excluded, node_modules skipped
        assert_eq!(outcome.stats.duplicate_groups, 2);
        let text_group = outcome
            .groups
            .iter()
            .find(|g| g.size == 17)
            .expect("text group");
        assert_eq!(text_group.count(), 3);
        assert_eq!(text_group.wasted_bytes, 34);
        assert!(text_group
            .files
            .iter()
            .all(|f| !f.path.contains("node_modules")));

        let bin_group = outcome
            .groups
            .iter()
            .find(|g| g.size == 5000)
            .expect("bin group");
        assert_eq!(bin_group.count(), 2);
        assert_eq!(outcome.stats.duplicate_files, 5);
        assert_eq!(outcome.stats.errors, 0);
        // 9 files on disk: node_modules is excluded by name, 2 empty files fail
        // the min-size (1 byte) filter -> 6 files discovered.
        assert_eq!(outcome.stats.files_scanned, 6);
    }

    #[test]
    fn groups_are_sorted_by_wasted_space() {
        let dir = build_fixture();
        let (outcome, _) = run_engine(test_options(dir.path()));
        let wasted: Vec<u64> = outcome.groups.iter().map(|g| g.wasted_bytes).collect();
        let mut sorted = wasted.clone();
        sorted.sort_by(|a, b| b.cmp(a));
        assert_eq!(wasted, sorted);
    }

    #[test]
    fn extension_filter_applies() {
        let dir = build_fixture();
        let mut options = test_options(dir.path());
        options.extensions = vec![".TXT".into()];
        let (outcome, _) = run_engine(options);
        assert_eq!(outcome.stats.duplicate_groups, 1);
        assert!(outcome.groups.iter().all(|g| g.size == 17));
    }

    #[test]
    fn size_filter_applies() {
        let dir = build_fixture();
        let mut options = test_options(dir.path());
        options.min_size = Some(1000);
        let (outcome, _) = run_engine(options);
        assert_eq!(outcome.stats.duplicate_groups, 1);
        assert_eq!(outcome.groups[0].size, 5000);
    }

    #[test]
    fn excluded_path_is_skipped() {
        let dir = build_fixture();
        let mut options = test_options(dir.path());
        options.excluded_paths = vec![dir.path().join("a").to_string_lossy().to_string()];
        let (outcome, _) = run_engine(options);
        // a/ removed -> only 2 text copies remain (b/copy + node_modules skipped)
        let text_group = outcome.groups.iter().find(|g| g.size == 17);
        assert!(text_group.is_none());
        assert_eq!(outcome.stats.duplicate_groups, 1);
    }

    #[test]
    fn system_location_detected_on_any_volume() {
        for root in [
            "C:\\Windows",
            "D:\\Windows",
            "d:\\windows",
            "E:\\Program Files",
            "F:\\ProgramData",
            "D:\\$Recycle.Bin",
            "\\\\server\\share\\Windows",
        ] {
            assert!(
                is_system_location(Path::new(root)),
                "{root} must be recognised as a system root"
            );
        }
        for root in [
            "D:\\WindowsApps",
            "C:\\Users\\me\\Documents",
            "D:\\MyWindows",
            "C:\\Program Files Stuff",
        ] {
            assert!(
                !is_system_location(Path::new(root)),
                "{root} is a normal folder"
            );
        }
    }

    #[test]
    fn empty_paths_rejected() {
        let options = ScanOptions {
            paths: vec![],
            ..ScanOptions::default()
        };
        let engine = ScanEngine::new(
            "x",
            options,
            ScanControl::new(),
            Arc::new(|_p: ScanProgress| {}),
            Arc::new(NoopHashCache),
            None,
        );
        assert!(matches!(engine.run(), Err(AppError::InvalidArgument(_))));
    }

    #[test]
    fn missing_path_rejected() {
        let options = ScanOptions {
            paths: vec!["/definitely/not/here".into()],
            ..ScanOptions::default()
        };
        let engine = ScanEngine::new(
            "x",
            options,
            ScanControl::new(),
            Arc::new(|_p: ScanProgress| {}),
            Arc::new(NoopHashCache),
            None,
        );
        assert!(matches!(engine.run(), Err(AppError::InvalidArgument(_))));
    }

    #[test]
    fn cancellation_stops_scan() {
        let dir = tempfile::tempdir().unwrap();
        for i in 0..50u8 {
            write(&dir.path().join(format!("f{i}.bin")), &[i; 10]);
        }
        let control = ScanControl::new();
        control.cancel();
        let engine = ScanEngine::new(
            "x",
            test_options(dir.path()),
            control,
            Arc::new(|_p: ScanProgress| {}),
            Arc::new(NoopHashCache),
            None,
        );
        assert!(matches!(engine.run(), Err(AppError::Cancelled)));
    }

    #[test]
    fn hash_cache_reused_between_runs() {
        type CacheKey = (String, u64, Option<i64>);

        #[derive(Default)]
        struct MemoryCache(Mutex<HashMap<CacheKey, String>>);
        impl HashCacheProvider for MemoryCache {
            fn lookup(&self, path: &str, size: u64, modified_ms: Option<i64>) -> Option<String> {
                self.0
                    .lock()
                    .ok()?
                    .get(&(path.to_string(), size, modified_ms))
                    .cloned()
            }
            fn store(&self, path: &str, size: u64, modified_ms: Option<i64>, sha256: &str) {
                if let Ok(mut map) = self.0.lock() {
                    map.insert((path.to_string(), size, modified_ms), sha256.to_string());
                }
            }
        }

        let dir = build_fixture();
        let cache = Arc::new(MemoryCache::default());
        let engine = ScanEngine::new(
            "cache-run",
            ScanOptions {
                paths: vec![dir.path().to_string_lossy().to_string()],
                excluded_paths: vec![],
                excluded_dir_names: vec!["node_modules".into()],
                extensions: vec![],
                min_size: Some(1),
                max_size: None,
                follow_symlinks: false,
                include_system_dirs: true,
                use_hash_cache: true,
            },
            ScanControl::new(),
            Arc::new(|_p: ScanProgress| {}),
            cache.clone(),
            None,
        );
        let outcome = engine.run().unwrap();
        assert_eq!(outcome.stats.duplicate_groups, 2);
        assert!(!cache.0.lock().unwrap().is_empty());
    }

    #[test]
    fn large_files_scan_returns_top_sorted() {
        let dir = tempfile::tempdir().unwrap();
        for (index, size) in [10usize, 5000, 900, 70_000].iter().enumerate() {
            write(&dir.path().join(format!("f{index}.bin")), &vec![7u8; *size]);
        }
        let result = scan_large_files(
            &[dir.path().to_string_lossy().to_string()],
            &[],
            true,
            2,
            &ScanControl::new(),
        )
        .unwrap();
        assert_eq!(result.files.len(), 2);
        assert_eq!(result.files[0].size, 70_000);
        assert_eq!(result.files[1].size, 5000);
        assert!(result.scanned_files >= 4);
        assert!(!result.category_totals.is_empty());
    }

    #[test]
    fn progress_is_emitted_during_scan() {
        let dir = build_fixture();
        let (_outcome, emitted) = run_engine(test_options(dir.path()));
        assert!(emitted > 0, "expected progress events");
    }

    #[test]
    fn system_dirs_skipped_by_default() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("Windows/x.dll"), b"same");
        write(&dir.path().join("Windows/y.dll"), b"same");
        write(&dir.path().join("data/a.txt"), b"other");
        write(&dir.path().join("data/b.txt"), b"other");

        let mut options = test_options(dir.path());
        options.include_system_dirs = false;
        let (outcome, _) = run_engine(options);
        assert_eq!(outcome.stats.duplicate_groups, 1);
        assert!(outcome
            .groups
            .iter()
            .all(|g| g.files.iter().all(|f| !f.path.contains("Windows"))));
    }

    #[test]
    fn pause_and_resume_have_no_effect_when_idle() {
        let control = ScanControl::new();
        control.set_paused(true);
        assert!(control.is_paused());
        control.set_paused(false);
        assert!(control.checkpoint().is_ok());
    }

    #[test]
    fn checkpoint_blocks_while_paused_and_releases_on_resume() {
        use std::time::Duration;

        // Pause first, then start a worker that hits a checkpoint: it must block.
        let control = ScanControl::new();
        control.set_paused(true);

        let worker_control = control.clone();
        let handle = std::thread::spawn(move || worker_control.checkpoint());

        std::thread::sleep(Duration::from_millis(80));
        assert!(!handle.is_finished(), "checkpoint must block while paused");

        // Cancelling while paused must release the waiter with Cancelled.
        control.cancel();
        let outcome = handle.join().expect("worker must finish after cancel");
        assert!(matches!(outcome, Err(AppError::Cancelled)));

        // Resuming (without cancel) lets a paused worker through.
        let control = ScanControl::new();
        control.set_paused(true);
        let worker_control = control.clone();
        let handle = std::thread::spawn(move || worker_control.checkpoint());
        std::thread::sleep(Duration::from_millis(50));
        assert!(!handle.is_finished());
        let started = std::time::Instant::now();
        control.set_paused(false);
        assert!(handle
            .join()
            .expect("worker must finish after resume")
            .is_ok());
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn pause_does_not_lose_progress_and_scan_completes_after_resume() {
        // The engine must resume from where it stopped, not restart, and the
        // final result must be identical to an uninterrupted run.
        let dir = build_fixture();
        let control = ScanControl::new();

        // Pause before the run starts; the scan will block on its first
        // checkpoint until we resume from another thread.
        control.set_paused(true);
        let resume_control = control.clone();
        let resumer = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(120));
            resume_control.set_paused(false);
        });

        let engine = ScanEngine::new(
            "paused",
            test_options(dir.path()),
            control,
            Arc::new(|_p: ScanProgress| {}),
            Arc::new(NoopHashCache),
            None,
        );
        let outcome = engine.run().expect("scan must finish after resume");
        resumer.join().expect("resumer thread");

        // Same fixture as the uninterrupted baseline: 2 groups, 5 duplicate files.
        assert_eq!(outcome.stats.duplicate_groups, 2);
        assert_eq!(outcome.stats.duplicate_files, 5);
        assert!(
            outcome.errors.is_empty(),
            "a clean fixture must not produce errors: {:?}",
            outcome.errors
        );
    }
}
