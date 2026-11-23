//! Safe HTML source rewriting with lol_html (review 6.3, TL-06, PR-03).
//!
//! Every edit is a streaming rewrite: only the targeted element's tag or the
//! targeted text node changes; every other byte of the file keeps its exact
//! original formatting, quoting and attribute order.
//!
//! Targeting contract with the preview bridge: element indices are assigned
//! in source document order and injected as `data-hsw-e` attributes at
//! serve time ([`annotate_and_inject`]); a text selection is reported as
//! (element index, text ordinal), where the ordinal counts text nodes inside
//! that element's content in document order - exactly the order in which a
//! selector-scoped text handler receives them, so an edit pass scoped to
//! `[data-hsw-e="N"]` hits the node the user clicked.

use lol_html::html_content::{ContentType, DocumentEnd, Element, TextChunk};
use lol_html::{DocumentContentHandlers, ElementContentHandlers, HtmlRewriter, Settings};
use std::borrow::Cow;
use std::cell::Cell;

#[derive(Debug, thiserror::Error)]
pub enum RewriteError {
    #[error("element #{index} not found - the file may have changed; refresh the preview")]
    ElementNotFound { index: usize },
    #[error("text node {ordinal} not found in element #{index}")]
    TextNodeNotFound { index: usize, ordinal: usize },
    #[error("text nodes inside script or style elements cannot be edited as text")]
    TextNodeForbidden,
    #[error("no <head> element found - add a head section first")]
    HeadNotFound,
    #[error("no <{tag}> element found in the document")]
    TagNotFound { tag: String },
    #[error("rewriter failure: {0}")]
    Rewriter(String),
}

fn parse_selector(expression: &str) -> Result<Cow<'static, lol_html::Selector>, RewriteError> {
    expression
        .parse::<lol_html::Selector>()
        .map(Cow::Owned)
        .map_err(|err| RewriteError::Rewriter(format!("invalid selector {expression:?}: {err}")))
}

fn all_elements_selector() -> Cow<'static, lol_html::Selector> {
    Cow::Owned("*".parse::<lol_html::Selector>().expect("valid selector"))
}

/// Escape only the characters that are unsafe inside HTML text content;
/// quotes and other entities are left untouched so diffs stay minimal.
fn escape_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            other => out.push(other),
        }
    }
    out
}

/// Adds `data-hsw-e="N"` to every start tag (N counting from 0 in document
/// order). This is the serve-time annotation the selection bridge relies on.
/// The injected attributes exist only in the served preview; source files on
/// disk are never touched by this function.
pub fn annotate_elements(html: &str) -> String {
    let counter = Cell::new(0usize);
    let mut element_handler = |element: &mut Element| {
        let _ = element.set_attribute("data-hsw-e", &counter.get().to_string());
        counter.set(counter.get() + 1);
        Ok(())
    };
    let handlers = ElementContentHandlers::default().element(&mut element_handler);
    let settings = Settings {
        element_content_handlers: vec![(all_elements_selector(), handlers)],
        ..Settings::default()
    };
    let mut output = Vec::with_capacity(html.len() + 64);
    let mut rewriter = HtmlRewriter::new(settings, |chunk: &[u8]| output.extend_from_slice(chunk));
    rewriter.write(html.as_bytes()).expect("in-memory write");
    rewriter.end().expect("in-memory end");
    String::from_utf8(output).expect("rewrite preserves UTF-8")
}

/// Combined serve-time transformation used by the preview server: annotates
/// every start tag with `data-hsw-e` and injects `script` at the end of the
/// `<head>` (or, failing that, at the end of the document). Source files on
/// disk are never touched by this function.
pub fn annotate_and_inject(html: &str, script: &str) -> String {
    let counter = Cell::new(0usize);
    let head_found = Cell::new(false);
    let mut annotate_handler = |element: &mut Element| {
        let _ = element.set_attribute("data-hsw-e", &counter.get().to_string());
        counter.set(counter.get() + 1);
        Ok(())
    };
    let mut head_handler = |element: &mut Element| {
        if !head_found.get() {
            element.append(script, ContentType::Html);
            head_found.set(true);
        }
        Ok(())
    };
    let mut end_handler = |doc_end: &mut DocumentEnd| {
        if !head_found.get() {
            doc_end.append(script, ContentType::Html);
            head_found.set(true);
        }
        Ok(())
    };
    let head_selector = Cow::Owned("head".parse::<lol_html::Selector>().expect("valid selector"));
    let annotate_handlers = ElementContentHandlers::default().element(&mut annotate_handler);
    let head_handlers = ElementContentHandlers::default().element(&mut head_handler);
    let document_handlers = DocumentContentHandlers::default().end(&mut end_handler);
    let settings = Settings {
        element_content_handlers: vec![
            (all_elements_selector(), annotate_handlers),
            (head_selector, head_handlers),
        ],
        document_content_handlers: vec![document_handlers],
        ..Settings::default()
    };
    let mut output = Vec::with_capacity(html.len() + script.len() + 64);
    let mut rewriter = HtmlRewriter::new(settings, |chunk: &[u8]| output.extend_from_slice(chunk));
    rewriter.write(html.as_bytes()).expect("in-memory write");
    rewriter.end().expect("in-memory end");
    String::from_utf8(output).expect("rewrite preserves UTF-8")
}

/// State machine for selector-scoped text replacement: the scoped handler
/// receives text chunks of the target element's content in document order;
/// the ordinal counts text nodes, and one node may arrive in fragments.
struct TextTarget<'a> {
    target_ordinal: usize,
    replacement: &'a str,
    ordinal: usize,
    node_open: bool,
    on_target_node: bool,
    replaced: bool,
    found: bool,
}

impl<'a> TextTarget<'a> {
    fn new(target_ordinal: usize, replacement: &'a str) -> Self {
        Self {
            target_ordinal,
            replacement,
            ordinal: 0,
            node_open: false,
            on_target_node: false,
            replaced: false,
            found: false,
        }
    }

    fn on_chunk(&mut self, chunk: &mut TextChunk) {
        if !self.node_open {
            self.node_open = true;
            self.on_target_node = self.ordinal == self.target_ordinal;
            if self.on_target_node {
                self.found = true;
            }
            self.replaced = false;
        }
        if self.on_target_node {
            if !self.replaced {
                chunk.replace(self.replacement, ContentType::Html);
                self.replaced = true;
            } else {
                chunk.remove();
            }
        }
        if chunk.last_in_text_node() {
            self.node_open = false;
            self.ordinal += 1;
            self.on_target_node = false;
        }
    }
}

/// Replaces the `ordinal`-th text node inside element `element_index` (0-based
/// start-tag order; the ordinal counts text nodes in the element's content in
/// document order, matching the preview bridge). The replacement is
/// HTML-escaped; every other byte of the file is preserved exactly.
///
/// Implementation: annotate -> scoped edit -> cleanup. The annotation pass
/// gives the target a matchable attribute, the scoped pass rewrites only its
/// text, and the cleanup pass strips the annotations again.
pub fn replace_text_node(
    html: &str,
    element_index: usize,
    ordinal: usize,
    replacement: &str,
) -> Result<String, RewriteError> {
    let elements = list_elements(html);
    let Some(summary) = elements.get(element_index) else {
        return Err(RewriteError::ElementNotFound { index: element_index });
    };
    let name = summary.name.to_ascii_lowercase();
    if name == "script" || name == "style" {
        return Err(RewriteError::TextNodeForbidden);
    }

    let annotated = annotate_elements(html);
    let escaped = escape_text(replacement);
    let selector = parse_selector(&format!("[data-hsw-e=\"{element_index}\"]"))?;
    let mut state = TextTarget::new(ordinal, &escaped);
    let mut text_handler = |chunk: &mut TextChunk| {
        state.on_chunk(chunk);
        Ok(())
    };
    let handlers = ElementContentHandlers::default().text(&mut text_handler);
    let settings = Settings {
        element_content_handlers: vec![(selector, handlers)],
        ..Settings::default()
    };
    let mut edited = Vec::with_capacity(annotated.len() + 64);
    {
        let mut rewriter = HtmlRewriter::new(settings, |chunk: &[u8]| edited.extend_from_slice(chunk));
        rewriter
            .write(annotated.as_bytes())
            .map_err(|err| RewriteError::Rewriter(err.to_string()))?;
        rewriter.end().map_err(|err| RewriteError::Rewriter(err.to_string()))?;
    }
    if !state.found {
        return Err(RewriteError::TextNodeNotFound { index: element_index, ordinal });
    }
    remove_annotations(&String::from_utf8(edited).expect("rewrite preserves UTF-8"))
}

/// A single in-place change to one element's start tag.
#[derive(Debug, Clone)]
pub enum ElementEdit {
    /// Sets an attribute to a value, creating it if it does not exist.
    SetAttribute { name: String, value: String },
    /// Removes an attribute if present.
    RemoveAttribute { name: String },
}

/// Applies `edit` to element `index` (0-based start-tag order) in one
/// streaming pass; every other byte of the file is preserved.
pub fn edit_element(html: &str, index: usize, edit: ElementEdit) -> Result<String, RewriteError> {
    let counter = Cell::new(0usize);
    let found = Cell::new(false);
    let mut element_handler = |element: &mut Element| {
        let current = counter.get();
        if current == index {
            found.set(true);
            match &edit {
                ElementEdit::SetAttribute { name, value } => {
                    let _ = element.set_attribute(name, value);
                }
                ElementEdit::RemoveAttribute { name } => {
                    element.remove_attribute(name);
                }
            }
        }
        counter.set(current + 1);
        Ok(())
    };
    let handlers = ElementContentHandlers::default().element(&mut element_handler);
    let settings = Settings {
        element_content_handlers: vec![(all_elements_selector(), handlers)],
        ..Settings::default()
    };
    let mut output = Vec::with_capacity(html.len() + 64);
    let mut rewriter = HtmlRewriter::new(settings, |chunk: &[u8]| output.extend_from_slice(chunk));
    rewriter
        .write(html.as_bytes())
        .map_err(|err| RewriteError::Rewriter(err.to_string()))?;
    rewriter.end().map_err(|err| RewriteError::Rewriter(err.to_string()))?;
    if !found.get() {
        return Err(RewriteError::ElementNotFound { index });
    }
    Ok(String::from_utf8(output).expect("rewrite preserves UTF-8"))
}

fn remove_annotations(html: &str) -> Result<String, RewriteError> {
    let mut element_handler = |element: &mut Element| {
        element.remove_attribute("data-hsw-e");
        Ok(())
    };
    let handlers = ElementContentHandlers::default().element(&mut element_handler);
    let settings = Settings {
        element_content_handlers: vec![(all_elements_selector(), handlers)],
        ..Settings::default()
    };
    let mut output = Vec::with_capacity(html.len());
    let mut rewriter = HtmlRewriter::new(settings, |chunk: &[u8]| output.extend_from_slice(chunk));
    rewriter
        .write(html.as_bytes())
        .map_err(|err| RewriteError::Rewriter(err.to_string()))?;
    rewriter.end().map_err(|err| RewriteError::Rewriter(err.to_string()))?;
    Ok(String::from_utf8(output).expect("rewrite preserves UTF-8"))
}

/// Appends an HTML snippet at the end of the `<head>` element's content.
/// Used by automatic fixes such as the viewport meta insertion; everything
/// else is byte-preserved.
pub fn append_to_head(html: &str, snippet: &str) -> Result<String, RewriteError> {
    let found = Cell::new(false);
    let snippet_ref = snippet;
    let mut element_handler = |element: &mut Element| {
        if !found.get() {
            element.append(snippet_ref, ContentType::Html);
            found.set(true);
        }
        Ok(())
    };
    let selector = Cow::Owned("head".parse::<lol_html::Selector>().expect("valid selector"));
    let handlers = ElementContentHandlers::default().element(&mut element_handler);
    let settings = Settings {
        element_content_handlers: vec![(selector, handlers)],
        ..Settings::default()
    };
    let mut output = Vec::with_capacity(html.len() + snippet.len() + 16);
    let mut rewriter = HtmlRewriter::new(settings, |chunk: &[u8]| output.extend_from_slice(chunk));
    rewriter
        .write(html.as_bytes())
        .map_err(|err| RewriteError::Rewriter(err.to_string()))?;
    rewriter.end().map_err(|err| RewriteError::Rewriter(err.to_string()))?;
    if !found.get() {
        return Err(RewriteError::HeadNotFound);
    }
    Ok(String::from_utf8(output).expect("rewrite preserves UTF-8"))
}

/// Sets an attribute on the first element matching `tag_name` (for example
/// `lang` on `<html>`). Byte-preserving for everything else.
pub fn set_attribute_on_first(
    html: &str,
    tag_name: &str,
    name: &str,
    value: &str,
) -> Result<String, RewriteError> {
    let matched = Cell::new(false);
    let name_ref = name;
    let value_ref = value;
    let mut element_handler = |element: &mut Element| {
        if !matched.get() {
            let _ = element.set_attribute(name_ref, value_ref);
            matched.set(true);
        }
        Ok(())
    };
    let selector = parse_selector(tag_name)?;
    let handlers = ElementContentHandlers::default().element(&mut element_handler);
    let settings = Settings {
        element_content_handlers: vec![(selector, handlers)],
        ..Settings::default()
    };
    let mut output = Vec::with_capacity(html.len() + 32);
    let mut rewriter = HtmlRewriter::new(settings, |chunk: &[u8]| output.extend_from_slice(chunk));
    rewriter
        .write(html.as_bytes())
        .map_err(|err| RewriteError::Rewriter(err.to_string()))?;
    rewriter.end().map_err(|err| RewriteError::Rewriter(err.to_string()))?;
    if !matched.get() {
        return Err(RewriteError::TagNotFound { tag: tag_name.to_string() });
    }
    Ok(String::from_utf8(output).expect("rewrite preserves UTF-8"))
}

/// Appends an HTML snippet just before `</body>`. Used by the footer-badge
/// mechanism to insert the exact user-supplied badge markup verbatim; all
/// other bytes are preserved.
pub fn append_to_body_end(html: &str, snippet: &str) -> Result<String, RewriteError> {
    let found = Cell::new(false);
    let snippet_ref = snippet;
    let mut element_handler = |element: &mut Element| {
        if !found.get() {
            element.append(snippet_ref, ContentType::Html);
            found.set(true);
        }
        Ok(())
    };
    let selector = Cow::Owned("body".parse::<lol_html::Selector>().expect("valid selector"));
    let handlers = ElementContentHandlers::default().element(&mut element_handler);
    let settings = Settings {
        element_content_handlers: vec![(selector, handlers)],
        ..Settings::default()
    };
    let mut output = Vec::with_capacity(html.len() + snippet.len() + 16);
    let mut rewriter = HtmlRewriter::new(settings, |chunk: &[u8]| output.extend_from_slice(chunk));
    rewriter
        .write(html.as_bytes())
        .map_err(|err| RewriteError::Rewriter(err.to_string()))?;
    rewriter.end().map_err(|err| RewriteError::Rewriter(err.to_string()))?;
    if !found.get() {
        return Err(RewriteError::TagNotFound { tag: "body".to_string() });
    }
    Ok(String::from_utf8(output).expect("rewrite preserves UTF-8"))
}

/// Diagnostic summary of one element, used by tests and the inspector.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElementSummary {
    pub index: usize,
    pub name: String,
    pub attributes: Vec<(String, String)>,
}

/// Lists every element with its index and attributes (start-tag order).
pub fn list_elements(html: &str) -> Vec<ElementSummary> {
    let elements: Cell<Vec<ElementSummary>> = Cell::new(Vec::new());
    let mut element_handler = |element: &mut Element| {
        let mut current = elements.take();
        current.push(ElementSummary {
            index: current.len(),
            name: element.tag_name(),
            attributes: element
                .attributes()
                .iter()
                .map(|attribute| (attribute.name(), attribute.value()))
                .collect(),
        });
        elements.set(current);
        Ok(())
    };
    let handlers = ElementContentHandlers::default().element(&mut element_handler);
    let settings = Settings {
        element_content_handlers: vec![(all_elements_selector(), handlers)],
        ..Settings::default()
    };
    let mut rewriter = HtmlRewriter::new(settings, |_: &[u8]| {});
    rewriter.write(html.as_bytes()).expect("in-memory write");
    rewriter.end().expect("in-memory end");
    elements.into_inner()
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = "<!doctype html><html><body>\n<h1>Old heading</h1>\n<p>First paragraph <b>bold</b> continues.</p>\n<img src=\"image/logo.png\" alt=\"Logo\">\n</body></html>";

    const PAGE_WITH_HEAD: &str = "<!doctype html><html><head><title>Home</title></head><body>\n<h1>Old heading</h1>\n<p>First paragraph <b>bold</b> continues.</p>\n<img src=\"image/logo.png\" alt=\"Logo\">\n</body></html>";

    #[test]
    fn annotate_assigns_indices_in_document_order() {
        let annotated = annotate_elements(PAGE);
        let elements = list_elements(&annotated);
        let expected: Vec<(usize, &str)> = vec![
            (0, "html"),
            (1, "body"),
            (2, "h1"),
            (3, "p"),
            (4, "b"),
            (5, "img"),
        ];
        for (index, name) in expected {
            assert_eq!(elements[index].name, name, "element #{index}");
            let value = elements[index]
                .attributes
                .iter()
                .find(|(n, _)| n == "data-hsw-e")
                .map(|(_, v)| v.clone())
                .unwrap_or_default();
            assert_eq!(value, index.to_string());
        }
    }

    #[test]
    fn annotate_preserves_surrounding_bytes() {
        let annotated = annotate_elements(PAGE);
        assert!(annotated.contains("</h1>\n"));
        assert!(annotated.contains("alt=\"Logo\""));
    }

    #[test]
    fn text_replacement_preserves_surrounding_bytes() {
        // Inner-content text of <p> (#3) in document order:
        // 0: "First paragraph ", 1: "bold" (inside <b>), 2: " continues."
        let edited = replace_text_node(PAGE, 3, 1, "strong").unwrap();
        assert!(edited.contains("<p>First paragraph <b>strong</b> continues.</p>"));
        assert!(edited.contains("<h1>Old heading</h1>"), "untouched region preserved");
    }

    #[test]
    fn text_replacement_escapes_unsafe_characters() {
        let edited = replace_text_node(PAGE, 2, 0, "Fish & Chips <today>").unwrap();
        assert!(edited.contains("<h1>Fish &amp; Chips &lt;today&gt;</h1>"));
    }

    #[test]
    fn cleanup_restores_unannotated_bytes() {
        let edited = replace_text_node(PAGE, 2, 0, "New heading").unwrap();
        assert!(!edited.contains("data-hsw-e"), "annotations must be stripped");
        assert!(edited.contains("src=\"image/logo.png\" alt=\"Logo\""), "attribute layout preserved");
    }

    #[test]
    fn missing_text_node_is_an_error_not_silence() {
        assert!(replace_text_node(PAGE, 3, 9, "x").is_err());
        assert!(replace_text_node(PAGE, 99, 0, "x").is_err());
    }

    #[test]
    fn script_text_is_not_editable() {
        let html = "<body><script>if (a < b) { c(); }</script></body>";
        let result = replace_text_node(html, 1, 0, "alert(1)");
        assert!(matches!(result, Err(RewriteError::TextNodeForbidden)));
    }

    #[test]
    fn attribute_edit_is_minimal() {
        let edited = edit_element(
            PAGE,
            5,
            ElementEdit::SetAttribute {
                name: "src".into(),
                value: "image/new-logo.png".into(),
            },
        )
        .unwrap();
        assert!(edited.contains("src=\"image/new-logo.png\""));
        assert!(edited.contains("alt=\"Logo\""), "other attributes preserved");
        assert!(edited.contains("<h1>Old heading</h1>"), "rest of file preserved");
    }

    #[test]
    fn attribute_can_be_created_and_removed() {
        let created = edit_element(
            PAGE,
            2,
            ElementEdit::SetAttribute { name: "id".into(), value: "main".into() },
        )
        .unwrap();
        assert!(created.contains("<h1 id=\"main\">"));
        let removed = edit_element(
            PAGE,
            5,
            ElementEdit::RemoveAttribute { name: "alt".into() },
        )
        .unwrap();
        assert!(!removed.contains("alt="));
    }

    #[test]
    fn missing_element_is_an_error() {
        assert!(matches!(
            edit_element(PAGE, 42, ElementEdit::RemoveAttribute { name: "x".into() }),
            Err(RewriteError::ElementNotFound { index: 42 })
        ));
    }

    #[test]
    fn annotate_then_edit_indices_align() {
        // The index of an element in the annotated output must equal its
        // index in the original source (the contract the bridge relies on).
        let annotated = annotate_elements(PAGE);
        let img_in_annotated = list_elements(&annotated)
            .into_iter()
            .find(|e| e.name == "img")
            .unwrap();
        let edited = edit_element(
            PAGE,
            img_in_annotated.index,
            ElementEdit::SetAttribute { name: "src".into(), value: "image/new.png".into() },
        )
        .unwrap();
        assert!(edited.contains("src=\"image/new.png\""));
    }

    #[test]
    fn combined_annotation_and_injection() {
        let script = "<script>console.log('bridge')</script>";
        let out = annotate_and_inject(PAGE_WITH_HEAD, script);
        // Elements: html, head, title, body, h1, p, b, img -> img is #7.
        assert!(out.contains("data-hsw-e=\"7\""));
        assert!(out.contains(script));
        // The script lands at the end of the head's content...
        let head_end = out.find("</head>").unwrap();
        let script_pos = out.find(script).unwrap();
        assert!(script_pos < head_end);
    }

    #[test]
    fn combined_injection_falls_back_to_document_end_without_head() {
        let script = "<script>x</script>";
        let out = annotate_and_inject("<body><p>no head here</p></body>", script);
        assert!(out.contains("</body>"), "content preserved");
        assert!(out.ends_with(script));
        assert!(out.contains("data-hsw-e=\""), "annotation still applied");
    }

    #[test]
    fn append_to_head_adds_viewport_like_snippets() {
        let snippet = "<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">";
        let out = append_to_head(PAGE_WITH_HEAD, snippet).unwrap();
        assert!(out.contains(&format!("{snippet}</head>")));
    }

    #[test]
    fn append_to_head_without_head_is_an_error() {
        let result = append_to_head("<body><p>x</p></body>", "<meta>");
        assert!(matches!(result, Err(RewriteError::HeadNotFound)));
    }

    #[test]
    fn set_attribute_on_first_sets_lang() {
        let out = set_attribute_on_first(PAGE, "html", "lang", "en-GB").unwrap();
        assert!(out.contains("<html lang=\"en-GB\""));
    }

    #[test]
    fn set_attribute_on_first_missing_tag_is_an_error() {
        let result = set_attribute_on_first(PAGE, "nosuchtag", "id", "x");
        assert!(matches!(result, Err(RewriteError::TagNotFound { tag }) if tag == "nosuchtag"));
    }

    #[test]
    fn badge_snippet_is_inserted_verbatim_before_body_end() {
        let snippet = "<footer class=\"hs-footer\"><a class=\"hs-footer-link\" href=\"https://hostsmith.example\">HostSmith</a></footer>";
        let out = append_to_body_end(PAGE, snippet).unwrap();
        assert!(out.contains(&format!("{snippet}</body>")));
        let missing = append_to_body_end("<html><head></head></html>", snippet);
        assert!(matches!(missing, Err(RewriteError::TagNotFound { tag }) if tag == "body"));
    }
}
