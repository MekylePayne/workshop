//! Read-only project scan and structure classification (spec Section 2).
//!
//! The scanner never writes; it classifies the website the way the HostSmith
//! convention expects pages to live: one folder per page with an index.html
//! inside, a shared assets/ tree, and the site-level markers (robots.txt,
//! sitemap.xml, 404.html, _headers, /image/). Hidden directories are skipped.

use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize)]
pub struct PageEntry {
    /// Page folder relative to the project root ("." for the home page).
    pub folder: PathBuf,
    /// Relative path of the page's index.html.
    pub index_file: PathBuf,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProjectScan {
    pub pages: Vec<PageEntry>,
    /// Every file under the shared assets/ tree, project-relative.
    pub asset_files: Vec<PathBuf>,
    pub robots_present: bool,
    pub sitemap_present: bool,
    pub not_found_page_present: bool,
    pub headers_present: bool,
    pub image_dir_present: bool,
    pub total_files: usize,
    pub total_bytes: u64,
}

impl ProjectScan {
    /// True when every site-level marker the HostSmith convention mandates
    /// is in place (_headers is advisory, PR-07, so it is not required).
    pub fn hostsmith_structure_complete(&self) -> bool {
        self.image_dir_present
            && self.robots_present
            && self.sitemap_present
            && self.not_found_page_present
    }
}

fn is_hidden(name: &str) -> bool {
    name.starts_with('.')
}

fn walk(dir: &Path, rel: &str, scan: &mut ProjectScan) -> std::io::Result<()> {
    let mut entries: Vec<_> = fs::read_dir(dir)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        let child_rel = if rel.is_empty() { name.clone() } else { format!("{rel}/{name}") };
        if path.is_dir() {
            if is_hidden(&name) {
                continue;
            }
            if name == "assets" {
                collect_assets(&path, "assets", &mut scan.asset_files)?;
                continue;
            }
            walk(&path, &child_rel, scan)?;
        } else if path.is_file() {
            scan.total_files += 1;
            scan.total_bytes += entry.metadata()?.len();
            if name.eq_ignore_ascii_case("index.html") {
                let folder = if rel.is_empty() { PathBuf::from(".") } else { PathBuf::from(&rel) };
                scan.pages.push(PageEntry {
                    folder,
                    index_file: PathBuf::from(&child_rel),
                });
            }
        }
    }
    Ok(())
}

fn collect_assets(dir: &Path, rel: &str, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    let mut entries: Vec<_> = fs::read_dir(dir)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        let child_rel = format!("{rel}/{name}");
        if path.is_dir() {
            collect_assets(&path, &child_rel, out)?;
        } else if path.is_file() {
            out.push(PathBuf::from(child_rel));
        }
    }
    Ok(())
}

/// Scans `root` without writing anything. A folder that cannot be read does
/// not abort the scan; the missing structure shows up as absent markers.
pub fn scan_project(root: &Path) -> std::io::Result<ProjectScan> {
    let mut scan = ProjectScan {
        image_dir_present: root.join("image").is_dir(),
        robots_present: root.join("robots.txt").is_file(),
        sitemap_present: root.join("sitemap.xml").is_file(),
        not_found_page_present: root.join("404.html").is_file(),
        headers_present: root.join("_headers").is_file(),
        ..ProjectScan::default()
    };
    walk(root, "", &mut scan)?;
    scan.pages.sort_by(|a, b| a.folder.cmp(&b.folder));
    Ok(scan)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn minimal_folder_reports_missing_markers_without_error() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("index.html"), "<h1>x</h1>").unwrap();
        let scan = scan_project(tmp.path()).unwrap();
        assert_eq!(scan.pages.len(), 1);
        assert_eq!(scan.pages[0].folder, PathBuf::from("."));
        assert!(!scan.robots_present);
        assert!(!scan.sitemap_present);
        assert!(!scan.not_found_page_present);
        assert!(!scan.headers_present);
        assert!(!scan.image_dir_present);
        assert_eq!(scan.total_files, 1);
    }

    #[test]
    fn pages_live_in_their_own_folders() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("index.html"), "home").unwrap();
        fs::create_dir_all(tmp.path().join("about")).unwrap();
        fs::write(tmp.path().join("about/index.html"), "about").unwrap();
        fs::create_dir_all(tmp.path().join("contact")).unwrap();
        fs::write(tmp.path().join("contact/index.html"), "contact").unwrap();
        let scan = scan_project(tmp.path()).unwrap();
        let folders: Vec<String> = scan
            .pages
            .iter()
            .map(|page| page.folder.to_string_lossy().to_string())
            .collect();
        assert!(folders.contains(&".".to_string()));
        assert!(folders.contains(&"about".to_string()));
        assert!(folders.contains(&"contact".to_string()));
    }

    #[test]
    fn assets_are_collected_and_markers_detected() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("index.html"), "home").unwrap();
        fs::create_dir_all(tmp.path().join("assets/css")).unwrap();
        fs::write(tmp.path().join("assets/css/style.css"), "body{}").unwrap();
        fs::create_dir_all(tmp.path().join("image")).unwrap();
        fs::write(tmp.path().join("robots.txt"), "User-agent: *").unwrap();
        fs::write(tmp.path().join("sitemap.xml"), "<urlset/>").unwrap();
        fs::write(tmp.path().join("404.html"), "404").unwrap();
        fs::write(tmp.path().join("_headers"), "/*\n  X-Test: 1\n").unwrap();
        let scan = scan_project(tmp.path()).unwrap();
        assert_eq!(scan.asset_files, vec![PathBuf::from("assets/css/style.css")]);
        assert!(scan.image_dir_present);
        assert!(scan.robots_present && scan.sitemap_present && scan.not_found_page_present && scan.headers_present);
        assert!(scan.hostsmith_structure_complete());
    }

    #[test]
    fn hidden_directories_are_skipped() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("index.html"), "home").unwrap();
        fs::create_dir_all(tmp.path().join(".git")).unwrap();
        fs::write(tmp.path().join(".git/config"), "secret").unwrap();
        let scan = scan_project(tmp.path()).unwrap();
        assert_eq!(scan.total_files, 1, "hidden directory contents are not scanned");
    }
}
