//! Filesystem helpers shared across the application.
//!
//! Windows long paths (> 260 chars) are handled by the `dunce`-style approach
//! used by std: paths obtained from `walkdir`/`std` are already `\\?\`-free,
//! and every open goes through [`open_file`] which adds the verbatim prefix on
//! Windows when a path exceeds `MAX_PATH`.

use std::fs::{File, Metadata};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Convert a `SystemTime` to a unix timestamp in milliseconds (UTC).
pub fn system_time_to_millis(time: Option<SystemTime>) -> Option<i64> {
    let time = time?;
    match time.duration_since(SystemTime::UNIX_EPOCH) {
        Ok(duration) => Some(duration.as_millis() as i64),
        Err(_) => None,
    }
}

/// `true` when the entry can be treated as a regular file.
pub fn is_regular_file(metadata: &Metadata) -> bool {
    metadata.is_file() && !metadata.file_type().is_symlink()
}

/// Open a file read-only, transparently handling Windows long paths.
#[cfg(windows)]
pub fn open_file(path: &Path) -> std::io::Result<File> {
    use std::os::windows::fs::OpenOptionsExt;
    const FILE_SHARE_READ: u32 = 0x1;
    const FILE_SHARE_WRITE: u32 = 0x2;
    const FILE_SHARE_DELETE: u32 = 0x4;

    let mut options = std::fs::OpenOptions::new();
    options
        .read(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE);
    options.open(extended_path(path))
}

#[cfg(not(windows))]
pub fn open_file(path: &Path) -> std::io::Result<File> {
    File::open(path)
}

/// Canonical path that is safe to pass to Win32 APIs (adds `\\?\` when long).
#[cfg(windows)]
pub fn extended_path(path: &Path) -> PathBuf {
    const MAX_PATH: usize = 260;

    let text = path.as_os_str().to_string_lossy();
    // Already verbatim, or a UNC share: Win32 accepts these as they are.
    if text.starts_with(r"\\?\") || text.starts_with(r"\\") {
        return path.to_path_buf();
    }
    // MAX_PATH counts UTF-16 code units, not bytes: a Cyrillic path of 130
    // characters is 260 bytes but still well below the limit. `chars().count()`
    // is the closest safe approximation available without Win32 calls.
    let short_enough = text.chars().count() < MAX_PATH;
    if short_enough && path.is_absolute() {
        return path.to_path_buf();
    }

    // Relative or long: make it absolute without requiring the file to exist.
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        match std::env::current_dir() {
            Ok(cwd) => cwd.join(path),
            Err(_) => return path.to_path_buf(),
        }
    };
    // `canonicalize` would fail for a file that no longer exists (the deletion
    // path relies on `symlink_metadata` reporting "missing"), so build the
    // prefix by hand instead.
    PathBuf::from(format!(r"\\?\{}", absolute.to_string_lossy()))
}

#[cfg(not(windows))]
pub fn extended_path(path: &Path) -> PathBuf {
    path.to_path_buf()
}

/// `std::fs::symlink_metadata` that also works for Windows long paths.
///
/// `symlink_metadata` is deliberately used instead of following links: a
/// symlink must never be reported as a deletable regular file.
pub fn symlink_metadata(path: &Path) -> std::io::Result<Metadata> {
    std::fs::symlink_metadata(extended_path(path))
}

/// `std::fs::remove_file` that also works for Windows long paths.
pub fn remove_file(path: &Path) -> std::io::Result<()> {
    std::fs::remove_file(extended_path(path))
}

/// Move a file to the Recycle Bin, transparently handling long paths.
///
/// `trash::delete` takes the path as given; on Windows a path over `MAX_PATH`
/// has to carry the `\\?\` prefix or the shell refuses it.
pub fn move_to_trash(path: &Path) -> Result<(), String> {
    trash::delete(extended_path(path)).map_err(|error| error.to_string())
}

/// Display path with the verbatim prefix stripped (for the UI).
pub fn display_path(path: &Path) -> String {
    let text = path.to_string_lossy().to_string();
    text.strip_prefix(r"\\?\")
        .map(|s| s.to_string())
        .unwrap_or(text)
}

/// Human readable size (B, KB, MB, GB, TB) — used by logs and tests.
pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    if bytes == 0 {
        return "0 B".to_string();
    }
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

/// Lower-case extension without the dot.
pub fn extension_of(path: &Path) -> String {
    path.extension()
        .map(|ext| ext.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

/// `true` if the extension is a supported image type for perceptual hashing.
pub fn is_supported_image(path: &Path) -> bool {
    matches!(
        extension_of(path).as_str(),
        "jpg" | "jpeg" | "png" | "gif" | "webp" | "bmp" | "tif" | "tiff"
    )
}

/// Lower-case, separator-normalised path with verbatim prefixes (`\\?\`,
/// `\\.\`) dropped and `.`/`..` resolved lexically.
///
/// A leading `//` is kept for UNC input so `\\server\share\Windows` stays
/// distinguishable from a local path. No filesystem access — this is a purely
/// textual normalisation meant for security comparisons.
pub fn normalize_for_security(raw: &str) -> String {
    let mut text = raw.trim().replace('\\', "/");
    let mut unc = text.starts_with("//");
    if let Some(stripped) = text
        .strip_prefix("//?/")
        .or_else(|| text.strip_prefix("//./"))
    {
        text = stripped.to_string();
        unc = false;
        // \\?\UNC\server\share\... is still a UNC path
        if let Some(rest) = text
            .strip_prefix("UNC/")
            .or_else(|| text.strip_prefix("unc/"))
        {
            text = rest.to_string();
            unc = true;
        }
    }

    let mut out: Vec<&str> = Vec::new();
    for segment in text.split('/') {
        match segment {
            "" | "." => continue,
            ".." => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    let joined = out.join("/").to_lowercase();
    if unc {
        format!("//{joined}")
    } else {
        joined
    }
}

/// The path segments that follow the volume/root prefix.
///
/// `C:\a\b` → `["a", "b"]`, `\\srv\share\a` → `["a"]`, `/home/x` → `["home", "x"]`.
pub fn rooted_segments(normalized: &str) -> Vec<&str> {
    let segments: Vec<&str> = normalized.split('/').filter(|s| !s.is_empty()).collect();
    let start = if segments
        .first()
        .map(|s| s.len() == 2 && s.ends_with(':'))
        .unwrap_or(false)
    {
        1
    } else if normalized.starts_with("//") {
        2.min(segments.len())
    } else {
        0
    };
    segments[start..].to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_path_helpers_behave_like_std_on_short_paths() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("x.bin");
        std::fs::write(&path, b"data").expect("write");

        let metadata = symlink_metadata(&path).expect("metadata");
        assert!(metadata.is_file());
        assert_eq!(metadata.len(), 4);

        remove_file(&path).expect("remove");
        assert!(!path.exists());
    }

    #[cfg(not(windows))]
    #[test]
    fn extended_path_is_a_noop_off_windows() {
        let long = "a/".repeat(200) + "file.bin";
        assert_eq!(extended_path(Path::new(&long)), Path::new(&long));
    }

    #[test]
    fn security_normalisation_handles_prefixes_and_traversal() {
        assert_eq!(
            normalize_for_security("C:\\Windows\\System32\\..\\a.dll"),
            "c:/windows/a.dll"
        );
        assert_eq!(normalize_for_security("\\\\?\\C:\\Windows"), "c:/windows");
        assert_eq!(
            normalize_for_security("\\\\server\\share\\Windows"),
            "//server/share/windows"
        );
        assert_eq!(
            normalize_for_security("\\\\?\\UNC\\srv\\share\\Windows"),
            "//srv/share/windows"
        );
        assert_eq!(
            rooted_segments("c:/windows/a.dll"),
            vec!["windows", "a.dll"]
        );
        assert_eq!(rooted_segments("//srv/share/windows"), vec!["windows"]);
        assert_eq!(rooted_segments("/home/user/x"), vec!["home", "user", "x"]);
    }

    #[test]
    fn human_size_formats() {
        assert_eq!(human_size(0), "0 B");
        assert_eq!(human_size(512), "512 B");
        assert_eq!(human_size(1024), "1.0 KB");
        assert_eq!(human_size(1024 * 1024 * 3), "3.0 MB");
    }

    #[test]
    fn extension_is_lowercase() {
        assert_eq!(extension_of(Path::new("a/B.PHOTO.JPG")), "jpg");
        assert_eq!(extension_of(Path::new("noext")), "");
    }

    #[test]
    fn image_detection() {
        assert!(is_supported_image(Path::new("x.png")));
        assert!(is_supported_image(Path::new("x.JPEG")));
        assert!(!is_supported_image(Path::new("x.txt")));
    }
}
