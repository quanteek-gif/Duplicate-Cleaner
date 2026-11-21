//! Drive information and disk-space map models.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DriveInfo {
    pub name: String,
    pub mount_point: String,
    pub file_system: String,
    pub total_bytes: u64,
    pub available_bytes: u64,
    pub used_bytes: u64,
    pub is_removable: bool,
    pub is_read_only: bool,
}

/// One node of the disk usage tree.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskNode {
    pub name: String,
    pub path: String,
    pub size: u64,
    pub is_dir: bool,
    /// Share of the parent size, 0..=100.
    pub percent: f64,
    /// Extension-derived category for files ("video", "image", ...).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    pub children: Vec<DiskNode>,
    pub file_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskMapResult {
    pub root: String,
    pub total_bytes: u64,
    pub scanned_files: u64,
    pub scanned_dirs: u64,
    pub free_bytes: u64,
    pub capacity_bytes: u64,
    pub categories: Vec<super::scan_result::CategoryTotal>,
    pub tree: DiskNode,
    pub elapsed_ms: u64,
    pub errors: u64,
}

/// File category buckets used by Large Files and Disk Map.
pub fn category_for_extension(extension: &str) -> &'static str {
    match extension {
        "mp4" | "mkv" | "avi" | "mov" | "wmv" | "flv" | "webm" | "m4v" | "mpg" | "mpeg" | "ts"
        | "m2ts" => "video",
        "jpg" | "jpeg" | "png" | "gif" | "bmp" | "webp" | "tif" | "tiff" | "heic" | "raw"
        | "cr2" | "nef" | "svg" | "ico" | "psd" => "image",
        "mp3" | "wav" | "flac" | "aac" | "ogg" | "wma" | "m4a" | "opus" | "aiff" => "audio",
        "zip" | "rar" | "7z" | "tar" | "gz" | "bz2" | "xz" | "iso" | "cab" | "zst" => "archive",
        "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "txt" | "rtf" | "odt"
        | "ods" | "odp" | "csv" | "md" | "epub" => "document",
        "exe" | "msi" | "dll" | "sys" | "com" | "bat" | "cmd" | "ps1" => "executable",
        "rs" | "tsx" | "js" | "jsx" | "py" | "c" | "cpp" | "h" | "hpp" | "java" | "go" | "cs"
        | "rb" | "php" | "html" | "css" | "json" | "toml" | "yaml" | "yml" | "xml" | "sql"
        | "sh" => "code",
        "ttf" | "otf" | "woff" | "woff2" => "font",
        _ => "other",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn categories_detected() {
        assert_eq!(category_for_extension("mp4"), "video");
        assert_eq!(
            category_for_extension("JPG".to_lowercase().as_str()),
            "image"
        );
        assert_eq!(category_for_extension("zip"), "archive");
        assert_eq!(category_for_extension("rs"), "code");
        assert_eq!(category_for_extension(""), "other");
    }
}
