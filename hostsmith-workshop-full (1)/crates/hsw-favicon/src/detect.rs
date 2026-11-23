//! Favicon detection (spec Section 9): reads the real references and files
//! of a page and reports what exists, what is referenced but missing, and
//! what should be generated.

use hsw_core::fsutil::{resolve_reference, safe_join};
use hsw_html::queries::{summarize, IconRef};
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Default)]
pub struct FaviconDetection {
    /// link[rel] entries whose rel contains "icon" (favicon links).
    pub icons: Vec<IconRef>,
    pub apple_touch_icons: Vec<IconRef>,
    pub manifest: Option<String>,
    pub favicon_ico_file: bool,
    /// Referenced local hrefs that do not resolve to an existing file.
    pub missing_files: Vec<String>,
    pub issues: Vec<String>,
}

/// Detects the favicon situation of one page. `page_dir` is the directory
/// of the page ("" for the root), used to resolve relative hrefs.
pub fn detect(project_root: &Path, page_html: &str, page_dir: &str) -> FaviconDetection {
    let summary = summarize(page_html);
    let mut detection = FaviconDetection::default();

    for link in &summary.link_rels {
        if link.rel.contains("apple-touch-icon") {
            detection.apple_touch_icons.push(link.clone());
        } else if link.rel.contains("icon") {
            detection.icons.push(link.clone());
        }
    }
    detection.manifest = summary.manifest_link.clone();
    detection.favicon_ico_file = project_root.join("favicon.ico").is_file();

    let mut referenced: Vec<&IconRef> = detection.icons.iter().collect();
    referenced.extend(detection.apple_touch_icons.iter());
    for link in &referenced {
        let resolved = resolve_reference(page_dir, &link.href);
        if resolved.starts_with("data:") || resolved.contains("://") || resolved.starts_with("//") {
            continue;
        }
        match safe_join(project_root, Path::new(&resolved)) {
            Ok(path) if !path.is_file() => detection.missing_files.push(link.href.clone()),
            Err(_) => detection.missing_files.push(link.href.clone()),
            Ok(_) => {}
        }
    }

    if detection.icons.is_empty() && detection.apple_touch_icons.is_empty() {
        detection
            .issues
            .push("no favicon references found in this page's head".to_string());
    }
    if !detection.missing_files.is_empty() {
        detection.issues.push(format!(
            "{} referenced icon file(s) are missing: {}",
            detection.missing_files.len(),
            detection.missing_files.join(", ")
        ));
    }
    if !detection.favicon_ico_file {
        detection
            .issues
            .push("no favicon.ico at the project root (older browsers and some crawlers request it)".to_string());
    }
    if detection.manifest.is_none() {
        detection
            .issues
            .push("no web app manifest linked (optional for a favicon, needed for installable PWAs)".to_string());
    }
    detection
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn detects_references_files_and_gaps() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("favicon.ico"), b"ico").unwrap();
        let page = r#"<html><head>
            <link rel="icon" href="/favicon.ico">
            <link rel="apple-touch-icon" href="/apple-touch-icon.png">
            <link rel="manifest" href="/manifest.webmanifest">
        </head><body></body></html>"#;
        let detection = detect(tmp.path(), page, "");
        assert_eq!(detection.icons.len(), 1);
        assert_eq!(detection.apple_touch_icons.len(), 1);
        assert!(detection.manifest.is_some());
        assert!(detection.favicon_ico_file);
        assert_eq!(detection.missing_files, vec!["/apple-touch-icon.png"]);
        assert!(detection.issues.iter().any(|i| i.contains("apple-touch-icon")));
    }

    #[test]
    fn a_page_without_icons_reports_the_gap() {
        let tmp = tempfile::tempdir().unwrap();
        let detection = detect(tmp.path(), "<html><head><title>x</title></head></html>", "");
        assert!(detection.icons.is_empty());
        assert!(detection.issues.iter().any(|i| i.contains("no favicon references")));
        assert!(!detection.favicon_ico_file);
    }
}
