//! Restart-safe operation journal persisted in the application data
//! directory (MR-03), with external-change invalidation (MR-08).
//!
//! Entries store full before/after content snapshots, so undo and redo are
//! pure journal walks that survive an application restart. Text files are
//! stored verbatim; binary files (image and video swaps) are stored
//! base64-encoded up to [`MAX_BINARY_SNAPSHOT`] bytes. When the watcher
//! reports a file changed outside the application, [`PersistentJournal::
//! invalidate_path`] marks the affected entries, and undo/redo skip them
//! instead of reverting onto foreign changes.

use crate::fsutil::{content_hash, safe_join};
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// Binary snapshots above this size are refused; backups cover recovery.
pub const MAX_BINARY_SNAPSHOT: usize = 24 * 1024 * 1024;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PersistedFile {
    /// Path relative to the project root, forward-slash separated.
    pub path: String,
    pub binary: bool,
    /// `None` means "this file did not exist" (before a creation or after a
    /// deletion). Text content is verbatim; binary content is base64.
    pub content_before: Option<String>,
    pub content_after: Option<String>,
}

impl PersistedFile {
    pub fn text(path: impl Into<String>, before: &str, after: &str) -> Self {
        Self {
            path: path.into(),
            binary: false,
            content_before: Some(before.to_string()),
            content_after: Some(after.to_string()),
        }
    }

    pub fn binary_bytes(path: impl Into<String>, before: &[u8], after: &[u8]) -> Self {
        Self {
            path: path.into(),
            binary: true,
            content_before: Some(B64.encode(before)),
            content_after: Some(B64.encode(after)),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PersistedEntry {
    pub description: String,
    pub time_unix: u64,
    pub files: Vec<PersistedFile>,
    /// `true` while the entry is currently undone (redoable).
    #[serde(default)]
    pub undone: bool,
    /// Set when a file changed outside the application (MR-08).
    #[serde(default)]
    pub invalidated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JournalOutcome {
    pub description: String,
    pub files_restored: Vec<PathBuf>,
    pub skipped: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum JournalError {
    #[error("binary snapshot exceeds the {MAX_BINARY_SNAPSHOT}-byte cap; use the backup instead")]
    BinaryTooLarge,
    #[error("invalid base64 in journal entry")]
    BadBase64,
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

pub struct PersistentJournal {
    path: PathBuf,
    entries: Vec<PersistedEntry>,
}

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

impl PersistentJournal {
    /// Loads the journal at `path`; a missing or unreadable file yields an
    /// empty journal (the file is rewritten on the next record).
    pub fn load(path: &Path) -> Self {
        let entries = fs::read(path)
            .ok()
            .and_then(|data| serde_json::from_slice(&data).ok())
            .unwrap_or_default();
        Self { path: path.to_path_buf(), entries }
    }

    fn save(&self) -> std::io::Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let tmp = self.path.with_extension("json.tmp");
        let data = serde_json::to_vec_pretty(&self.entries)
            .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidData, err))?;
        fs::write(&tmp, data)?;
        fs::rename(&tmp, &self.path)
    }

    /// Records an operation. `content_before`/`content_after` may be `None`
    /// for created/deleted files; binary files must be base64-encoded.
    pub fn record(&mut self, description: &str, files: Vec<PersistedFile>) -> Result<(), JournalError> {
        for file in &files {
            if !file.binary {
                continue;
            }
            for content in [&file.content_before, &file.content_after] {
                let Some(text) = content else { continue };
                let decoded = B64.decode(text).map_err(|_| JournalError::BadBase64)?;
                if decoded.len() > MAX_BINARY_SNAPSHOT {
                    return Err(JournalError::BinaryTooLarge);
                }
            }
        }
        self.entries.push(PersistedEntry {
            description: description.to_string(),
            time_unix: now_unix(),
            files,
            undone: false,
            invalidated: false,
        });
        self.save()?;
        Ok(())
    }

    fn snapshot_hash(entry: &PersistedEntry, file: &PersistedFile) -> Option<u64> {
        let content = if entry.undone {
            file.content_before.as_ref()?
        } else {
            file.content_after.as_ref()?
        };
        if file.binary {
            Some(content_hash(&B64.decode(content).ok()?))
        } else {
            Some(content_hash(content.as_bytes()))
        }
    }

    /// One undo (or redo) step: writes the recorded state back for every
    /// file whose current content still matches what the journal expects.
    /// Files changed outside the application are skipped and reported.
    pub fn step(&mut self, root: &Path, redo: bool) -> Result<Option<JournalOutcome>, JournalError> {
        let Some(index) = self
            .entries
            .iter()
            .rposition(|entry| if redo { entry.undone } else { !entry.undone } && !entry.invalidated)
        else {
            return Ok(None);
        };
        let mut outcome = JournalOutcome {
            description: self.entries[index].description.clone(),
            files_restored: Vec::new(),
            skipped: Vec::new(),
        };
        let mut entry = self.entries[index].clone();
        for file in &entry.files {
            let abs = safe_join(root, Path::new(&file.path))?;
            let exists_now = abs.exists();
            let expected = Self::snapshot_hash(&entry, file);
            let actual = if exists_now {
                fs::read(&abs).ok().map(|bytes| content_hash(&bytes))
            } else {
                None
            };
            let state_matches = match (expected, actual) {
                (None, None) => true,
                (Some(expected_hash), Some(actual_hash)) => expected_hash == actual_hash,
                _ => false,
            };
            if !state_matches {
                outcome
                    .skipped
                    .push(format!("{} changed outside the application", file.path));
                continue;
            }
            let restore_to = if entry.undone {
                file.content_after.clone()
            } else {
                file.content_before.clone()
            };
            match restore_to {
                None => {
                    if exists_now {
                        fs::remove_file(&abs)?;
                    }
                }
                Some(text) => {
                    let bytes = if file.binary {
                        B64.decode(text).map_err(|_| JournalError::BadBase64)?
                    } else {
                        text.into_bytes()
                    };
                    if let Some(parent) = abs.parent() {
                        fs::create_dir_all(parent)?;
                    }
                    fs::write(&abs, bytes)?;
                }
            }
            outcome.files_restored.push(abs);
        }
        entry.undone = !entry.undone;
        self.entries[index] = entry;
        self.save()?;
        Ok(Some(outcome))
    }

    /// Marks every entry touching `rel_path` as invalidated (MR-08).
    /// Returns how many entries were affected.
    pub fn invalidate_path(&mut self, rel_path: &str) -> std::io::Result<usize> {
        let mut affected = 0;
        for entry in &mut self.entries {
            if !entry.invalidated && entry.files.iter().any(|f| f.path == rel_path) {
                entry.invalidated = true;
                affected += 1;
            }
        }
        if affected > 0 {
            self.save()?;
        }
        Ok(affected)
    }

    pub fn entries(&self) -> &[PersistedEntry] {
        &self.entries
    }

    pub fn can_undo(&self) -> bool {
        self.entries.iter().any(|e| !e.undone && !e.invalidated)
    }

    pub fn can_redo(&self) -> bool {
        self.entries.iter().any(|e| e.undone && !e.invalidated)
    }

    /// Discards all entries (used when a different project is opened).
    pub fn clear(&mut self) -> std::io::Result<()> {
        self.entries.clear();
        self.save()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn journal_file(tmp: &Path) -> PersistentJournal {
        PersistentJournal::load(&tmp.join("journal.json"))
    }

    #[test]
    fn undo_reverts_file_content_and_redo_restores_it() {
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path().join("site");
        fs::create_dir_all(&project).unwrap();
        fs::write(project.join("index.html"), "<h1>New</h1>").unwrap();

        let mut journal = journal_file(tmp.path());
        journal
            .record(
                "Edit heading",
                vec![PersistedFile::text("index.html", "<h1>Old</h1>", "<h1>New</h1>")],
            )
            .unwrap();

        let outcome = journal.step(&project, false).unwrap().unwrap();
        assert_eq!(outcome.files_restored, vec![project.join("index.html")]);
        assert_eq!(fs::read_to_string(project.join("index.html")).unwrap(), "<h1>Old</h1>");

        let outcome = journal.step(&project, true).unwrap().unwrap();
        assert_eq!(outcome.files_restored.len(), 1);
        assert_eq!(fs::read_to_string(project.join("index.html")).unwrap(), "<h1>New</h1>");
    }

    #[test]
    fn undo_deletes_files_the_operation_created() {
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path().join("site");
        fs::create_dir_all(&project).unwrap();
        fs::write(project.join("new.txt"), "created").unwrap();

        let mut journal = journal_file(tmp.path());
        journal
            .record("Create file", vec![PersistedFile {
                path: "new.txt".into(),
                binary: false,
                content_before: None,
                content_after: Some("created".into()),
            }])
            .unwrap();
        journal.step(&project, false).unwrap().unwrap();
        assert!(!project.join("new.txt").exists());
        journal.step(&project, true).unwrap().unwrap();
        assert_eq!(fs::read_to_string(project.join("new.txt")).unwrap(), "created");
    }

    #[test]
    fn external_change_invalidates_entries() {
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path().join("site");
        fs::create_dir_all(&project).unwrap();
        fs::write(project.join("a.txt"), "current").unwrap();

        let mut journal = journal_file(tmp.path());
        journal
            .record("Edit a", vec![PersistedFile::text("a.txt", "old", "current")])
            .unwrap();
        journal.invalidate_path("a.txt").unwrap();
        assert!(!journal.can_undo());
        assert!(journal.step(&project, false).unwrap().is_none());
    }

    #[test]
    fn stale_content_is_skipped_not_reverted() {
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path().join("site");
        fs::create_dir_all(&project).unwrap();
        fs::write(project.join("a.txt"), "current").unwrap();
        fs::write(project.join("b.txt"), "current-b").unwrap();

        let mut journal = journal_file(tmp.path());
        journal
            .record(
                "Edit two files",
                vec![
                    PersistedFile::text("a.txt", "old-a", "current"),
                    PersistedFile::text("b.txt", "old-b", "current-b"),
                ],
            )
            .unwrap();
        // External edit only touches a.txt.
        fs::write(project.join("a.txt"), "externally edited").unwrap();
        let outcome = journal.step(&project, false).unwrap().unwrap();
        assert!(outcome.skipped.iter().any(|s| s.contains("a.txt")));
        assert_eq!(fs::read_to_string(project.join("a.txt")).unwrap(), "externally edited");
        assert_eq!(fs::read_to_string(project.join("b.txt")).unwrap(), "old-b");
    }

    #[test]
    fn journal_survives_restart() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("journal.json");
        {
            let mut journal = PersistentJournal::load(&path);
            journal
                .record("Edit", vec![PersistedFile::text("a.txt", "old", "new")])
                .unwrap();
        }
        let journal = PersistentJournal::load(&path);
        assert_eq!(journal.entries().len(), 1);
        assert!(journal.can_undo());
    }

    #[test]
    fn binary_swaps_roundtrip_through_base64() {
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path().join("site");
        fs::create_dir_all(project.join("image")).unwrap();
        fs::write(project.join("image/logo.png"), b"OLD-BYTES").unwrap();

        let mut journal = journal_file(tmp.path());
        journal
            .record(
                "Swap logo",
                vec![PersistedFile::binary_bytes("image/logo.png", b"OLD-BYTES", b"NEW-BYTES")],
            )
            .unwrap();
        journal.step(&project, false).unwrap().unwrap();
        assert_eq!(fs::read(project.join("image/logo.png")).unwrap(), b"OLD-BYTES");
        journal.step(&project, true).unwrap().unwrap();
        assert_eq!(fs::read(project.join("image/logo.png")).unwrap(), b"NEW-BYTES");
    }

    #[test]
    fn oversized_binary_is_refused() {
        let tmp = tempfile::tempdir().unwrap();
        let mut journal = journal_file(tmp.path());
        let big = vec![0u8; MAX_BINARY_SNAPSHOT + 1];
        let result = journal.record(
            "too large",
            vec![PersistedFile::binary_bytes("big.bin", &big, &big)],
        );
        assert!(matches!(result, Err(JournalError::BinaryTooLarge)));
    }
}
