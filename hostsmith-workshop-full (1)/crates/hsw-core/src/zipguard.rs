//! Safe ZIP import (spec Section 3; TL-09, MR-02).
//!
//! Extraction is planned first ([`plan_import`]) and only then performed
//! ([`extract_zip`]). The plan rejects traversal attacks, absolute paths,
//! drive letters, NUL characters and oversized archives, strips a single
//! common root folder, and skips link entries instead of materializing them.

use std::io::Read;
use std::path::{Component, Path, PathBuf};

/// Maximum size of a single decompressed entry.
pub const MAX_ENTRY_BYTES: u64 = 500 * 1024 * 1024;
/// Maximum total decompressed size of an import.
pub const MAX_TOTAL_BYTES: u64 = 2 * 1024 * 1024 * 1024;
/// Maximum number of entries in an import.
pub const MAX_ENTRIES: usize = 50_000;

#[derive(Debug, thiserror::Error)]
pub enum ZipGuardError {
    #[error("unsafe entry path in archive: {0:?}")]
    UnsafeEntry(String),
    #[error("archive exceeds the limit of {0} entries")]
    TooManyEntries(usize),
    #[error("archive exceeds the size limit: {0} bytes")]
    TooLarge(u64),
    #[error("archive contains no usable entries")]
    Empty,
    #[error("io error during extraction: {0}")]
    Io(#[from] std::io::Error),
    #[error("zip error during extraction: {0}")]
    Zip(#[from] zip::result::ZipError),
}

/// Minimal description of one archive entry, decoupled from the zip crate so
/// planning logic is unit-testable without building archives.
#[derive(Debug, Clone, PartialEq)]
pub struct RawEntry {
    pub name: String,
    pub uncompressed_size: u64,
    pub is_dir: bool,
    pub is_symlink: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    File,
    Dir,
}

#[derive(Debug, Clone)]
pub struct EntryPlan {
    pub source_name: String,
    pub destination: PathBuf,
    pub kind: EntryKind,
    pub uncompressed_size: u64,
}

#[derive(Debug)]
pub struct ImportPlan {
    /// Common first path component stripped from every entry, if any.
    pub root_stripped: Option<String>,
    pub entries: Vec<EntryPlan>,
    /// Link entries and other unsupported entries, excluded from the plan.
    pub skipped: Vec<String>,
}

/// Rejects any entry name that could escape the destination or damage the
/// file system. Returns a short reason when unsafe.
fn unsafe_reason(raw_name: &str) -> Option<&'static str> {
    if raw_name.is_empty() {
        return Some("empty name");
    }
    if raw_name.contains('\0') {
        return Some("NUL byte");
    }
    // Windows-authored archives historically use backslashes as separators.
    let name = raw_name.replace('\\', "/");
    if name.starts_with('/') {
        return Some("absolute path");
    }
    // A trailing slash is the conventional directory marker in archives,
    // not an empty path component; strip it before the component checks.
    let name = name.trim_end_matches('/');
    if name.is_empty() {
        return Some("empty name");
    }
    let mut components: Vec<&str> = name.split('/').collect();
    if components.iter().any(|c| c.is_empty()) {
        return Some("empty path component");
    }
    if let Some(first) = components.first() {
        let bytes = first.as_bytes();
        if bytes.len() >= 2 && bytes[1] == b':' && bytes[0].is_ascii_alphabetic() {
            return Some("drive letter");
        }
    }
    if components.iter().any(|c| *c == ".." || *c == ".") {
        return Some("dot or dot-dot component");
    }
    if components.iter().any(|c| c.contains(':')) {
        return Some("colon in path component");
    }
    let _ = components.pop();
    None
}

fn relative_destination(raw_name: &str, root_stripped: Option<&str>) -> PathBuf {
    let name = raw_name.replace('\\', "/");
    let name = match root_stripped {
        Some(root) => name.strip_prefix(root).unwrap_or(&name).to_string(),
        None => name,
    };
    let mut dest = PathBuf::new();
    for component in name.split('/') {
        if component.is_empty() || component == "." || component == ".." {
            // The plan stage already rejected these; defensive repeat.
            continue;
        }
        dest.push(component);
    }
    dest
}

/// Detects whether every entry lives under a single root folder (spec
/// Section 3, step 4) and returns that root including its trailing slash.
fn detect_single_root(entries: &[&RawEntry]) -> Option<String> {
    let mut root: Option<String> = None;
    for entry in entries {
        let name = entry.name.replace('\\', "/");
        let first = name.split('/').next()?;
        if first.is_empty() {
            return None;
        }
        match &root {
            None => root = Some(format!("{}/", first)),
            Some(existing) if *existing != format!("{}/", first) => return None,
            _ => {}
        }
    }
    root
}

/// Builds a validated import plan from raw archive metadata.
pub fn plan_import(raw: &[RawEntry]) -> Result<ImportPlan, ZipGuardError> {
    if raw.is_empty() {
        return Err(ZipGuardError::Empty);
    }
    if raw.len() > MAX_ENTRIES {
        return Err(ZipGuardError::TooManyEntries(raw.len()));
    }
    let mut total: u64 = 0;
    for entry in raw {
        if let Some(reason) = unsafe_reason(&entry.name) {
            return Err(ZipGuardError::UnsafeEntry(format!("{} ({})", entry.name, reason)));
        }
        if entry.uncompressed_size > MAX_ENTRY_BYTES {
            return Err(ZipGuardError::TooLarge(entry.uncompressed_size));
        }
        total += entry.uncompressed_size;
        if total > MAX_TOTAL_BYTES {
            return Err(ZipGuardError::TooLarge(total));
        }
    }

    let usable: Vec<&RawEntry> = raw.iter().filter(|e| !e.is_symlink).collect();
    let skipped: Vec<String> = raw
        .iter()
        .filter(|e| e.is_symlink)
        .map(|e| e.name.clone())
        .collect();
    if usable.is_empty() {
        return Err(ZipGuardError::Empty);
    }
    let root_stripped = detect_single_root(&usable);

    let mut entries = Vec::with_capacity(usable.len());
    for entry in &usable {
        let destination = relative_destination(&entry.name, root_stripped.as_deref());
        if destination.as_os_str().is_empty() {
            return Err(ZipGuardError::UnsafeEntry(entry.name.clone()));
        }
        // Belt and braces: the destination must contain only normal
        // components, so it cannot escape the destination root by
        // construction even if a validation case is ever missed.
        let safe_components = destination
            .components()
            .all(|c| matches!(c, Component::Normal(_)));
        if !safe_components {
            return Err(ZipGuardError::UnsafeEntry(entry.name.clone()));
        }
        let kind = if entry.is_dir { EntryKind::Dir } else { EntryKind::File };
        entries.push(EntryPlan {
            source_name: entry.name.clone(),
            destination,
            kind,
            uncompressed_size: entry.uncompressed_size,
        });
    }
    Ok(ImportPlan {
        root_stripped,
        entries,
        skipped,
    })
}

/// Extracts a zip archive into `destination_root`, applying
/// [`plan_import`] first. Link entries are skipped and reported.
pub fn extract_zip<R: Read + std::io::Seek>(
    reader: R,
    destination_root: &Path,
) -> Result<ImportPlan, ZipGuardError> {
    let mut archive = zip::ZipArchive::new(reader)?;
    let mut raw: Vec<RawEntry> = Vec::with_capacity(archive.len());
    for index in 0..archive.len() {
        let file = archive.by_index_raw(index)?;
        raw.push(RawEntry {
            name: file.name().to_string(),
            uncompressed_size: file.size(),
            is_dir: file.is_dir(),
            is_symlink: file
                .unix_mode()
                .map(|mode| mode & 0o170_000 == 0o120_000)
                .unwrap_or(false),
        });
    }
    let plan = plan_import(&raw)?;

    // Map planned entries back to their archive index (plan preserves the
    // raw order; usable entries are raw entries minus skipped links).
    let mut entry_index = 0usize;
    for entry in &plan.entries {
        while raw[entry_index].is_symlink {
            entry_index += 1;
        }
        let archive_index = entry_index;
        entry_index += 1;

        let target = destination_root.join(&entry.destination);
        match entry.kind {
            EntryKind::Dir => {
                std::fs::create_dir_all(&target)?;
            }
            EntryKind::File => {
                if let Some(parent) = target.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                // by_index (not by_index_raw): the written bytes must be
                // the decompressed file contents, not the raw deflate stream.
                let file = archive.by_index(archive_index)?;
                let mut out = std::fs::File::create(&target)?;
                let mut limited = file.take(MAX_ENTRY_BYTES);
                std::io::copy(&mut limited, &mut out)?;
            }
        }
    }
    Ok(plan)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, size: u64) -> RawEntry {
        RawEntry { name: name.to_string(), uncompressed_size: size, is_dir: false, is_symlink: false }
    }

    #[test]
    fn traversal_names_are_rejected() {
        for bad in [
            "../evil.txt",
            "a/../../evil.txt",
            "..\\..\\evil.txt",
            "/absolute/path.txt",
            "C:\\evil.txt",
            "c:/evil.txt",
            "a//double//slash.txt",
            "bad\0nul.txt",
            "colon:potion.txt",
            "./dot.txt",
        ] {
            let err = plan_import(&[entry(bad, 1)]).unwrap_err();
            assert!(matches!(err, ZipGuardError::UnsafeEntry(_)), "case {}", bad);
        }
    }

    #[test]
    fn single_root_folder_is_stripped() {
        let plan = plan_import(&[
            entry("site/index.html", 10),
            entry("site/about/index.html", 10),
            entry("site/image/logo.png", 10),
        ])
        .unwrap();
        assert_eq!(plan.root_stripped.as_deref(), Some("site/"));
        let dests: Vec<&str> = plan
            .entries
            .iter()
            .map(|e| e.destination.to_str().unwrap())
            .collect();
        assert!(dests.contains(&"index.html"));
        assert!(dests.contains(&"about/index.html"));
        assert!(dests.contains(&"image/logo.png"));
    }

    #[test]
    fn flat_archives_keep_their_layout() {
        let plan = plan_import(&[entry("index.html", 1), entry("assets/css/style.css", 1)]).unwrap();
        assert_eq!(plan.root_stripped, None);
        assert!(plan
            .entries
            .iter()
            .any(|e| e.destination == std::path::Path::new("assets/css/style.css")));
    }

    #[test]
    fn symlink_entries_are_skipped_not_created() {
        let mut link = entry("site/link", 1);
        link.is_symlink = true;
        let plan = plan_import(&[entry("site/index.html", 1), link]).unwrap();
        assert_eq!(plan.skipped, vec!["site/link".to_string()]);
        assert!(plan.entries.iter().all(|e| !e.source_name.contains("link")));
    }

    #[test]
    fn size_and_entry_limits_are_enforced() {
        let big = entry("big.bin", MAX_ENTRY_BYTES + 1);
        assert!(matches!(plan_import(&[big]), Err(ZipGuardError::TooLarge(_))));
        let many: Vec<RawEntry> = (0..=MAX_ENTRIES)
            .map(|i| entry(&format!("f{}.txt", i), 1))
            .collect();
        assert!(matches!(plan_import(&many), Err(ZipGuardError::TooManyEntries(_))));
    }

    #[test]
    fn directory_entries_plan_as_dirs() {
        let mut dir = entry("site/assets/", 0);
        dir.is_dir = true;
        let plan = plan_import(&[dir, entry("site/assets/css/style.css", 1)]).unwrap();
        let dirs: Vec<&EntryPlan> = plan.entries.iter().filter(|e| e.kind == EntryKind::Dir).collect();
        assert_eq!(dirs.len(), 1);
        assert_eq!(dirs[0].destination, PathBuf::from("assets"));
    }
}
