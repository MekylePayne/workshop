//! Local technical SEO checks (spec Section 10, TL-08).
//!
//! On-page technical checks only: no ranking predictions, no Core Web
//! Vitals, no off-page signals - the interface states this scope verbatim.
//! Every finding carries a severity, a concrete explanation and a
//! recommendation. Automatic fixes (viewport, lang) are flagged with an
//! [`AutoFix`] kind; guided fixes (title, description, canonical) are marked
//! `needs_input` and collect their value in the interface first.

use hsw_core::fsutil::{resolve_reference, safe_join, slash_separated};
use hsw_core::project::ProjectScan;
use hsw_html::queries::{self, PageSummary};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
    Passed,
    Info,
}

/// Automatic fixes the pipeline can apply without further input; every fix
/// still goes through the diff-confirmation and journal pipeline (PR-03).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AutoFix {
    Viewport,
    Lang,
}

#[derive(Debug, Clone, Serialize)]
pub struct Finding {
    pub id: &'static str,
    pub severity: Severity,
    /// Page the finding belongs to, project-relative; "site" for site-wide.
    pub page: String,
    pub message: String,
    pub recommendation: String,
    pub auto_fix: Option<AutoFix>,
    pub needs_input: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Analysis {
    pub findings: Vec<Finding>,
    pub pages_analyzed: usize,
}

fn finding(
    id: &'static str,
    severity: Severity,
    page: &str,
    message: String,
    recommendation: String,
    auto_fix: Option<AutoFix>,
    needs_input: bool,
) -> Finding {
    Finding { id, severity, page: page.to_string(), message, recommendation, auto_fix, needs_input }
}

fn load_page_summary(root: &Path, index_file: &Path) -> Option<PageSummary> {
    let path = safe_join(root, index_file).ok()?;
    let text = std::fs::read_to_string(path).ok()?;
    Some(queries::summarize(&text))
}

fn page_dir(folder: &Path) -> String {
    slash_separated(folder)
}

/// Analyses every page in the scan plus the site-level files.
pub fn analyze_project(root: &Path, scan: &ProjectScan) -> Analysis {
    let mut findings: Vec<Finding> = Vec::new();
    let mut pages = Vec::new();

    for page in &scan.pages {
        let Some(summary) = load_page_summary(root, &page.index_file) else {
            findings.push(finding(
                "SEO-READ",
                Severity::Warning,
                &page_dir(&page.folder),
                "page could not be read or decoded as UTF-8".into(),
                "Check the file encoding; the page was skipped for analysis.".into(),
                None,
                false,
            ));
            continue;
        };
        let dir = page_dir(&page.folder);
        analyze_page(&summary, &dir, root, &mut findings);
        pages.push((dir, summary));
    }

    check_duplicates(&pages, &mut findings);
    check_site_files(root, scan, &mut findings);

    // Passed entries keep the report balanced (the interface shows them
    // collapsed by default).
    for passed in [
        ("SEO-01", "every page has a title tag"),
        ("SEO-03", "every page has a meta description"),
        ("SEO-05", "every page has exactly one h1"),
        ("SEO-07", "every image has an alt attribute"),
        ("SEO-10", "every page has a viewport meta tag"),
        ("SITE-01", "robots.txt present"),
        ("SITE-02", "sitemap.xml present"),
    ] {
        let relevant = findings.iter().any(|f| f.id == passed.0 && f.severity != Severity::Passed);
        if !relevant {
            findings.push(finding(
                passed.0,
                Severity::Passed,
                "site",
                passed.1.into(),
                "No action needed.".into(),
                None,
                false,
            ));
        }
    }

    findings.sort_by(|a, b| a.id.cmp(b.id).then(a.page.cmp(&b.page)));
    Analysis { findings, pages_analyzed: pages.len() }
}

fn analyze_page(summary: &PageSummary, dir: &str, root: &Path, findings: &mut Vec<Finding>) {
    // Title
    match &summary.title {
        None => findings.push(finding(
            "SEO-01",
            Severity::Error,
            dir,
            "no title tag found".into(),
            "Add a title element describing the page in 15-65 characters.".into(),
            None,
            true,
        )),
        Some(title) => {
            let length = title.chars().count();
            if !(15..=65).contains(&length) {
                findings.push(finding(
                    "SEO-02",
                    Severity::Warning,
                    dir,
                    format!("title is {length} characters (recommended 15-65)"),
                    "Shorten or extend the title; the visible search-result title is truncated outside this range.".into(),
                    None,
                    true,
                ));
            }
        }
    }

    // Meta description
    match &summary.description {
        None => findings.push(finding(
            "SEO-03",
            Severity::Error,
            dir,
            "no meta description found".into(),
            "Add a meta description of 50-160 characters summarising the page.".into(),
            None,
            true,
        )),
        Some(description) => {
            let length = description.chars().count();
            if !(50..=160).contains(&length) {
                findings.push(finding(
                    "SEO-04",
                    Severity::Warning,
                    dir,
                    format!("meta description is {length} characters (recommended 50-160)"),
                    "Rewrite the description to fit the recommended range.".into(),
                    None,
                    true,
                ));
            }
        }
    }

    // H1 and heading hierarchy
    if summary.h1_count == 0 {
        findings.push(finding(
            "SEO-05",
            Severity::Error,
            dir,
            "no h1 heading found".into(),
            "Give the page exactly one top-level h1 heading.".into(),
            None,
            false,
        ));
    } else if summary.h1_count > 1 {
        findings.push(finding(
            "SEO-05",
            Severity::Error,
            dir,
            format!("{} h1 headings found (exactly one expected)", summary.h1_count),
            "Demote the additional h1 headings to h2 or merge the sections.".into(),
            None,
            false,
        ));
    }
    let mut previous_level = 0u8;
    for heading in &summary.headings {
        if previous_level > 0 && heading.level > previous_level + 1 {
            findings.push(finding(
                "SEO-06",
                Severity::Warning,
                dir,
                format!("heading level jumps from h{previous_level} to h{}", heading.level),
                "Use heading levels without skipping levels so the outline stays readable.".into(),
                None,
                false,
            ));
            break;
        }
        previous_level = heading.level;
    }

    // Images without alt
    if !summary.images_missing_alt.is_empty() {
        let preview: Vec<String> = summary
            .images_missing_alt
            .iter()
            .take(3)
            .cloned()
            .collect();
        findings.push(finding(
            "SEO-07",
            Severity::Error,
            dir,
            format!("{} image(s) missing alt attributes, e.g. {}", summary.images_missing_alt.len(), preview.join(", ")),
            "Add meaningful alt text to every image; decorative images use alt=\"\".".into(),
            None,
            true,
        ));
    }

    // Canonical
    match &summary.canonical {
        None => findings.push(finding(
            "SEO-08",
            Severity::Warning,
            dir,
            "no canonical link found".into(),
            "Add a canonical link element pointing at the page's absolute production URL.".into(),
            None,
            true,
        )),
        Some(canonical) => {
            if !canonical.starts_with("http://") && !canonical.starts_with("https://") {
                findings.push(finding(
                    "SEO-08",
                    Severity::Error,
                    dir,
                    format!("canonical URL is not absolute: {canonical}"),
                    "Canonical URLs must be absolute (https://...) to work across search engines.".into(),
                    None,
                    true,
                ));
            }
        }
    }

    // Open Graph
    let og_keys: Vec<&str> = summary.og.iter().map(|(key, _)| key.as_str()).collect();
    let missing_og: Vec<&str> = ["title", "description", "image"]
        .into_iter()
        .filter(|key| !og_keys.contains(key))
        .collect();
    if !missing_og.is_empty() {
        findings.push(finding(
            "SEO-09",
            Severity::Warning,
            dir,
            format!("Open Graph metadata missing: {}", missing_og.join(", ")),
            "Add the missing og: meta properties so link previews on social platforms render correctly.".into(),
            None,
            true,
        ));
    }

    // Viewport (auto-fixable)
    if summary.viewport.is_none() {
        findings.push(finding(
            "SEO-10",
            Severity::Error,
            dir,
            "no viewport meta tag found - mobile rendering falls back to a zoomed-out desktop layout".into(),
            "Insert the standard viewport meta tag (one click with the automatic fix).".into(),
            Some(AutoFix::Viewport),
            false,
        ));
    }

    // Language (auto-fixable)
    if summary.lang.is_none() {
        findings.push(finding(
            "SEO-11",
            Severity::Warning,
            dir,
            "no lang attribute on the html element".into(),
            "Set the document language, e.g. lang=\"en\" (one click with the automatic fix).".into(),
            Some(AutoFix::Lang),
            false,
        ));
    }

    // Favicon reference
    if !summary.link_rels.iter().any(|icon| icon.rel.contains("icon")) {
        findings.push(finding(
            "SEO-12",
            Severity::Info,
            dir,
            "no favicon link reference found".into(),
            "Use the Favicon Studio to generate icons and references.".into(),
            None,
            false,
        ));
    }

    // Broken internal links
    for link in &summary.links {
        let href = link.href.trim();
        if href.is_empty()
            || href.starts_with('#')
            || href.starts_with("http://")
            || href.starts_with("https://")
            || href.starts_with("//")
            || href.starts_with("mailto:")
            || href.starts_with("tel:")
            || resolve_reference(dir, href).starts_with("data:")
        {
            continue;
        }
        let resolved = resolve_reference(dir, href);
        let without_fragment = resolved.split('#').next().unwrap_or("").split('?').next().unwrap_or("").to_string();
        if without_fragment.is_empty() {
            continue;
        }
        let target = safe_join(root, Path::new(&without_fragment));
        let exists = match target {
            Ok(target) => {
                if target.is_file() {
                    true
                } else if target.is_dir() {
                    target.join("index.html").is_file()
                } else {
                    false
                }
            }
            Err(_) => false,
        };
        if !exists {
            findings.push(finding(
                "LINK-01",
                Severity::Error,
                dir,
                format!("broken internal link: {href}"),
                "Fix the link target or add the missing page/file.".into(),
                None,
                false,
            ));
        }
    }
}

fn check_duplicates(pages: &[(String, PageSummary)], findings: &mut Vec<Finding>) {
    let mut titles: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut descriptions: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (dir, summary) in pages {
        if let Some(title) = &summary.title {
            titles.entry(title.to_ascii_lowercase()).or_default().push(dir.clone());
        }
        if let Some(description) = &summary.description {
            descriptions
                .entry(description.to_ascii_lowercase())
                .or_default()
                .push(dir.clone());
        }
    }
    for (value, dirs) in &titles {
        if dirs.len() > 1 {
            findings.push(finding(
                "SEO-13",
                Severity::Error,
                "site",
                format!("duplicate title \"{value}\" on pages: {}", dirs.join(", ")),
                "Give each page a distinct title so search results can tell them apart.".into(),
                None,
                false,
            ));
        }
    }
    for dirs in descriptions.values() {
        if dirs.len() > 1 {
            findings.push(finding(
                "SEO-14",
                Severity::Warning,
                "site",
                format!("duplicate meta description on pages: {}", dirs.join(", ")),
                "Write per-page descriptions; duplicated descriptions compete with each other.".into(),
                None,
                false,
            ));
        }
    }
}

fn check_site_files(root: &Path, scan: &ProjectScan, findings: &mut Vec<Finding>) {
    // robots.txt
    let robots_path = root.join("robots.txt");
    if robots_path.is_file() {
        let text = std::fs::read_to_string(&robots_path).unwrap_or_default();
        if !text.to_ascii_lowercase().contains("sitemap") {
            findings.push(finding(
                "SITE-01",
                Severity::Warning,
                "site",
                "robots.txt does not reference the sitemap".into(),
                "Add a \"Sitemap: /sitemap.xml\" line so crawlers find the sitemap.".into(),
                None,
                false,
            ));
        }
    } else {
        findings.push(finding(
            "SITE-01",
            Severity::Error,
            "site",
            "robots.txt is missing".into(),
            "Add a robots.txt at the project root allowing crawlers and referencing the sitemap.".into(),
            None,
            false,
        ));
    }

    // sitemap.xml
    let sitemap_path = root.join("sitemap.xml");
    if sitemap_path.is_file() {
        let text = std::fs::read_to_string(&sitemap_path).unwrap_or_default();
        let urls = sitemap_urls(&text);
        findings.push(finding(
            "SITE-02",
            Severity::Passed,
            "site",
            format!("sitemap.xml present with {} URL entries", urls.len()),
            "Verify the URLs match the deployed domain after the first deployment.".into(),
            None,
            false,
        ));
    } else {
        findings.push(finding(
            "SITE-02",
            Severity::Error,
            "site",
            "sitemap.xml is missing".into(),
            "Add a sitemap.xml listing every page URL (one <url> entry per page folder).".into(),
            None,
            false,
        ));
    }

    // 404 page
    if !scan.not_found_page_present {
        findings.push(finding(
            "SITE-03",
            Severity::Warning,
            "site",
            "no 404.html found at the project root".into(),
            "Add a friendly 404 page; Cloudflare Pages serves it automatically for missing paths.".into(),
            None,
            false,
        ));
    }

    let _ = scan.hostsmith_structure_complete();
}

/// Extracts `<loc>` values from a sitemap without a full XML parser; the
/// file is machine-written and shallow, and malformed entries are skipped.
pub fn sitemap_urls(xml: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(start) = rest.find("<loc>") {
        let after = &rest[start + 5..];
        if let Some(end) = after.find("</loc>") {
            let url = after[..end].trim().to_string();
            if !url.is_empty() {
                out.push(url);
            }
            rest = &after[end + 6..];
        } else {
            break;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use hsw_core::project::scan_project;
    use std::fs;

    fn fixture() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        // Good page.
        fs::create_dir_all(root.join("about")).unwrap();
        fs::write(
            root.join("about/index.html"),
            "<!doctype html><html lang=\"en\"><head><title>About our workshop</title>\
             <meta name=\"description\" content=\"Everything about the HostSmith workshop studio and its tools for website builders.\">\
             <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
             <link rel=\"canonical\" href=\"https://example.com/about/\"></head>\
             <body><h1>About</h1><img src=\"../image/team.png\" alt=\"Team photo\"><a href=\"../index.html\">Home</a></body></html>",
        )
        .unwrap();
        // Problem page: missing title/description/viewport/lang/h1, broken link.
        fs::write(
            root.join("index.html"),
            "<html><head></head><body><p><a href=\"missing.html\">Gone</a></p><img src=\"x.png\"></body></html>",
        )
        .unwrap();
        fs::write(root.join("robots.txt"), "User-agent: *\nAllow: /\n").unwrap();
        tmp
    }

    #[test]
    fn finds_errors_on_the_problem_page() {
        let tmp = fixture();
        let scan = scan_project(tmp.path()).unwrap();
        let analysis = analyze_project(tmp.path(), &scan);
        let ids: Vec<&'static str> = analysis
            .findings
            .iter()
            .filter(|f| f.page == "." && f.severity == Severity::Error)
            .map(|f| f.id)
            .collect();
        for expected in ["SEO-01", "SEO-03", "SEO-05", "SEO-07", "SEO-10", "LINK-01"] {
            assert!(ids.contains(&expected), "missing {expected} in {ids:?}");
        }
        assert!(analysis.findings.iter().any(|f| f.id == "SEO-10" && f.auto_fix == Some(AutoFix::Viewport)));
    }

    #[test]
    fn good_page_only_produces_warnings_at_most() {
        let tmp = fixture();
        let scan = scan_project(tmp.path()).unwrap();
        let analysis = analyze_project(tmp.path(), &scan);
        let about_errors = analysis
            .findings
            .iter()
            .filter(|f| f.page == "about" && f.severity == Severity::Error)
            .count();
        assert_eq!(about_errors, 0, "well-formed page must be clean: {about_errors:?}");
    }

    #[test]
    fn site_files_are_checked() {
        let tmp = fixture();
        let scan = scan_project(tmp.path()).unwrap();
        let analysis = analyze_project(tmp.path(), &scan);
        assert!(analysis
            .findings
            .iter()
            .any(|f| f.id == "SITE-01" && f.message.contains("sitemap")));
        assert!(analysis
            .findings
            .iter()
            .any(|f| f.id == "SITE-02" && f.severity == Severity::Error));
    }

    #[test]
    fn sitemap_urls_extracted() {
        let xml = "<urlset><url><loc>https://x.dev/</loc></url><url><loc>https://x.dev/about/</loc></url></urlset>";
        assert_eq!(sitemap_urls(xml).len(), 2);
        assert!(sitemap_urls("broken").is_empty());
    }
}
