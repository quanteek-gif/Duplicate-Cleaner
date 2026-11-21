//! Error types for the whole application.
//!
//! Every fallible operation returns [`AppResult`]. Errors implement
//! [`serde::Serialize`] so they can be sent to the frontend as plain strings.

use serde::{Serialize, Serializer};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("image error: {0}")]
    Image(#[from] image::ImageError),

    #[error("recycle bin error: {0}")]
    Trash(#[from] trash::Error),

    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("scan '{0}' not found")]
    ScanNotFound(String),

    #[error("a scan is already running")]
    ScanAlreadyRunning,

    #[error("the scan has been cancelled")]
    Cancelled,

    #[error("invalid argument: {0}")]
    InvalidArgument(String),

    #[error("access denied: {0}")]
    AccessDenied(String),

    #[error("{0}")]
    Other(String),
}

impl AppError {
    pub fn other(message: impl Into<String>) -> Self {
        Self::Other(message.into())
    }

    pub fn invalid(message: impl Into<String>) -> Self {
        Self::InvalidArgument(message.into())
    }
}

impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;
