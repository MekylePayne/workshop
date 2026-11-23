//! HostSmith Workshop local technical SEO analysis (spec Section 10, TL-08).
//!
//! On-page technical checks only - no ranking predictions, no Core Web
//! Vitals, no off-page signals. Every finding carries a severity, an
//! explanation and a recommendation; automatic fixes emit reversible edits
//! through the patch pipeline, guided fixes require user input first.

pub mod checks;
pub mod fixes;
