//! HostSmith Workshop core crate.
//!
//! Pure Rust, no Tauri dependency, headless-testable (review Section 6.2).
//! Modules:
//! - [`naming`]: deterministic asset naming with collision handling (PR-02).
//! - [`zipguard`]: safe ZIP import: traversal protection, caps, single-root
//!   detection (spec Section 3, TL-09).
//! - [`patch`]: byte-range edit model with inverse operations and the undo
//!   journal (PR-03, MR-03).
//! - [`project`]: read-only project scan and structure classification
//!   (spec Section 2, Phase 1).
//! - [`fsutil`]: path-containment helpers and reference resolution.
//! - [`backup`]: timestamped ZIP backups with restore and retention (MR-02).
//! - [`persist`]: restart-safe operation journal with external-change
//!   invalidation (MR-03, MR-08).
//! - [`watcher`]: live file watching with debounced callbacks (MR-08).

pub mod backup;
pub mod fsutil;
pub mod naming;
pub mod patch;
pub mod persist;
pub mod project;
pub mod watcher;
pub mod zipguard;
