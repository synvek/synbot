//! Fixture builders for the office preview tests (unit tests and `tests/delivery_api_tests.rs`).
//!
//! These write *real* ZIP packages — stored and deflated entries, central directory, EOCD — so the
//! reader is exercised against the same container a word processor produces. Only the XML payloads
//! are reduced to the parts the extractors look at.

#![doc(hidden)]

/// One entry to write into a fixture package.
pub struct Entry {
    pub name: String,
    pub data: Vec<u8>,
    /// Compress with deflate; office producers do this for every XML part.
    pub deflate: bool,
}

impl Entry {
    pub fn stored(name: &str, data: &[u8]) -> Self {
        Self {
            name: name.to_string(),
            data: data.to_vec(),
            deflate: false,
        }
    }

    pub fn deflated(name: &str, data: &[u8]) -> Self {
        Self {
            name: name.to_string(),
            data: data.to_vec(),
            deflate: true,
        }
    }
}

/// Build a ZIP archive out of `entries`, in the given order.
pub fn zip_with(entries: &[Entry]) -> Vec<u8> {
    const FLAG_UTF8: u16 = 0x0800;
    let mut out = Vec::new();
    let mut central = Vec::new();

    for entry in entries {
        let (method, payload) = if entry.deflate {
            (8u16, deflate(&entry.data))
        } else {
            (0u16, entry.data.clone())
        };
        let crc = crc32(&entry.data);
        let offset = out.len() as u32;
        let name = entry.name.as_bytes();

        // Local file header.
        out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        out.extend_from_slice(&20u16.to_le_bytes());
        out.extend_from_slice(&FLAG_UTF8.to_le_bytes());
        out.extend_from_slice(&method.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes()); // mod time
        out.extend_from_slice(&0u16.to_le_bytes()); // mod date
        out.extend_from_slice(&crc.to_le_bytes());
        out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        out.extend_from_slice(&(entry.data.len() as u32).to_le_bytes());
        out.extend_from_slice(&(name.len() as u16).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes()); // extra len
        out.extend_from_slice(name);
        out.extend_from_slice(&payload);

        // Matching central directory record.
        central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        central.extend_from_slice(&20u16.to_le_bytes()); // version made by
        central.extend_from_slice(&20u16.to_le_bytes()); // version needed
        central.extend_from_slice(&FLAG_UTF8.to_le_bytes());
        central.extend_from_slice(&method.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&crc.to_le_bytes());
        central.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        central.extend_from_slice(&(entry.data.len() as u32).to_le_bytes());
        central.extend_from_slice(&(name.len() as u16).to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes()); // extra len
        central.extend_from_slice(&0u16.to_le_bytes()); // comment len
        central.extend_from_slice(&0u16.to_le_bytes()); // disk start
        central.extend_from_slice(&0u16.to_le_bytes()); // internal attrs
        central.extend_from_slice(&0u32.to_le_bytes()); // external attrs
        central.extend_from_slice(&offset.to_le_bytes());
        central.extend_from_slice(name);
    }

    let cd_offset = out.len() as u32;
    let cd_size = central.len() as u32;
    let count = entries.len() as u16;
    out.extend_from_slice(&central);
    out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes()); // disk
    out.extend_from_slice(&0u16.to_le_bytes()); // disk with cd
    out.extend_from_slice(&count.to_le_bytes());
    out.extend_from_slice(&count.to_le_bytes());
    out.extend_from_slice(&cd_size.to_le_bytes());
    out.extend_from_slice(&cd_offset.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes()); // comment len
    out
}

fn deflate(data: &[u8]) -> Vec<u8> {
    use flate2::write::DeflateEncoder;
    use flate2::Compression;
    use std::io::Write;

    let mut encoder = DeflateEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(data).expect("deflate to memory");
    encoder.finish().expect("finish deflate")
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for byte in data {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
}

fn escape(raw: &str) -> String {
    raw.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

const CONTENT_TYPES: &str = r#"<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"/>"#;

/// A `.docx` with one paragraph per string, plus a title/creator in `docProps/core.xml`.
pub fn docx(title: &str, author: &str, paragraphs: &[&str]) -> Vec<u8> {
    let body: String = paragraphs
        .iter()
        .map(|text| format!("<w:p><w:r><w:t>{}</w:t></w:r></w:p>", escape(text)))
        .collect();
    let document = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{body}</w:body></w:document>"#
    );
    let core = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>{}</dc:title><dc:creator>{}</dc:creator></cp:coreProperties>"#,
        escape(title),
        escape(author)
    );
    zip_with(&[
        Entry::deflated("[Content_Types].xml", CONTENT_TYPES.as_bytes()),
        Entry::deflated("word/document.xml", document.as_bytes()),
        Entry::deflated("docProps/core.xml", core.as_bytes()),
    ])
}

/// A `.xlsx` from `(sheet name, rows)` pairs. Strings go through `sharedStrings.xml`, numbers inline.
pub fn xlsx(sheets: &[(&str, Vec<Vec<&str>>)]) -> Vec<u8> {
    let mut shared: Vec<String> = Vec::new();
    let mut sheet_parts: Vec<String> = Vec::new();
    for (_, rows) in sheets {
        let mut body = String::new();
        for (row_index, row) in rows.iter().enumerate() {
            body.push_str(&format!("<row r=\"{}\">", row_index + 1));
            for (col_index, cell) in row.iter().enumerate() {
                let reference = format!("{}{}", column_letter(col_index), row_index + 1);
                match cell.parse::<f64>() {
                    Ok(_) => {
                        body.push_str(&format!("<c r=\"{reference}\"><v>{}</v></c>", escape(cell)))
                    }
                    Err(_) => {
                        shared.push(cell.to_string());
                        body.push_str(&format!(
                            "<c r=\"{reference}\" t=\"s\"><v>{}</v></c>",
                            shared.len() - 1
                        ));
                    }
                }
            }
            body.push_str("</row>");
        }
        sheet_parts.push(format!(
            r#"<?xml version="1.0" encoding="UTF-8"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData>{body}</sheetData></worksheet>"#
        ));
    }

    let sheet_tags: String = sheets
        .iter()
        .enumerate()
        .map(|(index, (name, _))| {
            format!(
                "<sheet name=\"{}\" sheetId=\"{}\" r:id=\"rId{}\"/>",
                escape(name),
                index + 1,
                index + 1
            )
        })
        .collect();
    let workbook = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets>{sheet_tags}</sheets></workbook>"#
    );
    let rels: String = sheets
        .iter()
        .enumerate()
        .map(|(index, _)| {
            format!(
                "<Relationship Id=\"rId{0}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet\" Target=\"worksheets/sheet{0}.xml\"/>",
                index + 1
            )
        })
        .collect();
    let rels = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">{rels}</Relationships>"#
    );
    let strings: String = shared
        .iter()
        .map(|value| format!("<si><t>{}</t></si>", escape(value)))
        .collect();
    let strings = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">{strings}</sst>"#
    );

    let mut entries = vec![
        Entry::deflated("[Content_Types].xml", CONTENT_TYPES.as_bytes()),
        Entry::deflated("xl/workbook.xml", workbook.as_bytes()),
        Entry::deflated("xl/_rels/workbook.xml.rels", rels.as_bytes()),
        Entry::deflated("xl/sharedStrings.xml", strings.as_bytes()),
    ];
    for (index, part) in sheet_parts.iter().enumerate() {
        entries.push(Entry::deflated(
            &format!("xl/worksheets/sheet{}.xml", index + 1),
            part.as_bytes(),
        ));
    }
    zip_with(&entries)
}

/// A `.pptx` with one text box per slide; the first line becomes the slide title.
pub fn pptx(slides: &[Vec<&str>]) -> Vec<u8> {
    let mut entries = vec![Entry::deflated(
        "[Content_Types].xml",
        CONTENT_TYPES.as_bytes(),
    )];
    for (index, lines) in slides.iter().enumerate() {
        let paragraphs: String = lines
            .iter()
            .map(|line| format!("<a:p><a:r><a:t>{}</a:t></a:r></a:p>", escape(line)))
            .collect();
        let slide = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?><p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><p:cSld><p:spTree><p:sp><p:txBody>{paragraphs}</p:txBody></p:sp></p:spTree></p:cSld></p:sld>"#
        );
        entries.push(Entry::deflated(
            &format!("ppt/slides/slide{}.xml", index + 1),
            slide.as_bytes(),
        ));
    }
    zip_with(&entries)
}

/// `0` → `A`, `25` → `Z`, `26` → `AA` (spreadsheet column letters).
pub fn column_letter(index: usize) -> String {
    let mut index = index + 1;
    let mut letters = Vec::new();
    while index > 0 {
        let remainder = (index - 1) % 26;
        letters.push((b'A' + remainder as u8) as char);
        index = (index - 1) / 26;
    }
    letters.iter().rev().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn column_letters_follow_spreadsheet_naming() {
        assert_eq!(column_letter(0), "A");
        assert_eq!(column_letter(25), "Z");
        assert_eq!(column_letter(26), "AA");
        assert_eq!(column_letter(27), "AB");
    }

    #[test]
    fn crc32_matches_the_reference_value() {
        assert_eq!(crc32(b"123456789"), 0xcbf4_3926);
    }
}
