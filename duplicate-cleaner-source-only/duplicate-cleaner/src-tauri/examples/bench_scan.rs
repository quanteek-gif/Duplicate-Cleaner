//! Performance benchmark for the scanning engine.
//!
//! Usage:
//!   cargo run --release --example bench_scan -- <root> [--files N]
//!
//! Generates nothing by itself: point it at a prepared tree. Use
//! `--generate <dir> <count>` to create a synthetic corpus first:
//!
//!   cargo run --release --example bench_scan -- --generate /tmp/bench 200000
//!   cargo run --release --example bench_scan -- /tmp/bench

use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use duplicate_cleaner_lib::core::scanner_engine::{ProgressSink, ScanControl, ScanEngine};
use duplicate_cleaner_lib::database::repository::DbHashCache;
use duplicate_cleaner_lib::database::Database;
use duplicate_cleaner_lib::models::scan_result::{ScanOptions, ScanProgress};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("usage: bench_scan <root> | bench_scan --generate <dir> <count>");
        std::process::exit(2);
    }

    if args[0] == "--generate" {
        generate(
            Path::new(&args[1]),
            args.get(2).and_then(|n| n.parse().ok()).unwrap_or(100_000),
        );
        return;
    }

    let root = &args[0];
    let db = Database::open(std::path::Path::new("/tmp/bench-cache.sqlite3")).expect("db");
    let cache = Arc::new(DbHashCache { db, enabled: true });

    let last = Arc::new(AtomicU64::new(0));
    let sink: ProgressSink = Arc::new(move |progress: ScanProgress| {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        if now.saturating_sub(last.load(Ordering::Relaxed)) > 1000 {
            last.store(now, Ordering::Relaxed);
            println!(
                "  {:?} discovered={} hashed={}/{} errors={}",
                progress.phase,
                progress.files_discovered,
                progress.files_processed,
                progress.files_to_hash,
                progress.errors
            );
        }
    });

    let options = ScanOptions {
        paths: vec![root.clone()],
        min_size: Some(1),
        include_system_dirs: true,
        ..ScanOptions::default()
    };

    for run in 1..=2 {
        let started = Instant::now();
        let engine = ScanEngine::new(
            format!("bench-{run}"),
            options.clone(),
            ScanControl::new(),
            sink.clone(),
            cache.clone(),
            None,
        );
        let outcome = engine.run().expect("scan");
        let elapsed = started.elapsed();
        println!(
            "run {run}: {:.2}s | files={} hashed={} groups={} dupes={} wasted={} MiB | {:.0} files/s",
            elapsed.as_secs_f64(),
            outcome.stats.files_scanned,
            outcome.stats.files_hashed,
            outcome.stats.duplicate_groups,
            outcome.stats.duplicate_files,
            outcome.stats.wasted_bytes as f64 / (1024.0 * 1024.0),
            outcome.stats.files_scanned as f64 / elapsed.as_secs_f64().max(0.001),
        );
    }
}

/// Create `count` small files, ~30% of them are exact duplicates.
fn generate(root: &Path, count: usize) {
    let started = Instant::now();
    let mut payloads: Vec<Vec<u8>> = Vec::new();
    for index in 0..64 {
        let size = 256 + index * 97;
        payloads.push(vec![(index % 251) as u8; size]);
    }
    for index in 0..count {
        let dir = root.join(format!("d{:02}/s{:02}", index % 64, (index / 64) % 64));
        std::fs::create_dir_all(&dir).expect("dir");
        let payload = &payloads[(index % 21) % payloads.len()]; // duplicates across buckets
        let path = dir.join(format!("file-{index:06}.bin"));
        let mut file = std::fs::File::create(&path).expect("file");
        file.write_all(payload).expect("write");
    }
    println!(
        "generated {count} files in {:.2}s at {}",
        started.elapsed().as_secs_f64(),
        root.display()
    );
}
