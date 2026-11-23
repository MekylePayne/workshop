//! HostSmith Workshop developer utilities (spec Section 12).
//!
//! Rust-side tools: JSON format/validate, HTML/CSS/JS minification, QR code
//! generation and image palette extraction. Formatting-heavy tools (pretty
//! HTML/CSS/JS formatting, markdown preview, diff view) run in the
//! interface layer with small bundled JavaScript libraries, as the
//! architecture's toolbox asymmetry decision documents; the command
//! adapter hides the split from the UI.

pub mod minify;
pub mod palette;
pub mod qr;
