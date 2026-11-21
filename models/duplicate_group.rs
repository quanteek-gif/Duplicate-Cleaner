//! Group of files whose content is byte-for-byte identical.

use serde::{Deserialize, Serialize};

use super::file::FileEntry;
use crate::core::analyzer::RecommendedCopy;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateGroup {
    /// Database row id (0 before persistence).
    pub id: i64,
    /// Hex SHA-256 shared by every member.
    pub hash: String,
    /// Size of one copy in bytes.
    pub size: u64,
    /// Total bytes wasted by redundant copies (`size * (count - 1)`).
    pub wasted_bytes: u64,
    /// Which copy the analyzer suggests keeping (and why). `None` for an empty group.
    pub recommended_keep: Option<RecommendedCopy>,
    pub files: Vec<FileEntry>,
}

impl DuplicateGroup {
    pub fn new(hash: String, mut files: Vec<FileEntry>) -> Self {
        files.sort_by(|a, b| a.path.cmp(&b.path));
        let size = files.first().map(|f| f.size).unwrap_or(0);
        let wasted_bytes = size.saturating_mul(files.len().saturating_sub(1) as u64);
        let recommended_keep = crate::core::analyzer::recommended_keeper(&files);
        Self {
            id: 0,
            hash,
            size,
            wasted_bytes,
            recommended_keep,
            files,
        }
    }

    pub fn count(&self) -> usize {
        self.files.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(path: &str, size: u64) -> FileEntry {
        FileEntry {
            id: 0,
            path: path.to_string(),
            name: path.rsplit('/').next().unwrap_or_default().to_string(),
            size,
            created_ms: None,
            modified_ms: None,
            extension: String::new(),
            sha256: None,
            phash: None,
        }
    }

    #[test]
    fn group_computes_wasted_space() {
        let group = DuplicateGroup::new(
            "abc".into(),
            vec![
                entry("b/x.bin", 100),
                entry("a/x.bin", 100),
                entry("c/x.bin", 100),
            ],
        );
        assert_eq!(group.count(), 3);
        assert_eq!(group.wasted_bytes, 200);
        assert_eq!(
            group
                .recommended_keep
                .as_ref()
                .map(|keep| keep.path.as_str()),
            Some("a/x.bin")
        );
        // sorted by path
        assert_eq!(group.files[0].path, "a/x.bin");
    }

    #[test]
    fn single_file_group_has_no_waste() {
        let group = DuplicateGroup::new("abc".into(), vec![entry("a", 10)]);
        assert_eq!(group.wasted_bytes, 0);
    }
}
