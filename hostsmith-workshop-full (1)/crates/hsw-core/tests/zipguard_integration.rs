//! End-to-end import tests with real archives (fixture discipline: MR-11).
//!
//! These tests are part of the Phase 0 exit criteria (review Table 6): the
//! traversal-rejection behaviour must hold against real archive bytes, not
//! only against the planning functions.

use std::io::Write;
use zip::write::SimpleFileOptions;

use hsw_core::zipguard::extract_zip;

fn write_archive(path: &std::path::Path, files: &[(&str, &str)]) {
    let file = std::fs::File::create(path).unwrap();
    let mut writer = zip::ZipWriter::new(file);
    for (name, content) in files {
        writer.start_file(*name, SimpleFileOptions::default()).unwrap();
        writer.write_all(content.as_bytes()).unwrap();
    }
    writer.finish().unwrap();
}

#[test]
fn benign_single_root_archive_extracts_safely() {
    let tmp = tempfile::tempdir().unwrap();
    let zip_path = tmp.path().join("site.zip");
    write_archive(
        &zip_path,
        &[
            ("site/index.html", "<h1>hi</h1>"),
            ("site/about/index.html", "<p>about</p>"),
            ("site/image/logo.png", "png-bytes"),
        ],
    );
    let dest = tempfile::tempdir().unwrap();
    let plan = extract_zip(std::fs::File::open(&zip_path).unwrap(), dest.path()).unwrap();
    assert_eq!(plan.root_stripped.as_deref(), Some("site/"));
    assert!(dest.path().join("index.html").exists());
    assert!(dest.path().join("about/index.html").exists());
    assert!(dest.path().join("image/logo.png").exists());
    assert_eq!(
        std::fs::read_to_string(dest.path().join("about/index.html")).unwrap(),
        "<p>about</p>"
    );
}

#[test]
fn flat_archive_without_root_extracts_as_is() {
    let tmp = tempfile::tempdir().unwrap();
    let zip_path = tmp.path().join("flat.zip");
    write_archive(&zip_path, &[("index.html", "flat"), ("robots.txt", "User-agent: *")]);
    let dest = tempfile::tempdir().unwrap();
    let plan = extract_zip(std::fs::File::open(&zip_path).unwrap(), dest.path()).unwrap();
    assert_eq!(plan.root_stripped, None);
    assert!(dest.path().join("index.html").exists());
    assert!(dest.path().join("robots.txt").exists());
}

#[test]
fn traversal_archive_is_rejected_before_any_write() {
    let tmp = tempfile::tempdir().unwrap();
    let zip_path = tmp.path().join("attack.zip");
    write_archive(
        &zip_path,
        &[("site/index.html", "ok"), ("site/../../escape.txt", "pwn")],
    );
    let dest = tempfile::tempdir().unwrap();
    let result = extract_zip(std::fs::File::open(&zip_path).unwrap(), dest.path());
    assert!(result.is_err(), "the traversal archive must be rejected");
    // Nothing was written anywhere, inside or outside the destination root.
    assert!(!dest.path().join("index.html").exists());
    assert!(!dest.path().join("escape.txt").exists());
    assert!(!dest.path().parent().unwrap().join("escape.txt").exists());
}

#[test]
fn empty_archive_is_rejected() {
    let tmp = tempfile::tempdir().unwrap();
    let zip_path = tmp.path().join("empty.zip");
    write_archive(&zip_path, &[]);
    let dest = tempfile::tempdir().unwrap();
    let result = extract_zip(std::fs::File::open(&zip_path).unwrap(), dest.path());
    assert!(result.is_err());
}
