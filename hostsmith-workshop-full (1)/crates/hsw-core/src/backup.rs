//! Timestamped ZIP backups of affected files (MR-02, review Section 6.5).
//!
//! Backups live in the per-project backup directory inside the application
//! data directory, never inside the website folder, so they are never
//! deployed or uploaded. Every destructive operation creates one before it
//! writes; restore puts files back verbatim.

use crate::fsutil;
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupInfo {
    pub path: PathBuf,
    pub created_unix: u64,
    pub files: Vec<String>,
}

fn timestamped_name() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    // Seconds + 9-digit nanoseconds: lexicographic order is chronological.
    format!("{:012}-{:09}.zip", now.as_secs(), now.subsec_nanos())
}

/// Creates a backup ZIP containing `files` (paths relative to
/// `project_root`) and returns its path. Files that do not exist on disk are
/// recorded as absent entries, so restore can delete them again.
pub fn create_backup(
    project_root: &Path,
    files: &[PathBuf],
    backup_dir: &Path,
) -> std::io::Result<PathBuf> {
    if files.is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "refusing to create an empty backup",
        ));
    }
    std::fs::create_dir_all(backup_dir)?;
    let out_path = backup_dir.join(timestamped_name());
    let file = std::fs::File::create(&out_path)?;
    let mut zip = zip::ZipWriter::new(file);
    let options =
        zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
    for rel in files {
        let rel_text = fsutil::slash_separated(rel);
        let abs = fsutil::safe_join(project_root, rel)?;
        match std::fs::read(&abs) {
            Ok(data) => {
                zip.start_file(rel_text.clone(), options)?;
                zip.write_all(&data)?;
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                zip.add_directory(format!("{rel_text}/"), options)?;
            }
            Err(err) => return Err(err),
        }
    }
    zip.finish()?;
    Ok(out_path)
}

/// Lists the backups in `backup_dir`, oldest first.
pub fn list_backups(backup_dir: &Path) -> std::io::Result<Vec<BackupInfo>> {
    let mut out = Vec::new();
    if !backup_dir.exists() {
        return Ok(out);
    }
    for entry in std::fs::read_dir(backup_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("zip") {
            continue;
        }
        let created_unix = entry
            .metadata()?
            .modified()
            .ok()
            .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let files = std::fs::File::open(&path)
            .ok()
            .and_then(|f| zip::ZipArchive::new(f).ok())
            .map(|archive| {
                (0..archive.len())
                    .filter_map(|i| archive.name_for_index(i))
                    .map(|name| name.trim_end_matches('/').to_string())
                    .filter(|name| !name.is_empty())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        out.push(BackupInfo { path, created_unix, files });
    }
    out.sort_by_key(|info| info.path.clone());
    Ok(out)
}

/// Restores files from a backup ZIP into `project_root`. Returns the number
/// of files restored. Entries that represent an absent file (directory
/// marker) delete the file at that path, reproducing the pre-operation state.
pub fn restore_backup(
    project_root: &Path,
    backup_zip: &Path,
    only: Option<&[String]>,
) -> std::io::Result<usize> {
    let file = std::fs::File::open(backup_zip)?;
    let mut archive = zip::ZipArchive::new(file)?;
    let mut restored = 0;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let name = entry.name().trim_end_matches('/').to_string();
        if name.is_empty() {
            continue;
        }
        if let Some(list) = only {
            if !list.iter().any(|candidate| candidate == &name) {
                continue;
            }
        }
        let dest = fsutil::safe_join(project_root, Path::new(&name))?;
        if entry.is_dir() {
            // Absent-file marker from create_backup: remove the current file.
            if dest.is_file() {
                std::fs::remove_file(&dest)?;
            }
            continue;
        }
        let mut data = Vec::with_capacity(entry.size() as usize);
        entry.read_to_end(&mut data)?;
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&dest, &data)?;
        restored += 1;
    }
    Ok(restored)
}

/// Deletes the oldest backups beyond `keep`. Returns how many were removed.
pub fn trim_backups(backup_dir: &Path, keep: usize) -> std::io::Result<usize> {
    let mut infos = list_backups(backup_dir)?;
    if infos.len() <= keep {
        return Ok(0);
    }
    let excess = infos.len() - keep;
    let mut removed = 0;
    for info in infos.drain(..excess) {
        std::fs::remove_file(info.path)?;
        removed += 1;
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn backup_and_restore_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path().join("site");
        let backups = tmp.path().join("backups");
        fs::create_dir_all(project.join("assets")).unwrap();
        fs::write(project.join("index.html"), "<h1>before</h1>").unwrap();
        fs::write(project.join("assets/style.css"), "body{}").unwrap();

        let files = vec![PathBuf::from("index.html"), PathBuf::from("assets/style.css")];
        let zip_path = create_backup(&project, &files, &backups).unwrap();
        assert!(zip_path.exists());

        fs::write(project.join("index.html"), "<h1>AFTER</h1>").unwrap();
        let restored = restore_backup(&project, &zip_path, None).unwrap();
        assert_eq!(restored, 2);
        assert_eq!(
            fs::read_to_string(project.join("index.html")).unwrap(),
            "<h1>before</h1>"
        );
    }

    #[test]
    fn restore_can_select_files_and_reproduce_deletions() {
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path().join("site");
        let backups = tmp.path().join("backups");
        fs::create_dir_all(&project).unwrap();
        fs::write(project.join("a.html"), "A").unwrap();
        fs::write(project.join("b.html"), "B").unwrap();

        let files = vec![PathBuf::from("a.html"), PathBuf::from("b.html")];
        let zip_path = create_backup(&project, &files, &backups).unwrap();
        fs::remove_file(project.join("a.html")).unwrap();
        fs::write(project.join("b.html"), "CHANGED").unwrap();

        let restored = restore_backup(&project, &zip_path, Some(&["a.html".into()])).unwrap();
        assert_eq!(restored, 1);
        assert_eq!(fs::read_to_string(project.join("a.html")).unwrap(), "A");
        assert_eq!(fs::read_to_string(project.join("b.html")).unwrap(), "CHANGED");
    }

    #[test]
    fn absent_files_become_deletion_markers() {
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path().join("site");
        let backups = tmp.path().join("backups");
        fs::create_dir_all(&project).unwrap();

        // Backup taken while a.html did not exist; then the file is created.
        let zip_path =
            create_backup(&project, &[PathBuf::from("a.html")], &backups).unwrap();
        fs::write(project.join("a.html"), "new").unwrap();
        restore_backup(&project, &zip_path, None).unwrap();
        assert!(!project.join("a.html").exists(), "restore must delete the file");
    }

    #[test]
    fn trim_keeps_the_newest() {
        let tmp = tempfile::tempdir().unwrap();
        let backups = tmp.path().join("backups");
        let project = tempfile::tempdir().unwrap();
        fs::write(project.path().join("x.txt"), "x").unwrap();
        let files = vec![PathBuf::from("x.txt")];
        for _ in 0..3 {
            create_backup(project.path(), &files, &backups).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let removed = trim_backups(&backups, 2).unwrap();
        assert_eq!(removed, 1);
        assert_eq!(list_backups(&backups).unwrap().len(), 2);
    }

    #[test]
    fn empty_backup_is_refused() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(create_backup(tmp.path(), &[], &tmp.path().join("b")).is_err());
    }
}
