//! Tauri command surface (thin wrappers over the service layer).

pub mod duplicates;
pub mod file_operations;
pub mod scanner;
pub mod statistics;

/// Argument normalisation shared by the commands.
///
/// Tauri commands can only be exercised through the Tauri runtime, so the
/// clamping/defaulting they do is kept in plain functions that unit tests can
/// call directly. Every value here is user-supplied from the UI.
pub mod limits {
    /// Rows returned by the "recent scans" list.
    pub const RECENT_SCANS_DEFAULT: u32 = 20;
    pub const RECENT_SCANS_MAX: u32 = 500;

    /// Rows returned by the deletion history.
    pub const DELETIONS_DEFAULT: u32 = 200;
    pub const DELETIONS_MAX: u32 = 1000;

    /// Entries in the "largest files" ranking.
    pub const LARGE_FILES_DEFAULT: usize = 100;
    pub const LARGE_FILES_MAX: usize = 1000;

    /// Longest edge of a generated image preview, in pixels.
    pub const PREVIEW_DIM_DEFAULT: u32 = 512;
    pub const PREVIEW_DIM_MIN: u32 = 64;
    pub const PREVIEW_DIM_MAX: u32 = 2048;

    pub fn recent_scans(limit: Option<u32>) -> u32 {
        limit
            .unwrap_or(RECENT_SCANS_DEFAULT)
            .clamp(1, RECENT_SCANS_MAX)
    }

    pub fn deletions(limit: Option<u32>) -> u32 {
        limit.unwrap_or(DELETIONS_DEFAULT).clamp(1, DELETIONS_MAX)
    }

    pub fn large_files(limit: Option<usize>) -> usize {
        limit
            .unwrap_or(LARGE_FILES_DEFAULT)
            .clamp(1, LARGE_FILES_MAX)
    }

    pub fn preview_dim(max_dim: Option<u32>) -> u32 {
        max_dim
            .unwrap_or(PREVIEW_DIM_DEFAULT)
            .clamp(PREVIEW_DIM_MIN, PREVIEW_DIM_MAX)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn recent_scans_defaults_and_clamps() {
            assert_eq!(recent_scans(None), RECENT_SCANS_DEFAULT);
            assert_eq!(
                recent_scans(Some(0)),
                1,
                "zero must not ask for an empty page"
            );
            assert_eq!(recent_scans(Some(5)), 5);
            assert_eq!(recent_scans(Some(u32::MAX)), RECENT_SCANS_MAX);
        }

        #[test]
        fn deletions_defaults_and_clamps() {
            assert_eq!(deletions(None), DELETIONS_DEFAULT);
            assert_eq!(deletions(Some(0)), 1);
            assert_eq!(deletions(Some(u32::MAX)), DELETIONS_MAX);
        }

        #[test]
        fn large_files_defaults_and_clamps() {
            assert_eq!(large_files(None), LARGE_FILES_DEFAULT);
            assert_eq!(large_files(Some(0)), 1);
            assert_eq!(large_files(Some(500)), 500);
            assert_eq!(large_files(Some(usize::MAX)), LARGE_FILES_MAX);
        }

        #[test]
        fn preview_dim_is_bounded_on_both_sides() {
            assert_eq!(preview_dim(None), PREVIEW_DIM_DEFAULT);
            assert_eq!(preview_dim(Some(1)), PREVIEW_DIM_MIN);
            assert_eq!(preview_dim(Some(1024)), 1024);
            assert_eq!(preview_dim(Some(u32::MAX)), PREVIEW_DIM_MAX);
        }
    }
}
