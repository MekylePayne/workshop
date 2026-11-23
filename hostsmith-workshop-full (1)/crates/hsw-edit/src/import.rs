//! Safe asset import with deterministic naming (PR-02, spec Section 6).
//!
//! Imported media always lands in the project's `/image/` directory with a
//! slugged, collision-free filename; an existing file is never overwritten
//! (a numbered suffix is used instead), and the slug rule never lets path
//! separators survive from the original name.

use hsw_core::naming::{asset_filename, unique_filename};
use std::path::Path;

/// The directory imported media is copied into (spec Section 2).
pub const MEDIA_DIR: &str = "image";

/// Importable media extensions (MR-05): images plus mp4/webm natively
/// previewable; mov/mkv import best effort as the specification allows.
const IMPORTABLE: [&str; 13] = [
    "jpg", "jpeg", "png", "webp", "gif", "svg", "avif", "ico", "bmp", "mp4", "webm", "mov",
    "mkv",
];

#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    #[error("file type .{ext} is not an importable media asset")]
    UnsupportedType { ext: String },
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportResult {
    /// The final (slugged, unique) filename inside /image/.
    pub file_name: String,
    /// Project-relative destination path, forward-slash separated.
    pub dest_rel: String,
    pub bytes: usize,
    /// The desired name before collision handling, when a suffix was added.
    pub renamed_from: Option<String>,
}

/// Copies `bytes` into the project's `/image/` directory under a clean,
/// unique name derived from `original_name`. Never overwrites an existing
/// file.
pub fn import_asset(
    project_root: &Path,
    original_name: &str,
    bytes: &[u8],
) -> Result<ImportResult, ImportError> {
    let (stem, ext) = asset_filename(original_name);
    if !IMPORTABLE.contains(&ext.as_str()) {
        return Err(ImportError::UnsupportedType { ext });
    }
    let image_dir = project_root.join(MEDIA_DIR);
    std::fs::create_dir_all(&image_dir)?;
    let existing: Vec<String> = std::fs::read_dir(&image_dir)?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .collect();
    let desired = if ext.is_empty() {
        stem
    } else {
        format!("{stem}.{ext}")
    };
    let final_name = unique_filename(&desired, &existing);
    let dest_rel = format!("{MEDIA_DIR}/{final_name}");
    std::fs::write(project_root.join(&dest_rel), bytes)?;
    let renamed_from = if final_name.eq_ignore_ascii_case(&desired) {
        None
    } else {
        Some(desired)
    };
    Ok(ImportResult {
        file_name: final_name,
        dest_rel,
        bytes: bytes.len(),
        renamed_from,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn specification_example_imports_with_the_documented_name() {
        let tmp = tempfile::tempdir().unwrap();
        let result =
            import_asset(tmp.path(), "My New Product Photo 2026.png", b"png").unwrap();
        assert_eq!(result.file_name, "my-new-product-photo-2026.png");
        assert_eq!(result.dest_rel, "image/my-new-product-photo-2026.png");
        assert!(tmp.path().join("image/my-new-product-photo-2026.png").exists());
    }

    #[test]
    fn collisions_are_never_overwritten() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir_all(tmp.path().join("image")).unwrap();
        fs::write(tmp.path().join("image/photo.png"), b"old").unwrap();
        let result = import_asset(tmp.path(), "photo.png", b"new").unwrap();
        assert_eq!(result.file_name, "photo-2.png");
        assert_eq!(fs::read(tmp.path().join("image/photo.png")).unwrap(), b"old");
        assert_eq!(fs::read(tmp.path().join("image/photo-2.png")).unwrap(), b"new");
    }

    #[test]
    fn unsafe_types_are_rejected() {
        let tmp = tempfile::tempdir().unwrap();
        let error = import_asset(tmp.path(), "setup.exe", b"MZ").unwrap_err();
        assert!(matches!(error, ImportError::UnsupportedType { ext } if ext == "exe"));
        assert!(!tmp.path().join("image").exists(), "no directory created on refusal");
    }

    #[test]
    fn path_separators_in_names_cannot_escape() {
        let tmp = tempfile::tempdir().unwrap();
        let result = import_asset(tmp.path(), "..\\..\\evil.png", b"x").unwrap();
        assert!(!result.dest_rel.contains(".."));
        assert!(result.dest_rel.starts_with("image/"));
        assert!(tmp.path().join("image").join(&result.file_name).exists());
    }

    #[test]
    fn video_extensions_import_best_effort() {
        let tmp = tempfile::tempdir().unwrap();
        let result = import_asset(tmp.path(), "Launch Video 2026.mp4", b"mp4").unwrap();
        assert_eq!(result.dest_rel, "image/launch-video-2026.mp4");
    }
}
