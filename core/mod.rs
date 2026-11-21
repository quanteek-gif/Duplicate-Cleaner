//! Core business logic: scanning, hashing, comparison, disk analysis.

pub mod analyzer;
pub mod disk_analyzer;
pub mod hasher;
pub mod image_analyzer;
pub mod image_comparator;
pub mod scanner_engine;

pub use scanner_engine::{NoopHashCache, ScanControl, ScanEngine};
