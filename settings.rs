//! User settings persisted in SQLite.

use serde::{Deserialize, Serialize};

use super::scan_result::default_excluded_dirs;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    /// Maximum number of Hamming bits between two considered-similar photos.
    pub similar_photos_threshold: u32,
    /// Skip files smaller than this during scans (bytes).
    pub min_file_size: u64,
    /// Number of parallel hashing workers (0 = auto).
    pub hash_concurrency: usize,
    /// Directory names ignored during scans.
    pub excluded_dir_names: Vec<String>,
    /// Ask for confirmation before moving files to the trash.
    pub confirm_deletion: bool,
    /// Verify SHA-256 again right before deletion.
    pub verify_hash_before_delete: bool,
    /// Keep files in the trash instead of permanent deletion (Windows trash).
    pub use_recycle_bin: bool,
    /// Number of entries kept in the scan history.
    pub history_limit: u32,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            similar_photos_threshold: 8,
            min_file_size: 1,
            hash_concurrency: 0,
            excluded_dir_names: default_excluded_dirs(),
            confirm_deletion: true,
            verify_hash_before_delete: true,
            use_recycle_bin: true,
            history_limit: 50,
        }
    }
}

/// Upper bound for the perceptual-hash Hamming threshold. A 64-bit pHash can
/// differ by at most 64 bits; anything above half of that matches unrelated
/// pictures, so the value is capped well below it.
pub const MAX_SIMILAR_THRESHOLD: u32 = 32;

impl AppSettings {
    /// Clamp every field into its supported range.
    ///
    /// Applied on both save and load so the value stored in SQLite, the value
    /// returned to the UI and the value used by the scanner always agree.
    pub fn sanitize(&mut self) {
        self.similar_photos_threshold = self.similar_photos_threshold.min(MAX_SIMILAR_THRESHOLD);
        self.hash_concurrency = self.hash_concurrency.min(32);
        self.history_limit = self.history_limit.clamp(1, 1000);
    }

    /// Resolve the effective worker count for the current machine.
    pub fn effective_concurrency(&self) -> usize {
        if self.hash_concurrency > 0 {
            self.hash_concurrency.clamp(1, 32)
        } else {
            std::thread::available_parallelism()
                .map(|n| n.get().clamp(2, 16))
                .unwrap_or(4)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_safe() {
        let s = AppSettings::default();
        assert!(s.confirm_deletion);
        assert!(s.verify_hash_before_delete);
        assert!(s.use_recycle_bin);
        assert!(s.effective_concurrency() >= 1);
    }

    #[test]
    fn sanitize_caps_out_of_range_values() {
        let mut s = AppSettings {
            similar_photos_threshold: 99,
            hash_concurrency: 500,
            history_limit: 0,
            ..AppSettings::default()
        };
        s.sanitize();
        assert_eq!(s.similar_photos_threshold, MAX_SIMILAR_THRESHOLD);
        assert_eq!(s.hash_concurrency, 32);
        assert_eq!(s.history_limit, 1);
    }

    #[test]
    fn sanitize_leaves_valid_values_alone() {
        let mut s = AppSettings::default();
        let before = s.clone();
        s.sanitize();
        assert_eq!(s, before);
    }

    #[test]
    fn concurrency_is_clamped() {
        let s = AppSettings {
            hash_concurrency: 1000,
            ..AppSettings::default()
        };
        assert_eq!(s.effective_concurrency(), 32);
    }
}
