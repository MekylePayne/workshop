//! Minimal multi-size ICO writer (spec Section 9).
//!
//! Writes an ICO container with PNG-compressed entries (the format every
//! modern browser accepts). Verified by decoding the output with the image
//! crate in tests.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum IcoError {
    #[error("an ICO needs at least one entry")]
    Empty,
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// Builds an ICO file from `(size, png_bytes)` entries. Sizes of 256 and
/// above are stored with the 0-byte dimension marker per the ICO format.
pub fn write_ico(entries: &[(u32, Vec<u8>)]) -> Result<Vec<u8>, IcoError> {
    if entries.is_empty() {
        return Err(IcoError::Empty);
    }
    let mut out = Vec::with_capacity(6 + entries.len() * 16 + entries.iter().map(|(_, data)| data.len()).sum::<usize>());
    out.extend_from_slice(&0u16.to_le_bytes()); // reserved
    out.extend_from_slice(&1u16.to_le_bytes()); // 1 = icon
    out.extend_from_slice(&(entries.len() as u16).to_le_bytes());

    let mut offset = 6 + entries.len() * 16;
    for (size, data) in entries {
        let marker = u8::try_from(*size).unwrap_or(0);
        out.push(marker); // width (0 = 256)
        out.push(marker); // height
        out.push(0); // palette colours
        out.push(0); // reserved
        out.extend_from_slice(&1u16.to_le_bytes()); // colour planes
        out.extend_from_slice(&32u16.to_le_bytes()); // bits per pixel
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&(offset as u32).to_le_bytes());
        offset += data.len();
    }
    for (_, data) in entries {
        out.extend_from_slice(data);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png_bytes(size: u32) -> Vec<u8> {
        let image = image::RgbaImage::from_fn(size, size, |x, y| {
            image::Rgba([(x % 256) as u8, (y % 256) as u8, 90, 255])
        });
        let mut out = std::io::Cursor::new(Vec::new());
        image
            .write_with_encoder(image::codecs::png::PngEncoder::new(&mut out))
            .unwrap();
        out.into_inner()
    }

    #[test]
    fn ico_is_readable_and_reports_dimensions() {
        let entries = vec![(16u32, png_bytes(16)), (32u32, png_bytes(32)), (48u32, png_bytes(48))];
        let bytes = write_ico(&entries).unwrap();
        let decoded = image::ImageReader::new(std::io::Cursor::new(&bytes))
            .with_guessed_format()
            .unwrap()
            .decode()
            .unwrap();
        // The image crate's ICO decoder picks the best (largest) entry.
        assert_eq!(decoded.width(), 48);
    }

    #[test]
    fn empty_ico_is_refused() {
        assert!(write_ico(&[]).is_err());
    }

    #[test]
    fn sizes_of_256_use_the_zero_marker() {
        let entries = vec![(256u32, png_bytes(256))];
        let bytes = write_ico(&entries).unwrap();
        // Width byte of the first entry sits at offset 6.
        assert_eq!(bytes[6], 0);
        assert_eq!(bytes[7], 0);
    }
}
