//! HostSmith Workshop image operations (spec Sections 6 and 12, TL-07).
//!
//! Lossless operations (resize to new file, crop, rotate, PNG optimization,
//! lossless WebP) write new files and never touch originals (MR-04); the
//! one lossy-capable path (JPEG re-encode) is labelled and always returns
//! before/after sizes so the interface can show honest comparisons.

pub mod ops;
