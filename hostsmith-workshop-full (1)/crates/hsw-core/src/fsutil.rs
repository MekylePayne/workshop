//! Filesystem helpers shared by every crate (TL-09, spec Sections 2 and 3).
//!
//! Every path that combines the project root with a user- or archive-supplied
//! relative path goes through [`safe_join`], which refuses absolute paths,
//! parent components and drive-letter components, so no project operation can
//! write outside the project root.

use std::path::{Component, Path, PathBuf};

/// Joins `rel` onto `root`, refusing anything that escapes `root`:
/// parent components, absolute paths, and drive-letter / NTFS-stream
/// components (`c:`, `file.txt:stream`) so Windows-authored names can
/// never point outside the project.
pub fn safe_join(root: &Path, rel: &Path) -> std::io::Result<PathBuf> {
    let mut out = root.to_path_buf();
    for component in rel.components() {
        match component {
            Component::Normal(part) => {
                let text = part.to_string_lossy();
                if text.contains(':') {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        format!("colon in path component {rel:?}"),
                    ));
                }
                out.push(part);
            }
            Component::CurDir => {}
            other => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!("unsafe path component in {rel:?}: {other:?}"),
                ))
            }
        }
    }
    Ok(out)
}

/// Formats a relative path with forward slashes, for journals and manifests.
pub fn slash_separated(rel: &Path) -> String {
    let text = rel.to_string_lossy().replace('\\', "/");
    text.trim_end_matches('/').to_string()
}

/// Stable 64-bit FNV-1a content hash, used to detect files whose content no
/// longer matches a journal snapshot (MR-08). Not a cryptographic hash.
pub fn content_hash(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn normalize_parts(path: &str) -> String {
    let mut stack: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                stack.pop();
            }
            other => stack.push(other),
        }
    }
    stack.join("/")
}

/// Resolves an asset reference found in a page or stylesheet against the
/// directory of the file that contains it. Site-absolute references
/// ("/image/x.png") resolve from the project root; external references
/// (data:, http:, //host/...) are returned unchanged.
pub fn resolve_reference(base_dir: &str, reference: &str) -> String {
    let reference = reference.trim();
    if let Some(stripped) = reference.strip_prefix('/') {
        return normalize_parts(stripped);
    }
    let lower = reference.to_ascii_lowercase();
    if lower.starts_with("data:")
        || lower.starts_with("http://")
        || lower.starts_with("https://")
        || lower.starts_with("//")
        || lower.starts_with('#')
        || lower.starts_with("mailto:")
        || lower.starts_with("tel:")
    {
        return reference.to_string();
    }
    if base_dir.is_empty() {
        normalize_parts(reference)
    } else {
        normalize_parts(&format!("{base_dir}/{reference}"))
    }
}

/// Builds the relative reference from `from_dir` (the directory of the file
/// that will contain the reference) to `to` (project-root relative), e.g.
/// ("about", "image/logo.png") becomes "../image/logo.png".
pub fn relative_reference(from_dir: &str, to: &str) -> String {
    let from: Vec<&str> = from_dir
        .split('/')
        .filter(|part| !part.is_empty() && *part != ".")
        .collect();
    let to_parts: Vec<&str> = to.split('/').filter(|part| !part.is_empty()).collect();
    let common = from
        .iter()
        .zip(to_parts.iter())
        .take_while(|(a, b)| a == b)
        .count();
    let mut out = String::new();
    for _ in 0..(from.len() - common) {
        if !out.is_empty() {
            out.push('/');
        }
        out.push_str("..");
    }
    for part in &to_parts[common..] {
        if !out.is_empty() {
            out.push('/');
        }
        out.push_str(part);
    }
    if out.is_empty() {
        out.push_str(to_parts.last().copied().unwrap_or("."));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn normal_relative_paths_are_joined() {
        let root = Path::new("/tmp/proj");
        let joined = safe_join(root, Path::new("assets/./css/style.css")).unwrap();
        assert_eq!(joined, Path::new("/tmp/proj/assets/css/style.css"));
    }

    #[test]
    fn traversal_is_rejected() {
        let root = Path::new("/tmp/proj");
        assert!(safe_join(root, Path::new("../escape.txt")).is_err());
        assert!(safe_join(root, Path::new("site/../../escape.txt")).is_err());
        assert!(safe_join(root, Path::new("/etc/passwd")).is_err());
    }

    #[test]
    fn windows_drive_letters_are_rejected() {
        let root = Path::new("/tmp/proj");
        assert!(safe_join(root, Path::new("c:/evil.txt")).is_err());
    }

    #[test]
    fn slash_separated_normalises_separators_and_trailing_slash() {
        assert_eq!(slash_separated(Path::new("assets/css/style.css")), "assets/css/style.css");
        assert_eq!(slash_separated(Path::new("site/")), "site");
    }

    #[test]
    fn content_hash_is_stable_and_sensitive() {
        assert_eq!(content_hash(b"hello"), content_hash(b"hello"));
        assert_ne!(content_hash(b"hello"), content_hash(b"hellp"));
    }

    #[test]
    fn references_resolve_against_the_containing_directory() {
        assert_eq!(resolve_reference("about", "../image/logo.png"), "image/logo.png");
        assert_eq!(resolve_reference("", "assets/css/style.css"), "assets/css/style.css");
        assert_eq!(resolve_reference("about", "/image/logo.png"), "image/logo.png");
        assert_eq!(resolve_reference("about", "./contact.html"), "about/contact.html");
    }

    #[test]
    fn external_references_pass_through() {
        assert_eq!(resolve_reference("about", "https://cdn.example/x.png"), "https://cdn.example/x.png");
        assert_eq!(resolve_reference("about", "data:image/png;base64,AAA"), "data:image/png;base64,AAA");
        assert_eq!(resolve_reference("about", "#anchor"), "#anchor");
    }

    #[test]
    fn relative_references_point_from_page_to_target() {
        assert_eq!(relative_reference("about", "image/logo.png"), "../image/logo.png");
        assert_eq!(relative_reference("", "image/logo.png"), "image/logo.png");
        assert_eq!(relative_reference("about", "about/team.html"), "team.html");
        assert_eq!(relative_reference("assets/css", "assets/img/x.png"), "../img/x.png");
    }
}
