//! HostSmith Workshop local preview server (spec Section 5, TL-09).
//!
//! - Binds 127.0.0.1 on a random port, never file:// URLs.
//! - Resolves every request path strictly inside the project root.
//! - Serves the per-folder index.html for directory paths.
//! - Applies `_headers` rules so previews match deployment behaviour.
//! - Disables caching and stops when the project closes.
//! - Injects the selection bridge script and element annotations.

pub mod headers;
pub mod server;
