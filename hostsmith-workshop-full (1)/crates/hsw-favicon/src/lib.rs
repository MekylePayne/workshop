//! HostSmith Workshop favicon tooling (spec Section 9).
//!
//! Detection reads real references and files; generation writes real files
//! (multi-size ICO plus PNG sizes and web manifest entries) derived from a
//! source image the user chooses. Tab and search-result previews are
//! clearly labelled simulations; the Google search-result preview in
//! particular is a drawing, not data from Google.

pub mod detect;
pub mod generate;
pub mod ico;
