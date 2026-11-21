//! Deciding which copy of a duplicate group is worth keeping.
//!
//! Every group must keep at least one copy; everything else is redundant. "Keep
//! the first path alphabetically" is deterministic but arbitrary — it will
//! happily keep `D:\Backups\2019\report.docx` and delete the working copy in
//! `Documents`. The analyzer ranks candidates with a few cheap, explainable
//! heuristics instead, so the UI can pre-select the redundant copies and state
//! *why* one copy was kept.
//!
//! The heuristics are deliberately conservative and only ever re-order copies:
//! the deletion service still independently protects the last copy of a group,
//! so a bad ranking can cost a wrong *suggestion*, never data.
//!
//! | Signal | Effect on score |
//! | --- | --- |
//! | path under a user content folder (Documents, Desktop, Pictures, …) | `+30` |
//! | path containing `backup`, `old`, `copy`, `temp`, `trash`, … | `-40` |
//! | deeper nesting (each level) | `-3` |
//! | newest modification time in the group | `+20` |

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::models::file::FileEntry;

/// Folder names that hold content the user actively works with.
const USER_FOLDERS: &[&str] = &[
    "documents",
    "desktop",
    "pictures",
    "photos",
    "downloads",
    "music",
    "videos",
    "movies",
];

/// Path segments that mark throw-away locations.
const JUNK_SEGMENTS: &[&str] = &[
    "backup",
    "backups",
    "old",
    "copy",
    "copies",
    "duplicate",
    "duplicates",
    "temp",
    "tmp",
    "trash",
    "recycle",
    "cached",
    "cache",
];

const USER_FOLDER_BONUS: i64 = 30;
const JUNK_PENALTY: i64 = -40;
const DEPTH_PENALTY: i64 = -3;
const NEWEST_BONUS: i64 = 20;

/// Why the analyzer proposes keeping this copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum KeepReason {
    /// The copy lives in a folder the user actually works in.
    UserFolder,
    /// The copy has the most recent modification time in the group.
    Newest,
    /// The copy sits closest to the root of the tree.
    TopLevel,
    /// Nothing distinguishing — the shortest path wins the tie-break.
    ShortestPath,
}

impl KeepReason {
    /// Short human readable explanation shown next to the kept file.
    pub fn describe(self) -> &'static str {
        match self {
            KeepReason::UserFolder => "inside your documents/pictures folder",
            KeepReason::Newest => "the most recently modified copy",
            KeepReason::TopLevel => "the least deeply nested copy",
            KeepReason::ShortestPath => "the shortest path",
        }
    }
}

/// One copy with its score and the reason that explains the recommendation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RankedCopy {
    pub path: String,
    pub score: i64,
    pub reason: KeepReason,
}

impl RankedCopy {
    pub fn to_entry(&self) -> RecommendedCopy {
        RecommendedCopy {
            path: self.path.clone(),
            score: self.score,
            reason: self.reason,
            explanation: self.reason.describe().to_string(),
        }
    }
}

/// Serializable recommendation sent to the UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecommendedCopy {
    pub path: String,
    pub score: i64,
    pub reason: KeepReason,
    /// Human readable sentence, e.g. "the most recently modified copy".
    pub explanation: String,
}

/// Split a path into lower-case segments, treating both separators alike.
fn segments(path: &str) -> Vec<String> {
    path.split(['/', '\\'])
        .filter(|segment| !segment.is_empty())
        .map(|segment| segment.to_ascii_lowercase())
        .collect()
}

/// Number of directory levels between the root and the file itself.
///
/// `D:\a.bin` → 0, `D:\x\a.bin` → 1, `D:\a\b\c\a.bin` → 3. Both the root
/// segment (drive letter or share) and the file name are excluded.
fn depth(path: &str) -> i64 {
    segments(path).len().saturating_sub(2) as i64
}

fn in_user_folder(parts: &[String]) -> bool {
    parts
        .iter()
        .any(|part| USER_FOLDERS.contains(&part.as_str()))
}

fn in_junk_folder(parts: &[String]) -> bool {
    parts.iter().any(|part| {
        JUNK_SEGMENTS.contains(&part.as_str())
            // `report (copy).docx` and friends: the *file name* is junk too.
            || JUNK_SEGMENTS
                .iter()
                .any(|junk| part.contains(&format!("({junk}")) || part.contains(&format!("_{junk}")))
    })
}

/// Rank every copy of a group, best candidate to keep first.
///
/// The comparison is fully deterministic: ties fall back to the shorter path,
/// then to the case-insensitive path, so two runs on the same input always
/// produce the same selection.
pub fn rank_copies(files: &[FileEntry]) -> Vec<RankedCopy> {
    let newest = files.iter().filter_map(|file| file.modified_ms).max();

    let mut ranked: Vec<RankedCopy> = files
        .iter()
        .map(|file| {
            let parts = segments(&file.path);
            let mut score = 0;
            let user_folder = in_user_folder(&parts);
            if user_folder {
                score += USER_FOLDER_BONUS;
            }
            if in_junk_folder(&parts) {
                score += JUNK_PENALTY;
            }
            let nesting = depth(&file.path);
            score += DEPTH_PENALTY * nesting;

            let is_newest =
                matches!((file.modified_ms, newest), (Some(time), Some(max)) if time == max);
            if is_newest {
                score += NEWEST_BONUS;
            }

            let reason = if user_folder {
                KeepReason::UserFolder
            } else if is_newest {
                KeepReason::Newest
            } else if nesting <= 1 {
                KeepReason::TopLevel
            } else {
                KeepReason::ShortestPath
            };

            RankedCopy {
                path: file.path.clone(),
                score,
                reason,
            }
        })
        .collect();

    ranked.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| a.path.len().cmp(&b.path.len()))
            .then_with(|| a.path.to_lowercase().cmp(&b.path.to_lowercase()))
            .then_with(|| a.path.cmp(&b.path))
    });
    ranked
}

/// The copy the analyzer recommends keeping, if the group is non-empty.
pub fn recommended_keeper(files: &[FileEntry]) -> Option<RecommendedCopy> {
    rank_copies(files).first().map(RankedCopy::to_entry)
}

/// Every path except the recommended keeper — i.e. the safe auto-selection.
pub fn redundant_copies(files: &[FileEntry]) -> Vec<String> {
    let mut ranked = rank_copies(files);
    if ranked.len() < 2 {
        return Vec::new();
    }
    ranked.remove(0);
    ranked.into_iter().map(|copy| copy.path).collect()
}

/// `true` when the path sits in a location the analyzer considers throw-away.
pub fn looks_like_backup(path: &str) -> bool {
    in_junk_folder(&segments(path))
}

/// Depth helper exposed for logging/diagnostics.
pub fn path_depth(path: &Path) -> i64 {
    depth(&path.to_string_lossy())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(path: &str, modified: Option<i64>) -> FileEntry {
        FileEntry {
            id: 0,
            path: path.to_string(),
            name: path
                .rsplit(['/', '\\'])
                .next()
                .unwrap_or_default()
                .to_string(),
            size: 1024,
            created_ms: None,
            modified_ms: modified,
            extension: "docx".to_string(),
            sha256: None,
            phash: None,
        }
    }

    #[test]
    fn working_copy_beats_backup_copy() {
        let files = vec![
            file(r"D:\Backups\2019\report.docx", Some(1_000)),
            file(r"C:\Users\Ann\Documents\report.docx", Some(500)),
        ];
        let keeper = recommended_keeper(&files).expect("keeper");
        assert_eq!(keeper.path, r"C:\Users\Ann\Documents\report.docx");
        assert_eq!(keeper.reason, KeepReason::UserFolder);
        assert!(keeper.explanation.contains("documents"));
        assert!(
            keeper.score > 0,
            "score {} should be positive",
            keeper.score
        );
    }

    #[test]
    fn newer_copy_wins_within_plain_folders() {
        let files = vec![
            file("D:/archive/a.bin", Some(100)),
            file("D:/archive/b.bin", Some(900)),
        ];
        let keeper = recommended_keeper(&files).expect("keeper");
        assert_eq!(keeper.path, "D:/archive/b.bin");
        assert_eq!(keeper.reason, KeepReason::Newest);
    }

    #[test]
    fn newest_missing_timestamps_do_not_panic() {
        let files = vec![file("D:/x/a.bin", None), file("D:/x/b.bin", None)];
        let keeper = recommended_keeper(&files).expect("keeper");
        // No timestamps at all -> deterministic tie-break, shorter path first.
        assert_eq!(keeper.path, "D:/x/a.bin");
        assert_eq!(keeper.reason, KeepReason::TopLevel);
    }

    #[test]
    fn shallower_path_wins_when_otherwise_equal() {
        let files = vec![
            file("D:/a/b/c/d/deep.bin", None),
            file("D:/shallow.bin", None),
        ];
        let keeper = recommended_keeper(&files).expect("keeper");
        assert_eq!(keeper.path, "D:/shallow.bin");
        assert_eq!(keeper.reason, KeepReason::TopLevel);
    }

    #[test]
    fn junk_file_names_are_penalised() {
        let files = vec![
            file("D:/work/report (copy).docx", None),
            file("D:/work/report.docx", None),
        ];
        let keeper = recommended_keeper(&files).expect("keeper");
        assert_eq!(keeper.path, "D:/work/report.docx");
        assert!(looks_like_backup("D:/work/report (copy).docx"));
        assert!(!looks_like_backup("D:/work/report.docx"));
    }

    #[test]
    fn redundant_copies_exclude_exactly_the_keeper() {
        let files = vec![
            file("D:/Backups/a.bin", None),
            file("D:/Users/Bo/Pictures/a.bin", None),
            file("D:/tmp/a.bin", None),
        ];
        let redundant = redundant_copies(&files);
        assert_eq!(redundant.len(), 2);
        assert!(!redundant.contains(&"D:/Users/Bo/Pictures/a.bin".to_string()));
        assert!(redundant.contains(&"D:/Backups/a.bin".to_string()));
        assert!(redundant.contains(&"D:/tmp/a.bin".to_string()));
    }

    #[test]
    fn single_and_empty_groups_have_nothing_redundant() {
        assert!(redundant_copies(&[]).is_empty());
        assert!(redundant_copies(&[file("D:/a.bin", None)]).is_empty());
        assert!(recommended_keeper(&[]).is_none());
    }

    #[test]
    fn ranking_is_deterministic_for_identical_scores() {
        let files = vec![
            file("D:/b/same.bin", None),
            file("D:/a/same.bin", None),
            file("D:/c/same.bin", None),
        ];
        let first = rank_copies(&files);
        let second = rank_copies(&files);
        assert_eq!(first, second);
        assert_eq!(
            first.iter().map(|c| c.path.as_str()).collect::<Vec<_>>(),
            vec!["D:/a/same.bin", "D:/b/same.bin", "D:/c/same.bin"]
        );
    }

    #[test]
    fn separator_style_does_not_change_the_result() {
        let files = vec![
            file(r"C:\Users\Ann\Desktop\pic.png", None),
            file("C:/Users/Ann/Desktop/pic.png", None),
        ];
        let ranked = rank_copies(&files);
        assert_eq!(ranked.len(), 2);
        assert_eq!(ranked[0].score, ranked[1].score);
        assert_eq!(ranked[0].reason, KeepReason::UserFolder);
    }
}
