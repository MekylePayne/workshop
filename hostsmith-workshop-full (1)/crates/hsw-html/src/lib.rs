//! HostSmith Workshop HTML analysis queries (spec Sections 9, 10, 11, 13).
//!
//! Read-only queries over parsed documents, built on html5ever via scraper.
//! Malformed HTML parses per html5ever rules; every query degrades to
//! "absent" instead of erroring (TL-06 behaviour is documented in the
//! interface).

pub mod queries;
