//! Visually similar images grouped by perceptual hash distance.

use serde::{Deserialize, Serialize};

use super::file::FileEntry;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SimilarImage {
    pub file: FileEntry,
    /// Perceptual hash as 16 hex characters.
    pub phash: String,
    /// Hamming distance to the group reference image (0 = identical pHash).
    pub distance: u32,
    /// Base64 encoded JPEG thumbnail (`data:image/jpeg;base64,`).
    pub thumbnail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SimilarGroup {
    pub id: i64,
    pub reference_path: String,
    /// Largest Hamming distance inside the group.
    pub max_distance: u32,
    pub images: Vec<SimilarImage>,
}

impl SimilarGroup {
    /// Rough "wasted" space assuming only one copy is kept.
    pub fn wasted_bytes(&self) -> u64 {
        let total: u64 = self.images.iter().map(|i| i.file.size).sum();
        let keep = self.images.iter().map(|i| i.file.size).max().unwrap_or(0);
        total.saturating_sub(keep)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SimilarPhotosResult {
    pub scan_id: String,
    pub groups: Vec<SimilarGroup>,
    pub images_scanned: u64,
    pub images_hashed: u64,
    pub errors: u64,
    pub threshold: u32,
    pub elapsed_ms: u64,
}
