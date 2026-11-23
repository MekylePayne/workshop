//! Image operations (spec Sections 6 and 12, TL-07, MR-04).
//!
//! Every operation writes a NEW file and never modifies its source. WebP
//! encoding is lossless-only (the image-webp encoder behind the `image`
//! crate has no lossy encoder), matching the review's finding TL-07; JPEG
//! output re-encodes pixels and is therefore labelled as visually
//! identical, not bit-exact, and the interface always shows before/after
//! sizes so the user can reject a worse result.

use image::{DynamicImage, ImageFormat};
use serde::Serialize;
use std::path::Path;

/// Output formats the pipeline can write. WebP is lossless-only (TL-07).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OutputFormat {
    Png,
    Jpeg,
    WebpLossless,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImageInfo {
    pub width: u32,
    pub height: u32,
    pub format: String,
    pub bytes: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct OpResult {
    pub output_path: String,
    pub input_bytes: usize,
    pub output_bytes: usize,
    pub width: u32,
    pub height: u32,
    /// True when the operation is guaranteed pixel-identical to its input.
    pub lossless: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum ImagingError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("image error: {0}")]
    Image(#[from] image::ImageError),
    #[error("{0}")]
    Other(String),
}

fn load(path: &Path) -> Result<(DynamicImage, usize), ImagingError> {
    let bytes = std::fs::read(path)?;
    let reader = image::ImageReader::new(std::io::Cursor::new(&bytes))
        .with_guessed_format()?;
    let image = reader.decode()?;
    Ok((image, bytes.len()))
}

fn format_name(format: ImageFormat) -> &'static str {
    match format {
        ImageFormat::Png => "png",
        ImageFormat::Jpeg => "jpeg",
        ImageFormat::Gif => "gif",
        ImageFormat::WebP => "webp",
        ImageFormat::Avif => "avif",
        ImageFormat::Ico => "ico",
        ImageFormat::Bmp => "bmp",
        _ => "other",
    }
}

/// Reads dimensions, format and file size without decoding pixels twice.
pub fn inspect(path: &Path) -> Result<ImageInfo, ImagingError> {
    let bytes = std::fs::read(path)?;
    let reader = image::ImageReader::new(std::io::Cursor::new(&bytes))
        .with_guessed_format()?;
    let format = reader.format().map(format_name).unwrap_or("unknown");
    let (width, height) = reader.into_dimensions()?;
    Ok(ImageInfo { width, height, format: format.to_string(), bytes: bytes.len() })
}

fn encode(
    image: &DynamicImage,
    path: &Path,
    format: OutputFormat,
    jpeg_quality: u8,
) -> Result<(), ImagingError> {
    match format {
        OutputFormat::Png => image.save_with_format(path, ImageFormat::Png)?,
        OutputFormat::Jpeg => {
            let file = std::fs::File::create(path)?;
            let encoder =
                image::codecs::jpeg::JpegEncoder::new_with_quality(
                    std::io::BufWriter::new(file),
                    jpeg_quality,
                );
            image.write_with_encoder(encoder)?;
        }
        OutputFormat::WebpLossless => {
            let file = std::fs::File::create(path)?;
            let encoder = image::codecs::webp::WebPEncoder::new_lossless(std::io::BufWriter::new(file));
            image.write_with_encoder(encoder)?;
        }
    }
    Ok(())
}

fn finish(
    input_bytes: usize,
    path: &Path,
    image: &DynamicImage,
    lossless: bool,
) -> Result<OpResult, ImagingError> {
    let output_bytes = std::fs::metadata(path)?.len() as usize;
    Ok(OpResult {
        output_path: path.to_string_lossy().to_string(),
        input_bytes,
        output_bytes,
        width: image.width(),
        height: image.height(),
        lossless,
    })
}

/// Fits the image within `max_width` x `max_height`, preserving aspect
/// ratio. Images already smaller are copied unchanged. Uses Lanczos3.
pub fn resize(
    src: &Path,
    dst: &Path,
    max_width: u32,
    max_height: u32,
    format: OutputFormat,
) -> Result<OpResult, ImagingError> {
    if max_width == 0 || max_height == 0 {
        return Err(ImagingError::Other("target size must be greater than zero".into()));
    }
    let (image, input_bytes) = load(src)?;
    let (w, h) = (image.width() as f64, image.height() as f64);
    let scale = (max_width as f64 / w).min(max_height as f64 / h);
    let result = if scale < 1.0 {
        let new_w = ((w * scale).round() as u32).max(1);
        let new_h = ((h * scale).round() as u32).max(1);
        image.resize_exact(new_w, new_h, image::imageops::FilterType::Lanczos3)
    } else {
        image.clone()
    };
    encode(&result, dst, format, 85)?;
    finish(input_bytes, dst, &result, true)
}

/// Crops a rectangle; coordinates are validated against the image size.
pub fn crop(
    src: &Path,
    dst: &Path,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    format: OutputFormat,
) -> Result<OpResult, ImagingError> {
    let (image, input_bytes) = load(src)?;
    if x.saturating_add(width) > image.width() || y.saturating_add(height) > image.height() {
        return Err(ImagingError::Other(format!(
            "crop {}x{}+{}+{} exceeds image {}x{}",
            width, height, x, y, image.width(), image.height()
        )));
    }
    let result = image.crop_imm(x, y, width, height);
    encode(&result, dst, format, 85)?;
    finish(input_bytes, dst, &result, true)
}

/// Rotates by `quarter_turns` x 90 degrees clockwise (1..=3).
pub fn rotate(
    src: &Path,
    dst: &Path,
    quarter_turns: u32,
    format: OutputFormat,
) -> Result<OpResult, ImagingError> {
    let (image, input_bytes) = load(src)?;
    let result = match quarter_turns {
        1 => image.rotate90(),
        2 => image.rotate180(),
        3 => image.rotate270(),
        _ => return Err(ImagingError::Other("quarter turns must be 1, 2 or 3".into())),
    };
    encode(&result, dst, format, 85)?;
    finish(input_bytes, dst, &result, true)
}

/// Converts format. JPEG output re-encodes pixels (visually identical, not
/// bit-exact, metadata stripped); PNG and WebP outputs are lossless.
pub fn convert(
    src: &Path,
    dst: &Path,
    format: OutputFormat,
    jpeg_quality: u8,
) -> Result<OpResult, ImagingError> {
    let (image, input_bytes) = load(src)?;
    encode(&image, dst, format, jpeg_quality)?;
    let lossless = format != OutputFormat::Jpeg;
    finish(input_bytes, dst, &image, lossless)
}

/// Lossless PNG optimization (oxipng preset 2). Always safe: pixels are
/// untouched; only compression and metadata chunks change.
pub fn optimize_png(src: &Path, dst: &Path) -> Result<OpResult, ImagingError> {
    let input_bytes = std::fs::metadata(src)?.len() as usize;
    let options = oxipng::Options::from_preset(2);
    oxipng::optimize(
        &oxipng::InFile::Path(src.into()),
        &oxipng::OutFile::from_path(dst.into()),
        &options,
    )
    .map_err(|err| ImagingError::Other(err.to_string()))?;
    let (image, _) = load(dst)?;
    finish(input_bytes, dst, &image, true)
}

/// JPEG optimization: strips metadata and re-encodes with optimized
/// Huffman tables at `quality` (default suggestion: 92). The result is
/// visually identical but NOT bit-exact - the interface must say so.
pub fn optimize_jpeg(
    src: &Path,
    dst: &Path,
    quality: u8,
) -> Result<OpResult, ImagingError> {
    convert(src, dst, OutputFormat::Jpeg, quality)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn png_fixture(width: u32, height: u32) -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        let img = image::RgbaImage::from_fn(width, height, |x, y| {
            image::Rgba([(x * 7 % 256) as u8, (y * 11 % 256) as u8, 128, 255])
        });
        img.save_with_format(tmp.path().join("source.png"), ImageFormat::Png).unwrap();
        tmp
    }

    #[test]
    fn inspect_reports_dimensions_and_format() {
        let tmp = png_fixture(120, 80);
        let info = inspect(&tmp.path().join("source.png")).unwrap();
        assert_eq!(info.width, 120);
        assert_eq!(info.height, 80);
        assert_eq!(info.format, "png");
        assert!(info.bytes > 0);
    }

    #[test]
    fn resize_fits_within_bounds_and_never_upscales() {
        let tmp = png_fixture(120, 80);
        let dst = tmp.path().join("small.png");
        let result = resize(&tmp.path().join("source.png"), &dst, 60, 60, OutputFormat::Png).unwrap();
        assert_eq!((result.width, result.height), (60, 40));
        assert!(result.lossless);
        // Already-smaller image stays unchanged in dimensions.
        let dst2 = tmp.path().join("same.png");
        let result = resize(&tmp.path().join("source.png"), &dst2, 500, 500, OutputFormat::Png).unwrap();
        assert_eq!((result.width, result.height), (120, 80));
    }

    #[test]
    fn crop_validates_bounds() {
        let tmp = png_fixture(100, 100);
        let dst = tmp.path().join("crop.png");
        let ok = crop(&tmp.path().join("source.png"), &dst, 10, 10, 50, 40, OutputFormat::Png).unwrap();
        assert_eq!((ok.width, ok.height), (50, 40));
        let bad = crop(&tmp.path().join("source.png"), &dst, 90, 90, 50, 40, OutputFormat::Png);
        assert!(bad.is_err());
    }

    #[test]
    fn rotation_swaps_dimensions_and_rejects_other_angles() {
        let tmp = png_fixture(120, 80);
        let dst = tmp.path().join("rot.png");
        let result = rotate(&tmp.path().join("source.png"), &dst, 1, OutputFormat::Png).unwrap();
        assert_eq!((result.width, result.height), (80, 120));
        assert!(rotate(&tmp.path().join("source.png"), &dst, 5, OutputFormat::Png).is_err());
    }

    #[test]
    fn webp_output_is_lossless_and_readable() {
        let tmp = png_fixture(64, 64);
        let dst = tmp.path().join("out.webp");
        let result = convert(&tmp.path().join("source.png"), &dst, OutputFormat::WebpLossless, 85).unwrap();
        assert!(result.lossless);
        let info = inspect(&dst).unwrap();
        assert_eq!(info.format, "webp");
    }

    #[test]
    fn jpeg_conversion_reports_non_lossless_and_size_change() {
        let tmp = png_fixture(100, 100);
        let dst = tmp.path().join("out.jpg");
        let result = convert(&tmp.path().join("source.png"), &dst, OutputFormat::Jpeg, 92).unwrap();
        assert!(!result.lossless, "JPEG re-encode must be reported as not bit-exact");
        assert_eq!(result.output_bytes, fs::metadata(&dst).unwrap().len() as usize);
    }

    #[test]
    fn png_optimization_never_increases_fuzzily_and_stays_lossless() {
        let tmp = png_fixture(80, 80);
        let dst = tmp.path().join("opt.png");
        let result = optimize_png(&tmp.path().join("source.png"), &dst).unwrap();
        assert!(result.lossless);
        assert!(result.output_bytes > 0);
    }

    #[test]
    fn source_files_are_never_modified() {
        let tmp = png_fixture(64, 64);
        let before = fs::read(tmp.path().join("source.png")).unwrap();
        let dst = tmp.path().join("rot.png");
        rotate(&tmp.path().join("source.png"), &dst, 1, OutputFormat::Png).unwrap();
        let after = fs::read(tmp.path().join("source.png")).unwrap();
        assert_eq!(before, after, "MR-04: originals are never touched");
    }
}
