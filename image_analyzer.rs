//! Similar-photos discovery: recursive image scan, parallel pHash computation
//! and clustering into visual groups with thumbnails for side-by-side preview.

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

use crate::core::hasher::{self, phash_from_hex, phash_to_hex};
use crate::core::image_comparator::cluster_hashes;
use crate::core::scanner_engine::ScanControl;
use crate::models::file::FileEntry;
use crate::models::similar::{SimilarGroup, SimilarImage, SimilarPhotosResult};
use crate::utils::error::{AppError, AppResult};
use crate::utils::file_utils;

/// Skip images smaller than this (thumbnails, icons) and above this size
/// (decoding huge images would exhaust memory).
pub const MIN_IMAGE_BYTES: u64 = 1024;
pub const MAX_IMAGE_BYTES: u64 = 60 * 1024 * 1024;

/// Limit thumbnail generation so the IPC payload stays reasonable.
/// Groups that carry thumbnails. Beyond this the payload (base64 JPEGs over
/// IPC) would grow into tens of megabytes; the rest of the groups are still
/// returned and remain fully usable, they simply arrive without a preview.
const MAX_THUMBNAIL_GROUPS: usize = 80;
/// Upper bound on members rendered with a preview inside one group. A cluster
/// can be huge (a folder of identical icons); the UI keeps the largest ones,
/// which is what `members.sort_by_key` below establishes.
const MAX_THUMBNAILS_PER_GROUP: usize = 60;
const THUMBNAIL_SIZE: u32 = 224;

const PROGRESS_INTERVAL: Duration = Duration::from_millis(150);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SimilarProgress {
    pub scan_id: String,
    pub phase: String,
    pub images_found: u64,
    pub images_to_hash: u64,
    pub images_hashed: u64,
    pub current_file: Option<String>,
    pub groups_found: u64,
    pub errors: u64,
    pub elapsed_ms: u64,
}

pub type SimilarProgressSink = Arc<dyn Fn(SimilarProgress) + Send + Sync>;

pub struct SimilarPhotosAnalyzer {
    pub scan_id: String,
    pub roots: Vec<String>,
    pub threshold: u32,
    pub excluded_dir_names: Vec<String>,
    pub include_system_dirs: bool,
    pub control: Arc<ScanControl>,
    pub progress: SimilarProgressSink,
    pub pool: Option<Arc<rayon::ThreadPool>>,
}

impl SimilarPhotosAnalyzer {
    pub fn run(&self) -> AppResult<SimilarPhotosResult> {
        let started = Instant::now();
        if self.roots.is_empty() {
            return Err(AppError::invalid("select at least one folder with photos"));
        }
        if self.threshold > 32 {
            return Err(AppError::invalid("threshold must be between 0 and 32 bits"));
        }

        // ---- discovery -------------------------------------------------
        let mut images: Vec<FileEntry> = Vec::new();
        for root in &self.roots {
            if !Path::new(root).exists() {
                return Err(AppError::invalid(format!("path does not exist: {root}")));
            }
            let walker = WalkDir::new(root)
                .follow_links(false)
                .into_iter()
                .filter_entry(|entry| {
                    if entry.file_type().is_dir() {
                        let name = entry.file_name().to_string_lossy().to_lowercase();
                        if crate::core::scanner_engine::is_system_dir_name(&name)
                            && !self.include_system_dirs
                        {
                            return false;
                        }
                        return !self
                            .excluded_dir_names
                            .iter()
                            .any(|excluded| excluded.trim().to_lowercase() == name);
                    }
                    true
                });

            for entry in walker.filter_map(Result::ok) {
                self.control.checkpoint()?;
                let file_type = entry.file_type();
                if !file_type.is_file() || file_type.is_symlink() {
                    continue;
                }
                if !file_utils::is_supported_image(entry.path()) {
                    continue;
                }
                let metadata = match entry.metadata() {
                    Ok(metadata) => metadata,
                    Err(_) => continue,
                };
                if metadata.len() < MIN_IMAGE_BYTES || metadata.len() > MAX_IMAGE_BYTES {
                    continue;
                }
                images.push(FileEntry::from_metadata(entry.path(), &metadata));
                if images.len() % 500 == 0 {
                    self.emit(SimilarProgress {
                        scan_id: self.scan_id.clone(),
                        phase: "discovering".into(),
                        images_found: images.len() as u64,
                        images_to_hash: 0,
                        images_hashed: 0,
                        current_file: entry.path().to_str().map(str::to_string),
                        groups_found: 0,
                        errors: 0,
                        elapsed_ms: started.elapsed().as_millis() as u64,
                    });
                }
            }
        }

        let total = images.len() as u64;
        self.emit(SimilarProgress {
            scan_id: self.scan_id.clone(),
            phase: "hashing".into(),
            images_found: total,
            images_to_hash: total,
            images_hashed: 0,
            current_file: None,
            groups_found: 0,
            errors: 0,
            elapsed_ms: started.elapsed().as_millis() as u64,
        });

        // ---- parallel pHash --------------------------------------------
        let hashed_counter = AtomicU64::new(0);
        let errors = AtomicU64::new(0);
        let current = Mutex::new(None::<String>);
        let last_emit = Mutex::new(Instant::now() - PROGRESS_INTERVAL);
        let chunk = ((self.worker_count()) * 8).max(16);

        let mut results: Vec<(usize, Option<u64>)> = Vec::with_capacity(images.len());
        let hashes_ref = &mut results;

        for (chunk_index, slice) in images.chunks(chunk).enumerate() {
            self.control.checkpoint()?;
            let offset = chunk_index * chunk;
            let partial: Vec<(usize, Option<u64>)> = self.install_pool(|| {
                slice
                    .par_iter()
                    .enumerate()
                    .map(|(i, image)| {
                        if let Ok(mut slot) = current.lock() {
                            *slot = Some(image.name.clone());
                        }
                        match hasher::perceptual_hash(Path::new(&image.path)) {
                            Ok(hash) => (offset + i, Some(hash)),
                            Err(_) => {
                                errors.fetch_add(1, Ordering::Relaxed);
                                (offset + i, None)
                            }
                        }
                    })
                    .collect()
            });
            hashes_ref.extend(partial);

            let done = hashed_counter.fetch_add(slice.len() as u64, Ordering::Relaxed)
                + slice.len() as u64;
            let emit_now = last_emit
                .lock()
                .map(|mut last| {
                    if last.elapsed() >= PROGRESS_INTERVAL {
                        *last = Instant::now();
                        true
                    } else {
                        false
                    }
                })
                .unwrap_or(false);
            if emit_now {
                self.emit(SimilarProgress {
                    scan_id: self.scan_id.clone(),
                    phase: "hashing".into(),
                    images_found: total,
                    images_to_hash: total,
                    images_hashed: done,
                    current_file: current.lock().ok().and_then(|slot| slot.clone()),
                    groups_found: 0,
                    errors: errors.load(Ordering::Relaxed),
                    elapsed_ms: started.elapsed().as_millis() as u64,
                });
            }
        }

        // ---- clustering ------------------------------------------------
        let hash_pairs: Vec<(usize, u64)> = results
            .iter()
            .filter_map(|(index, hash)| hash.map(|hash| (*index, hash)))
            .collect();
        let clusters = cluster_hashes(&hash_pairs, self.threshold);

        // ---- thumbnails & result assembly ------------------------------
        let mut groups: Vec<SimilarGroup> = Vec::with_capacity(clusters.len());
        for (group_index, cluster) in clusters.iter().enumerate() {
            let add_thumbnails = group_index < MAX_THUMBNAIL_GROUPS;
            let mut members: Vec<SimilarImage> = Vec::with_capacity(cluster.members.len());
            for (position, member) in cluster.members.iter().enumerate() {
                let image_index = hash_pairs[*member].0;
                let hash = hash_pairs[*member].1;
                let file = match images.get(image_index) {
                    Some(file) => file.clone(),
                    None => continue,
                };
                // Every member of a displayed group gets a thumbnail: the card
                // renders a name and a size regardless, so a missing image just
                // looks broken. The cap that keeps the IPC payload small is on
                // the number of *groups*, not on members within a group.
                let thumbnail = if add_thumbnails && position < MAX_THUMBNAILS_PER_GROUP {
                    hasher::thumbnail_data_uri(Path::new(&file.path), THUMBNAIL_SIZE).ok()
                } else {
                    None
                };
                members.push(SimilarImage {
                    distance: crate::core::image_comparator::distance(
                        hash_pairs[cluster.representative].1,
                        hash,
                    ),
                    file,
                    phash: phash_to_hex(hash),
                    thumbnail,
                });
            }
            if members.len() < 2 {
                continue;
            }
            members.sort_by_key(|image| std::cmp::Reverse(image.file.size));
            let reference_path = members
                .first()
                .map(|m| m.file.path.clone())
                .unwrap_or_default();
            groups.push(SimilarGroup {
                id: group_index as i64 + 1,
                reference_path,
                max_distance: cluster.max_distance,
                images: members,
            });
        }
        groups.sort_by_key(|group| std::cmp::Reverse(group.wasted_bytes()));

        let result = SimilarPhotosResult {
            scan_id: self.scan_id.clone(),
            groups,
            images_scanned: total,
            images_hashed: hash_pairs.len() as u64,
            errors: errors.load(Ordering::Relaxed),
            threshold: self.threshold,
            elapsed_ms: started.elapsed().as_millis() as u64,
        };

        self.emit(SimilarProgress {
            scan_id: self.scan_id.clone(),
            phase: "completed".into(),
            images_found: total,
            images_to_hash: total,
            images_hashed: hash_pairs.len() as u64,
            current_file: None,
            groups_found: result.groups.len() as u64,
            errors: result.errors,
            elapsed_ms: result.elapsed_ms,
        });

        tracing::info!(
            images = total,
            groups = result.groups.len(),
            errors = result.errors,
            "similar-photo analysis finished"
        );
        Ok(result)
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

    fn install_pool<R: Send>(&self, f: impl FnOnce() -> R + Send) -> R {
        match &self.pool {
            Some(pool) => pool.install(f),
            None => f(),
        }
    }

    fn emit(&self, progress: SimilarProgress) {
        (self.progress)(progress);
    }
}

/// Compute a pHash for an arbitrary file (used by integration tests and the
/// `hash_images` helper command).
pub fn hash_single_image(path: &Path) -> AppResult<String> {
    let hash = hasher::perceptual_hash(path)?;
    Ok(phash_to_hex(hash))
}

/// Hex distance helper used by tests.
pub fn hex_distance(left: &str, right: &str) -> Option<u32> {
    let a = phash_from_hex(left)?;
    let b = phash_from_hex(right)?;
    Some(crate::core::image_comparator::distance(a, b))
}

/// Build a map of phash → paths for debugging / tests.
pub fn collect_hashes(paths: &[&Path]) -> AppResult<HashMap<String, Vec<String>>> {
    let mut map: HashMap<String, Vec<String>> = HashMap::new();
    for path in paths {
        let hash = hash_single_image(path)?;
        map.entry(hash)
            .or_default()
            .push(path.to_string_lossy().to_string());
    }
    Ok(map)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// 256×256 photo-like image (smooth gradients survive scaling, and the
    /// PNG is large enough to pass the minimum-size filter).
    fn draw(path: &Path, variant: u8) {
        let mut image = image::RgbImage::new(256, 256);
        for (x, y, pixel) in image.enumerate_pixels_mut() {
            let sky: u8 = if y < 128 { 200 } else { 60 };
            let shade = ((x as f32 / 256.0) * 50.0) as u8;
            let r = sky.saturating_sub(shade / 2).saturating_add(variant % 8);
            let g = sky.saturating_sub(shade).saturating_add(variant % 5);
            let b = sky;
            *pixel = image::Rgb([r, g, b]);
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        image.save(path).unwrap();
    }

    fn analyze(root: &Path, threshold: u32) -> SimilarPhotosResult {
        let analyzer = SimilarPhotosAnalyzer {
            scan_id: "similar-test".into(),
            roots: vec![root.to_string_lossy().to_string()],
            threshold,
            excluded_dir_names: vec!["node_modules".into()],
            include_system_dirs: true,
            control: ScanControl::new(),
            progress: Arc::new(|_p: SimilarProgress| {}),
            pool: None,
        };
        analyzer.run().expect("analysis")
    }

    fn fixture() -> (tempfile::TempDir, Vec<PathBuf>) {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.png");
        let b = dir.path().join("sub/b.png");
        let c = dir.path().join("sub/deeper/c.png");
        draw(&a, 0);
        draw(&b, 3);
        draw(&c, 0);
        // different picture
        draw(&dir.path().join("other.png"), 120);
        (dir, vec![a, b, c])
    }

    #[test]
    fn finds_similar_images() {
        let (dir, _paths) = fixture();
        let result = analyze(dir.path(), 8);
        assert!(result.images_scanned >= 4);
        assert!(
            !result.groups.is_empty(),
            "expected at least one similar group"
        );
        let group = &result.groups[0];
        assert!(group.images.len() >= 2);
        assert!(group.images.iter().all(|image| !image.phash.is_empty()));
        assert_eq!(result.errors, 0);
    }

    #[test]
    fn thumbnails_generated_for_first_groups() {
        let (dir, _paths) = fixture();
        let result = analyze(dir.path(), 8);
        let group = result.groups.first().expect("group");
        assert!(group.images.iter().any(|image| image
            .thumbnail
            .as_deref()
            .unwrap_or("")
            .starts_with("data:image/jpeg")));
    }

    #[test]
    fn rejects_empty_roots() {
        let analyzer = SimilarPhotosAnalyzer {
            scan_id: "x".into(),
            roots: vec![],
            threshold: 8,
            excluded_dir_names: vec![],
            include_system_dirs: true,
            control: ScanControl::new(),
            progress: Arc::new(|_p: SimilarProgress| {}),
            pool: None,
        };
        assert!(analyzer.run().is_err());
    }

    #[test]
    fn hex_distance_is_symmetric() {
        assert_eq!(
            hex_distance("00000000000000ff", "0000000000000000"),
            Some(8)
        );
        assert_eq!(hex_distance("xyz", "0000000000000000"), None);
    }

    #[test]
    fn single_image_hash_is_hex() {
        let (dir, paths) = fixture();
        let hash = hash_single_image(&paths[0]).unwrap();
        assert_eq!(hash.len(), 16);
        assert!(u64::from_str_radix(&hash, 16).is_ok());
        drop(dir);
    }
}
