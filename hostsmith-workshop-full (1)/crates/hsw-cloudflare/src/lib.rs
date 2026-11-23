//! HostSmith Workshop Cloudflare integration (spec Section 15, TL-03, TL-04).
//!
//! The Pages deployment uses the documented native REST asset-upload flow
//! (upload token, check-missing, upload, upsert-hashes, create deployment)
//! as the primary implementation; the Wrangler sidecar is a documented
//! fallback behind the same adapter interface and is never required.
//! Workers deployment is a separate, explicitly separated module (TL-04).
//! Tokens are only ever stored in the OS credential store (Windows
//! Credential Manager); a non-persistent in-memory store exists solely for
//! tests and non-Windows development builds.

pub mod hashing;
pub mod manifest;
pub mod pages;
pub mod store;
pub mod workers;
