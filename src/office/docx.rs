//! Word (`.docx`) text extraction: paragraphs in document order, plus title/creator metadata.
//!
//! Formatting is out of scope — the preview answers "what does this document say", so runs are
//! concatenated, tab stops and line breaks become whitespace, and empty paragraphs are dropped
//! (Word documents are full of spacers). Tables read as their cell text, one paragraph per cell.

use anyhow::{Context, Result};
use serde::Serialize;

use super::xml::{decode_entities, squash_whitespace, tokenize, Token};
use super::{Limits, ZipArchive};

const DOCUMENT_PART: &str = "word/document.xml";
const CORE_PART: &str = "docProps/core.xml";

#[derive(Debug, Default, Serialize)]
pub struct DocxPreview {
    /// `dc:title` from `docProps/core.xml`, when the producer wrote one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// `dc:creator`, i.e. the last person who saved the file.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    /// Non-empty paragraphs in the whole document, including the ones past the display cap.
    pub paragraphs_total: usize,
    pub words: usize,
    pub paragraphs: Vec<String>,
    pub truncated: bool,
}

/// Extract the preview of a `.docx` package.
pub fn preview(zip: &ZipArchive, limits: &Limits) -> Result<DocxPreview> {
    let document = zip
        .read(DOCUMENT_PART, limits)
        .with_context(|| format!("{DOCUMENT_PART} is missing, so this is not a Word document"))?;
    let document = String::from_utf8_lossy(&document);
    let mut preview = extract(&document, limits);
    if zip.contains(CORE_PART) {
        let core = zip.read(CORE_PART, limits)?;
        let core = String::from_utf8_lossy(&core);
        preview.title = element_value(&core, "title");
        preview.author = element_value(&core, "creator");
    }
    Ok(preview)
}

fn extract(xml: &str, limits: &Limits) -> DocxPreview {
    let mut preview = DocxPreview::default();
    let mut buffer = String::new();
    let mut in_text = false;
    let mut chars = 0usize;

    for token in tokenize(xml) {
        match token {
            Token::Start {
                name, self_closing, ..
            } => match name {
                "p" => buffer.clear(),
                "t" if !self_closing => in_text = true,
                // `<w:tab/>`, `<w:br/>`, `<w:cr/>` are empty elements, so they only ever arrive
                // as self-closing starts.
                "tab" | "br" | "cr" => buffer.push(' '),
                _ => {}
            },
            Token::End { name } => match name {
                "t" => in_text = false,
                "p" => {
                    let text = squash_whitespace(&decode_entities(&buffer));
                    if text.is_empty() {
                        continue;
                    }
                    preview.paragraphs_total += 1;
                    preview.words += text.split_whitespace().count();
                    if preview.paragraphs.len() >= limits.max_paragraphs
                        || chars >= limits.max_text_chars
                    {
                        preview.truncated = true;
                        continue;
                    }
                    chars += text.chars().count();
                    preview.paragraphs.push(text);
                }
                _ => {}
            },
            Token::Text(text) if in_text => buffer.push_str(text),
            _ => {}
        }
    }
    preview
}

/// Text of the first `<local>` element, e.g. `<dc:title>` in the core properties part.
fn element_value(xml: &str, local: &str) -> Option<String> {
    let mut inside = false;
    let mut buffer = String::new();
    for token in tokenize(xml) {
        match token {
            Token::Start { name, .. } if name == local => inside = true,
            Token::End { name } if name == local => {
                let text = squash_whitespace(&decode_entities(&buffer));
                return (!text.is_empty()).then_some(text);
            }
            Token::Text(text) if inside => buffer.push_str(text),
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::office::testing::{docx, zip_with, Entry};

    fn limits() -> Limits {
        Limits::default()
    }

    #[test]
    fn reads_paragraphs_metadata_and_counts() {
        let bytes = docx(
            "Quarterly report",
            "Ada",
            &["First line", "", "Second & last"],
        );
        let zip = ZipArchive::open(&bytes).unwrap();
        let preview = preview(&zip, &limits()).unwrap();
        assert_eq!(preview.title.as_deref(), Some("Quarterly report"));
        assert_eq!(preview.author.as_deref(), Some("Ada"));
        assert_eq!(preview.paragraphs, vec!["First line", "Second & last"]);
        assert_eq!(preview.paragraphs_total, 2);
        assert_eq!(preview.words, 5);
        assert!(!preview.truncated);
    }

    #[test]
    fn table_cells_become_their_own_paragraphs() {
        let document = r#"<w:document xmlns:w="w"><w:body><w:tbl><w:tr><w:tc><w:p><w:r><w:t>a</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>b</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:p><w:r><w:t>c</w:t></w:r></w:p></w:body></w:document>"#;
        let bytes = zip_with(&[Entry::deflated("word/document.xml", document.as_bytes())]);
        let zip = ZipArchive::open(&bytes).unwrap();
        let preview = preview(&zip, &limits()).unwrap();
        assert_eq!(preview.paragraphs, vec!["a", "b", "c"]);
    }

    #[test]
    fn splits_runs_tabs_and_breaks() {
        let document = r#"<w:document><w:body><w:p><w:r><w:t>Hello</w:t></w:r><w:r><w:t xml:space="preserve"> world</w:t></w:r><w:r><w:tab/><w:t>tail</w:t></w:r></w:p></w:body></w:document>"#;
        let bytes = zip_with(&[Entry::deflated("word/document.xml", document.as_bytes())]);
        let zip = ZipArchive::open(&bytes).unwrap();
        let preview = preview(&zip, &limits()).unwrap();
        assert_eq!(preview.paragraphs, vec!["Hello world tail"]);
    }

    #[test]
    fn caps_paragraphs_and_flags_truncation() {
        let long: Vec<&str> = vec!["paragraph"; 12];
        let bytes = docx("t", "a", &long);
        let zip = ZipArchive::open(&bytes).unwrap();
        let preview = preview(
            &zip,
            &Limits {
                max_paragraphs: 5,
                ..limits()
            },
        )
        .unwrap();
        assert_eq!(preview.paragraphs.len(), 5);
        assert!(preview.truncated);
        assert_eq!(preview.paragraphs_total, 12);
    }

    #[test]
    fn rejects_packages_without_a_document_part() {
        let bytes = zip_with(&[Entry::stored("word/other.xml", b"<x/>")]);
        let zip = ZipArchive::open(&bytes).unwrap();
        let err = preview(&zip, &limits()).unwrap_err();
        assert!(format!("{err:#}").contains("not a Word document"));
    }
}
