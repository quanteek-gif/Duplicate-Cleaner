//! End-to-end integration tests: scan a real directory tree, group
//! duplicates, persist to SQLite, then validate the deletion safety net.

use std::collections::HashMap;
use std::io::Write;
use std::path::Path;
use std::sync::Arc;

use duplicate_cleaner_lib::core::scanner_engine::{ProgressSink, ScanControl, ScanEngine};
use duplicate_cleaner_lib::database::repository;
use duplicate_cleaner_lib::database::Database;
use duplicate_cleaner_lib::models::scan_result::{ScanOptions, ScanProgress};
use duplicate_cleaner_lib::services::deletion_service::{DeletionRequest, DeletionService};

fn write(path: &Path, bytes: &[u8]) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    let mut file = std::fs::File::create(path).unwrap();
    file.write_all(bytes).unwrap();
}

fn options(root: &Path) -> ScanOptions {
    ScanOptions {
        paths: vec![root.to_string_lossy().to_string()],
        excluded_dir_names: vec![],
        min_size: Some(1),
        include_system_dirs: true,
        use_hash_cache: true,
        ..ScanOptions::default()
    }
}

type CacheKey = (String, u64, Option<i64>);

#[derive(Default)]
struct MemCache(std::sync::Mutex<HashMap<CacheKey, String>>);

impl duplicate_cleaner_lib::core::scanner_engine::HashCacheProvider for MemCache {
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

/// Build a tree with known duplicate structure:
/// - 3 copies of the same 300 KiB payload (a big, multi-block file)
/// - 2 copies of a small config file
/// - unique files that must not be reported
fn fixture(root: &Path) -> Vec<String> {
    let mut payload = vec![0u8; 300 * 1024];
    for (index, byte) in payload.iter_mut().enumerate() {
        *byte = (index % 251) as u8;
    }
    write(&root.join("photos/IMG_0001.jpg"), &payload);
    write(&root.join("backup/photos/IMG_0001.jpg"), &payload);
    write(&root.join("Downloads/IMG_0001 (1).jpg"), &payload);

    write(&root.join("app/config.ini"), b"[main]\ntheme=dark\n");
    write(&root.join("app/config.ini.bak"), b"[main]\ntheme=dark\n");

    write(&root.join("docs/report.pdf"), b"%PDF-1.7 unique content");
    write(&root.join("docs/notes.txt"), b"completely different notes");

    vec![
        root.join("photos/IMG_0001.jpg")
            .to_string_lossy()
            .to_string(),
        root.join("backup/photos/IMG_0001.jpg")
            .to_string_lossy()
            .to_string(),
        root.join("Downloads/IMG_0001 (1).jpg")
            .to_string_lossy()
            .to_string(),
    ]
}

fn run_scan(
    root: &Path,
    cache: Arc<MemCache>,
) -> duplicate_cleaner_lib::core::scanner_engine::EngineOutcome {
    let sink: ProgressSink = Arc::new(|_progress: ScanProgress| {});
    let engine = ScanEngine::new(
        "integration-scan",
        options(root),
        ScanControl::new(),
        sink,
        cache,
        None,
    );
    engine.run().expect("scan")
}

#[test]
fn full_pipeline_scan_persist_and_protect() {
    let dir = tempfile::tempdir().unwrap();
    let copies = fixture(dir.path());

    let cache = Arc::new(MemCache::default());
    let outcome = run_scan(dir.path(), cache.clone());

    // --- grouping -----------------------------------------------------
    assert_eq!(outcome.stats.duplicate_groups, 2, "two duplicate groups");
    assert_eq!(outcome.stats.duplicate_files, 5);
    let big = outcome
        .groups
        .iter()
        .find(|group| group.size == 300 * 1024)
        .expect("big group");
    assert_eq!(big.count(), 3);
    assert_eq!(big.wasted_bytes, 600 * 1024);
    assert_eq!(big.hash.len(), 64);

    // every path in the group is one of the fixture copies
    for file in &big.files {
        assert!(copies.contains(&file.path), "unexpected path {}", file.path);
        assert!(file.sha256.is_none() || file.sha256.as_deref() == Some(big.hash.as_str()));
    }

    // --- persistence --------------------------------------------------
    let db = Database::open(&dir.path().join("test.sqlite3")).unwrap();
    let now = repository::now_millis();
    repository::insert_scan_started(&db, "integration-scan", now, &options(dir.path())).unwrap();
    let result = duplicate_cleaner_lib::models::scan_result::ScanResult {
        scan_id: "integration-scan".into(),
        started_at: now,
        finished_at: now,
        options: options(dir.path()),
        stats: outcome.stats.clone(),
        groups: outcome.groups.clone(),
        errors: outcome.errors.clone(),
    };
    repository::mark_scan_finished(&db, "integration-scan", "completed", Some(&result)).unwrap();

    let loaded = repository::load_scan_result(&db, "integration-scan").unwrap();
    assert_eq!(loaded.groups.len(), 2);
    assert_eq!(
        loaded.groups.iter().map(|g| g.files.len()).sum::<usize>(),
        5
    );

    // --- deletion safety ---------------------------------------------
    let service = DeletionService::new(db.clone());
    let hashes: HashMap<String, String> = big
        .files
        .iter()
        .map(|file| (file.path.clone(), big.hash.clone()))
        .collect();

    // Requesting every copy of the big group: one must survive.
    let request = DeletionRequest {
        paths: big.files.iter().map(|file| file.path.clone()).collect(),
        scan_id: Some("integration-scan".into()),
        expected_hashes: hashes.clone(),
        group_members: vec![big.files.iter().map(|file| file.path.clone()).collect()],
        confirmed: true,
    };
    let plan = service.plan(&request);
    assert_eq!(plan.allowed.len(), 2);
    assert_eq!(plan.rejected.len(), 1);

    let report = service.delete(&request).unwrap();
    assert_eq!(report.deleted, 2);
    assert_eq!(report.failed, 1);
    let survivors: Vec<&duplicate_cleaner_lib::models::file::FileEntry> = big
        .files
        .iter()
        .filter(|file| Path::new(&file.path).exists())
        .collect();
    assert_eq!(survivors.len(), 1, "exactly one copy must remain on disk");
    assert_eq!(std::fs::read(&survivors[0].path).unwrap().len(), 300 * 1024);

    // deletion history is persisted
    let log = repository::list_deletions(&db, 10).unwrap();
    assert_eq!(log.iter().filter(|entry| entry.success).count(), 2);

    // --- persisted snapshot follows the deletions ---------------------
    let reloaded = repository::load_scan_result(&db, "integration-scan").unwrap();
    assert_eq!(
        reloaded.groups.len(),
        1,
        "only the untouched small group may survive: {:?}",
        reloaded
            .groups
            .iter()
            .map(|group| (group.size, group.files.len()))
            .collect::<Vec<_>>()
    );
    assert_eq!(reloaded.stats.duplicate_groups, 1);
    assert_eq!(reloaded.stats.duplicate_files, 2);
    assert!(reloaded.stats.wasted_bytes > 0);
    let deleted_paths: Vec<String> = big.files.iter().map(|file| file.path.clone()).collect();
    let still_listed = repository::load_scan_result(&db, "integration-scan")
        .unwrap()
        .groups
        .iter()
        .flat_map(|group| group.files.iter())
        .filter(|file| deleted_paths.contains(&file.path) && !Path::new(&file.path).exists())
        .count();
    assert_eq!(
        still_listed, 0,
        "deleted files must not linger in the saved scan"
    );
}

#[test]
fn second_scan_reuses_cached_hashes() {
    let dir = tempfile::tempdir().unwrap();
    fixture(dir.path());
    let cache = Arc::new(MemCache::default());

    let first = run_scan(dir.path(), cache.clone());
    assert_eq!(first.stats.duplicate_groups, 2);
    let cached_entries = cache.0.lock().unwrap().len();
    assert!(cached_entries > 0);

    // Change nothing: the second scan must find the same groups and still
    // consult (and hit) the cache for every candidate.
    let second = run_scan(dir.path(), cache.clone());
    assert_eq!(second.stats.duplicate_groups, 2);
    assert_eq!(
        cache.0.lock().unwrap().len(),
        cached_entries,
        "cache size must not grow when files are unchanged"
    );
}

#[test]
fn modified_file_is_not_deleted_after_rescan() {
    let dir = tempfile::tempdir().unwrap();
    fixture(dir.path());
    let cache = Arc::new(MemCache::default());
    let outcome = run_scan(dir.path(), cache.clone());
    let group = outcome
        .groups
        .iter()
        .find(|group| group.files.iter().any(|file| file.extension == "ini"))
        .expect("ini group");
    let target = group.files[0].path.clone();

    // Simulate the user editing the file between scan and deletion.
    std::fs::write(&target, b"[main]\ntheme=LIGHT\n").unwrap();

    let db = Database::open_in_memory().unwrap();
    let service = DeletionService::new(db);
    let mut hashes = HashMap::new();
    hashes.insert(target.clone(), group.hash.clone());
    let request = DeletionRequest {
        paths: vec![target.clone()],
        scan_id: None,
        expected_hashes: hashes,
        group_members: vec![group.files.iter().map(|file| file.path.clone()).collect()],
        confirmed: true,
    };
    let report = service.delete(&request).unwrap();
    assert_eq!(report.deleted, 0);
    assert!(Path::new(&target).exists());
    assert!(report.outcomes.iter().any(|outcome| outcome
        .reason
        .as_deref()
        .unwrap_or("")
        .contains("hash mismatch")));
}
