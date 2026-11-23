//! Byte-preserving CSS reference scanning and rewriting (TL-06).
//!
//! A full CSS parse-serialize round trip would reformat untouched rules, so
//! reference updates scan for `url(...)` and `@import "..."` tokens and
//! replace only the path text inside them. Everything else - comments,
//! indentation, unknown syntax - passes through byte for byte.

use hsw_core::fsutil::{relative_reference, resolve_reference};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CssReference {
    /// Byte range of the path text inside the token (without quotes and
    /// surrounding whitespace).
    pub start: usize,
    pub end: usize,
    /// The raw path text exactly as written.
    pub raw_path: String,
    /// Project-root-relative resolved path; external references (data:,
    /// http(s):, //host, #fragment) resolve to themselves.
    pub resolved: String,
}

fn is_external(raw: &str) -> bool {
    let lower = raw.to_ascii_lowercase();
    lower.starts_with("data:")
        || lower.starts_with("http://")
        || lower.starts_with("https://")
        || lower.starts_with("//")
        || lower.starts_with('#')
}

fn is_ascii_space(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n' | b'\r' | 0x0c)
}

fn extract_path(css: &str, open: usize, close: usize) -> Option<CssReference> {
    let bytes = css.as_bytes();
    let mut start = open;
    let mut end = close;
    while start < end && is_ascii_space(bytes[start]) {
        start += 1;
    }
    while end > start && is_ascii_space(bytes[end - 1]) {
        end -= 1;
    }
    if end > start && (bytes[start] == b'"' || bytes[start] == b'\'') && bytes[end - 1] == bytes[start] {
        start += 1;
        end -= 1;
    }
    if start >= end {
        return None;
    }
    Some(CssReference {
        start,
        end,
        raw_path: css[start..end].to_string(),
        resolved: String::new(),
    })
}

/// Scans `css` for `url(...)` and `@import "..."` references and resolves
/// each against `css_dir` (the directory of the stylesheet).
pub fn collect_references(css: &str, css_dir: &str) -> Vec<CssReference> {
    let bytes = css.as_bytes();
    let mut refs = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i..].len() >= 4 && bytes[i..i + 4].eq_ignore_ascii_case(b"url(") {
            if let Some(close) = css[i + 4..].find(')').map(|offset| i + 4 + offset) {
                if let Some(mut reference) = extract_path(css, i + 4, close) {
                    reference.resolved = if is_external(&reference.raw_path) {
                        reference.raw_path.clone()
                    } else {
                        resolve_reference(css_dir, &reference.raw_path)
                    };
                    refs.push(reference);
                }
                i = close + 1;
                continue;
            }
        }
        if bytes[i..].len() >= 7 && bytes[i..i + 7].eq_ignore_ascii_case(b"@import") {
            // @import "theme.css"; or @import 'theme.css';
            let mut cursor = i + 7;
            while cursor < bytes.len() && is_ascii_space(bytes[cursor]) {
                cursor += 1;
            }
            if cursor < bytes.len() && (bytes[cursor] == b'"' || bytes[cursor] == b'\'') {
                let quote = bytes[cursor];
                let mut end = cursor + 1;
                while end < bytes.len() && bytes[end] != quote {
                    end += 1;
                }
                if end < bytes.len() {
                    let reference = CssReference {
                        start: cursor + 1,
                        end,
                        raw_path: css[cursor + 1..end].to_string(),
                        resolved: if is_external(&css[cursor + 1..end]) {
                            css[cursor + 1..end].to_string()
                        } else {
                            resolve_reference(css_dir, &css[cursor + 1..end])
                        },
                    };
                    refs.push(reference);
                    i = end + 1;
                    continue;
                }
            }
        }
        i += 1;
    }
    refs
}

/// The replacement path text for one reference: site-absolute references
/// stay site-absolute, relative references stay relative to `base_dir`.
pub fn replacement_value(old_raw: &str, base_dir: &str, new_target: &str) -> String {
    if old_raw.starts_with('/') {
        format!("/{new_target}")
    } else {
        relative_reference(base_dir, new_target)
    }
}

/// Replaces every reference that resolves to `old_resolved` with
/// `new_value` (see [`replacement_value`]). All other bytes are preserved.
pub fn update_references(
    css: &str,
    css_dir: &str,
    old_resolved: &str,
    new_value: &str,
) -> String {
    let mut out = String::with_capacity(css.len() + 32);
    let mut last = 0;
    for reference in collect_references(css, css_dir) {
        if reference.resolved == old_resolved && reference.raw_path != new_value {
            out.push_str(&css[last..reference.start]);
            out.push_str(new_value);
            last = reference.end;
        }
    }
    out.push_str(&css[last..]);
    out
}

/// Replaces every reference that resolves to `old_resolved`, keeping each
/// reference's original style (site-absolute stays site-absolute, relative
/// stays relative to `base_dir`), pointing at `new_target`.
pub fn update_references_to_target(
    css: &str,
    base_dir: &str,
    old_resolved: &str,
    new_target: &str,
) -> String {
    let mut out = String::with_capacity(css.len() + 32);
    let mut last = 0;
    for reference in collect_references(css, base_dir) {
        if reference.resolved == old_resolved && reference.raw_path != new_target {
            let replacement = replacement_value(&reference.raw_path, base_dir, new_target);
            if reference.raw_path != replacement {
                out.push_str(&css[last..reference.start]);
                out.push_str(&replacement);
                last = reference.end;
            }
        }
    }
    out.push_str(&css[last..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const CSS: &str = "/* Layout - caf\u{e9} */\nbody {\n  background: url(\"../image/logo.png\") no-repeat;\n  border-image: url(../image/logo.png) 30;\n}\n@import 'theme.css';\n.hero { background-image: url(\"https://cdn.example/remote.png\"); }\n";

    #[test]
    fn references_resolve_relative_to_the_stylesheet() {
        let refs = collect_references(CSS, "assets/css");
        let resolved: Vec<String> = refs.iter().map(|r| r.resolved.clone()).collect();
        // The stylesheet lives in assets/css, so ../image/ resolves to
        // assets/image/ - the resolution math, not wishful thinking.
        assert!(resolved.contains(&"assets/image/logo.png".to_string()));
        assert!(resolved.contains(&"assets/css/theme.css".to_string()));
        assert!(resolved.contains(&"https://cdn.example/remote.png".to_string()));
    }

    #[test]
    fn update_preserves_every_byte_except_the_changed_paths() {
        let updated =
            update_references(CSS, "assets/css", "assets/image/logo.png", "../image/new-logo.png");
        assert!(updated.contains("url(\"../image/new-logo.png\") no-repeat"));
        assert!(updated.contains("url(../image/new-logo.png) 30"));
        assert!(updated.contains("@import 'theme.css';"));
        assert!(updated.contains("https://cdn.example/remote.png"));
        assert!(updated.contains("/* Layout - caf\u{e9} */"), "non-ASCII bytes preserved");
        // Untouched whitespace and newlines survive.
        assert!(updated.contains("body {\n  background:"));
    }

    #[test]
    fn update_is_idempotent_when_already_current() {
        let updated = update_references(CSS, "assets/css", "assets/image/logo.png", "../image/logo.png");
        assert_eq!(updated, CSS);
    }

    #[test]
    fn replacement_value_preserves_reference_style() {
        assert_eq!(
            replacement_value("/image/logo.png", "about", "image/new.png"),
            "/image/new.png"
        );
        assert_eq!(
            replacement_value("../image/logo.png", "about", "image/new.png"),
            "../image/new.png"
        );
    }

    #[test]
    fn targeted_update_keeps_each_references_style() {
        let css = ".a{background:url(\"/image/logo.png\")}\n.b{background:url(\"../image/logo.png\")}\n.c{background:url(\"/image/logo.png\")}";
        let updated = update_references_to_target(css, "about", "image/logo.png", "image/new-logo.png");
        assert!(updated.contains(".a{background:url(\"/image/new-logo.png\")}"));
        assert!(updated.contains(".b{background:url(\"../image/new-logo.png\")}"));
        assert!(updated.contains(".c{background:url(\"/image/new-logo.png\")}"));
        assert!(!updated.contains("image/logo.png"), "old reference fully gone");
    }

    #[test]
    fn quoted_and_unquoted_forms_are_both_found() {
        let css = "a{background:url( x.png )}\nb{background:url('y.png')}\nc{background:url(\"z.png\")}";
        let refs = collect_references(css, "");
        let raw: Vec<String> = refs.iter().map(|r| r.raw_path.clone()).collect();
        assert_eq!(raw, vec!["x.png", "y.png", "z.png"]);
    }
}
