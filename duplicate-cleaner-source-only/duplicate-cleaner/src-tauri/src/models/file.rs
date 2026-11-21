//! A single file entry as seen by the scanner and stored in the database.

use serde::{Deserialize, Serialize};

use crate::utils::file_utils::{extension_of, system_time_to_millis};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    /// Database row id (0 while the entry is not persisted yet).
    pub id: i64,
    /// Full path including the long-path prefix stripped.
    pub path: String,
    pub name: String,
    pub size: u64,
    /// Creation time, unix millis (may be unavailable on some filesystems).
    pub created_ms: Option<i64>,
    /// Last modification time, unix millis.
    pub modified_ms: Option<i64>,
    /// Lower-case extension without a dot ("" for none).
    pub extension: String,
    /// Hex encoded SHA-256 (only for files of duplicated size).
    pub sha256: Option<String>,
    /// Perceptual hash as 16 hex chars (only for images).
    pub phash: Option<String>,
}

impl FileEntry {
    pub fn from_metadata(path: &std::path::Path, metadata: &std::fs::Metadata) -> Self {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        Self {
            id: 0,
            path: crate::utils::file_utils::display_path(path),
            name,
            size: metadata.len(),
            created_ms: system_time_to_millis(metadata.created().ok()),
            modified_ms: system_time_to_millis(metadata.modified().ok()),
            extension: extension_of(path),
            sha256: None,
            phash: None,
        }
    }

    /// Parent directory of the file.
    pub fn parent_dir(&self) -> String {
        std::path::Path::new(&self.path)
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default()
    }
}

/// Sort/index helpers used by the UI.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SortDirection {
    Asc,
    Desc,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn entry_from_metadata() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("sample.TXT");
        let mut file = std::fs::File::create(&path).expect("create");
        file.write_all(b"hello").expect("write");

        let metadata = std::fs::metadata(&path).expect("metadata");
        let entry = FileEntry::from_metadata(&path, &metadata);
        assert_eq!(entry.name, "sample.TXT");
        assert_eq!(entry.size, 5);
        assert_eq!(entry.extension, "txt");
        assert!(entry.modified_ms.is_some());
        assert_eq!(entry.id, 0);
    }
}
