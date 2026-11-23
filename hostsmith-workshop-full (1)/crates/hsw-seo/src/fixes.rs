//! SEO fix planners (PR-06): automatic fixes run without input, guided
//! fixes receive their value from the interface first. Every planner
//! returns the new HTML string only - applying it, diffing it and journalling
//! it happen in the application layer through the same pipeline as every
//! other edit, so every fix is previewable and reversible.

use hsw_edit::rewrite::{
    append_to_head, edit_element, list_elements, replace_text_node, set_attribute_on_first,
    ElementEdit, RewriteError,
};
use hsw_html::queries;
use thiserror::Error;

pub const VIEWPORT_SNIPPET: &str =
    "<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">";
pub const DEFAULT_LANG: &str = "en";

#[derive(Debug, Error)]
pub enum FixError {
    #[error("nothing to fix - the page already satisfies this check")]
    NoChange,
    #[error(transparent)]
    Rewrite(#[from] RewriteError),
}

type AttributeMatcher<'a> = &'a dyn Fn(&str, &[(String, String)]) -> bool;

fn find_element(html: &str, matches: AttributeMatcher<'_>) -> Option<usize> {
    list_elements(html)
        .into_iter()
        .find(|element| matches(&element.name, &element.attributes))
        .map(|element| element.index)
}

/// Automatic fix: insert the standard viewport meta tag.
pub fn fix_viewport(html: &str) -> Result<Option<String>, FixError> {
    if queries::summarize(html).viewport.is_some() {
        return Ok(None);
    }
    Ok(Some(append_to_head(html, VIEWPORT_SNIPPET)?))
}

/// Automatic fix: set the document language on the html element.
pub fn fix_lang(html: &str, lang: &str) -> Result<Option<String>, FixError> {
    if queries::summarize(html).lang.is_some() {
        return Ok(None);
    }
    Ok(Some(set_attribute_on_first(html, "html", "lang", lang)?))
}

/// Guided fix: set (or create) the title.
pub fn fix_title(html: &str, value: &str) -> Result<Option<String>, FixError> {
    let title_index = find_element(html, &|name, _| name.eq_ignore_ascii_case("title"));
    match title_index {
        Some(index) => {
            let updated = replace_text_node(html, index, 0, value)?;
            Ok(Some(updated))
        }
        None => {
            let snippet = format!("<title>{}</title>", escape(value));
            Ok(Some(append_to_head(html, &snippet)?))
        }
    }
}

/// Guided fix: set (or create) the meta description.
pub fn fix_description(html: &str, value: &str) -> Result<Option<String>, FixError> {
    let index = find_element(html, &|name, attributes| {
        name.eq_ignore_ascii_case("meta")
            && attributes
                .iter()
                .any(|(n, v)| n.eq_ignore_ascii_case("name") && v.eq_ignore_ascii_case("description"))
    });
    match index {
        Some(index) => {
            let updated = edit_element(
                html,
                index,
                ElementEdit::SetAttribute { name: "content".into(), value: value.into() },
            )?;
            Ok(Some(updated))
        }
        None => {
            let snippet = format!(
                "<meta name=\"description\" content=\"{}\">",
                escape_attribute(value)
            );
            Ok(Some(append_to_head(html, &snippet)?))
        }
    }
}

/// Guided fix: set (or create) the canonical link.
pub fn fix_canonical(html: &str, value: &str) -> Result<Option<String>, FixError> {
    let index = find_element(html, &|name, attributes| {
        name.eq_ignore_ascii_case("link")
            && attributes
                .iter()
                .any(|(n, v)| n.eq_ignore_ascii_case("rel") && v.eq_ignore_ascii_case("canonical"))
    });
    match index {
        Some(index) => {
            let updated = edit_element(
                html,
                index,
                ElementEdit::SetAttribute { name: "href".into(), value: value.into() },
            )?;
            Ok(Some(updated))
        }
        None => {
            let snippet = format!(
                "<link rel=\"canonical\" href=\"{}\">",
                escape_attribute(value)
            );
            Ok(Some(append_to_head(html, &snippet)?))
        }
    }
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

fn escape_attribute(text: &str) -> String {
    escape(text).replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use hsw_html::queries::summarize;

    const PAGE: &str = "<!doctype html><html><head><title>Old</title></head><body><h1>x</h1></body></html>";

    #[test]
    fn viewport_fix_inserts_and_then_reports_no_change() {
        let fixed = fix_viewport(PAGE).unwrap().unwrap();
        assert!(fixed.contains(VIEWPORT_SNIPPET));
        assert!(fix_viewport(&fixed).unwrap().is_none());
    }

    #[test]
    fn lang_fix_sets_attribute_and_then_reports_no_change() {
        let fixed = fix_lang(PAGE, "en-GB").unwrap().unwrap();
        assert!(fixed.contains("<html lang=\"en-GB\""));
        assert!(fix_lang(&fixed, "en-GB").unwrap().is_none());
    }

    #[test]
    fn title_fix_replaces_existing_title_text() {
        let fixed = fix_title(PAGE, "New title").unwrap().unwrap();
        assert!(fixed.contains("<title>New title</title>"));
        assert!(fixed.contains("<h1>x</h1>"), "rest preserved");
    }

    #[test]
    fn title_fix_creates_a_title_when_missing() {
        let html = "<html><head></head><body></body></html>";
        let fixed = fix_title(html, "Created").unwrap().unwrap();
        assert!(fixed.contains("<title>Created</title>"));
    }

    #[test]
    fn description_fix_sets_existing_meta_and_creates_when_missing() {
        let html = "<html><head><meta name=\"description\" content=\"old\"></head><body></body></html>";
        let fixed = fix_description(html, "brand new description").unwrap().unwrap();
        assert!(fixed.contains("content=\"brand new description\""));
        assert!(!fixed.contains("content=\"old\""));

        let html2 = "<html><head></head><body></body></html>";
        let created = fix_description(html2, "another description").unwrap().unwrap();
        assert!(created.contains("<meta name=\"description\" content=\"another description\">"));
    }

    #[test]
    fn canonical_fix_updates_existing_link() {
        let html = "<html><head><link rel=\"canonical\" href=\"https://old.dev/\"></head><body></body></html>";
        let fixed = fix_canonical(html, "https://new.dev/page/").unwrap().unwrap();
        assert!(fixed.contains("href=\"https://new.dev/page/\""));
        assert!(!fixed.contains("https://old.dev/"));
    }

    #[test]
    fn title_escaping_is_safe() {
        let fixed = fix_title(PAGE, "Fish & Chips <best>").unwrap().unwrap();
        assert!(fixed.contains("<title>Fish &amp; Chips &lt;best&gt;</title>"));
        assert_eq!(summarize(&fixed).title.as_deref(), Some("Fish & Chips <best>"));
    }
}
