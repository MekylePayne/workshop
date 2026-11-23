//! Application state: the open project session and its resources.
//!
//! The shell stays a thin adapter (review 6.2): state here only holds what
//! lives across commands - the project root, the preview server, the file
//! watcher and the paths of the per-project journal and backups in the
//! application data directory.

use hsw_core::persist::PersistentJournal;
use hsw_core::watcher::WatchHandle;
use hsw_server::server::PreviewServer;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub struct ProjectSession {
    pub root: PathBuf,
    pub app_data_dir: PathBuf,
    pub server: Option<PreviewServer>,
    pub watcher: Option<WatchHandle>,
    /// Relative paths the application itself just wrote; the watcher skips
    /// them so app writes are not mistaken for external edits (MR-08).
    pub suppress: Arc<Mutex<HashSet<String>>>,
}

impl ProjectSession {
    pub fn journal_path(&self) -> PathBuf {
        self.app_data_dir.join("journal.json")
    }

    pub fn backups_dir(&self) -> PathBuf {
        self.app_data_dir.join("backups")
    }

    pub fn journal(&self) -> PersistentJournal {
        PersistentJournal::load(&self.journal_path())
    }

    /// Marks a relative path as just-written so the watcher ignores it.
    pub fn suppress_path(&self, rel: &str) {
        self.suppress.lock().expect("suppress mutex").insert(rel.to_string());
    }
}

#[derive(Default)]
pub struct AppState {
    pub session: Mutex<Option<ProjectSession>>,
}

/// Per-project directory name inside the application data directory: a
/// stable content hash of the canonical project path.
pub fn project_dir_name(root: &Path) -> String {
    let canonical = root
        .canonicalize()
        .unwrap_or_else(|_| root.to_path_buf());
    format!("{:016x}", hsw_core::fsutil::content_hash(canonical.to_string_lossy().as_bytes()))
}
