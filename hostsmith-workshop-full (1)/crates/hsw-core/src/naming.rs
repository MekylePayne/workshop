//! Deterministic asset naming (review finding PR-02).
//!
//! Slug rule: lowercase ASCII; spaces, underscores, dots and hyphens become
//! single hyphens; every other character that is not a lowercase letter or a
//! digit is removed; hyphen runs collapse; leading and trailing hyphens are
//! trimmed. Numerals and all words are kept, so the specification example
//! `My New Product Photo 2026.png` becomes `my-new-product-photo-2026.png`.
//! (The specification's shorter example, `product-photo.png`, drops tokens
//! without stating a rule; PR-02 recommends this explicit, reversible rule
//! instead. Non-ASCII letters are currently removed and must be documented
//! in the interface before Phase 2.)

/// Slugifies a filename stem according to the PR-02 rule: spaces,
/// underscores, dots and hyphens become single hyphens; every other unsafe
/// or non-ASCII character is removed entirely.
pub fn slugify_stem(original_stem: &str) -> String {
    let mut out = String::new();
    let mut last_hyphen = false;
    for ch in original_stem.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            last_hyphen = false;
        } else if matches!(ch, ' ' | '_' | '.' | '-') {
            if !last_hyphen && !out.is_empty() {
                out.push('-');
            }
            last_hyphen = true;
        }
        // Every other character (unsafe or non-ASCII) is removed.
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() {
        "asset".to_string()
    } else {
        out
    }
}

/// Splits an original filename into `(slugged stem, lowercase extension)`.
/// The extension keeps only ASCII alphanumerics.
pub fn asset_filename(original: &str) -> (String, String) {
    let (stem, raw_ext) = match original.rfind('.') {
        Some(idx) if idx > 0 && idx < original.len() - 1 => (&original[..idx], &original[idx + 1..]),
        _ => (original, ""),
    };
    let ext: String = raw_ext
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect();
    (slugify_stem(stem), ext)
}

/// Returns `desired` or, when it collides with an existing name, a numbered
/// variant `stem-2`, `stem-3`, ... before the extension. Comparison is
/// case-insensitive because Windows file systems are case-insensitive.
/// Falls back to a timestamp suffix in the (unreachable) case of 10,000
/// collisions, so a call never loops forever.
pub fn unique_filename(desired: &str, existing: &[String]) -> String {
    if !existing.iter().any(|e| e.eq_ignore_ascii_case(desired)) {
        return desired.to_string();
    }
    let (stem, ext) = match desired.rfind('.') {
        Some(i) if i > 0 => (&desired[..i], &desired[i..]),
        _ => (desired, ""),
    };
    for n in 2..10_000 {
        let candidate = format!("{}-{}{}", stem, n, ext);
        if !existing.iter().any(|e| e.eq_ignore_ascii_case(&candidate)) {
            return candidate;
        }
    }
    let fallback = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    format!("{}-{}{}", stem, fallback, ext)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn specification_example_keeps_numerals_and_words() {
        // PR-02: the documented rule keeps all tokens, unlike the spec's
        // illustrative (rule-less) example.
        assert_eq!(
            asset_filename("My New Product Photo 2026.png"),
            ("my-new-product-photo-2026".to_string(), "png".to_string())
        );
    }

    #[test]
    fn unsafe_characters_are_removed() {
        assert_eq!(
            asset_filename("Photo 1 & 2.PNG"),
            ("photo-1-2".to_string(), "png".to_string())
        );
        assert_eq!(
            asset_filename("video: final/render?.mp4"),
            ("video-finalrender".to_string(), "mp4".to_string())
        );
    }

    #[test]
    fn path_separators_never_survive() {
        // A stem containing slashes cannot smuggle a path into /image/.
        let (stem, _ext) = asset_filename("..\\..\\evil.png");
        assert!(!stem.contains('\\'));
        assert!(!stem.contains('/'));
        assert!(!stem.contains(".."));
    }

    #[test]
    fn non_ascii_letters_are_removed_and_documented() {
        assert_eq!(
            asset_filename("Produkt Fr\u{00f6}hlich.png"),
            ("produkt-frhlich".to_string(), "png".to_string())
        );
    }

    #[test]
    fn empty_stem_falls_back() {
        assert_eq!(asset_filename("???"), ("asset".to_string(), String::new()));
    }

    #[test]
    fn collisions_get_numbered_names_case_insensitive() {
        let existing = vec!["photo.png".to_string(), "photo-2.PNG".to_string()];
        assert_eq!(unique_filename("photo.png", &existing), "photo-3.png");
        let empty: Vec<String> = vec![];
        assert_eq!(unique_filename("photo.png", &empty), "photo.png");
    }
}
