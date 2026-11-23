//! Deployment manifest construction (TL-03): walks a project directory,
//! hashes every file, and produces the manifest Cloudflare expects
//! (path -> hash). Directory entries, hidden directories and dependency
//! folders are never uploaded.

use crate::hashing::asset_hash;
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::Path;

const SKIPPED_DIRS: [&str; 3] = [".git", "node_modules", "target"];

#[derive(Debug, Clone, Serialize)]
pub struct UploadFile {
    /// Project-relative path, forward-slash separated.
    pub path: String,
    pub hash: String,
    #[serde(skip)]
    pub contents: Vec<u8>,
    pub content_type: &'static str,
}

fn is_skipped(name: &str) -> bool {
    name.starts_with('.') || SKIPPED_DIRS.contains(&name)
}

fn content_type_for(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
        .as_deref()
    {
        Some("html" | "htm") => "text/html",
        Some("css") => "text/css",
        Some("js" | "mjs") => "text/javascript",
        Some("json" | "map" | "webmanifest") => "application/json",
        Some("xml") => "application/xml",
        Some("txt" | "md") => "text/plain",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("svg") => "image/svg+xml",
        Some("avif") => "image/avif",
        Some("ico") => "image/x-icon",
        Some("bmp") => "image/bmp",
        Some("mp4") => "video/mp4",
        Some("webm") => "video/webm",
        Some("woff") => "font/woff",
        Some("woff2") => "font/woff2",
        Some("ttf") => "font/ttf",
        _ => "application/octet-stream",
    }
}

fn walk(dir: &Path, rel: &str, files: &mut Vec<UploadFile>) -> std::io::Result<()> {
    let mut entries: Vec<_> = std::fs::read_dir(dir)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        let child_rel = if rel.is_empty() { name.clone() } else { format!("{rel}/{name}") };
        if path.is_dir() {
            if is_skipped(&name) {
                continue;
            }
            walk(&path, &child_rel, files)?;
        } else if path.is_file() {
            let contents = std::fs::read(&path)?;
            let hash = asset_hash(&contents);
            files.push(UploadFile {
                path: child_rel,
                hash,
                content_type: content_type_for(&path),
                contents,
            });
        }
    }
    Ok(())
}

/// Walks `root` and hashes every uploadable file, sorted by path.
pub fn build_from_directory(root: &Path) -> std::io::Result<Vec<UploadFile>> {
    let mut files = Vec::new();
    walk(root, "", &mut files)?;
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(files)
}

/// The deployment manifest: a JSON object mapping each path to its hash.
pub fn manifest_json(files: &[UploadFile]) -> String {
    let map: BTreeMap<&str, &str> = files.iter().map(|file| (file.path.as_str(), file.hash.as_str())).collect();
    serde_json::to_string(&map).unwrap_or_default()
}

/// Splits files into upload buckets of at most `max_bytes` base64-encoded
/// payload bytes per request (the upload endpoint takes ~50 MB buckets).
pub fn bucket_files(files: &[UploadFile], max_bytes: usize) -> Vec<Vec<&UploadFile>> {
    let mut buckets = Vec::new();
    let mut current: Vec<&UploadFile> = Vec::new();
    let mut current_size = 0usize;
    for file in files {
        let encoded_size = file.contents.len().div_ceil(3) * 4;
        if !current.is_empty() && current_size + encoded_size > max_bytes {
            buckets.push(std::mem::take(&mut current));
            current_size = 0;
        }
        current_size += encoded_size;
        current.push(file);
    }
    if !current.is_empty() {
        buckets.push(current);
    }
    buckets
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn walks_and_hashes_every_project_file() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("index.html"), b"<h1>home</h1>").unwrap();
        fs::create_dir_all(tmp.path().join("assets/css")).unwrap();
        fs::write(tmp.path().join("assets/css/style.css"), b"body{}").unwrap();
        fs::create_dir_all(tmp.path().join(".git")).unwrap();
        fs::write(tmp.path().join(".git/config"), b"secret").unwrap();

        let files = build_from_directory(tmp.path()).unwrap();
        assert_eq!(files.len(), 2, "hidden directories are skipped");
        assert_eq!(files[0].path, "assets/css/style.css");
        assert_eq!(files[1].path, "index.html");
        assert_eq!(files[1].hash, asset_hash(b"<h1>home</h1>"));
        assert_eq!(files[1].content_type, "text/html");
    }

    #[test]
    fn manifest_is_a_path_to_hash_object() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("index.html"), b"<h1>home</h1>").unwrap();
        let files = build_from_directory(tmp.path()).unwrap();
        let manifest = manifest_json(&files);
        let value: serde_json::Value = serde_json::from_str(&manifest).unwrap();
        assert_eq!(value["index.html"], asset_hash(b"<h1>home</h1>"));
    }

    #[test]
    fn bucketing_respects_the_byte_cap() {
        // Base64 inflates payloads by 4/3; sizes chosen so the arithmetic is
        // unambiguous: (60+2)/3*4 = 82 and (10+2)/3*4 = 16.
        let file = |size: usize| UploadFile {
            path: format!("f{size}.bin"),
            hash: String::new(),
            contents: vec![0u8; size],
            content_type: "application/octet-stream",
        };
        let files = vec![file(60), file(60), file(10)];
        let buckets = bucket_files(&files, 100);
        assert_eq!(buckets.len(), 2);
        assert_eq!(buckets[0].len(), 1, "82 + 82 exceeds the 100-byte cap");
        assert_eq!(buckets[1].len(), 2, "82 + 16 fits in one bucket");
    }
}
