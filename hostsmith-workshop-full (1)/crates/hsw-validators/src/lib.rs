//! HostSmith Workshop structure validator with HostSmith and generic profiles.
//!
//! Two profiles: a generic static-site profile (advisory) and the HostSmith
//! profile (enforcing /image/ and the footer badge). The validator only
//! reports - it never restructures a client's website (spec Section 13).

pub mod structure;
