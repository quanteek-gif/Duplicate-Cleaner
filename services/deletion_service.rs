//! Safe deletion: verify, protect the last copy, move to the Recycle Bin and
//! report per-file results. Never deletes anything without an explicit list
//! coming from the UI.
//!
//! Safety guarantees:
//! * the path must still exist and be a regular file (no symlinks, no dirs);
//! * when a hash is known, the file is re-hashed and compared — a file that
//!   changed after the scan is never removed;
//! * for each duplicate group at least one copy must remain on disk;
//! * files go to the Recycle Bin on Windows (restorable), never `std::fs::remove`
//!   unless the user explicitly disabled the recycle bin;
//! * protected system locations are always rejected.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::core::hasher;
use crate::database::repository::{self, DeletionLogInput};
use crate::database::Database;
use crate::models::settings::AppSettings;
use crate::utils::error::{AppError, AppResult};
use crate::utils::file_utils::{
    move_to_trash, normalize_for_security, remove_file, rooted_segments, symlink_metadata,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeletionRequest {
    pub paths: Vec<String>,
    /// scan the files belong to (for the history log)
    pub scan_id: Option<String>,
    /// hash per path from the scan results, used to re-verify content
    #[serde(default)]
    pub expected_hashes: HashMap<String, String>,
    /// all files known per duplicate group (to protect the last copy)
    #[serde(default)]
    pub group_members: Vec<Vec<String>>,
    /// Consent flag. `delete()` refuses to touch the disk when this is false —
    /// the caller must have shown the confirmation dialog (or the user
    /// explicitly turned it off in Settings, in which case the UI sends true).
    /// `plan()` ignores the flag so a preview can be built before consent.
    #[serde(default = "default_true")]
    pub confirmed: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeletionOutcome {
    pub path: String,
    pub success: bool,
    pub size: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeletionReport {
    pub deleted: u64,
    pub failed: u64,
    pub freed_bytes: u64,
    pub restored_to_trash: bool,
    pub outcomes: Vec<DeletionOutcome>,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeletionPlan {
    pub allowed: Vec<DeletionEntry>,
    pub rejected: Vec<DeletionOutcome>,
    pub total_bytes: u64,
    pub uses_recycle_bin: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeletionEntry {
    pub path: String,
    pub size: u64,
    pub name: String,
}

pub struct DeletionService {
    db: Arc<Database>,
}

impl DeletionService {
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }

    fn settings(&self) -> AppSettings {
        repository::load_settings(&self.db).unwrap_or_default()
    }

    /// Record a file that was *not* deleted because a safety check refused it.
    ///
    /// The UI shows these as a toast that disappears; the deletion log is the
    /// only durable record that the app protected the user's data, so a refused
    /// delete has to leave a trace just like a successful one. Failures to log
    /// are ignored on purpose: a logging problem must never change the outcome
    /// of a safety check.
    fn log_refusal(&self, request: &DeletionRequest, entry: &DeletionEntry, reason: &str) {
        let method = if self.settings().use_recycle_bin {
            "recycle_bin"
        } else {
            "permanent"
        };
        let _ = repository::log_deletion(
            &self.db,
            DeletionLogInput {
                path: &entry.path,
                size: entry.size,
                sha256: request.expected_hashes.get(&entry.path).map(String::as_str),
                scan_id: request.scan_id.as_deref(),
                method,
                success: false,
                message: Some(reason),
            },
        );
    }

    /// Validate a deletion request *without* touching the disk.
    ///
    /// `protected_keep`: optional set of paths that must never be deleted
    /// (e.g. the copy the user chose to keep). For every duplicate group one
    /// existing member is automatically kept: the one chosen by the user when
    /// present, otherwise the first existing member in path order.
    pub fn plan(&self, request: &DeletionRequest) -> DeletionPlan {
        let settings = self.settings();
        let mut allowed: Vec<DeletionEntry> = Vec::new();
        let mut rejected: Vec<DeletionOutcome> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();

        let requested: HashSet<&str> = request.paths.iter().map(String::as_str).collect();

        // Choose the protected "keeper" for every group.
        let mut protected: HashSet<String> = HashSet::new();
        for members in &request.group_members {
            if members.len() < 2 {
                continue; // not a duplicate group — single files are not protected
            }
            let existing: Vec<&String> = members
                .iter()
                .filter(|member| Path::new(member.as_str()).is_file())
                .collect();
            if existing.is_empty() {
                continue;
            }
            let all_requested = existing
                .iter()
                .all(|member| requested.contains(member.as_str()));
            if all_requested {
                // Keep the first member in sorted order; it will be rejected below.
                let mut sorted = existing.clone();
                sorted.sort();
                if let Some(keeper) = sorted.first() {
                    protected.insert((*keeper).clone());
                }
            }
        }

        for path_text in &request.paths {
            let as_path = PathBuf::from(path_text);

            if path_text.trim().is_empty() {
                rejected.push(rejected_outcome(path_text, 0, "empty path"));
                continue;
            }
            if !seen.insert(path_text.clone()) {
                continue; // duplicate entry in the request
            }

            let metadata = match symlink_metadata(&as_path) {
                Ok(metadata) => metadata,
                Err(_) => {
                    rejected.push(rejected_outcome(
                        path_text,
                        0,
                        "file no longer exists (it may have been removed or moved)",
                    ));
                    continue;
                }
            };
            let size = metadata.len();

            if metadata.file_type().is_symlink() {
                rejected.push(rejected_outcome(
                    path_text,
                    size,
                    "symbolic links are never deleted",
                ));
                continue;
            }
            if !metadata.is_file() {
                rejected.push(rejected_outcome(path_text, size, "not a regular file"));
                continue;
            }
            if protected.contains(path_text) {
                rejected.push(rejected_outcome(
                    path_text,
                    size,
                    "refusing to delete the last copy of a file",
                ));
                continue;
            }
            if is_protected_path(&as_path) {
                rejected.push(rejected_outcome(
                    path_text,
                    size,
                    "path is in a protected system location",
                ));
                continue;
            }

            let name = as_path
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_default();
            allowed.push(DeletionEntry {
                path: path_text.clone(),
                size,
                name,
            });
        }

        let total_bytes = allowed.iter().map(|entry| entry.size).sum();
        DeletionPlan {
            allowed,
            rejected,
            total_bytes,
            uses_recycle_bin: settings.use_recycle_bin,
        }
    }

    /// Move the requested files to the Recycle Bin after re-verification.
    ///
    /// Refuses to delete anything when [`DeletionRequest::confirmed`] is false:
    /// consent is enforced in Rust, not only in the dialog, so a bug or a
    /// third-party caller can never remove files without an explicit
    /// confirmation.
    pub fn delete(&self, request: &DeletionRequest) -> AppResult<DeletionReport> {
        if !request.confirmed {
            tracing::warn!(
                paths = request.paths.len(),
                "deletion refused: request was not confirmed"
            );
            return Err(AppError::invalid("deletion was not confirmed by the user"));
        }
        let started = std::time::Instant::now();
        let settings = self.settings();
        let plan = self.plan(request);

        let mut outcomes: Vec<DeletionOutcome> = plan.rejected;

        for entry in plan.allowed {
            let path = PathBuf::from(&entry.path);

            // Re-verify the content when a hash is known and verification is on.
            if settings.verify_hash_before_delete {
                if let Some(expected) = request.expected_hashes.get(&entry.path) {
                    match hasher::hash_file(&path) {
                        Ok((actual, _)) => {
                            if !actual.eq_ignore_ascii_case(expected) {
                                let reason =
                                    "file changed after the scan (hash mismatch) — not deleted";
                                // A refusal driven by a safety check belongs in the
                                // deletion log too: the toast disappears, the audit
                                // trail should not.
                                self.log_refusal(request, &entry, reason);
                                outcomes.push(DeletionOutcome {
                                    path: entry.path.clone(),
                                    success: false,
                                    size: entry.size,
                                    reason: Some(reason.into()),
                                });
                                continue;
                            }
                        }
                        Err(_) => {
                            let reason = "file disappeared or is not readable — not deleted";
                            self.log_refusal(request, &entry, reason);
                            outcomes.push(DeletionOutcome {
                                path: entry.path.clone(),
                                success: false,
                                size: entry.size,
                                reason: Some(reason.into()),
                            });
                            continue;
                        }
                    }
                }
            }

            let method = if settings.use_recycle_bin {
                "recycle_bin"
            } else {
                "permanent"
            };
            let result = if settings.use_recycle_bin {
                move_to_trash(&path)
            } else {
                remove_file(&path).map_err(|error| error.to_string())
            };

            match result {
                Ok(()) => {
                    let _ = repository::log_deletion(
                        &self.db,
                        DeletionLogInput {
                            path: &entry.path,
                            size: entry.size,
                            sha256: request.expected_hashes.get(&entry.path).map(String::as_str),
                            scan_id: request.scan_id.as_deref(),
                            method,
                            success: true,
                            message: None,
                        },
                    );
                    outcomes.push(DeletionOutcome {
                        path: entry.path,
                        success: true,
                        size: entry.size,
                        reason: None,
                    });
                }
                Err(message) => {
                    let _ = repository::log_deletion(
                        &self.db,
                        DeletionLogInput {
                            path: &entry.path,
                            size: entry.size,
                            sha256: None,
                            scan_id: request.scan_id.as_deref(),
                            method,
                            success: false,
                            message: Some(&message),
                        },
                    );
                    outcomes.push(DeletionOutcome {
                        path: entry.path,
                        success: false,
                        size: entry.size,
                        reason: Some(message),
                    });
                }
            }
        }

        let deleted = outcomes.iter().filter(|outcome| outcome.success).count() as u64;
        let failed = outcomes.len() as u64 - deleted;
        let freed_bytes = outcomes
            .iter()
            .filter(|outcome| outcome.success)
            .map(|outcome| outcome.size)
            .sum();

        tracing::info!(deleted, failed, freed_bytes, "deletion finished");

        Ok(DeletionReport {
            deleted,
            failed,
            freed_bytes,
            restored_to_trash: settings.use_recycle_bin,
            outcomes,
            duration_ms: started.elapsed().as_millis() as u64,
        })
    }
}

fn rejected_outcome(path: &str, size: u64, reason: &str) -> DeletionOutcome {
    DeletionOutcome {
        path: path.to_string(),
        success: false,
        size,
        reason: Some(reason.to_string()),
    }
}

/// Directory names that belong to Windows or to an installed program.
///
/// These are matched on **any** volume: Windows is usually on `C:`, but a
/// machine can be installed with the system on `D:` or boot from another disk,
/// and a `D:\Windows\System32` file must be just as undeletable as
/// `C:\Windows\System32`.
const PROTECTED_DIR_NAMES: &[&str] = &[
    "windows",
    "program files",
    "program files (x86)",
    "programdata",
    "$recycle.bin",
    "system volume information",
    "recovery",
    "boot",
];

/// Path segments that only ever make sense as a prefix of a system path
/// (matched with a trailing separator so `C:\Program Files Stuff` is allowed).
const PROTECTED_LEAF_SEGMENTS: &[&str] = &["system volume information", "$recycle.bin"];

/// `true` when the file lives in a protected system location.
///
/// The check is purely lexical (no canonicalisation) so it also covers paths
/// that cannot be resolved anymore, and it normalises `\`, `/`, `\\?\`
/// verbatim prefixes, duplicated separators, `..` segments and case — the
/// tricks a path can use to slip past a prefix comparison.
fn is_protected_path(path: &Path) -> bool {
    let normalized = normalize_for_security(&path.to_string_lossy());
    let body = rooted_segments(&normalized);
    let Some(first) = body.first() else {
        return false;
    };

    // `D:\Windows`, `E:\Program Files`, ... — the top-level system folder.
    if PROTECTED_DIR_NAMES.contains(first) {
        return true;
    }

    // System folders that only ever exist as the first component of a path
    // are also rejected at any depth, matching the old prefix behaviour.
    body.iter()
        .take(body.len().saturating_sub(1))
        .any(|segment| PROTECTED_LEAF_SEGMENTS.contains(segment))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn db() -> Arc<Database> {
        Database::open_in_memory().unwrap()
    }

    fn write_file(dir: &Path, name: &str, content: &[u8]) -> String {
        let path = dir.join(name);
        let mut file = std::fs::File::create(&path).unwrap();
        file.write_all(content).unwrap();
        path.to_string_lossy().to_string()
    }

    #[test]
    fn plan_rejects_missing_files() {
        let service = DeletionService::new(db());
        let request = DeletionRequest {
            paths: vec!["/not/existing/file.bin".into()],
            scan_id: None,
            expected_hashes: HashMap::new(),
            group_members: vec![],
            confirmed: true,
        };
        let plan = service.plan(&request);
        assert!(plan.allowed.is_empty());
        assert_eq!(plan.rejected.len(), 1);
        assert!(plan.rejected[0]
            .reason
            .as_deref()
            .unwrap_or("")
            .contains("no longer exists"));
    }

    #[test]
    fn plan_rejects_directories() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("sub");
        std::fs::create_dir(&sub).unwrap();
        let service = DeletionService::new(db());
        let request = DeletionRequest {
            paths: vec![sub.to_string_lossy().to_string()],
            scan_id: None,
            expected_hashes: HashMap::new(),
            group_members: vec![],
            confirmed: true,
        };
        let plan = service.plan(&request);
        assert!(plan.allowed.is_empty());
        assert_eq!(plan.rejected.len(), 1);
        assert!(plan.rejected[0]
            .reason
            .as_deref()
            .unwrap_or("")
            .contains("regular file"));
    }

    #[test]
    fn plan_protects_last_copy_of_group() {
        let dir = tempfile::tempdir().unwrap();
        let a = write_file(dir.path(), "a.txt", b"same bytes");
        let b = write_file(dir.path(), "b.txt", b"same bytes");
        let service = DeletionService::new(db());

        // Selecting every copy: exactly one must be kept.
        let request = DeletionRequest {
            paths: vec![a.clone(), b.clone()],
            scan_id: None,
            expected_hashes: HashMap::new(),
            group_members: vec![vec![a.clone(), b.clone()]],
            confirmed: true,
        };
        let plan = service.plan(&request);
        assert_eq!(plan.allowed.len(), 1, "one copy is allowed");
        assert_eq!(plan.rejected.len(), 1, "one copy is protected");
        let kept = if plan.allowed[0].path == a {
            b.clone()
        } else {
            a.clone()
        };
        assert!(plan.rejected.iter().any(|r| r.path == kept));
        assert!(plan.rejected[0]
            .reason
            .as_deref()
            .unwrap_or("")
            .contains("last copy"));

        // Deleting a single copy is fine.
        let request = DeletionRequest {
            paths: vec![a.clone()],
            scan_id: None,
            expected_hashes: HashMap::new(),
            group_members: vec![vec![a.clone(), b.clone()]],
            confirmed: true,
        };
        let plan = service.plan(&request);
        assert_eq!(plan.allowed.len(), 1);
        assert!(plan.rejected.is_empty());
    }

    #[test]
    fn plan_ignores_already_missing_group_members_for_protection() {
        let dir = tempfile::tempdir().unwrap();
        let a = write_file(dir.path(), "a.txt", b"same");
        let b = dir.path().join("b.txt").to_string_lossy().to_string();
        // b never existed
        let service = DeletionService::new(db());
        let request = DeletionRequest {
            paths: vec![a.clone()],
            scan_id: None,
            expected_hashes: HashMap::new(),
            group_members: vec![vec![a.clone(), b]],
            confirmed: true,
        };
        let plan = service.plan(&request);
        // only one existing copy -> protected
        assert!(plan.allowed.is_empty());
        assert_eq!(plan.rejected.len(), 1);
    }

    #[test]
    fn delete_moves_file_and_logs() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_file(dir.path(), "junk.bin", b"junk content");
        let database = db();
        let service = DeletionService::new(database.clone());
        let request = DeletionRequest {
            paths: vec![path.clone()],
            scan_id: Some("s1".into()),
            expected_hashes: HashMap::new(),
            group_members: vec![],
            confirmed: true,
        };
        let report = service.delete(&request).unwrap();
        assert_eq!(report.deleted, 1);
        assert_eq!(report.failed, 0);
        assert!(report.freed_bytes > 0);
        assert!(!Path::new(&path).exists());

        let log = repository::list_deletions(&database, 10).unwrap();
        assert_eq!(log.len(), 1);
        assert!(log[0].success);
    }

    #[test]
    fn delete_refuses_changed_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_file(dir.path(), "changed.bin", b"original");
        let expected = hasher::hash_file(Path::new(&path)).unwrap().0;
        std::fs::write(&path, b"modified later").unwrap();

        let service = DeletionService::new(db());
        let mut hashes = HashMap::new();
        hashes.insert(path.clone(), expected);

        let request = DeletionRequest {
            paths: vec![path.clone()],
            scan_id: None,
            expected_hashes: hashes,
            group_members: vec![],
            confirmed: true,
        };
        let report = service.delete(&request).unwrap();
        assert_eq!(report.deleted, 0);
        assert_eq!(report.failed, 1);
        assert!(Path::new(&path).exists(), "changed file must survive");
        assert!(report.outcomes[0]
            .reason
            .as_deref()
            .unwrap_or("")
            .contains("hash mismatch"));

        // The refusal must leave a durable audit trail, not only a toast.
        let log = repository::list_deletions(&service.db, 10).unwrap();
        assert_eq!(log.len(), 1, "refused delete must be logged");
        assert!(!log[0].success);
        assert_eq!(log[0].path, path);
        assert!(log[0]
            .message
            .as_deref()
            .unwrap_or("")
            .contains("hash mismatch"));
    }

    #[test]
    fn delete_skips_already_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_file(dir.path(), "temp.bin", b"x");
        let service = DeletionService::new(db());
        std::fs::remove_file(&path).unwrap();

        let request = DeletionRequest {
            paths: vec![path],
            scan_id: None,
            expected_hashes: HashMap::new(),
            group_members: vec![],
            confirmed: true,
        };
        let report = service.delete(&request).unwrap();
        assert_eq!(report.deleted, 0);
        assert_eq!(report.failed, 1);
    }

    #[test]
    fn delete_refuses_an_unconfirmed_request() {
        let dir = tempfile::tempdir().unwrap();
        let a = write_file(dir.path(), "a.txt", b"payload");
        let b = write_file(dir.path(), "b.txt", b"payload");
        let service = DeletionService::new(db());
        let request = DeletionRequest {
            paths: vec![b.clone()],
            scan_id: None,
            expected_hashes: HashMap::new(),
            group_members: vec![vec![a.clone(), b.clone()]],
            confirmed: false,
        };

        let error = service.delete(&request).expect_err("must refuse");
        assert!(
            error.to_string().contains("not confirmed"),
            "unexpected error: {error}"
        );
        assert!(
            Path::new(&b).exists(),
            "file must survive an unconfirmed request"
        );
        assert!(Path::new(&a).exists());
    }

    #[test]
    fn delete_proceeds_when_confirmed() {
        let dir = tempfile::tempdir().unwrap();
        let a = write_file(dir.path(), "a.txt", b"payload");
        let b = write_file(dir.path(), "b.txt", b"payload");
        let service = DeletionService::new(db());
        let request = DeletionRequest {
            paths: vec![b.clone()],
            scan_id: None,
            expected_hashes: HashMap::new(),
            group_members: vec![vec![a.clone(), b.clone()]],
            confirmed: true,
        };

        let report = service.delete(&request).expect("delete");
        assert_eq!(report.deleted, 1, "{:?}", report.outcomes);
        assert!(!Path::new(&b).exists());
        assert!(Path::new(&a).exists(), "the kept copy must survive");
    }

    #[test]
    fn unconfirmed_request_still_protects_last_copy() {
        // `confirmed = false` only means the UI skipped its own dialog; every
        // safety guard must still apply.
        let dir = tempfile::tempdir().unwrap();
        let a = write_file(dir.path(), "a.txt", b"same");
        let b = write_file(dir.path(), "b.txt", b"same");
        let service = DeletionService::new(db());
        let request = DeletionRequest {
            paths: vec![a.clone(), b.clone()],
            scan_id: None,
            expected_hashes: HashMap::new(),
            group_members: vec![vec![a.clone(), b.clone()]],
            confirmed: false,
        };
        let plan = service.plan(&request);
        assert_eq!(plan.allowed.len(), 1);
        assert_eq!(plan.rejected.len(), 1);
    }

    #[test]
    fn protected_system_paths_rejected() {
        assert!(is_protected_path(Path::new(
            "C:\\Windows\\System32\\kernel32.dll"
        )));
        assert!(is_protected_path(Path::new(
            "C:\\Program Files\\App\\a.dll"
        )));
        assert!(!is_protected_path(Path::new(
            "C:\\Users\\me\\Pictures\\a.jpg"
        )));
    }

    #[test]
    fn protection_covers_every_volume_not_just_c() {
        for path in [
            "D:\\Windows\\System32\\kernel32.dll",
            "E:\\Program Files\\App\\a.dll",
            "F:\\Program Files (x86)\\App\\a.dll",
            "d:\\programdata\\Microsoft\\x.log",
            "D:\\$Recycle.Bin\\S-1-5-21\\$R123.jpg",
            "D:\\System Volume Information\\WPSettings.dat",
            // forward slashes and mixed case
            "D:/WINDOWS/System32/drivers/etc/hosts",
            // verbatim prefix
            "\\\\?\\C:\\Windows\\notepad.exe",
            // UNC share
            "\\\\server\\share\\Windows\\System32\\a.dll",
        ] {
            assert!(
                is_protected_path(Path::new(path)),
                "{path} must be protected"
            );
        }
    }

    #[test]
    fn traversal_and_similar_names_cannot_escape_protection() {
        // `..` must not be able to walk out of (or into) a protected dir
        assert!(is_protected_path(Path::new(
            "C:\\Windows\\System32\\..\\notepad.exe"
        )));
        assert!(is_protected_path(Path::new(
            "D:\\temp\\..\\Windows\\notepad.exe"
        )));
        // duplicated separators
        assert!(is_protected_path(Path::new("C:\\\\Windows\\a.dll")));
        // look-alikes that are NOT system locations
        assert!(!is_protected_path(Path::new(
            "C:\\WindowsApps\\MyApp\\a.dll"
        )));
        assert!(!is_protected_path(Path::new(
            "D:\\Program Files Stuff\\a.dll"
        )));
        assert!(!is_protected_path(Path::new(
            "C:\\Users\\me\\Documents\\Windows\\notes.txt"
        )));
        // a user folder that merely contains the word "windows"
        assert!(!is_protected_path(Path::new("D:\\MyWindows\\a.jpg")));
    }
}
