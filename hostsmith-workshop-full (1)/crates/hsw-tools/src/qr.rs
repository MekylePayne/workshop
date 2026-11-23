//! QR code generation (spec Section 12): renders a QR code as an SVG string
//! with configurable colours.

use crate::minify::ToolsError;

pub fn qr_svg(text: &str, dark: &str, light: &str, min_width: usize) -> Result<String, ToolsError> {
    use qrcode::render::svg;
    let code = qrcode::QrCode::new(text.as_bytes())
        .map_err(|err| ToolsError::Other(format!("QR encoding failed: {err}")))?;
    let width = min_width.max(64).clamp(64, 4000) as u32;
    Ok(code
        .render::<svg::Color>()
        .min_dimensions(width, width)
        .dark_color(svg::Color(dark))
        .light_color(svg::Color(light))
        .build())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qr_svg_contains_svg_and_shape_data() {
        let svg = qr_svg("https://example.com", "#000000", "#ffffff", 200).unwrap();
        assert!(svg.contains("<svg"));
        assert!(svg.contains("path") || svg.contains("rect"));
        assert!(svg.contains("#000000"));
    }

    #[test]
    fn qr_survives_long_urls_and_unicode() {
        let long = format!("https://example.com/{}", "p".repeat(400));
        assert!(qr_svg(&long, "#000", "#fff", 64).is_ok());
        assert!(qr_svg("HostSmith Werkstatt – Café Müller ☕", "#000", "#fff", 64).is_ok());
    }

    #[test]
    fn empty_input_still_renders_a_valid_qr() {
        // An empty payload is a valid (version-1) QR code.
        assert!(qr_svg("", "#000", "#fff", 64).is_ok());
    }
}
