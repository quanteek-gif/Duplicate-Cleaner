//! Disk usage analysis: per-directory sizes, category breakdown and a
//! depth-limited tree for the Disk Map visualisation.
//!
//! Pseudo/virtual kernel filesystems (procfs, sysfs, tmpfs… on Linux) report
//! meaningless multi-terabyte sizes, so they are skipped by name.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use walkdir::WalkDir;

use crate::core::scanner_engine::ScanControl;
use crate::models::disk::{category_for_extension, DiskMapResult, DiskNode};
use crate::models::scan_result::CategoryTotal;
use crate::utils::error::{AppError, AppResult};
use crate::utils::file_utils;

/// Depth of the returned tree (deeper directories are folded into their parent).
pub const TREE_DEPTH: usize = 4;
const MAX_CHILDREN_PER_NODE: usize = 60;

/// Linux virtual filesystems whose sizes are not real disk usage. Matched by
/// directory *name* on any level.
///
/// Compiled out on Windows: a Windows volume legitimately contains folders
/// called `dev`, `run` or `snap`, and hiding them would under-report the map.
#[cfg(not(windows))]
// Only directories that really appear at a Linux root. `tmpfs` and `snap` are
// not root entries (`/tmpfs` and `/snap` never exist — `tmpfs` is a filesystem
// type and `snap` lives under `/dev`), and `lost+found` is a real ext4
// directory that can hold recovered data, so it must be measured, not hidden.
const VIRTUAL_DIRS: &[&str] = &["proc", "sys", "dev", "run"];
#[cfg(windows)]
const VIRTUAL_DIRS: &[&str] = &[];

/// Linux container stores given as paths relative to the analysed root — a
/// name check can never match `var/lib/docker`. Also Windows-free.
#[cfg(not(windows))]
const VIRTUAL_SUBPATHS: &[&str] = &["var/lib/docker", "var/lib/containers", "var/lib/lxcfs"];
#[cfg(windows)]
const VIRTUAL_SUBPATHS: &[&str] = &[];

#[derive(Debug, Clone)]
struct DirAggregate {
    size: u64,
    files: u64,
}

/// Analyse the disk usage under `root`.
pub fn analyze_disk(root: &str, control: &ScanControl) -> AppResult<DiskMapResult> {
    let started = Instant::now();
    let root_path = PathBuf::from(root);
    if !root_path.exists() {
        return Err(AppError::invalid(format!("path does not exist: {root}")));
    }

    let mut dirs: HashMap<PathBuf, DirAggregate> = HashMap::new();
    dirs.insert(root_path.clone(), DirAggregate { size: 0, files: 0 });

    let mut categories: HashMap<&'static str, (u64, u64)> = HashMap::new();
    let mut scanned_files: u64 = 0;
    let mut scanned_dirs: u64 = 1;
    let mut errors: u64 = 0;

    let walker = WalkDir::new(&root_path)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| {
            if !entry.file_type().is_dir() || entry.depth() == 0 {
                return true;
            }
            let name = entry.file_name().to_string_lossy().to_lowercase();
            if VIRTUAL_DIRS.contains(&name.as_str()) {
                return false;
            }
            !is_virtual_subpath(&root_path, entry.path())
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
        if file_type.is_dir() {
            scanned_dirs += 1;
            dirs.entry(entry.path().to_path_buf())
                .or_insert(DirAggregate { size: 0, files: 0 });
            continue;
        }
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
        let size = metadata.len();
        scanned_files += 1;

        let extension = file_utils::extension_of(entry.path());
        let category = category_for_extension(&extension);
        let bucket = categories.entry(category).or_insert((0, 0));
        bucket.0 += size;
        bucket.1 += 1;

        // propagate the size to every ancestor up to the root
        let mut current = entry.path().parent();
        while let Some(dir) = current {
            let aggregate = dirs
                .entry(dir.to_path_buf())
                .or_insert(DirAggregate { size: 0, files: 0 });
            aggregate.size += size;
            aggregate.files += 1;
            if dir == root_path {
                break;
            }
            current = dir.parent();
        }
    }

    let total_bytes = dirs
        .get(&root_path)
        .map(|aggregate| aggregate.size)
        .unwrap_or(0);

    let tree = build_tree(&root_path, &dirs, total_bytes, 0);

    let mut category_totals: Vec<CategoryTotal> = categories
        .into_iter()
        .map(|(category, (bytes, count))| CategoryTotal {
            category: category.to_string(),
            bytes,
            count,
        })
        .collect();
    category_totals.sort_by_key(|total| std::cmp::Reverse(total.bytes));

    let (capacity, free) = disk_capacity(&root_path);

    Ok(DiskMapResult {
        root: root.to_string(),
        total_bytes,
        scanned_files,
        scanned_dirs,
        free_bytes: free,
        capacity_bytes: capacity,
        categories: category_totals,
        tree,
        elapsed_ms: started.elapsed().as_millis() as u64,
        errors,
    })
}

/// Capacity/free space of the volume that hosts `path`.
pub fn disk_capacity(path: &Path) -> (u64, u64) {
    let disks = match sysinfo::Disks::new_with_refreshed_list() {
        Ok(disks) => disks,
        Err(_) => return (0, 0),
    };

    let normalized = path
        .to_string_lossy()
        .to_lowercase()
        .replace('/', std::path::MAIN_SEPARATOR_STR);

    let mut best: Option<(u64, u64, usize)> = None;
    for disk in &disks {
        let mount = disk.mount_point().to_string_lossy().to_lowercase();
        let matches = if cfg!(windows) {
            let mount_trimmed = mount.trim_end_matches(['\\', '/']);
            normalized.starts_with(mount_trimmed)
        } else {
            normalized.starts_with(&mount)
        };
        if matches {
            let length = mount.len();
            if best
                .as_ref()
                .map(|(_, _, best_length)| length > *best_length)
                .unwrap_or(true)
            {
                best = Some((disk.total_space(), disk.available_space(), length));
            }
        }
    }
    best.map(|(total, free, _)| (total, free)).unwrap_or((0, 0))
}

fn build_tree(
    path: &Path,
    dirs: &HashMap<PathBuf, DirAggregate>,
    parent_size: u64,
    depth: usize,
) -> DiskNode {
    let aggregate = dirs
        .get(path)
        .cloned()
        .unwrap_or(DirAggregate { size: 0, files: 0 });
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| file_utils::display_path(path));

    let mut children: Vec<DiskNode> = Vec::new();
    if depth < TREE_DEPTH {
        let mut child_dirs: Vec<PathBuf> = dirs
            .keys()
            .filter(|key| key.parent() == Some(path))
            .cloned()
            .collect();
        child_dirs.sort_by_key(|dir| std::cmp::Reverse(dirs.get(dir).map(|d| d.size).unwrap_or(0)));
        for child in child_dirs.into_iter().take(MAX_CHILDREN_PER_NODE) {
            children.push(build_tree(&child, dirs, aggregate.size, depth + 1));
        }
    }

    DiskNode {
        name,
        path: file_utils::display_path(path),
        size: aggregate.size,
        is_dir: true,
        percent: if parent_size > 0 {
            (aggregate.size as f64 / parent_size as f64) * 100.0
        } else {
            0.0
        },
        category: None,
        children,
        file_count: aggregate.files,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write(path: &Path, bytes: usize) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        let mut file = std::fs::File::create(path).unwrap();
        file.write_all(&vec![0u8; bytes]).unwrap();
    }

    #[test]
    fn aggregates_sizes_and_categories() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("photos/a.jpg"), 1000);
        write(&dir.path().join("photos/b.jpg"), 2000);
        write(&dir.path().join("video/movie.mp4"), 50_000);
        write(&dir.path().join("notes.txt"), 10);

        let result = analyze_disk(&dir.path().to_string_lossy(), &ScanControl::new()).unwrap();
        assert_eq!(result.total_bytes, 53_010);
        assert_eq!(result.scanned_files, 4);
        assert!(!result.categories.is_empty());
        assert_eq!(result.categories[0].category, "video");

        let photos = result
            .tree
            .children
            .iter()
            .find(|node| node.name == "photos")
            .expect("photos node");
        assert_eq!(photos.size, 3000);
        assert_eq!(photos.file_count, 2);
        assert!((photos.percent - (3000.0 / 53_010.0 * 100.0)).abs() < 0.01);
    }

    #[test]
    fn missing_root_fails() {
        assert!(analyze_disk("Z:\\nope", &ScanControl::new()).is_err());
    }

    #[test]
    fn cancellation_is_respected() {
        let dir = tempfile::tempdir().unwrap();
        for index in 0..10 {
            write(&dir.path().join(format!("d{index}/f.bin")), 10);
        }
        let control = ScanControl::new();
        control.cancel();
        assert!(analyze_disk(&dir.path().to_string_lossy(), &control).is_err());
    }

    #[test]
    fn capacity_is_reported_for_root() {
        let (total, free) = disk_capacity(Path::new("/"));
        assert!(total >= free);
    }
}

/// `true` when `path` sits inside one of [`VIRTUAL_SUBPATHS`] under `root`.
fn is_virtual_subpath(root: &Path, path: &Path) -> bool {
    let Ok(relative) = path.strip_prefix(root) else {
        return false;
    };
    matches_virtual_subpath(&relative.to_string_lossy(), VIRTUAL_SUBPATHS)
}

/// Pure matcher shared by every platform, so Windows (where the list is empty)
/// still exercises the comparison logic in tests.
fn matches_virtual_subpath(relative: &str, subpaths: &[&str]) -> bool {
    let normalized = relative.replace('\\', "/").to_lowercase();
    subpaths
        .iter()
        .any(|subpath| normalized == *subpath || normalized.starts_with(&format!("{subpath}/")))
}

#[cfg(test)]
mod virtual_tests {
    use super::*;

    const STORES: &[&str] = &["var/lib/docker", "var/lib/containers"];

    /// Names that must never be skipped by accident: `lost+found` holds
    /// recovered files, and `tmpfs`/`snap` only ever appear *inside* `/dev`,
    /// where the `/dev` exclusion already covers them.
    #[cfg(not(windows))]
    #[test]
    fn only_real_root_pseudo_filesystems_are_virtual() {
        for name in ["proc", "sys", "dev", "run"] {
            assert!(VIRTUAL_DIRS.contains(&name), "{name} must stay virtual");
        }
        for name in ["lost+found", "tmpfs", "snap", "home", "var", "usr"] {
            assert!(
                !VIRTUAL_DIRS.contains(&name),
                "{name} is real data and must be measured"
            );
        }
    }

    #[test]
    fn virtual_subpaths_are_matched_relative_to_the_root() {
        assert!(matches_virtual_subpath("var/lib/docker", STORES));
        assert!(matches_virtual_subpath(
            "var/lib/docker/overlay2/abc",
            STORES
        ));
        // Windows separators must work too
        assert!(matches_virtual_subpath(
            "var\\lib\\docker\\overlay2",
            STORES
        ));
        // a same-named folder elsewhere must still be analysed
        assert!(!matches_virtual_subpath("home/me/docker", STORES));
        assert!(!matches_virtual_subpath("var/lib/dockerless", STORES));
        assert!(!matches_virtual_subpath("var/lib", STORES));
    }

    #[cfg(not(windows))]
    #[test]
    fn subpath_matching_respects_the_analysed_root() {
        assert!(is_virtual_subpath(
            Path::new("/"),
            Path::new("/var/lib/docker")
        ));
        let nested = Path::new("/data");
        assert!(is_virtual_subpath(
            nested,
            Path::new("/data/var/lib/docker/x")
        ));
        assert!(!is_virtual_subpath(
            nested,
            Path::new("/other/var/lib/docker")
        ));
    }

    #[cfg(not(windows))]
    #[test]
    fn virtual_dir_names_do_not_include_paths() {
        // Every entry must be a bare directory name, otherwise the name check
        // in the walker silently never fires.
        for name in VIRTUAL_DIRS {
            assert!(
                !name.contains('/'),
                "{name} is a path, not a directory name"
            );
        }
        for subpath in VIRTUAL_SUBPATHS {
            assert!(subpath.contains('/'), "{subpath} should be a relative path");
        }
    }
}
