//! Image palette extraction (spec Section 12): dominant colours of an
//! image, quantised to 4 bits per channel and averaged per bucket, with
//! each colour's share of the sampled pixels.

use crate::minify::ToolsError;
use serde::Serialize;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PaletteEntry {
    pub hex: String,
    /// Share of sampled pixels, 0.0-1.0.
    pub share: f64,
}

pub fn extract_palette(
    image_bytes: &[u8],
    max_colors: usize,
) -> Result<Vec<PaletteEntry>, ToolsError> {
    let reader = image::ImageReader::new(std::io::Cursor::new(image_bytes)).with_guessed_format()?;
    let image = reader.decode()?;
    let small = if image.width() > 64 || image.height() > 64 {
        image.resize(64, 64, image::imageops::FilterType::Triangle)
    } else {
        image
    };
    let rgba = small.to_rgba8();
    let mut buckets: HashMap<u32, (u64, u64, u64, u64)> = HashMap::new();
    let mut total = 0u64;
    for pixel in rgba.pixels() {
        let (r, g, b, a) = (pixel[0], pixel[1], pixel[2], pixel[3]);
        if a < 16 {
            continue; // fully transparent pixels carry no colour
        }
        let key = (((r >> 4) as u32) << 8) | (((g >> 4) as u32) << 4) | (b >> 4) as u32;
        let entry = buckets.entry(key).or_insert((0, 0, 0, 0));
        entry.0 += u64::from(r);
        entry.1 += u64::from(g);
        entry.2 += u64::from(b);
        entry.3 += 1;
        total += 1;
    }
    let mut entries: Vec<(u64, u64, u64, u64)> = buckets.into_values().collect();
    entries.sort_by_key(|entry| std::cmp::Reverse(entry.3));
    let total = total.max(1);
    Ok(entries
        .into_iter()
        .take(max_colors.max(1))
        .map(|(r, g, b, count)| PaletteEntry {
            hex: format!(
                "#{:02x}{:02x}{:02x}",
                (r / count) as u8,
                (g / count) as u8,
                (b / count) as u8
            ),
            share: count as f64 / total as f64,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png_bytes() -> Vec<u8> {
        let image = image::RgbaImage::from_fn(32, 32, |x, _| {
            if x < 16 {
                image::Rgba([255, 0, 0, 255])
            } else {
                image::Rgba([0, 255, 0, 255])
            }
        });
        let mut out = std::io::Cursor::new(Vec::new());
        image
            .write_with_encoder(image::codecs::png::PngEncoder::new(&mut out))
            .unwrap();
        out.into_inner()
    }

    #[test]
    fn palette_extracts_dominant_colour_and_complete_shares() {
        let palette = extract_palette(&png_bytes(), 4).unwrap();
        assert!(palette.len() >= 2);
        assert!(palette[0].share > 0.4);
        assert!(palette[0].hex.starts_with('#'));
        let shares: f64 = palette.iter().map(|entry| entry.share).sum();
        assert!((shares - 1.0).abs() < 0.01);
    }

    #[test]
    fn max_colors_limits_the_result() {
        let palette = extract_palette(&png_bytes(), 1).unwrap();
        assert_eq!(palette.len(), 1);
    }

    #[test]
    fn invalid_bytes_are_an_error_not_a_panic() {
        assert!(extract_palette(b"not an image", 4).is_err());
    }
}
