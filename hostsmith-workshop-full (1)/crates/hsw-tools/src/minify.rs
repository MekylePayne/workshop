//! JSON and minification tools (spec Section 12). Pretty formatting of
//! HTML/CSS/JS runs in the interface with bundled formatters; minification
//! and JSON validation run here in Rust.

use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ToolsError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("image error: {0}")]
    Image(#[from] image::ImageError),
    #[error("{0}")]
    Other(String),
}

#[derive(Debug, Clone, Serialize)]
pub struct JsonDiagnostic {
    pub line: usize,
    pub column: usize,
    pub message: String,
}

/// Pretty-prints JSON with a two-space indent; errors carry line/column.
pub fn format_json(source: &str) -> Result<String, ToolsError> {
    let value: serde_json::Value =
        serde_json::from_str(source).map_err(|err| ToolsError::Other(err.to_string()))?;
    serde_json::to_string_pretty(&value).map_err(|err| ToolsError::Other(err.to_string()))
}

/// Minifies JSON (compact separators).
pub fn minify_json(source: &str) -> Result<String, ToolsError> {
    let value: serde_json::Value =
        serde_json::from_str(source).map_err(|err| ToolsError::Other(err.to_string()))?;
    serde_json::to_string(&value).map_err(|err| ToolsError::Other(err.to_string()))
}

/// Validates JSON and returns line/column diagnostics on failure.
pub fn validate_json(source: &str) -> Result<(), JsonDiagnostic> {
    if let Err(err) = serde_json::from_str::<serde_json::Value>(source) {
        return Err(JsonDiagnostic {
            line: err.line(),
            column: err.column(),
            message: err.to_string(),
        });
    }
    Ok(())
}

/// Minifies HTML via minify-html.
pub fn minify_html_str(source: &str) -> String {
    let cfg = minify_html::Cfg::default();
    String::from_utf8_lossy(&minify_html::minify(source.as_bytes(), &cfg)).into_owned()
}

/// Minifies JavaScript via minify-js (global script mode).
pub fn minify_js_str(source: &str) -> Result<String, ToolsError> {
    let session = minify_js::Session::new();
    let mut out = Vec::new();
    minify_js::minify(&session, minify_js::TopLevelMode::Global, source.as_bytes(), &mut out)
        .map_err(|error| ToolsError::Other(error.to_string()))?;
    Ok(String::from_utf8_lossy(&out).into_owned())
}

/// Simple, string-safe CSS minifier: strips comments, collapses whitespace
/// and removes spaces around punctuation. String contents are preserved
/// verbatim, so URLs with spaces survive and malformed CSS cannot be turned
/// into different valid CSS by a re-formatting parser.
pub fn minify_css_str(source: &str) -> String {
    #[derive(PartialEq)]
    enum State {
        Code,
        String(char),
        Comment,
    }
    const SUPPRESS_SPACE_BEFORE: [char; 6] = ['{', '(', ';', ':', ',', '>'];
    let mut out = String::with_capacity(source.len());
    let mut state = State::Code;
    let mut pending_space = false;
    let mut chars = source.chars().peekable();
    while let Some(c) = chars.next() {
        match state {
            State::Comment => {
                if c == '*' && chars.peek() == Some(&'/') {
                    chars.next();
                    state = State::Code;
                    pending_space = true;
                }
            }
            State::String(quote) => {
                out.push(c);
                if c == quote {
                    state = State::Code;
                }
            }
            State::Code => match c {
                '\'' | '"' => {
                    if pending_space && !out.is_empty() && !out.ends_with(SUPPRESS_SPACE_BEFORE) {
                        out.push(' ');
                    }
                    pending_space = false;
                    out.push(c);
                    state = State::String(c);
                }
                '/' if chars.peek() == Some(&'*') => {
                    chars.next();
                    state = State::Comment;
                }
                c if c.is_whitespace() => pending_space = true,
                c => {
                    if "{}();:,>".contains(c) {
                        if out.ends_with(' ') {
                            out.pop();
                        }
                        out.push(c);
                    } else {
                        if pending_space
                            && !out.is_empty()
                            && !out.ends_with(SUPPRESS_SPACE_BEFORE)
                            && !out.ends_with(' ')
                        {
                            out.push(' ');
                        }
                        out.push(c);
                    }
                    pending_space = false;
                }
            },
        }
    }
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_format_and_minify_roundtrip() {
        let formatted = format_json("{\"a\":1,\"b\":[2,3]}").unwrap();
        assert!(formatted.contains("  \"a\": 1"));
        let minified = minify_json(&formatted).unwrap();
        assert_eq!(minified, "{\"a\":1,\"b\":[2,3]}");
    }

    #[test]
    fn json_validation_reports_line_and_column() {
        let error = validate_json("{\n  \"a\":\n}").unwrap_err();
        assert!(error.line >= 2, "got line {}", error.line);
    }

    #[test]
    fn valid_json_passes_validation() {
        assert!(validate_json("{\"ok\":true}").is_ok());
    }

    #[test]
    fn html_minification_removes_whitespace() {
        let out = minify_html_str("<html>\n  <body>\n    <h1>Hi</h1>\n  </body>\n</html>");
        assert!(out.contains("<h1>Hi</h1>"));
        assert!(out.len() < 30, "got: {out}");
    }

    #[test]
    fn js_minification_removes_comments_and_whitespace() {
        let out = minify_js_str("// comment\nlet answer = 42;\nfunction f ( ) { return answer; }").unwrap();
        assert!(out.contains("42"));
        assert!(!out.contains("comment"));
    }

    #[test]
    fn css_minifier_is_string_safe() {
        let out = minify_css_str(
            "body {\n  color: #222;\n  background: url(\"my file.png\") no-repeat;\n}\n",
        );
        // The separator space after the string survives - strings never
        // enter the collapsing logic, so URLs with spaces cannot break.
        assert_eq!(
            out,
            "body{color:#222;background:url(\"my file.png\") no-repeat;}"
        );
    }

    #[test]
    fn css_minifier_handles_media_queries_and_comments() {
        // Dropping the space before "(" is valid CSS; media queries parse
        // identically without it.
        let out = minify_css_str("@media (max-width: 600px) { /* note */ .a { color : red; } }");
        assert_eq!(out, "@media(max-width:600px){.a{color:red;}}");
    }
}
