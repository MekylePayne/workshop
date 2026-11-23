//! Favicon generation (spec Section 9): writes real files derived from a
//! user-chosen source image - favicon.ico (16/32/48), per-size PNGs, an
//! Apple touch icon and a web manifest - plus the HTML references for the
//! head. Tab and search-result previews in the interface are drawn from
//! these files locally and are labelled simulations.

use crate::ico::write_ico;
use image::DynamicImage;
use serde::Serialize;
use std::io::Cursor;
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum GenerateError {
    #[error("the source image must be at least 64 x 64 pixels; found {width} x {height}")]
    SourceTooSmall { width: u32, height: u32 },
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("image error: {0}")]
    Image(#[from] image::ImageError),
    #[error("ico error: {0}")]
    Ico(#[from] crate::ico::IcoError),
}

#[derive(Debug, Clone, Serialize)]
pub struct GeneratedFavicons {
    /// Files written to the project root, relative paths.
    pub files: Vec<String>,
    /// HTML references to paste-verified into every page head (also offered
    /// as an automatic apply).
    pub html_snippet: String,
    /// The manifest JSON this tool would write (shown for review).
    pub manifest_json: String,
    /// True when manifest.webmanifest was written (false when it already
    /// existed - the existing manifest is never overwritten silently).
    pub manifest_written: bool,
}

const PNG_OUTPUTS: [(u32, &str); 5] = [
    (16, "favicon-16x16.png"),
    (32, "favicon-32x32.png"),
    (180, "apple-touch-icon.png"),
    (192, "android-chrome-192x192.png"),
    (512, "android-chrome-512x512.png"),
];

fn png_bytes(image: &DynamicImage) -> Result<Vec<u8>, GenerateError> {
    let mut out = Cursor::new(Vec::new());
    image.write_with_encoder(image::codecs::png::PngEncoder::new(&mut out))?;
    Ok(out.into_inner())
}

/// Generates all favicon artifacts from `source` into the project root.
pub fn generate(project_root: &Path, source: &Path) -> Result<GeneratedFavicons, GenerateError> {
    let source_image = image::ImageReader::open(source)?
        .with_guessed_format()?
        .decode()?;
    if source_image.width() < 64 || source_image.height() < 64 {
        return Err(GenerateError::SourceTooSmall {
            width: source_image.width(),
            height: source_image.height(),
        });
    }

    let mut files = Vec::new();

    // Per-size PNGs (upscaling a small source is intentional and shown in
    // the interface so the user can pick a larger source instead).
    let mut sized: Vec<(u32, Vec<u8>)> = Vec::new();
    for (size, name) in PNG_OUTPUTS {
        let scaled = source_image.resize_exact(size, size, image::imageops::FilterType::Lanczos3);
        let bytes = png_bytes(&scaled)?;
        std::fs::write(project_root.join(name), &bytes)?;
        files.push(name.to_string());
        sized.push((size, bytes));
    }

    // Multi-size ICO from 16/32/48.
    let ico_entries: Vec<(u32, Vec<u8>)> = sized
        .iter()
        .filter(|(size, _)| *size <= 48)
        .cloned()
        .chain(vec![(48u32, {
            let scaled = source_image.resize_exact(48, 48, image::imageops::FilterType::Lanczos3);
            png_bytes(&scaled)?
        })])
        .collect();
    let ico = write_ico(&ico_entries)?;
    std::fs::write(project_root.join("favicon.ico"), &ico)?;
    files.push("favicon.ico".to_string());

    let html_snippet = concat!(
        "<link rel=\"icon\" href=\"/favicon.ico\" sizes=\"any\">",
        "<link rel=\"icon\" href=\"/favicon.svg\" type=\"image/svg+xml\">",
        "<link rel=\"apple-touch-icon\" href=\"/apple-touch-icon.png\">",
        "<link rel=\"manifest\" href=\"/manifest.webmanifest\">"
    );

    let manifest_json = serde_json::json!({
        "name": "",
        "short_name": "",
        "icons": [
            { "src": "/android-chrome-192x192.png", "sizes": "192x192", "type": "image/png" },
            { "src": "/android-chrome-512x512.png", "sizes": "512x512", "type": "image/png" }
        ],
        "theme_color": "#ffffff",
        "background_color": "#ffffff",
        "display": "standalone"
    })
    .to_string();

    let manifest_path = project_root.join("manifest.webmanifest");
    let manifest_written = if manifest_path.exists() {
        false
    } else {
        std::fs::write(&manifest_path, &manifest_json)?;
        files.push("manifest.webmanifest".to_string());
        true
    };

    Ok(GeneratedFavicons { files, html_snippet: html_snippet.to_string(), manifest_json, manifest_written })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source_png(size: u32) -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        let image = image::RgbaImage::from_fn(size, size, |x, y| {
            image::Rgba([(x * 3 % 256) as u8, (y * 5 % 256) as u8, 60, 255])
        });
        image
            .save_with_format(tmp.path().join("source.png"), image::ImageFormat::Png)
            .unwrap();
        tmp
    }

    #[test]
    fn generates_all_files_and_a_readable_ico() {
        let tmp = source_png(256);
        let project = tempfile::tempdir().unwrap();
        let result = generate(project.path(), &tmp.path().join("source.png")).unwrap();
        for name in ["favicon.ico", "favicon-16x16.png", "apple-touch-icon.png", "android-chrome-512x512.png"] {
            assert!(project.path().join(name).is_file(), "{name} missing");
            assert!(result.files.iter().any(|f| f == name));
        }
        let ico = image::ImageReader::open(project.path().join("favicon.ico"))
            .unwrap()
            .with_guessed_format()
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!(ico.width(), 48, "the decoder selects the largest entry");
        assert!(result.manifest_written);
        assert!(project.path().join("manifest.webmanifest").is_file());
        assert!(result.html_snippet.contains("apple-touch-icon"));
    }

    #[test]
    fn existing_manifest_is_never_overwritten() {
        let tmp = source_png(128);
        let project = tempfile::tempdir().unwrap();
        std::fs::write(project.path().join("manifest.webmanifest"), "{\"name\":\"existing\"}").unwrap();
        let result = generate(project.path(), &tmp.path().join("source.png")).unwrap();
        assert!(!result.manifest_written);
        assert_eq!(
            std::fs::read_to_string(project.path().join("manifest.webmanifest")).unwrap(),
            "{\"name\":\"existing\"}"
        );
    }

    #[test]
    fn tiny_sources_are_refused_with_a_clear_error() {
        let tmp = source_png(32);
        let project = tempfile::tempdir().unwrap();
        let error = generate(project.path(), &tmp.path().join("source.png")).unwrap_err();
        assert!(error.to_string().contains("64 x 64"));
    }
}
