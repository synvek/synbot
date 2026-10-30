//! Office delivery preview: read the *content* of generated documents without a word processor.
//!
//! Agents produce `.docx`/`.xlsx`/`.pptx`/`.pdf` files; until now the dashboard could only say
//! "binary file, 42 KB — download it". This module turns those artifacts into something the Web UI
//! can render: paragraphs, sheets and slides as text, with the same hard limits as the project file
//! preview so a 300-page document cannot stall the daemon.
//!
//! Everything here is local and synchronous — the REST layer calls it from `web::block`. No
//! LibreOffice, no new process, no network: OOXML is a ZIP of XML parts, so a small ZIP reader plus
//! a small XML tokenizer is enough to answer "what does this file say". Visual fidelity (fonts,
//! layout, images, number formats) is explicitly out of scope — see the docs for the boundary.

use std::io::Read;
use std::path::Path;

use anyhow::{bail, Context, Result};
use serde::Serialize;

pub mod docx;
pub mod pptx;
pub mod testing;
pub mod xlsx;
pub mod xml;
pub mod zip;

pub use docx::DocxPreview;
pub use pptx::PptxPreview;
pub use xlsx::XlsxPreview;
pub use zip::ZipArchive;

/// Text previews stop at this many bytes of source.
const MAX_TEXT_BYTES: u64 = 256 * 1024;

/// What kind of thing a path is, as far as the delivery panel cares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OfficeKind {
    Docx,
    Xlsx,
    Pptx,
    Pdf,
    Csv,
    Markdown,
    Html,
    Image,
    Text,
    Other,
}

impl OfficeKind {
    /// Classify by extension, case-insensitively.
    pub fn of(path: &Path) -> Self {
        let extension = path
            .extension()
            .map(|ext| ext.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        match extension.as_str() {
            "docx" => Self::Docx,
            "xlsx" | "xlsm" => Self::Xlsx,
            "pptx" => Self::Pptx,
            "pdf" => Self::Pdf,
            "csv" | "tsv" => Self::Csv,
            "md" | "markdown" => Self::Markdown,
            "html" | "htm" => Self::Html,
            "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "bmp" => Self::Image,
            "txt" | "log" | "json" | "yml" | "yaml" | "toml" => Self::Text,
            _ => Self::Other,
        }
    }

    /// Stable identifier used by the UI and the REST payload.
    pub fn slug(self) -> &'static str {
        match self {
            Self::Docx => "docx",
            Self::Xlsx => "xlsx",
            Self::Pptx => "pptx",
            Self::Pdf => "pdf",
            Self::Csv => "csv",
            Self::Markdown => "markdown",
            Self::Html => "html",
            Self::Image => "image",
            Self::Text => "text",
            Self::Other => "other",
        }
    }

    /// Deliverables are the artifacts worth listing and exporting: everything but `Other`.
    pub fn is_deliverable(self) -> bool {
        self != Self::Other
    }

    /// Whether the body is a ZIP package this module can read.
    pub fn is_ooxml(self) -> bool {
        matches!(self, Self::Docx | Self::Xlsx | Self::Pptx)
    }

    /// Kinds the browser can render by itself once it has the bytes.
    pub fn is_passthrough(self) -> bool {
        matches!(self, Self::Pdf | Self::Image)
    }

    /// Whether the content should come back as plain text.
    pub fn is_text(self) -> bool {
        matches!(self, Self::Csv | Self::Markdown | Self::Html | Self::Text)
    }

    /// MIME type used when a deliverable is downloaded or rendered by the browser.
    pub fn content_type(self) -> &'static str {
        match self {
            Self::Docx => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
            Self::Xlsx => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
            Self::Pptx => {
                "application/vnd.openxmlformats-officedocument.presentationml.presentation"
            }
            Self::Pdf => "application/pdf",
            Self::Csv => "text/csv; charset=utf-8",
            Self::Markdown => "text/markdown; charset=utf-8",
            Self::Html => "text/html; charset=utf-8",
            Self::Image => "image/*",
            Self::Text | Self::Other => "text/plain; charset=utf-8",
        }
    }
}

/// Caps applied while reading, so one hostile or huge file cannot stall the daemon.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    /// Largest file we will open at all.
    pub max_bytes: u64,
    /// Largest single ZIP entry we will inflate.
    pub max_entry_bytes: usize,
    /// Characters of plain text returned for a text-like file.
    pub max_text_chars: usize,
    pub max_paragraphs: usize,
    pub max_sheets: usize,
    pub max_rows: usize,
    pub max_cols: usize,
    pub max_shared_strings: usize,
    pub max_slides: usize,
    pub max_slide_lines: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_bytes: 64 * 1024 * 1024,
            max_entry_bytes: 32 * 1024 * 1024,
            max_text_chars: 40_000,
            max_paragraphs: 300,
            max_sheets: 20,
            max_rows: 200,
            max_cols: 40,
            max_shared_strings: 5_000,
            max_slides: 60,
            max_slide_lines: 40,
        }
    }
}

/// Everything the UI needs to render one deliverable.
#[derive(Debug, Default, Serialize)]
pub struct OfficePreview {
    /// `OfficeKind::slug()`.
    pub kind: &'static str,
    /// File size on disk (not the size of the preview).
    pub size: u64,
    /// The browser should fetch the file and render it (PDF, images).
    pub passthrough: bool,
    /// A cap was hit: the UI says so and offers the download.
    pub truncated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub docx: Option<DocxPreview>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub xlsx: Option<XlsxPreview>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pptx: Option<PptxPreview>,
}

impl OfficePreview {
    fn new(kind: OfficeKind, size: u64) -> Self {
        Self {
            kind: kind.slug(),
            size,
            passthrough: kind.is_passthrough(),
            ..Self::default()
        }
    }
}

/// Build the preview for `path`. `size` is the caller's already-known file size (from `metadata`).
pub fn preview(path: &Path, size: u64, limits: &Limits) -> Result<OfficePreview> {
    let kind = OfficeKind::of(path);
    if kind == OfficeKind::Other {
        bail!("'{kind:?}' files are not previewable");
    }
    if size > limits.max_bytes {
        bail!("file is {size} bytes (preview limit {})", limits.max_bytes);
    }
    let mut preview = OfficePreview::new(kind, size);
    if kind.is_passthrough() {
        return Ok(preview);
    }
    if kind.is_text() {
        let (text, truncated) = read_text(path, limits)?;
        preview.text = Some(text);
        preview.truncated = truncated;
        return Ok(preview);
    }

    let bytes = std::fs::read(path).with_context(|| format!("read {}", path.display()))?;
    let archive = ZipArchive::open(&bytes)?;
    match kind {
        OfficeKind::Docx => {
            let parsed = docx::preview(&archive, limits)?;
            preview.truncated = parsed.truncated;
            preview.docx = Some(parsed);
        }
        OfficeKind::Xlsx => {
            let parsed = xlsx::preview(&archive, limits)?;
            preview.truncated = parsed.truncated;
            preview.xlsx = Some(parsed);
        }
        OfficeKind::Pptx => {
            let parsed = pptx::preview(&archive, limits)?;
            preview.truncated = parsed.truncated;
            preview.pptx = Some(parsed);
        }
        _ => bail!("no previewer for '{kind:?}'"),
    }
    Ok(preview)
}

/// Read the head of a text-like file. Invalid UTF-8 becomes replacement characters instead of an
/// error, because a CSV exported from a legacy spreadsheet is still worth showing.
fn read_text(path: &Path, limits: &Limits) -> Result<(String, bool)> {
    let file = std::fs::File::open(path).with_context(|| format!("open {}", path.display()))?;
    let bytes_limit = limits.max_text_chars as u64 * 4;
    let mut buffer = Vec::new();
    file.take(bytes_limit.min(MAX_TEXT_BYTES) + 1)
        .read_to_end(&mut buffer)
        .with_context(|| format!("read {}", path.display()))?;
    let truncated_bytes = buffer.len() as u64 > bytes_limit.min(MAX_TEXT_BYTES);
    if truncated_bytes {
        buffer.truncate(bytes_limit.min(MAX_TEXT_BYTES) as usize);
    }
    let mut text = String::from_utf8_lossy(&buffer).into_owned();
    let chars: Vec<char> = text.chars().collect();
    let truncated_chars = chars.len() > limits.max_text_chars;
    if truncated_chars {
        text = chars[..limits.max_text_chars].iter().collect();
    }
    Ok((text, truncated_bytes || truncated_chars))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limits() -> Limits {
        Limits::default()
    }

    fn write(dir: &Path, name: &str, bytes: &[u8]) -> std::path::PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, bytes).unwrap();
        path
    }

    #[test]
    fn classifies_paths_by_extension() {
        assert_eq!(
            OfficeKind::of(Path::new("a/b/report.DOCX")),
            OfficeKind::Docx
        );
        assert_eq!(OfficeKind::of(Path::new("budget.xlsm")), OfficeKind::Xlsx);
        assert_eq!(OfficeKind::of(Path::new("deck.pptx")), OfficeKind::Pptx);
        assert_eq!(OfficeKind::of(Path::new("scan.pdf")), OfficeKind::Pdf);
        assert_eq!(OfficeKind::of(Path::new("notes.md")), OfficeKind::Markdown);
        assert_eq!(OfficeKind::of(Path::new("photo.PNG")), OfficeKind::Image);
        assert_eq!(OfficeKind::of(Path::new("crate.rs")), OfficeKind::Other);
        assert!(!OfficeKind::Other.is_deliverable());
        assert!(OfficeKind::Docx.is_deliverable());
        assert!(OfficeKind::Docx.is_ooxml());
        assert!(OfficeKind::Pdf.is_passthrough());
        assert!(OfficeKind::Csv.is_text());
    }

    #[test]
    fn previews_a_docx_end_to_end() {
        let dir = tempfile::tempdir().unwrap();
        let path = write(
            dir.path(),
            "report.docx",
            &testing::docx("Title", "Ada", &["Hello", "World"]),
        );
        let size = std::fs::metadata(&path).unwrap().len();
        let preview = preview(&path, size, &limits()).unwrap();
        assert_eq!(preview.kind, "docx");
        assert!(!preview.passthrough);
        let docx = preview.docx.unwrap();
        assert_eq!(docx.paragraphs, vec!["Hello", "World"]);
        assert_eq!(docx.words, 2);
    }

    #[test]
    fn previews_sheets_and_slides() {
        let dir = tempfile::tempdir().unwrap();
        let sheet = write(
            dir.path(),
            "b.xlsx",
            &testing::xlsx(&[("S", vec![vec!["a"]])]),
        );
        let size = std::fs::metadata(&sheet).unwrap().len();
        assert_eq!(
            preview(&sheet, size, &limits())
                .unwrap()
                .xlsx
                .unwrap()
                .sheets_total,
            1
        );

        let deck = write(dir.path(), "d.pptx", &testing::pptx(&[vec!["one"]]));
        let size = std::fs::metadata(&deck).unwrap().len();
        assert_eq!(
            preview(&deck, size, &limits())
                .unwrap()
                .pptx
                .unwrap()
                .slides_total,
            1
        );
    }

    #[test]
    fn pdf_and_images_are_passthrough() {
        let dir = tempfile::tempdir().unwrap();
        let pdf = write(dir.path(), "scan.pdf", b"%PDF-1.7 fake");
        let size = std::fs::metadata(&pdf).unwrap().len();
        let preview = preview(&pdf, size, &limits()).unwrap();
        assert!(preview.passthrough);
        assert!(preview.text.is_none() && preview.docx.is_none());
    }

    #[test]
    fn text_previews_stop_at_the_character_cap() {
        let dir = tempfile::tempdir().unwrap();
        let path = write(dir.path(), "table.csv", b"a,b\n1,2\n3,4\n");
        let size = std::fs::metadata(&path).unwrap().len();
        let full = super::preview(&path, size, &limits()).unwrap();
        assert_eq!(full.text.as_deref(), Some("a,b\n1,2\n3,4\n"));
        assert!(!full.truncated);

        let capped = super::preview(
            &path,
            size,
            &Limits {
                max_text_chars: 3,
                ..limits()
            },
        )
        .unwrap();
        assert_eq!(capped.text.as_deref(), Some("a,b"));
        assert!(capped.truncated);
    }

    #[test]
    fn refuses_oversized_and_unknown_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = write(dir.path(), "big.docx", b"whatever");
        let err = preview(&path, 128 * 1024 * 1024, &limits()).unwrap_err();
        assert!(format!("{err:#}").contains("preview limit"));

        let other = write(dir.path(), "main.rs", b"fn main() {}");
        let err = preview(&other, 12, &limits()).unwrap_err();
        assert!(format!("{err:#}").contains("not previewable"));
    }
}
