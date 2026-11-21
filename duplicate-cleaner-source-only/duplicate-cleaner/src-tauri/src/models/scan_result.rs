//! Scan configuration, progress reports and persisted scan results.

use serde::{Deserialize, Serialize};

use super::duplicate_group::DuplicateGroup;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanOptions {
    /// Roots to scan (drives or folders).
    pub paths: Vec<String>,
    /// Additional folders to skip (recursively).
    #[serde(default)]
    pub excluded_paths: Vec<String>,
    /// Directory *names* to skip anywhere in the tree (e.g. `node_modules`).
    #[serde(default)]
    pub excluded_dir_names: Vec<String>,
    /// Only include these extensions (lower-case, no dot). Empty = all files.
    #[serde(default)]
    pub extensions: Vec<String>,
    #[serde(default)]
    pub min_size: Option<u64>,
    #[serde(default)]
    pub max_size: Option<u64>,
    /// Follow symbolic links. Off by default (safety + loop protection).
    #[serde(default)]
    pub follow_symlinks: bool,
    /// Explicit permission to scan system directories such as `C:\Windows`.
    #[serde(default)]
    pub include_system_dirs: bool,
    /// Reuse hashes from previous scans when size and mtime match.
    #[serde(default = "default_true")]
    pub use_hash_cache: bool,
}

fn default_true() -> bool {
    true
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            paths: Vec::new(),
            excluded_paths: Vec::new(),
            excluded_dir_names: default_excluded_dirs(),
            extensions: Vec::new(),
            min_size: None,
            max_size: None,
            follow_symlinks: false,
            include_system_dirs: false,
            use_hash_cache: true,
        }
    }
}

/// Directories ignored by default — system, cache and developer folders where
/// duplicates are guaranteed but removing them can break software.
pub fn default_excluded_dirs() -> Vec<String> {
    [
        "node_modules",
        ".git",
        ".svn",
        ".hg",
        "target",
        "obj",
        "bin",
        "dist",
        "build",
        "$recycle.bin",
        "system volume information",
        "recovery",
        "windows",
        "winsxs",
        "program files",
        "program files (x86)",
        "programdata",
        "appdata",
        ".cache",
        "__pycache__",
        ".venv",
        "venv",
        ".next",
        ".nuxt",
        ".gradle",
        ".m2",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ScanPhase {
    Discovering,
    Sizing,
    Hashing,
    Grouping,
    Completed,
    Cancelled,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanProgress {
    pub scan_id: String,
    pub phase: ScanPhase,
    pub files_discovered: u64,
    pub files_processed: u64,
    pub files_to_hash: u64,
    pub bytes_discovered: u64,
    pub bytes_hashed: u64,
    pub current_file: Option<String>,
    pub duplicates_found: u64,
    pub groups_found: u64,
    pub errors: u64,
    pub elapsed_ms: u64,
    pub message: Option<String>,
}

impl ScanProgress {
    /// Percentage 0..=100 across all phases (discovery counts for 25%).
    pub fn percent(&self) -> f64 {
        match self.phase {
            ScanPhase::Discovering => {
                if self.files_discovered == 0 {
                    2.0
                } else {
                    12.0
                }
            }
            ScanPhase::Sizing => 15.0,
            ScanPhase::Hashing => {
                if self.files_to_hash == 0 {
                    92.0
                } else {
                    20.0 + 72.0 * (self.files_processed as f64 / self.files_to_hash as f64)
                }
            }
            ScanPhase::Grouping => 95.0,
            ScanPhase::Completed => 100.0,
            ScanPhase::Cancelled | ScanPhase::Failed => {
                if self.files_to_hash == 0 {
                    0.0
                } else {
                    20.0 + 72.0 * (self.files_processed as f64 / self.files_to_hash as f64)
                }
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanErrorEntry {
    pub path: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanStats {
    pub files_scanned: u64,
    pub files_hashed: u64,
    pub bytes_scanned: u64,
    pub duplicate_groups: u64,
    pub duplicate_files: u64,
    pub wasted_bytes: u64,
    pub errors: u64,
}

impl ScanStats {
    pub fn empty() -> Self {
        Self {
            files_scanned: 0,
            files_hashed: 0,
            bytes_scanned: 0,
            duplicate_groups: 0,
            duplicate_files: 0,
            wasted_bytes: 0,
            errors: 0,
        }
    }
}

/// Full result of a scanning session, returned to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanResult {
    pub scan_id: String,
    pub started_at: i64,
    pub finished_at: i64,
    pub options: ScanOptions,
    pub stats: ScanStats,
    pub groups: Vec<DuplicateGroup>,
    pub errors: Vec<ScanErrorEntry>,
}

/// Row shown in "Recent scans" lists.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanRecord {
    pub id: i64,
    pub scan_id: String,
    pub started_at: i64,
    pub finished_at: Option<i64>,
    pub status: String,
    pub paths: Vec<String>,
    pub files_scanned: u64,
    pub files_hashed: u64,
    pub duplicate_groups: u64,
    pub duplicate_files: u64,
    pub wasted_bytes: u64,
    pub errors: u64,
}

/// Aggregate numbers for the Dashboard.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardStats {
    pub total_scans: u64,
    pub last_scan: Option<ScanRecord>,
    pub files_scanned: u64,
    pub duplicate_files: u64,
    pub duplicate_groups: u64,
    pub wasted_bytes: u64,
    pub deleted_files: u64,
    pub deleted_bytes: u64,
    pub recent_scans: Vec<ScanRecord>,
}

/// Largest file entry for the "Large Files" section.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LargeFile {
    pub path: String,
    pub name: String,
    pub size: u64,
    pub extension: String,
    pub modified_ms: Option<i64>,
    pub category: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LargeFilesResult {
    pub scanned_files: u64,
    pub files: Vec<LargeFile>,
    pub category_totals: Vec<CategoryTotal>,
    pub elapsed_ms: u64,
    /// Directories/files that could not be read.
    pub errors: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryTotal {
    pub category: String,
    pub bytes: u64,
    pub count: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_percent_is_monotonic_across_phases() {
        let mut p = ScanProgress {
            scan_id: "x".into(),
            phase: ScanPhase::Discovering,
            files_discovered: 0,
            files_processed: 0,
            files_to_hash: 0,
            bytes_discovered: 0,
            bytes_hashed: 0,
            current_file: None,
            duplicates_found: 0,
            groups_found: 0,
            errors: 0,
            elapsed_ms: 0,
            message: None,
        };
        assert!(p.percent() > 0.0);
        p.files_discovered = 100;
        assert!(p.percent() >= 12.0);
        p.phase = ScanPhase::Sizing;
        p.phase = ScanPhase::Hashing;
        p.files_to_hash = 10;
        p.files_processed = 5;
        let mid = p.percent();
        assert!((20.0..=92.0).contains(&mid));
        p.phase = ScanPhase::Completed;
        assert_eq!(p.percent(), 100.0);
    }

    #[test]
    fn default_options_exclude_development_dirs() {
        let options = ScanOptions::default();
        assert!(options
            .excluded_dir_names
            .contains(&"node_modules".to_string()));
        assert!(!options.follow_symlinks);
        assert!(options.use_hash_cache);
    }
}
