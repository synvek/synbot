//! PowerPoint (`.pptx`) text extraction: one entry per slide, title first, body lines after.
//!
//! Shapes are flattened in document order, so a slide reads as "title, then everything else". The
//! first text line becomes the title because that is what a slide title placeholder holds in
//! practice — and reading the real placeholder requires the layout tree, which adds a lot of XML
//! for very little gain.

use anyhow::{bail, Context, Result};
use serde::Serialize;

use super::xml::{decode_entities, squash_whitespace, tokenize, Token};
use super::{Limits, ZipArchive};

const SLIDE_PREFIX: &str = "ppt/slides/slide";
const SLIDE_SUFFIX: &str = ".xml";

#[derive(Debug, Default, Serialize)]
pub struct SlidePreview {
    /// 1-based slide number as it appears in PowerPoint.
    pub index: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub lines: Vec<String>,
}

#[derive(Debug, Default, Serialize)]
pub struct PptxPreview {
    /// Slides in the package, including the ones past the display cap.
    pub slides_total: usize,
    pub slides: Vec<SlidePreview>,
    pub truncated: bool,
}

/// Extract the preview of a `.pptx` deck.
pub fn preview(zip: &ZipArchive, limits: &Limits) -> Result<PptxPreview> {
    let mut parts = slide_parts(zip);
    if parts.is_empty() {
        bail!("no ppt/slides part found, so this is not a presentation");
    }
    parts.sort_unstable_by_key(|(number, _)| *number);

    let mut preview = PptxPreview {
        slides_total: parts.len(),
        ..PptxPreview::default()
    };
    for (number, name) in &parts {
        if preview.slides.len() >= limits.max_slides {
            preview.truncated = true;
            break;
        }
        let xml = zip
            .read(name, limits)
            .with_context(|| format!("read slide {number}"))?;
        preview
            .slides
            .push(slide(*number, &String::from_utf8_lossy(&xml), limits));
    }
    Ok(preview)
}

fn slide(index: usize, xml: &str, limits: &Limits) -> SlidePreview {
    let mut lines: Vec<String> = Vec::new();
    let mut paragraph = String::new();
    let mut in_text = false;
    for token in tokenize(xml) {
        match token {
            // Runs of one paragraph are separate `<a:t>` nodes, so they accumulate and flush
            // together — otherwise a bold word would split its sentence into two lines.
            Token::Start {
                name,
                self_closing: false,
                ..
            } => match name {
                "p" => paragraph.clear(),
                "t" => in_text = true,
                _ => {}
            },
            Token::End { name } => match name {
                "t" => in_text = false,
                "p" => {
                    let text = squash_whitespace(&decode_entities(&paragraph));
                    paragraph.clear();
                    if !text.is_empty() && lines.len() < limits.max_slide_lines {
                        lines.push(text);
                    }
                }
                _ => {}
            },
            Token::Text(text) if in_text => paragraph.push_str(text),
            _ => {}
        }
    }
    let title = if lines.is_empty() {
        None
    } else {
        Some(lines.remove(0))
    };
    SlidePreview {
        index,
        title,
        lines,
    }
}

/// Every `ppt/slides/slideN.xml` entry, as `(N, entry name)`.
fn slide_parts(zip: &ZipArchive) -> Vec<(usize, String)> {
    zip.names()
        .filter_map(|name| {
            let rest = name.strip_prefix(SLIDE_PREFIX)?;
            let number = rest.strip_suffix(SLIDE_SUFFIX)?.parse::<usize>().ok()?;
            Some((number, name.to_string()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::office::testing::{pptx, zip_with, Entry};

    fn limits() -> Limits {
        Limits::default()
    }

    #[test]
    fn reads_slides_in_numeric_order_with_title_first() {
        let bytes = pptx(&[
            vec!["Intro", "welcome", "agenda"],
            vec!["Numbers", "revenue up"],
        ]);
        let zip = ZipArchive::open(&bytes).unwrap();
        let preview = preview(&zip, &limits()).unwrap();
        assert_eq!(preview.slides_total, 2);
        assert_eq!(preview.slides[0].index, 1);
        assert_eq!(preview.slides[0].title.as_deref(), Some("Intro"));
        assert_eq!(preview.slides[0].lines, vec!["welcome", "agenda"]);
        assert_eq!(preview.slides[1].title.as_deref(), Some("Numbers"));
    }

    #[test]
    fn sorts_slides_by_number_not_by_name() {
        let one = r#"<p:sld xmlns:p="p" xmlns:a="a"><a:p><a:t>one</a:t></a:p></p:sld>"#;
        let two = r#"<p:sld xmlns:p="p" xmlns:a="a"><a:p><a:t>two</a:t></a:p></p:sld>"#;
        let ten = r#"<p:sld xmlns:p="p" xmlns:a="a"><a:p><a:t>ten</a:t></a:p></p:sld>"#;
        let bytes = zip_with(&[
            Entry::deflated("ppt/slides/slide10.xml", ten.as_bytes()),
            Entry::deflated("ppt/slides/slide2.xml", two.as_bytes()),
            Entry::deflated("ppt/slides/slide1.xml", one.as_bytes()),
        ]);
        let zip = ZipArchive::open(&bytes).unwrap();
        let preview = preview(&zip, &limits()).unwrap();
        let titles: Vec<&str> = preview
            .slides
            .iter()
            .map(|slide| slide.title.as_deref().unwrap_or(""))
            .collect();
        assert_eq!(titles, vec!["one", "two", "ten"]);
    }

    #[test]
    fn caps_slides_and_flags_truncation() {
        let deck: Vec<Vec<&str>> = (0..5).map(|_| vec!["slide"]).collect();
        let bytes = pptx(&deck);
        let zip = ZipArchive::open(&bytes).unwrap();
        let preview = preview(
            &zip,
            &Limits {
                max_slides: 2,
                ..limits()
            },
        )
        .unwrap();
        assert_eq!(preview.slides.len(), 2);
        assert_eq!(preview.slides_total, 5);
        assert!(preview.truncated);
    }

    #[test]
    fn joins_runs_of_one_paragraph_into_one_line() {
        let slide = r#"<p:sld xmlns:p="p" xmlns:a="a"><a:p><a:r><a:t>Bold</a:t></a:r><a:r><a:t xml:space="preserve"> tail</a:t></a:r></a:p><a:p><a:r><a:t>second</a:t></a:r></a:p></p:sld>"#;
        let bytes = zip_with(&[Entry::deflated("ppt/slides/slide1.xml", slide.as_bytes())]);
        let zip = ZipArchive::open(&bytes).unwrap();
        let preview = preview(&zip, &limits()).unwrap();
        assert_eq!(preview.slides[0].title.as_deref(), Some("Bold tail"));
        assert_eq!(preview.slides[0].lines, vec!["second"]);
    }

    #[test]
    fn rejects_packages_without_slides() {
        let bytes = zip_with(&[Entry::stored("ppt/presentation.xml", b"<x/>")]);
        let zip = ZipArchive::open(&bytes).unwrap();
        assert!(preview(&zip, &limits()).is_err());
    }
}
