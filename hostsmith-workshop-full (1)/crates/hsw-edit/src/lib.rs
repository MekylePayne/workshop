//! HostSmith Workshop editing engine.
//!
//! Every source modification goes through this crate so that the guarantees
//! of Sections 5 and 14 hold: untouched bytes are preserved (streaming
//! rewrites only), changes are previewable as diffs, and every operation is
//! reversible through the journal in `hsw-core::persist`.
//!
//! Modules:
//! - [`rewrite`]: element/text targeting on the original source (lol_html).
//! - [`css`]: byte-preserving CSS reference scanning and rewriting.
//! - [`import`]: safe asset import with deterministic naming (PR-02).
//! - [`diff`]: unified diffs for the confirmation step (PR-03).

pub mod css;
pub mod diff;
pub mod import;
pub mod rewrite;
