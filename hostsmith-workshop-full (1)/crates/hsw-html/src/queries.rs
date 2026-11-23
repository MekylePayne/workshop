//! Read-only queries over parsed HTML documents (scraper/html5ever).
//!
//! Every query degrades to "absent" on malformed input - a broken page must
//! produce findings, not crashes. Used by the SEO Inspector, Favicon
//! Studio, Open Graph preview and the validators.

use scraper::{Html, Selector};
use serde::Serialize;
use std::sync::LazyLock;

macro_rules! selector {
    ($name:ident, $expr:literal) => {
        static $name: LazyLock<Selector> =
            LazyLock::new(|| Selector::parse($expr).expect("valid selector"));
    };
}

selector!(S_TITLE, "title");
selector!(S_META, "meta");
selector!(S_HTML, "html");
selector!(S_HEADINGS, "h1, h2, h3, h4, h5, h6");
selector!(S_IMG, "img");
selector!(S_LINK, "a");
selector!(S_LINK_TAG, "link");
selector!(S_VIDEO, "video");
selector!(S_SOURCE, "source");
selector!(S_ALL, "*");

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ImageRef {
    pub src: String,
    pub alt: Option<String>,
    pub has_srcset: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct LinkRef {
    pub href: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct IconRef {
    pub rel: String,
    pub href: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Heading {
    pub level: u8,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct PageSummary {
    pub title: Option<String>,
    pub description: Option<String>,
    pub canonical: Option<String>,
    pub lang: Option<String>,
    pub viewport: Option<String>,
    pub robots_meta: Option<String>,
    /// Open Graph properties in document order, without the "og:" prefix.
    pub og: Vec<(String, String)>,
    pub headings: Vec<Heading>,
    pub h1_count: usize,
    pub images: Vec<ImageRef>,
    pub images_missing_alt: Vec<String>,
    pub links: Vec<LinkRef>,
    /// Every link[rel] entry; the favicon module classifies them.
    pub link_rels: Vec<IconRef>,
    pub manifest_link: Option<String>,
    pub video_count: usize,
    pub video_sources: Vec<String>,
}

fn element_text(element: &scraper::ElementRef) -> String {
    element.text().collect::<String>().trim().to_string()
}

/// Extracts the summary of one HTML document.
pub fn summarize(html: &str) -> PageSummary {
    let document = Html::parse_document(html);
    let mut summary = PageSummary::default();

    if let Some(title) = document.select(&S_TITLE).next() {
        let text = element_text(&title);
        if !text.is_empty() {
            summary.title = Some(text);
        }
    }

    for meta in document.select(&S_META) {
        let value = meta.value();
        let name = value.attr("name").unwrap_or("").to_ascii_lowercase();
        let property = value.attr("property").unwrap_or("").to_ascii_lowercase();
        let content = value.attr("content").map(str::to_string);
        match (name.as_str(), property.as_str()) {
            ("description", _) => summary.description = content,
            ("viewport", _) => summary.viewport = content,
            ("robots", _) => summary.robots_meta = content,
            (_, p) if p.starts_with("og:") => {
                if let Some(content) = content {
                    summary.og.push((p[3..].to_string(), content));
                }
            }
            _ => {}
        }
    }

    if let Some(html_element) = document.select(&S_HTML).next() {
        summary.lang = html_element.value().attr("lang").map(str::to_string);
    }

    for link_tag in document.select(&S_LINK_TAG) {
        let value = link_tag.value();
        let rel = value.attr("rel").unwrap_or("").to_ascii_lowercase();
        let href = value.attr("href").unwrap_or("").to_string();
        if rel.is_empty() || href.is_empty() {
            continue;
        }
        if rel == "canonical" && summary.canonical.is_none() {
            summary.canonical = Some(href.clone());
        } else if rel == "manifest" && summary.manifest_link.is_none() {
            summary.manifest_link = Some(href.clone());
        }
        summary.link_rels.push(IconRef { rel, href });
    }

    for heading in document.select(&S_HEADINGS) {
        let name = heading.value().name();
        let level = name.chars().last().and_then(|c| c.to_digit(10)).unwrap_or(0) as u8;
        summary.headings.push(Heading { level, text: element_text(&heading) });
        if level == 1 {
            summary.h1_count += 1;
        }
    }

    for image in document.select(&S_IMG) {
        let value = image.value();
        let src = value.attr("src").unwrap_or("").to_string();
        let alt = value.attr("alt").map(str::to_string);
        if alt.is_none() {
            summary.images_missing_alt.push(src.clone());
        }
        summary.images.push(ImageRef {
            src,
            alt,
            has_srcset: value.attr("srcset").is_some(),
        });
    }

    for link in document.select(&S_LINK) {
        let value = link.value();
        if let Some(href) = value.attr("href") {
            summary.links.push(LinkRef {
                href: href.to_string(),
                text: element_text(&link),
            });
        }
    }

    summary.video_count = document.select(&S_VIDEO).count();
    for source in document.select(&S_SOURCE) {
        if let Some(src) = source.value().attr("src") {
            summary.video_sources.push(src.to_string());
        }
    }

    summary
}

/// Every element's class list, in document order (badge signature checks).
pub fn element_class_lists(html: &str) -> Vec<Vec<String>> {
    let document = Html::parse_document(html);
    document
        .select(&S_ALL)
        .filter_map(|element| {
            let classes = element.value().attr("class")?;
            let list: Vec<String> = classes
                .split_whitespace()
                .map(str::to_string)
                .collect();
            Some(list)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = r##"<!doctype html>
<html lang="en-GB">
<head>
  <title>Cafe Menu</title>
  <meta name="description" content="Seasonal menu for the corner cafe">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <meta name="robots" content="noindex">
  <meta property="og:title" content="Cafe Menu">
  <meta property="og:image" content="/image/og.png">
  <link rel="canonical" href="https://example.com/menu/">
  <link rel="icon" href="/favicon.ico">
  <link rel="apple-touch-icon" href="/apple-touch-icon.png">
  <link rel="manifest" href="/manifest.webmanifest">
</head>
<body>
  <h1>Our menu</h1>
  <h3>Drinks</h3>
  <img src="image/menu.png" alt="Menu photo">
  <img src="image/missing-alt.png">
  <img src="image/slider.png" alt="Slide" srcset="image/slider.png 1x, image/slider-2x.png 2x">
  <a href="about.html">About us</a>
  <a href="https://external.example/">External</a>
  <a href="#top">Anchor</a>
  <video><source src="video/tour.mp4"></video>
</body>
</html>"##;

    #[test]
    fn summary_extracts_metadata() {
        let summary = summarize(PAGE);
        assert_eq!(summary.title.as_deref(), Some("Cafe Menu"));
        assert_eq!(summary.description.as_deref(), Some("Seasonal menu for the corner cafe"));
        assert_eq!(summary.lang.as_deref(), Some("en-GB"));
        assert!(summary.viewport.is_some());
        assert_eq!(summary.canonical.as_deref(), Some("https://example.com/menu/"));
        assert_eq!(summary.manifest_link.as_deref(), Some("/manifest.webmanifest"));
    }

    #[test]
    fn open_graph_properties_are_collected_without_prefix() {
        let summary = summarize(PAGE);
        assert!(summary.og.contains(&("title".to_string(), "Cafe Menu".to_string())));
        assert!(summary.og.contains(&("image".to_string(), "/image/og.png".to_string())));
    }

    #[test]
    fn headings_and_hierarchy_data() {
        let summary = summarize(PAGE);
        assert_eq!(summary.h1_count, 1);
        let levels: Vec<u8> = summary.headings.iter().map(|h| h.level).collect();
        assert_eq!(levels, vec![1, 3]);
    }

    #[test]
    fn images_without_alt_are_listed() {
        let summary = summarize(PAGE);
        assert_eq!(summary.images_missing_alt, vec!["image/missing-alt.png"]);
        assert!(summary.images[2].has_srcset);
    }

    #[test]
    fn links_are_collected_with_text() {
        let summary = summarize(PAGE);
        assert!(summary.links.iter().any(|l| l.href == "about.html" && l.text == "About us"));
        assert!(summary.links.iter().any(|l| l.href == "#top"));
        assert!(summary.links.iter().any(|l| l.href.starts_with("https://")));
    }

    #[test]
    fn icon_links_include_touch_and_manifest() {
        let summary = summarize(PAGE);
        assert!(summary.link_rels.iter().any(|r| r.rel == "icon"));
        assert!(summary.link_rels.iter().any(|r| r.rel == "apple-touch-icon"));
        assert_eq!(summary.video_count, 1);
        assert_eq!(summary.video_sources, vec!["video/tour.mp4"]);
    }

    #[test]
    fn malformed_html_degrades_without_panic() {
        let summary = summarize("<html><body><h1>Broken <b>page</h1><img");
        assert_eq!(summary.h1_count, 1);
        assert!(summary.title.is_none());
    }

    #[test]
    fn class_lists_are_collected_in_document_order() {
        let lists = element_class_lists(r#"<body><div class="a b"></div><span class="c"></span><p></p></body>"#);
        assert_eq!(lists.len(), 2, "elements without classes are skipped");
        assert_eq!(lists[0], vec!["a", "b"]);
        assert_eq!(lists[1], vec!["c"]);
    }
}
