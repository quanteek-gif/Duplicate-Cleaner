//! Logging setup: structured logs to stdout and a rotating file in the
//! application log directory.

use std::path::PathBuf;
use tracing_appender::non_blocking::WorkerGuard;

use crate::utils::error::AppResult;

/// Keeps the non-blocking log writer alive for the whole program lifetime.
pub struct LoggerGuard {
    _file: WorkerGuard,
}

/// Tauri-managed holder so the guard lives as long as the application.
pub struct LoggerGuardHolder {
    _guard: Option<LoggerGuard>,
}

impl LoggerGuardHolder {
    pub fn new(guard: Option<LoggerGuard>) -> Self {
        Self { _guard: guard }
    }
}

/// Initialise tracing with console + rolling file output.
pub fn init(log_dir: PathBuf) -> AppResult<LoggerGuard> {
    std::fs::create_dir_all(&log_dir)?;

    let env_filter = tracing_subscriber::EnvFilter::try_from_env("DUPLICATE_CLEANER_LOG")
        .unwrap_or_else(|_| {
            tracing_subscriber::EnvFilter::new("info,warn,duplicate_cleaner_lib=debug")
        });

    let file_appender = tracing_appender::rolling::daily(&log_dir, "duplicate-cleaner.log");
    let (file_writer, guard) = tracing_appender::non_blocking(file_appender);

    use tracing_subscriber::prelude::*;
    tracing_subscriber::registry()
        .with(env_filter)
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(file_writer)
                .with_ansi(false)
                .with_target(true),
        )
        .with(tracing_subscriber::fmt::layer().with_target(false))
        .try_init()
        .ok();

    tracing::info!(?log_dir, "logging initialised");
    Ok(LoggerGuard { _file: guard })
}
