//! Live file watching with debounced callbacks (MR-08, spec Section 5).
//!
//! The watcher reports relative paths of files created, modified or removed
//! inside the project root. The application layer decides what to do with
//! them: invalidate journal entries, refresh the preview, re-run checks. The
//! watcher deliberately does not distinguish who wrote the file; the
//! application suppresses paths it just wrote itself.

use std::collections::HashSet;
use notify::Watcher as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

/// Handle for a running watcher. Dropping it does not stop the thread;
/// call [`WatchHandle::stop`] for a clean shutdown.
pub struct WatchHandle {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    // Must outlive the notification thread; dropped after `stop` joins.
    _watcher: Option<notify::RecommendedWatcher>,
}

impl WatchHandle {
    /// Signals the thread to finish and waits for it.
    pub fn stop(mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(handle) = self.thread.take() {
            let _ = handle.join();
        }
    }
}

/// Watches `root` recursively. After every burst of events that stays quiet
/// for `quiet_period`, `on_change` receives the deduplicated relative paths.
pub fn watch_project<F>(
    root: &Path,
    quiet_period: Duration,
    on_change: F,
) -> std::io::Result<WatchHandle>
where
    F: Fn(Vec<PathBuf>) + Send + 'static,
{
    let (tx, rx) = mpsc::channel();
    let mut watcher = notify::recommended_watcher(
        move |res: Result<notify::Event, notify::Error>| {
            if let Ok(event) = res {
                if matches!(
                    event.kind,
                    notify::EventKind::Modify(_)
                        | notify::EventKind::Create(_)
                        | notify::EventKind::Remove(_)
                ) {
                    for path in event.paths {
                        let _ = tx.send(path);
                    }
                }
            }
        },
    )
    .map_err(|err| std::io::Error::other(format!("watcher init failed: {err}")))?;
    watcher
        .watch(root, notify::RecursiveMode::Recursive)
        .map_err(|err| std::io::Error::other(format!("watch failed: {err}")))?;

    let stop = Arc::new(AtomicBool::new(false));
    let stop_clone = stop.clone();
    let root_buf = root.to_path_buf();

    let thread = std::thread::spawn(move || {
        let mut pending: Vec<PathBuf> = Vec::new();
        let mut seen: HashSet<PathBuf> = HashSet::new();
        loop {
            if stop_clone.load(Ordering::SeqCst) {
                return;
            }
            match rx.recv_timeout(quiet_period) {
                Ok(path) => {
                    if let Ok(rel) = path.strip_prefix(&root_buf) {
                        let rel = rel.to_path_buf();
                        if seen.insert(rel.clone()) {
                            pending.push(rel);
                        }
                    }
                }
                Err(RecvTimeoutError::Timeout) => {
                    if !pending.is_empty() {
                        on_change(std::mem::take(&mut pending));
                        seen.clear();
                    }
                }
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
    });

    Ok(WatchHandle {
        stop,
        thread: Some(thread),
        _watcher: Some(watcher),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    #[test]
    fn reports_created_files_after_quiet_period() {
        let tmp = tempfile::tempdir().unwrap();
        let seen = Arc::new(Mutex::new(Vec::<PathBuf>::new()));
        let sink = seen.clone();
        let handle =
            watch_project(tmp.path(), Duration::from_millis(100), move |paths| {
                sink.lock().unwrap().extend(paths);
            })
            .unwrap();

        fs::write(tmp.path().join("hello.txt"), "hi").unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if !seen.lock().unwrap().is_empty() {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let reported = seen.lock().unwrap().clone();
        handle.stop();
        assert!(
            reported.iter().any(|p| p == &PathBuf::from("hello.txt")),
            "watcher did not report hello.txt: {reported:?}"
        );
    }

    #[test]
    fn stop_joins_the_thread() {
        let tmp = tempfile::tempdir().unwrap();
        let handle =
            watch_project(tmp.path(), Duration::from_millis(50), |_| {}).unwrap();
        handle.stop();
    }
}
