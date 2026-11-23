//! `_headers` parsing and matching (spec Sections 2 and 5, PR-07).
//!
//! The format follows Cloudflare's `_headers` convention so previews match
//! deployment behaviour: a path pattern line, then indented `Name: value`
//! lines beneath it. Malformed lines are skipped with no error - a broken
//! `_headers` file must not break the preview.

use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeaderRule {
    pub pattern: String,
    pub headers: Vec<(String, String)>,
}

#[derive(Debug, Clone, Default)]
pub struct HeaderRules {
    rules: Vec<HeaderRule>,
}

impl HeaderRules {
    pub fn empty() -> Self {
        Self::default()
    }

    /// Parses a `_headers` file; a missing file yields no rules.
    pub fn load(path: &Path) -> Self {
        let Ok(text) = std::fs::read_to_string(path) else {
            return Self::empty();
        };
        Self::parse(&text)
    }

    pub fn parse(text: &str) -> Self {
        let mut rules: Vec<HeaderRule> = Vec::new();
        for line in text.lines() {
            let trimmed = line.trim_end();
            if trimmed.trim().is_empty() || trimmed.trim_start().starts_with('#') {
                continue;
            }
            let indented = line.starts_with(' ') || line.starts_with('\t');
            let content = trimmed.trim();
            if indented && !rules.is_empty() {
                if let Some((name, value)) = content.split_once(':') {
                    rules
                        .last_mut()
                        .expect("checked non-empty above")
                        .headers
                        .push((name.trim().to_string(), value.trim().to_string()));
                }
            } else if !indented {
                rules.push(HeaderRule {
                    pattern: content.to_string(),
                    headers: Vec::new(),
                });
            }
        }
        Self { rules }
    }

    /// All headers from rules whose pattern matches `request_path`
    /// (a leading-slash path like "/assets/style.css").
    pub fn matching(&self, request_path: &str) -> Vec<(String, String)> {
        let mut out = Vec::new();
        for rule in &self.rules {
            if Self::pattern_matches(&rule.pattern, request_path) {
                out.extend(rule.headers.iter().cloned());
            }
        }
        out
    }

    fn pattern_matches(pattern: &str, path: &str) -> bool {
        if pattern == "*" {
            return true;
        }
        if let Some(prefix) = pattern.strip_suffix("/*") {
            return path == prefix || path.starts_with(&format!("{prefix}/"));
        }
        pattern == path
    }

    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    pub fn rules(&self) -> &[HeaderRule] {
        &self.rules
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "# comments allowed\n/*\n  X-Preview: always\n\n/assets/*\n  Cache-Control: no-store\n  X-Test: yes\n/index.html\n  X-Exact: one\n";

    #[test]
    fn parse_groups_headers_under_patterns() {
        let rules = HeaderRules::parse(SAMPLE);
        assert_eq!(rules.rules().len(), 3);
        assert_eq!(rules.rules()[0].headers.len(), 1);
        assert_eq!(rules.rules()[1].headers.len(), 2);
    }

    #[test]
    fn wildcard_prefix_matching() {
        let rules = HeaderRules::parse(SAMPLE);
        let headers = rules.matching("/assets/css/style.css");
        assert!(headers.contains(&("X-Preview".to_string(), "always".to_string())));
        assert!(headers.contains(&("X-Test".to_string(), "yes".to_string())));
        assert!(!headers.iter().any(|(n, _)| n == "X-Exact"));
    }

    #[test]
    fn exact_matching_and_root_wildcard() {
        let rules = HeaderRules::parse(SAMPLE);
        let headers = rules.matching("/index.html");
        assert!(headers.contains(&("X-Exact".to_string(), "one".to_string())));
        assert!(headers.contains(&("X-Preview".to_string(), "always".to_string())));
    }

    #[test]
    fn missing_file_yields_no_rules() {
        let rules = HeaderRules::load(Path::new("/nonexistent/_headers"));
        assert!(rules.is_empty());
        assert!(rules.matching("/anything").is_empty());
    }
}
