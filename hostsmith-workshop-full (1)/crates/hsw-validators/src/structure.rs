//! Structure validation with the two profiles of PR-01: the HostSmith
//! profile (enforcing) and a generic static-site profile (advisory). The
//! validator reports only - it never restructures a website (spec
//! Section 13). The footer badge check (PR-08) is configuration-driven;
//! until the exact markup arrives, it runs in pending mode against any
//! `hs-` prefixed class.

use hsw_core::fsutil::safe_join;
use hsw_core::project::ProjectScan;
use hsw_html::queries::element_class_lists;
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Profile {
    HostSmith,
    Generic,
}

#[derive(Debug, Clone, Serialize)]
pub struct BadgeSignature {
    /// Class names the badge element must carry (PR-08: supplied by the
    /// project owner). Empty means pending mode.
    pub required_classes: Vec<String>,
}

impl BadgeSignature {
    /// Pending mode: any `hs-` prefixed class counts as a candidate badge.
    pub fn pending() -> Self {
        Self { required_classes: Vec::new() }
    }

    pub fn is_pending(&self) -> bool {
        self.required_classes.is_empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
    Passed,
    Info,
}

#[derive(Debug, Clone, Serialize)]
pub struct CheckResult {
    pub id: &'static str,
    pub severity: Severity,
    pub message: String,
    pub explanation: String,
}

/// In the generic profile every HostSmith-enforced check is downgraded one
/// level (Error -> Warning, Warning -> Info).
fn profile_severity(profile: Profile, enforced: Severity) -> Severity {
    match (profile, enforced) {
        (Profile::HostSmith, severity) => severity,
        (Profile::Generic, Severity::Error) => Severity::Warning,
        (Profile::Generic, Severity::Warning) => Severity::Info,
        (Profile::Generic, severity) => severity,
    }
}

fn check(
    id: &'static str,
    profile: Profile,
    enforced: Severity,
    condition: bool,
    ok_message: String,
    fail_message: String,
    explanation: String,
) -> Option<CheckResult> {
    let severity = if condition { Severity::Passed } else { profile_severity(profile, enforced) };
    let _ = severity;
    Some(CheckResult {
        id,
        severity: if condition { Severity::Passed } else { profile_severity(profile, enforced) },
        message: if condition { ok_message } else { fail_message },
        explanation,
    })
}

/// Runs the full structure validation.
pub fn validate(
    root: &Path,
    scan: &ProjectScan,
    profile: Profile,
    badge: &BadgeSignature,
) -> Vec<CheckResult> {
    let mut results = Vec::new();

    results.extend(check(
        "V-01",
        profile,
        Severity::Error,
        scan.image_dir_present,
        "mandatory /image/ directory present".into(),
        "missing /image/ directory".into(),
        "HostSmith sites keep every imported photo and video in /image/ (spec Section 2).".into(),
    ));

    // Pages live in their own folder with an index.html; root-level pages
    // other than index.html / 404.html break that convention.
    let mut stray: Vec<String> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(root) {
        for entry in entries.filter_map(|entry| entry.ok()) {
            let name = entry.file_name().to_string_lossy().to_string();
            if entry.path().is_file()
                && name.to_ascii_lowercase().ends_with(".html")
                && !matches!(name.to_ascii_lowercase().as_str(), "index.html" | "404.html")
            {
                stray.push(name);
            }
        }
    }
    results.extend(check(
        "V-02",
        profile,
        Severity::Error,
        stray.is_empty(),
        "every page lives in its own folder".into(),
        format!("page(s) outside their own folder: {}", stray.join(", ")),
        "Each page gets a folder with an index.html inside (spec Section 2); move these pages into folders rather than letting the tool restructure them.".into(),
    ));

    results.extend(check(
        "V-03",
        profile,
        Severity::Warning,
        !scan.asset_files.is_empty(),
        "shared assets directory in use".into(),
        "no shared assets/ directory found".into(),
        "Page-specific assets alongside the page are allowed; a shared assets/ tree is the recommended default.".into(),
    ));

    results.extend(check(
        "V-04",
        profile,
        Severity::Error,
        scan.robots_present,
        "robots.txt present".into(),
        "robots.txt is missing".into(),
        "Crawlers expect robots.txt at the site root; reference the sitemap in it.".into(),
    ));

    results.extend(check(
        "V-05",
        profile,
        Severity::Error,
        scan.sitemap_present,
        "sitemap.xml present".into(),
        "sitemap.xml is missing".into(),
        "A sitemap lists every page URL for search engines; keep it in sync when pages are added.".into(),
    ));

    results.extend(check(
        "V-06",
        profile,
        Severity::Error,
        scan.not_found_page_present,
        "404.html present".into(),
        "404.html is missing".into(),
        "Cloudflare Pages serves the project's 404.html for missing paths; visitors should get a helpful page.".into(),
    ));

    results.extend(check(
        "V-07",
        profile,
        Severity::Warning,
        scan.headers_present,
        "_headers present".into(),
        "_headers is missing".into(),
        "Required when the project already ships one or when deploying to Cloudflare Pages (PR-07); otherwise advisory.".into(),
    ));

    let css_dir = safe_join(root, Path::new("assets/css")).map(|p| p.is_dir()).unwrap_or(false);
    let js_dir = safe_join(root, Path::new("assets/js")).map(|p| p.is_dir()).unwrap_or(false);
    results.extend(check(
        "V-08",
        profile,
        Severity::Info,
        css_dir && js_dir,
        "CSS and JavaScript organised under assets/css and assets/js".into(),
        "CSS and JavaScript are not in the recommended assets/css and assets/js layout".into(),
        "Informational: the convention keeps stylesheets and scripts predictable across projects.".into(),
    ));

    results.push(badge_check(root, scan, profile, badge));

    results
}

fn badge_check(root: &Path, scan: &ProjectScan, profile: Profile, badge: &BadgeSignature) -> CheckResult {
    let mut pages_total = 0;
    let mut pages_with_badge = 0;
    for page in &scan.pages {
        let Ok(path) = safe_join(root, &page.index_file) else { continue };
        let Ok(html) = std::fs::read_to_string(path) else { continue };
        pages_total += 1;
        if page_has_badge(&html, badge) {
            pages_with_badge += 1;
        }
    }

    let pending = badge.is_pending();
    let (severity, message, explanation) = if pages_total == 0 {
        (
            profile_severity(profile, Severity::Warning),
            "no pages found to check for the footer badge".into(),
            "The badge check needs at least one readable page.".into(),
        )
    } else if pages_with_badge == pages_total {
        (
            Severity::Passed,
            format!("footer badge present on all {pages_total} page(s)"),
            if pending {
                "Pending-mode signature (any hs- class). Replace with the exact supplied markup signature (PR-08) for the final check.".into()
            } else {
                format!("Signature classes matched: {}", badge.required_classes.join(", "))
            },
        )
    } else {
        (
            profile_severity(profile, Severity::Error),
            format!("footer badge missing on {} of {pages_total} page(s)", pages_total - pages_with_badge),
            if pending {
                "Pending mode: the detector looks for any element with an hs- prefixed class because the exact HostSmith badge markup has not been supplied yet (PR-08). Do not invent a replacement badge; add the supplied snippet to each page.".into()
            } else {
                "Every HostSmith page must carry the exact footer badge markup supplied by the project owner, unchanged.".into()
            },
        )
    };
    CheckResult { id: "V-09", severity, message, explanation }
}

fn page_has_badge(html: &str, badge: &BadgeSignature) -> bool {
    let classes = element_class_lists(html);
    if badge.is_pending() {
        classes
            .iter()
            .any(|list| list.iter().any(|class| class.starts_with("hs-")))
    } else {
        classes
            .iter()
            .any(|list| badge.required_classes.iter().all(|required| list.contains(required)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hsw_core::project::scan_project;
    use std::fs;

    fn fixture(with_badge: bool, complete: bool) -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        let badge = if with_badge {
            "<footer class=\"hs-footer hs-footer-light\"><a class=\"hs-footer-link\" href=\"#\">HostSmith</a></footer>"
        } else {
            "<footer><p>plain footer</p></footer>"
        };
        let image_dir = if complete { tmp.path().join("image") } else { tmp.path().join("no-image") };
        fs::create_dir_all(&image_dir).unwrap();
        fs::create_dir_all(tmp.path().join("assets/css")).unwrap();
        fs::create_dir_all(tmp.path().join("assets/js")).unwrap();
        fs::write(tmp.path().join("assets/css/style.css"), "body{}").unwrap();
        fs::write(tmp.path().join("assets/js/main.js"), "// js").unwrap();
        fs::write(
            tmp.path().join("index.html"),
            format!("<html><head><title>Home</title></head><body><h1>x</h1>{badge}</body></html>"),
        )
        .unwrap();
        fs::create_dir_all(tmp.path().join("about")).unwrap();
        fs::write(
            tmp.path().join("about/index.html"),
            format!("<html><body>{badge}</body></html>"),
        )
        .unwrap();
        fs::write(tmp.path().join("robots.txt"), "User-agent: *").unwrap();
        fs::write(tmp.path().join("sitemap.xml"), "<urlset></urlset>").unwrap();
        fs::write(tmp.path().join("404.html"), "404").unwrap();
        fs::write(tmp.path().join("_headers"), "/*\n  X-Test: 1\n").unwrap();
        tmp
    }

    #[test]
    fn complete_hostsmith_project_passes_structure() {
        let tmp = fixture(true, true);
        let scan = scan_project(tmp.path()).unwrap();
        let results = validate(tmp.path(), &scan, Profile::HostSmith, &BadgeSignature::pending());
        for id in ["V-01", "V-02", "V-03", "V-04", "V-05", "V-06", "V-07", "V-09"] {
            let result = results.iter().find(|r| r.id == id).unwrap_or_else(|| panic!("{id} missing"));
            assert_eq!(result.severity, Severity::Passed, "{id}: {}", result.message);
        }
    }

    #[test]
    fn missing_image_dir_is_an_error_in_hostsmith_profile_only() {
        let tmp = fixture(true, false);
        let scan = scan_project(tmp.path()).unwrap();
        let hostsmith = validate(tmp.path(), &scan, Profile::HostSmith, &BadgeSignature::pending());
        let generic = validate(tmp.path(), &scan, Profile::Generic, &BadgeSignature::pending());
        let hs_v01 = hostsmith.iter().find(|r| r.id == "V-01").unwrap();
        let gen_v01 = generic.iter().find(|r| r.id == "V-01").unwrap();
        assert_eq!(hs_v01.severity, Severity::Error);
        assert_eq!(gen_v01.severity, Severity::Warning);
    }

    #[test]
    fn missing_badge_is_reported_with_pending_explanation() {
        let tmp = fixture(false, true);
        let scan = scan_project(tmp.path()).unwrap();
        let results = validate(tmp.path(), &scan, Profile::HostSmith, &BadgeSignature::pending());
        let badge = results.iter().find(|r| r.id == "V-09").unwrap();
        assert_eq!(badge.severity, Severity::Error);
        assert!(badge.explanation.contains("PR-08"), "must point at the pending input");
        assert!(badge.explanation.contains("Do not invent"), "must forbid a replacement badge");
    }

    #[test]
    fn strict_signature_requires_all_classes_on_one_element() {
        let tmp = fixture(true, true);
        let scan = scan_project(tmp.path()).unwrap();
        let strict = BadgeSignature {
            required_classes: vec!["hs-footer".into(), "hs-footer-link".into()],
        };
        let results = validate(tmp.path(), &scan, Profile::HostSmith, &strict);
        // The fixture badge carries both classes on different elements, so a
        // strict same-element signature fails - by design (PR-08 will pin it).
        let badge = results.iter().find(|r| r.id == "V-09").unwrap();
        assert_eq!(badge.severity, Severity::Error);

        let relaxed = BadgeSignature { required_classes: vec!["hs-footer".into()] };
        let results = validate(tmp.path(), &scan, Profile::HostSmith, &relaxed);
        let badge = results.iter().find(|r| r.id == "V-09").unwrap();
        assert_eq!(badge.severity, Severity::Passed);
    }

    #[test]
    fn stray_root_pages_are_flagged() {
        let tmp = fixture(true, true);
        fs::write(tmp.path().join("pricing.html"), "<html><body>p</body></html>").unwrap();
        let scan = scan_project(tmp.path()).unwrap();
        let results = validate(tmp.path(), &scan, Profile::HostSmith, &BadgeSignature::pending());
        let v02 = results.iter().find(|r| r.id == "V-02").unwrap();
        assert_eq!(v02.severity, Severity::Error);
        assert!(v02.message.contains("pricing.html"));
    }
}
