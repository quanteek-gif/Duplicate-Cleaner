//! Database access layer: a single shared connection behind a mutex.
//!
//! All access is synchronous and short; scanners use their own pool and only
//! touch the DB through [`Database::with_connection`] under a lock (SQLite
//! connections are not `Sync`, so one writer connection is the simplest
//! correct design here and is fast enough: writes are batched in transactions).

use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};

use rusqlite::Connection;

use crate::utils::error::AppError;
use crate::utils::error::AppResult;

pub mod migrations;
pub mod repository;

pub struct Database {
    connection: Mutex<Connection>,
}

impl Database {
    pub fn open(path: &Path) -> AppResult<Arc<Self>> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let connection = Connection::open(path)?;
        migrations::run(&connection)?;
        Ok(Arc::new(Self {
            connection: Mutex::new(connection),
        }))
    }

    pub fn open_in_memory() -> AppResult<Arc<Self>> {
        let connection = Connection::open_in_memory()?;
        migrations::run(&connection)?;
        Ok(Arc::new(Self {
            connection: Mutex::new(connection),
        }))
    }

    /// Run a closure with exclusive access to the connection.
    pub fn with_connection<T>(
        &self,
        f: impl FnOnce(&mut Connection) -> AppResult<T>,
    ) -> AppResult<T> {
        let mut guard: MutexGuard<'_, Connection> = self
            .connection
            .lock()
            .map_err(|_| AppError::other("database lock poisoned"))?;
        f(&mut guard)
    }
}
