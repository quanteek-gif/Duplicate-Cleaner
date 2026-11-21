//! Data-access functions on top of [`crate::database::Database`].
//!
//! Each public function takes `&Database` and performs a short synchronous
//! operation. Scan results are written inside a single transaction.

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, OptionalExtension};

use crate::core::scanner_engine::HashCacheProvider;
use crate::database::Database;
use crate::models::duplicate_group::DuplicateGroup;
use crate::models::file::FileEntry;
use crate::models::scan_result::{DashboardStats, ScanOptions, ScanRecord, ScanResult};
use crate::models::settings::AppSettings;
use crate::utils::error::{AppError, AppResult};

/// rusqlite stores integers as `i64`; sizes are clamped to `i64::MAX`.
fn to_i64(value: u64) -> i64 {
    value.min(i64::MAX as u64) as i64
}

fn from_u64(value: i64) -> u64 {
    if value < 0 {
        0
    } else {
        value as u64
    }
}

pub fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Scans
// ---------------------------------------------------------------------------

pub fn insert_scan_started(
    db: &Database,
    scan_id: &str,
    started_at: i64,
    options: &ScanOptions,
) -> AppResult<()> {
    let paths = serde_json::to_string(&options.paths)?;
    db.with_connection(|connection| {
        connection.execute(
            "INSERT INTO scans (scan_id, started_at, status, paths) VALUES (?1, ?2, 'running', ?3)",
            params![scan_id, started_at, paths],
        )?;
        Ok(())
    })
}

pub fn insert_running_scan(db: &Database, scan_id: &str, options: &ScanOptions) -> AppResult<()> {
    insert_scan_started(db, scan_id, now_millis(), options)
}

pub fn mark_scan_finished(
    db: &Database,
    scan_id: &str,
    status: &str,
    result: Option<&ScanResult>,
) -> AppResult<()> {
    db.with_connection(|connection| {
        let transaction = connection.transaction()?;
        match result {
            Some(result) => {
                transaction.execute(
                    "UPDATE scans SET finished_at=?1, status=?2, files_scanned=?3, files_hashed=?4,
                     duplicate_groups=?5, duplicate_files=?6, wasted_bytes=?7, errors=?8
                     WHERE scan_id=?9",
                    params![
                        result.finished_at,
                        status,
                        to_i64(result.stats.files_scanned),
                        to_i64(result.stats.files_hashed),
                        to_i64(result.stats.duplicate_groups),
                        to_i64(result.stats.duplicate_files),
                        to_i64(result.stats.wasted_bytes),
                        to_i64(result.stats.errors),
                        scan_id
                    ],
                )?;
                for group in &result.groups {
                    transaction.execute(
                        "INSERT INTO scan_groups (scan_id, hash, size, wasted) VALUES (?1, ?2, ?3, ?4)",
                        params![scan_id, group.hash, to_i64(group.size), to_i64(group.wasted_bytes)],
                    )?;
                    let group_row = transaction.last_insert_rowid();
                    for file in &group.files {
                        transaction.execute(
                            "INSERT INTO scan_files
                             (scan_id, group_id, path, name, size, created_ms, modified_ms, extension, sha256, phash)
                             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                            params![
                                scan_id,
                                group_row,
                                file.path,
                                file.name,
                                to_i64(file.size),
                                file.created_ms,
                                file.modified_ms,
                                file.extension,
                                file.sha256.as_deref().unwrap_or(&group.hash),
                                file.phash
                            ],
                        )?;
                    }
                }
            }
            None => {
                transaction.execute(
                    "UPDATE scans SET finished_at=?1, status=?2 WHERE scan_id=?3",
                    params![now_millis(), status, scan_id],
                )?;
            }
        }
        transaction.commit()?;
        Ok(())
    })
}

pub fn load_scan_result(db: &Database, scan_id: &str) -> AppResult<ScanResult> {
    db.with_connection(|connection| load_scan_result_inner(connection, scan_id))
}

fn load_scan_result_inner(
    connection: &mut rusqlite::Connection,
    scan_id: &str,
) -> AppResult<ScanResult> {
    let row = connection
        .query_row(
            "SELECT started_at, finished_at, paths, files_scanned, files_hashed, duplicate_groups,
                    duplicate_files, wasted_bytes, errors, status
             FROM scans WHERE scan_id=?1",
            [scan_id],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Option<i64>>(1)?,
                    row.get::<_, String>(2)?,
                    from_u64(row.get::<_, i64>(3)?),
                    from_u64(row.get::<_, i64>(4)?),
                    from_u64(row.get::<_, i64>(5)?),
                    from_u64(row.get::<_, i64>(6)?),
                    from_u64(row.get::<_, i64>(7)?),
                    from_u64(row.get::<_, i64>(8)?),
                    row.get::<_, String>(9)?,
                ))
            },
        )
        .optional()?
        .ok_or_else(|| AppError::ScanNotFound(scan_id.to_string()))?;

    let (
        started_at,
        finished_at,
        paths_json,
        files_scanned,
        files_hashed,
        // Kept for the record; the counters below come from the live groups.
        _recorded_duplicate_groups,
        _recorded_duplicate_files,
        _recorded_wasted_bytes,
        errors,
        _status,
    ) = row;

    let paths: Vec<String> = serde_json::from_str(&paths_json).unwrap_or_default();
    let options = ScanOptions {
        paths,
        ..ScanOptions::default()
    };

    // groups ordered the same way the engine produced them
    let mut group_query = connection.prepare(
        "SELECT id, hash, size, wasted FROM scan_groups WHERE scan_id=?1 ORDER BY wasted DESC, id ASC",
    )?;
    let group_rows: Vec<(i64, String, u64, u64)> = group_query
        .query_map([scan_id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                from_u64(row.get::<_, i64>(2)?),
                from_u64(row.get::<_, i64>(3)?),
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;

    let mut file_query = connection.prepare(
        "SELECT path, name, size, created_ms, modified_ms, extension, sha256, phash
         FROM scan_files WHERE group_id=?1 ORDER BY path ASC",
    )?;

    let mut groups: Vec<DuplicateGroup> = Vec::with_capacity(group_rows.len());
    for (group_id, hash, size, wasted) in group_rows {
        let files: Vec<FileEntry> = file_query
            .query_map([group_id], |row| {
                Ok(FileEntry {
                    id: 0,
                    path: row.get(0)?,
                    name: row.get(1)?,
                    size: from_u64(row.get::<_, i64>(2)?),
                    created_ms: row.get(3)?,
                    modified_ms: row.get(4)?,
                    extension: row.get(5)?,
                    sha256: row.get(6)?,
                    phash: row.get(7)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        let recommended_keep = crate::core::analyzer::recommended_keeper(&files);
        groups.push(DuplicateGroup {
            id: group_id,
            hash,
            size,
            wasted_bytes: wasted,
            recommended_keep,
            files,
        });
    }

    // The `scans` row records what the scan *found* (that is what History
    // shows), but the groups returned here may have been pruned by deletions
    // since. Derive the duplicate counters from the groups actually present so
    // a single response never contradicts itself.
    let live_groups = groups.len() as u64;
    let live_files: u64 = groups.iter().map(|group| group.count() as u64).sum();
    let live_wasted: u64 = groups.iter().map(|group| group.wasted_bytes).sum();

    Ok(ScanResult {
        scan_id: scan_id.to_string(),
        started_at,
        finished_at: finished_at.unwrap_or(started_at),
        options,
        stats: crate::models::scan_result::ScanStats {
            files_scanned,
            files_hashed,
            bytes_scanned: 0,
            duplicate_groups: live_groups,
            duplicate_files: live_files,
            wasted_bytes: live_wasted,
            errors,
        },
        groups,
        errors: Vec::new(),
    })
}

fn scan_record_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ScanRecord> {
    let paths_json: String = row.get(5)?;
    Ok(ScanRecord {
        id: row.get(0)?,
        scan_id: row.get(1)?,
        started_at: row.get(2)?,
        finished_at: row.get(3)?,
        status: row.get(4)?,
        paths: serde_json::from_str(&paths_json).unwrap_or_default(),
        files_scanned: from_u64(row.get::<_, i64>(6)?),
        files_hashed: from_u64(row.get::<_, i64>(7)?),
        duplicate_groups: from_u64(row.get::<_, i64>(8)?),
        duplicate_files: from_u64(row.get::<_, i64>(9)?),
        wasted_bytes: from_u64(row.get::<_, i64>(10)?),
        errors: from_u64(row.get::<_, i64>(11)?),
    })
}

fn scan_records(connection: &mut rusqlite::Connection, limit: u32) -> AppResult<Vec<ScanRecord>> {
    let mut statement = connection.prepare(
        "SELECT id, scan_id, started_at, finished_at, status, paths, files_scanned, files_hashed,
                duplicate_groups, duplicate_files, wasted_bytes, errors
         FROM scans ORDER BY started_at DESC LIMIT ?1",
    )?;
    let records = statement
        .query_map([limit], scan_record_from_row)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(records)
}

pub fn list_scans(db: &Database, limit: u32) -> AppResult<Vec<ScanRecord>> {
    db.with_connection(|connection| scan_records(connection, limit))
}

pub fn delete_scan(db: &Database, scan_id: &str) -> AppResult<()> {
    db.with_connection(|connection| {
        connection.execute("DELETE FROM scans WHERE scan_id=?1", [scan_id])?;
        Ok(())
    })
}

/// Remove all scans except the newest `keep` ones (history limit setting).
pub fn trim_history(db: &Database, keep: u32) -> AppResult<()> {
    db.with_connection(|connection| {
        connection.execute(
            "DELETE FROM scans WHERE scan_id NOT IN
             (SELECT scan_id FROM scans ORDER BY started_at DESC LIMIT ?1)",
            [keep.max(1)],
        )?;
        // cascade deletes groups/files
        connection.execute(
            "DELETE FROM scan_groups WHERE scan_id NOT IN (SELECT scan_id FROM scans)",
            [],
        )?;
        connection.execute(
            "DELETE FROM scan_files WHERE scan_id NOT IN (SELECT scan_id FROM scans)",
            [],
        )?;
        Ok(())
    })
}

pub fn dashboard_stats(db: &Database) -> AppResult<DashboardStats> {
    db.with_connection(|connection| {
        let total_scans: u64 = from_u64(connection.query_row(
            "SELECT COUNT(*) FROM scans",
            [],
            |row| row.get::<_, i64>(0),
        )?);
        let last: Option<ScanRecord> = connection
            .query_row(
                "SELECT id, scan_id, started_at, finished_at, status, paths, files_scanned,
                        files_hashed, duplicate_groups, duplicate_files, wasted_bytes, errors
                 FROM scans WHERE status='completed' ORDER BY started_at DESC LIMIT 1",
                [],
                scan_record_from_row,
            )
            .optional()?;

        let recent = scan_records(connection, 5)?;

        let (deleted_files, deleted_bytes): (u64, u64) = connection.query_row(
            "SELECT COUNT(*), COALESCE(SUM(size), 0) FROM deletion_log WHERE success=1",
            [],
            |row| {
                Ok((
                    from_u64(row.get::<_, i64>(0)?),
                    from_u64(row.get::<_, i64>(1)?),
                ))
            },
        )?;

        Ok(DashboardStats {
            total_scans,
            files_scanned: last.as_ref().map(|r| r.files_scanned).unwrap_or(0),
            duplicate_files: last.as_ref().map(|r| r.duplicate_files).unwrap_or(0),
            duplicate_groups: last.as_ref().map(|r| r.duplicate_groups).unwrap_or(0),
            wasted_bytes: last.as_ref().map(|r| r.wasted_bytes).unwrap_or(0),
            last_scan: last,
            deleted_files,
            deleted_bytes,
            recent_scans: recent,
        })
    })
}

// ---------------------------------------------------------------------------
// Settings
// ---------------------------------------------------------------------------

pub fn load_settings(db: &Database) -> AppResult<AppSettings> {
    let defaults = AppSettings::default();
    db.with_connection(|connection| {
        let mut statement = connection.prepare("SELECT key, value FROM settings")?;
        let rows: Vec<(String, String)> = statement
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        if rows.is_empty() {
            let mut settings = defaults;
            settings.sanitize();
            return Ok(settings);
        }
        let map: HashMap<String, String> = rows.into_iter().collect();
        let mut settings = defaults;
        if let Some(value) = map.get("similar_photos_threshold") {
            if let Ok(parsed) = value.parse::<u32>() {
                settings.similar_photos_threshold = parsed;
            }
        }
        if let Some(value) = map.get("min_file_size") {
            if let Ok(parsed) = value.parse::<u64>() {
                settings.min_file_size = parsed;
            }
        }
        if let Some(value) = map.get("hash_concurrency") {
            if let Ok(parsed) = value.parse::<usize>() {
                settings.hash_concurrency = parsed;
            }
        }
        if let Some(value) = map.get("excluded_dir_names") {
            if let Ok(parsed) = serde_json::from_str::<Vec<String>>(value) {
                settings.excluded_dir_names = parsed;
            }
        }
        if let Some(value) = map.get("confirm_deletion") {
            settings.confirm_deletion = value == "true";
        }
        if let Some(value) = map.get("verify_hash_before_delete") {
            settings.verify_hash_before_delete = value == "true";
        }
        if let Some(value) = map.get("use_recycle_bin") {
            settings.use_recycle_bin = value == "true";
        }
        if let Some(value) = map.get("history_limit") {
            if let Ok(parsed) = value.parse::<u32>() {
                settings.history_limit = parsed;
            }
        }
        settings.sanitize();
        Ok(settings)
    })
}

pub fn save_settings(db: &Database, settings: &AppSettings) -> AppResult<()> {
    let mut settings = settings.clone();
    settings.sanitize();
    let settings = &settings;
    db.with_connection(|connection| {
        let transaction = connection.transaction()?;
        let entries: Vec<(&str, String)> = vec![
            (
                "similar_photos_threshold",
                settings.similar_photos_threshold.to_string(),
            ),
            ("min_file_size", settings.min_file_size.to_string()),
            ("hash_concurrency", settings.hash_concurrency.to_string()),
            (
                "excluded_dir_names",
                serde_json::to_string(&settings.excluded_dir_names)?,
            ),
            ("confirm_deletion", settings.confirm_deletion.to_string()),
            (
                "verify_hash_before_delete",
                settings.verify_hash_before_delete.to_string(),
            ),
            ("use_recycle_bin", settings.use_recycle_bin.to_string()),
            ("history_limit", settings.history_limit.to_string()),
        ];
        for (key, value) in entries {
            transaction.execute(
                "INSERT INTO settings (key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value=excluded.value",
                params![key, value],
            )?;
        }
        transaction.commit()?;
        Ok(())
    })
}

// ---------------------------------------------------------------------------
// Hash cache
// ---------------------------------------------------------------------------

pub fn lookup_hash(
    db: &Database,
    path: &str,
    size: u64,
    modified_ms: Option<i64>,
) -> Option<String> {
    db.with_connection(|connection| {
        let cached: Option<(u64, Option<i64>, String)> = connection
            .query_row(
                "SELECT size, modified_ms, sha256 FROM hash_cache WHERE path=?1",
                [path],
                |row| Ok((from_u64(row.get::<_, i64>(0)?), row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        match cached {
            Some((cached_size, cached_modified, sha256))
                if cached_size == size && cached_modified == modified_ms =>
            {
                Ok(Some(sha256))
            }
            _ => Ok(None),
        }
    })
    .ok()
    .flatten()
}

pub fn store_hash(db: &Database, path: &str, size: u64, modified_ms: Option<i64>, sha256: &str) {
    let _ = db.with_connection(|connection| {
        connection.execute(
            "INSERT INTO hash_cache (path, size, modified_ms, sha256, cached_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(path) DO UPDATE SET
                size=excluded.size, modified_ms=excluded.modified_ms,
                sha256=excluded.sha256, cached_at=excluded.cached_at",
            params![path, to_i64(size), modified_ms, sha256, now_millis()],
        )?;
        Ok(())
    });
}

pub fn clear_hash_cache(db: &Database) -> AppResult<u64> {
    db.with_connection(|connection| {
        let removed = connection.execute("DELETE FROM hash_cache", [])?;
        Ok(removed as u64)
    })
}

/// Evict cache rows that were not reused for 90 days (keeps the DB small).
pub fn evict_stale_hash_cache(db: &Database, older_than_days: i64) -> AppResult<u64> {
    db.with_connection(|connection| {
        let cutoff = now_millis() - older_than_days * 24 * 60 * 60 * 1000;
        let removed =
            connection.execute("DELETE FROM hash_cache WHERE cached_at < ?1", [cutoff])?;
        Ok(removed as u64)
    })
}

/// Adapter making [`Database`] usable as an engine [`HashCacheProvider`].
pub struct DbHashCache {
    pub db: std::sync::Arc<Database>,
    pub enabled: bool,
}

impl HashCacheProvider for DbHashCache {
    fn lookup(&self, path: &str, size: u64, modified_ms: Option<i64>) -> Option<String> {
        if !self.enabled {
            return None;
        }
        lookup_hash(&self.db, path, size, modified_ms)
    }

    fn store(&self, path: &str, size: u64, modified_ms: Option<i64>, sha256: &str) {
        if self.enabled {
            store_hash(&self.db, path, size, modified_ms, sha256);
        }
    }
}

// ---------------------------------------------------------------------------
// Deletion log
// ---------------------------------------------------------------------------

/// Parameters of a deletion-log entry.
pub struct DeletionLogInput<'a> {
    pub path: &'a str,
    pub size: u64,
    pub sha256: Option<&'a str>,
    pub scan_id: Option<&'a str>,
    pub method: &'a str,
    pub success: bool,
    pub message: Option<&'a str>,
}

pub fn log_deletion(db: &Database, input: DeletionLogInput<'_>) -> AppResult<()> {
    db.with_connection(|connection| {
        connection.execute(
            "INSERT INTO deletion_log (deleted_at, path, size, sha256, scan_id, method, success, message)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                now_millis(),
                input.path,
                to_i64(input.size),
                input.sha256,
                input.scan_id,
                input.method,
                input.success as i64,
                input.message
            ],
        )?;
        // The hash cache entry is stale after deletion.
        connection.execute("DELETE FROM hash_cache WHERE path=?1", [input.path])?;
        if input.success {
            if let Some(scan_id) = input.scan_id {
                prune_deleted_file(connection, scan_id, input.path)?;
            }
        }
        Ok(())
    })
}

/// Remove a deleted path from the persisted scan snapshot, so a reloaded scan
/// never lists a file that is gone. The counters on the `scans` row are
/// deliberately left alone: they record what the scan *found*, and History is a
/// log of past runs, not a live view of what is still on disk.
fn prune_deleted_file(
    connection: &mut rusqlite::Connection,
    scan_id: &str,
    path: &str,
) -> AppResult<()> {
    let group_ids: Vec<i64> = {
        let mut statement = connection
            .prepare("SELECT DISTINCT group_id FROM scan_files WHERE scan_id=?1 AND path=?2")?;
        let rows = statement.query_map(params![scan_id, path], |row| row.get::<_, i64>(0))?;
        rows.collect::<Result<Vec<_>, _>>()?
    };
    for group_id in group_ids {
        connection.execute(
            "DELETE FROM scan_files WHERE scan_id=?1 AND group_id=?2 AND path=?3",
            params![scan_id, group_id, path],
        )?;

        let remaining: i64 = connection.query_row(
            "SELECT COUNT(*) FROM scan_files WHERE group_id=?1",
            [group_id],
            |row| row.get(0),
        )?;
        if remaining <= 1 {
            // A group with one copy left is not a duplicate group any more.
            connection.execute("DELETE FROM scan_files WHERE group_id=?1", [group_id])?;
            connection.execute("DELETE FROM scan_groups WHERE id=?1", [group_id])?;
        } else {
            let size: i64 = connection.query_row(
                "SELECT size FROM scan_groups WHERE id=?1",
                [group_id],
                |row| row.get(0),
            )?;
            connection.execute(
                "UPDATE scan_groups SET wasted=?1 WHERE id=?2",
                params![size * (remaining - 1), group_id],
            )?;
        }
    }

    Ok(())
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeletionLogEntry {
    pub id: i64,
    pub deleted_at: i64,
    pub path: String,
    pub size: u64,
    pub sha256: Option<String>,
    pub scan_id: Option<String>,
    pub method: String,
    pub success: bool,
    pub message: Option<String>,
}

pub fn list_deletions(db: &Database, limit: u32) -> AppResult<Vec<DeletionLogEntry>> {
    db.with_connection(|connection| {
        let mut statement = connection.prepare(
            "SELECT id, deleted_at, path, size, sha256, scan_id, method, success, message
             FROM deletion_log ORDER BY deleted_at DESC LIMIT ?1",
        )?;
        let entries = statement
            .query_map([limit], |row| {
                Ok(DeletionLogEntry {
                    id: row.get(0)?,
                    deleted_at: row.get(1)?,
                    path: row.get(2)?,
                    size: from_u64(row.get::<_, i64>(3)?),
                    sha256: row.get(4)?,
                    scan_id: row.get(5)?,
                    method: row.get(6)?,
                    success: row.get::<_, i64>(7)? != 0,
                    message: row.get(8)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(entries)
    })
}

pub fn clear_deletions(db: &Database) -> AppResult<()> {
    db.with_connection(|connection| {
        connection.execute("DELETE FROM deletion_log", [])?;
        Ok(())
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::scan_result::{ScanResult, ScanStats};
    use crate::models::settings::MAX_SIMILAR_THRESHOLD;

    fn sample_result(scan_id: &str) -> ScanResult {
        let file = |path: &str, name: &str| FileEntry {
            id: 0,
            path: path.to_string(),
            name: name.to_string(),
            size: 100,
            created_ms: Some(1),
            modified_ms: Some(2),
            extension: "txt".into(),
            sha256: Some("abc".into()),
            phash: None,
        };
        ScanResult {
            scan_id: scan_id.to_string(),
            started_at: 1,
            finished_at: 2,
            options: ScanOptions {
                paths: vec!["C:\\data".into()],
                ..ScanOptions::default()
            },
            stats: ScanStats {
                files_scanned: 10,
                files_hashed: 4,
                bytes_scanned: 1000,
                duplicate_groups: 1,
                duplicate_files: 2,
                wasted_bytes: 100,
                errors: 0,
            },
            groups: vec![DuplicateGroup::new(
                "abc".into(),
                vec![file("a/x.txt", "x.txt"), file("b/x.txt", "x.txt")],
            )],
            errors: vec![],
        }
    }

    #[test]
    fn scan_roundtrip() {
        let db = Database::open_in_memory().unwrap();
        let result = sample_result("scan-1");
        insert_running_scan(&db, "scan-1", &result.options).unwrap();
        mark_scan_finished(&db, "scan-1", "completed", Some(&result)).unwrap();

        let loaded = load_scan_result(&db, "scan-1").unwrap();
        assert_eq!(loaded.groups.len(), 1);
        assert_eq!(loaded.groups[0].files.len(), 2);
        assert_eq!(loaded.stats.wasted_bytes, 100);
        assert_eq!(loaded.options.paths, vec!["C:\\data".to_string()]);

        let records = list_scans(&db, 10).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].status, "completed");
        assert_eq!(records[0].duplicate_groups, 1);
    }

    /// History is a log of what each scan found. Deleting files afterwards must
    /// not rewrite that log — it only prunes the file rows used to re-open the
    /// results.
    #[test]
    fn deleting_files_does_not_rewrite_scan_statistics() {
        let db = Database::open_in_memory().unwrap();
        let result = sample_result("scan-1");
        insert_running_scan(&db, "scan-1", &result.options).unwrap();
        mark_scan_finished(&db, "scan-1", "completed", Some(&result)).unwrap();

        for path in ["a/x.txt", "b/x.txt"] {
            log_deletion(
                &db,
                DeletionLogInput {
                    path,
                    size: 50,
                    sha256: Some("abc"),
                    scan_id: Some("scan-1"),
                    method: "recycle_bin",
                    success: true,
                    message: None,
                },
            )
            .unwrap();
        }

        let record = list_scans(&db, 10).unwrap().remove(0);
        assert_eq!(record.duplicate_groups, 1, "the scan found one group");
        assert_eq!(
            record.duplicate_files, 2,
            "the scan found two duplicate files"
        );
        assert_eq!(record.wasted_bytes, 100, "reclaimable space at scan time");

        // Both copies are gone, so the reloaded result has no groups left.
        let reloaded = load_scan_result(&db, "scan-1").unwrap();
        assert!(reloaded.groups.is_empty());
    }

    #[test]
    fn unknown_scan_is_not_found() {
        let db = Database::open_in_memory().unwrap();
        assert!(matches!(
            load_scan_result(&db, "nope"),
            Err(AppError::ScanNotFound(_))
        ));
    }

    #[test]
    fn settings_roundtrip() {
        let db = Database::open_in_memory().unwrap();
        let settings = AppSettings {
            similar_photos_threshold: 12,
            min_file_size: 4096,
            excluded_dir_names: vec!["node_modules".into(), "custom".into()],
            ..AppSettings::default()
        };
        save_settings(&db, &settings).unwrap();

        let loaded = load_settings(&db).unwrap();
        assert_eq!(loaded.similar_photos_threshold, 12);
        assert_eq!(loaded.min_file_size, 4096);
        assert_eq!(loaded.excluded_dir_names, settings.excluded_dir_names);
    }

    #[test]
    fn out_of_range_settings_are_sanitised_before_persisting() {
        let db = Database::open_in_memory().unwrap();
        let settings = AppSettings {
            similar_photos_threshold: 99,
            ..AppSettings::default()
        };
        save_settings(&db, &settings).unwrap();

        let stored: String = db
            .with_connection(|connection| {
                Ok(connection.query_row(
                    "SELECT value FROM settings WHERE key = 'similar_photos_threshold'",
                    [],
                    |row| row.get(0),
                )?)
            })
            .unwrap();
        let loaded = load_settings(&db).unwrap();

        assert_eq!(stored, MAX_SIMILAR_THRESHOLD.to_string());
        assert_eq!(loaded.similar_photos_threshold, MAX_SIMILAR_THRESHOLD);
    }

    #[test]
    fn hash_cache_respects_size_and_mtime() {
        let db = Database::open_in_memory().unwrap();
        store_hash(&db, "C:\\f.bin", 10, Some(5), "deadbeef");
        assert_eq!(
            lookup_hash(&db, "C:\\f.bin", 10, Some(5)),
            Some("deadbeef".into())
        );
        assert_eq!(lookup_hash(&db, "C:\\f.bin", 11, Some(5)), None);
        assert_eq!(lookup_hash(&db, "C:\\f.bin", 10, Some(6)), None);
        assert_eq!(clear_hash_cache(&db).unwrap(), 1);
    }

    #[test]
    fn deletion_log_counts() {
        let db = Database::open_in_memory().unwrap();
        log_deletion(
            &db,
            DeletionLogInput {
                path: "C:\\a.bin",
                size: 100,
                sha256: Some("h"),
                scan_id: Some("s"),
                method: "recycle_bin",
                success: true,
                message: None,
            },
        )
        .unwrap();
        log_deletion(
            &db,
            DeletionLogInput {
                path: "C:\\b.bin",
                size: 50,
                sha256: None,
                scan_id: None,
                method: "recycle_bin",
                success: false,
                message: Some("locked"),
            },
        )
        .unwrap();
        let entries = list_deletions(&db, 10).unwrap();
        assert_eq!(entries.len(), 2);
        assert!(entries.iter().any(|e| e.success));
        assert!(entries.iter().any(|e| !e.success));

        let stats = dashboard_stats(&db).unwrap();
        assert_eq!(stats.deleted_files, 1);
        assert_eq!(stats.deleted_bytes, 100);
    }

    #[test]
    fn history_trim_keeps_newest() {
        let db = Database::open_in_memory().unwrap();
        for index in 0..5 {
            let id = format!("scan-{index}");
            insert_scan_started(&db, &id, index as i64, &ScanOptions::default()).unwrap();
            mark_scan_finished(&db, &id, "completed", None).unwrap();
        }
        trim_history(&db, 2).unwrap();
        let records = list_scans(&db, 10).unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].scan_id, "scan-4");
    }

    #[test]
    fn deleting_scan_cascades() {
        let db = Database::open_in_memory().unwrap();
        let result = sample_result("scan-1");
        insert_running_scan(&db, "scan-1", &result.options).unwrap();
        mark_scan_finished(&db, "scan-1", "completed", Some(&result)).unwrap();
        delete_scan(&db, "scan-1").unwrap();
        assert!(list_scans(&db, 10).unwrap().is_empty());
        let files: i64 = db
            .with_connection(|c| {
                Ok(c.query_row("SELECT COUNT(*) FROM scan_files", [], |row| row.get(0))?)
            })
            .unwrap();
        assert_eq!(files, 0);
    }

    #[test]
    fn dashboard_reports_last_completed_scan() {
        let db = Database::open_in_memory().unwrap();
        let result = sample_result("scan-1");
        insert_running_scan(&db, "scan-1", &result.options).unwrap();
        mark_scan_finished(&db, "scan-1", "completed", Some(&result)).unwrap();

        let stats = dashboard_stats(&db).unwrap();
        assert_eq!(stats.total_scans, 1);
        assert_eq!(stats.files_scanned, 10);
        assert_eq!(stats.wasted_bytes, 100);
        assert_eq!(stats.recent_scans.len(), 1);
        assert!(stats.last_scan.is_some());
    }
}
