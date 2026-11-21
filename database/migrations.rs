//! SQLite migrations executed on startup (`user_version` pragma based).

use rusqlite::Connection;

use crate::utils::error::AppResult;

pub const SCHEMA_VERSION: i64 = 1;

const MIGRATION_V1: &str = r#"
CREATE TABLE IF NOT EXISTS scans (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    scan_id           TEXT    NOT NULL UNIQUE,
    started_at        INTEGER NOT NULL,
    finished_at       INTEGER,
    status            TEXT    NOT NULL,
    paths             TEXT    NOT NULL,
    files_scanned     INTEGER NOT NULL DEFAULT 0,
    files_hashed      INTEGER NOT NULL DEFAULT 0,
    duplicate_groups  INTEGER NOT NULL DEFAULT 0,
    duplicate_files   INTEGER NOT NULL DEFAULT 0,
    wasted_bytes      INTEGER NOT NULL DEFAULT 0,
    errors            INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS scan_groups (
    id        INTEGER PRIMARY KEY AUTOINCREMENT,
    scan_id   TEXT    NOT NULL REFERENCES scans(scan_id) ON DELETE CASCADE,
    hash      TEXT    NOT NULL,
    size      INTEGER NOT NULL,
    wasted    INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_scan_groups_scan ON scan_groups(scan_id);

CREATE TABLE IF NOT EXISTS scan_files (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    scan_id       TEXT    NOT NULL REFERENCES scans(scan_id) ON DELETE CASCADE,
    group_id      INTEGER NOT NULL REFERENCES scan_groups(id) ON DELETE CASCADE,
    path          TEXT    NOT NULL,
    name          TEXT    NOT NULL,
    size          INTEGER NOT NULL,
    created_ms    INTEGER,
    modified_ms   INTEGER,
    extension     TEXT    NOT NULL DEFAULT '',
    sha256        TEXT,
    phash         TEXT
);
CREATE INDEX IF NOT EXISTS idx_scan_files_scan ON scan_files(scan_id);
CREATE INDEX IF NOT EXISTS idx_scan_files_group ON scan_files(group_id);

CREATE TABLE IF NOT EXISTS hash_cache (
    path        TEXT    PRIMARY KEY,
    size        INTEGER NOT NULL,
    modified_ms INTEGER,
    sha256      TEXT    NOT NULL,
    cached_at   INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_hash_cache_cached_at ON hash_cache(cached_at);

CREATE TABLE IF NOT EXISTS settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS deletion_log (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    deleted_at  INTEGER NOT NULL,
    path        TEXT    NOT NULL,
    size        INTEGER NOT NULL,
    sha256      TEXT,
    scan_id     TEXT,
    method      TEXT    NOT NULL,
    success     INTEGER NOT NULL,
    message     TEXT
);
CREATE INDEX IF NOT EXISTS idx_deletion_log_deleted_at ON deletion_log(deleted_at);
"#;

/// Apply pending migrations.
pub fn run(connection: &Connection) -> AppResult<()> {
    connection.pragma_update(None, "journal_mode", "WAL")?;
    connection.pragma_update(None, "synchronous", "NORMAL")?;
    connection.pragma_update(None, "foreign_keys", "ON")?;

    let version: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version < 1 {
        connection.execute_batch(MIGRATION_V1)?;
    }
    if version < SCHEMA_VERSION {
        connection.pragma_update(None, "user_version", SCHEMA_VERSION)?;
        tracing::info!(from = version, to = SCHEMA_VERSION, "database migrated");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_are_idempotent() {
        let connection = Connection::open_in_memory().unwrap();
        run(&connection).unwrap();
        run(&connection).unwrap();
        let version: i64 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, SCHEMA_VERSION);

        // all tables exist
        for table in [
            "scans",
            "scan_groups",
            "scan_files",
            "hash_cache",
            "settings",
            "deletion_log",
        ] {
            let count: i64 = connection
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                    [table],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(count, 1, "missing table {table}");
        }
    }
}
