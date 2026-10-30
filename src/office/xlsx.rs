//! Excel (`.xlsx`) cell extraction: sheet names, and the used cells of each sheet as text.
//!
//! Values are shown the way they are stored: shared strings are resolved through
//! `xl/sharedStrings.xml`, numbers keep their raw representation (a date cell shows its serial
//! number, not a formatted date — number formats live in styles and are out of scope). Formulas are
//! not evaluated; the cached value Excel wrote is what you see.

use std::collections::HashMap;

use anyhow::{bail, Context, Result};
use serde::Serialize;

use super::xml::{attr, decode_entities, squash_whitespace, tokenize, Token};
use super::{Limits, ZipArchive};

const WORKBOOK_PART: &str = "xl/workbook.xml";
const RELS_PART: &str = "xl/_rels/workbook.xml.rels";
const SHARED_STRINGS_PART: &str = "xl/sharedStrings.xml";

#[derive(Debug, Default, Serialize)]
pub struct SheetPreview {
    pub name: String,
    pub rows: Vec<Vec<String>>,
    /// Rows in the sheet, including the ones past the display cap.
    pub rows_total: usize,
    /// Widest displayed row.
    pub cols: usize,
    pub truncated: bool,
}

#[derive(Debug, Default, Serialize)]
pub struct XlsxPreview {
    pub sheets: Vec<SheetPreview>,
    pub sheets_total: usize,
    pub truncated: bool,
}

/// Extract the preview of a `.xlsx` workbook.
pub fn preview(zip: &ZipArchive, limits: &Limits) -> Result<XlsxPreview> {
    let workbook = zip
        .read(WORKBOOK_PART, limits)
        .with_context(|| format!("{WORKBOOK_PART} is missing, so this is not a workbook"))?;
    let workbook = String::from_utf8_lossy(&workbook);
    let sheets = sheet_parts(&workbook, zip, limits)?;
    let shared = if zip.contains(SHARED_STRINGS_PART) {
        let xml = zip.read(SHARED_STRINGS_PART, limits)?;
        shared_strings(&String::from_utf8_lossy(&xml), limits)
    } else {
        Vec::new()
    };

    let mut preview = XlsxPreview {
        sheets_total: sheets.len(),
        ..XlsxPreview::default()
    };
    for (index, (name, part)) in sheets.iter().enumerate() {
        if index >= limits.max_sheets {
            preview.truncated = true;
            break;
        }
        let xml = match zip.read(part, limits) {
            Ok(xml) => xml,
            Err(_) => continue, // a missing sheet part must not hide the others
        };
        let (rows, rows_total, cols, truncated) =
            rows_of(&String::from_utf8_lossy(&xml), &shared, limits);
        preview.truncated |= truncated;
        preview.sheets.push(SheetPreview {
            name: name.clone(),
            rows,
            rows_total,
            cols,
            truncated,
        });
    }
    Ok(preview)
}

/// `(sheet name, part path)` in workbook order, resolving `r:id` through the workbook rels.
fn sheet_parts(workbook: &str, zip: &ZipArchive, limits: &Limits) -> Result<Vec<(String, String)>> {
    let mut rels: HashMap<String, String> = HashMap::new();
    if zip.contains(RELS_PART) {
        let xml = zip.read(RELS_PART, limits)?;
        let xml = String::from_utf8_lossy(&xml);
        for token in tokenize(&xml) {
            let Token::Start { name, attrs, .. } = token else {
                continue;
            };
            if name != "Relationship" {
                continue;
            }
            if let (Some(id), Some(target)) = (attr(attrs, "Id"), attr(attrs, "Target")) {
                rels.insert(id.to_string(), target.to_string());
            }
        }
    }

    let mut sheets = Vec::new();
    for token in tokenize(workbook) {
        let Token::Start { name, attrs, .. } = token else {
            continue;
        };
        if name != "sheet" {
            continue;
        }
        let sheet_name = attr(attrs, "name").unwrap_or("Sheet");
        let part = match attr(attrs, "r:id").and_then(|id| rels.get(id)) {
            Some(target) => join_part(target),
            // No rels part: fall back to the conventional file name for this position.
            None => format!("xl/worksheets/sheet{}.xml", sheets.len() + 1),
        };
        sheets.push((sheet_name.to_string(), part));
    }
    if sheets.is_empty() {
        bail!("workbook lists no sheets");
    }
    Ok(sheets)
}

/// `worksheets/sheet1.xml` → `xl/worksheets/sheet1.xml`; absolute targets are kept as-is.
fn join_part(target: &str) -> String {
    let target = target.trim_start_matches('/');
    if target.starts_with("xl/") {
        target.to_string()
    } else {
        format!("xl/{target}")
    }
}

fn shared_strings(xml: &str, limits: &Limits) -> Vec<String> {
    let mut strings = Vec::new();
    let mut buffer = String::new();
    let mut inside = false;
    for token in tokenize(xml) {
        match token {
            Token::Start { name: "si", .. } => {
                buffer.clear();
                inside = true;
            }
            Token::Start { name: "t", .. } => inside = true,
            Token::End { name: "si" } => {
                strings.push(squash_whitespace(&decode_entities(&buffer)));
                inside = false;
                if strings.len() >= limits.max_shared_strings {
                    return strings;
                }
            }
            Token::Text(text) if inside => buffer.push_str(text),
            _ => {}
        }
    }
    strings
}

/// `(rows, rows_total, widest row, truncated)` for one sheet part.
fn rows_of(
    xml: &str,
    shared: &[String],
    limits: &Limits,
) -> (Vec<Vec<String>>, usize, usize, bool) {
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut row: Vec<String> = Vec::new();
    let mut rows_total = 0usize;
    let mut cols_total = 0usize;
    let mut truncated = false;

    let mut cell_type = String::new();
    let mut cell_ref: Option<usize> = None;
    let mut cell_value = String::new();
    let mut in_value = false;

    for token in tokenize(xml) {
        match token {
            Token::Start { name, attrs, .. } => match name {
                "row" => row.clear(),
                "c" => {
                    cell_type = attr(attrs, "t").unwrap_or("").to_string();
                    cell_ref = attr(attrs, "r").and_then(column_index);
                    cell_value.clear();
                }
                "v" | "t" => in_value = true,
                _ => {}
            },
            Token::End { name } => match name {
                "v" | "t" => in_value = false,
                "c" => {
                    let raw = decode_entities(&cell_value);
                    let value = match cell_type.as_str() {
                        "s" => raw
                            .trim()
                            .parse::<usize>()
                            .ok()
                            .and_then(|index| shared.get(index))
                            .cloned()
                            .unwrap_or_default(),
                        _ => squash_whitespace(&raw),
                    };
                    if limits.max_cols == 0 {
                        continue;
                    }
                    let column = cell_ref.unwrap_or(row.len()).min(limits.max_cols - 1);
                    if row.len() <= column {
                        row.resize(column + 1, String::new());
                    }
                    row[column] = value;
                }
                "row" => {
                    rows_total += 1;
                    let width = row
                        .iter()
                        .rposition(|cell| !cell.is_empty())
                        .map(|last| last + 1)
                        .unwrap_or(0);
                    cols_total = cols_total.max(width);
                    row.truncate(width);
                    if row.is_empty() {
                        continue;
                    }
                    if rows.len() < limits.max_rows {
                        rows.push(row.clone());
                    } else {
                        truncated = true;
                    }
                }
                _ => {}
            },
            Token::Text(text) if in_value => cell_value.push_str(text),
            _ => {}
        }
    }
    (rows, rows_total, cols_total, truncated)
}

/// `B12` → `1` (zero-based column index).
fn column_index(reference: &str) -> Option<usize> {
    let letters = reference
        .chars()
        .take_while(|ch| ch.is_ascii_alphabetic())
        .collect::<String>();
    if letters.is_empty() {
        return None;
    }
    let mut index = 0usize;
    for ch in letters.chars() {
        index = index * 26 + (ch.to_ascii_uppercase() as usize - 'A' as usize + 1);
    }
    Some(index - 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::office::testing::{xlsx, zip_with, Entry};

    fn limits() -> Limits {
        Limits::default()
    }

    #[test]
    fn resolves_shared_strings_numbers_and_sheet_names() {
        let bytes = xlsx(&[
            ("Budget", vec![vec!["Item", "Cost"], vec!["Rent", "1200"]]),
            ("Notes", vec![vec!["done"]]),
        ]);
        let zip = ZipArchive::open(&bytes).unwrap();
        let preview = preview(&zip, &limits()).unwrap();
        assert_eq!(preview.sheets_total, 2);
        assert_eq!(preview.sheets[0].name, "Budget");
        assert_eq!(
            preview.sheets[0].rows,
            vec![
                vec!["Item".to_string(), "Cost".to_string()],
                vec!["Rent".to_string(), "1200".to_string()],
            ]
        );
        assert_eq!(preview.sheets[0].cols, 2);
        assert_eq!(preview.sheets[0].rows_total, 2);
        assert_eq!(preview.sheets[1].rows, vec![vec!["done".to_string()]]);
    }

    #[test]
    fn keeps_gaps_where_cells_are_missing() {
        let sheet = r#"<worksheet><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>a</t></is></c><c r="C1" t="inlineStr"><is><t>c</t></is></c></row></sheetData></worksheet>"#;
        let workbook = r#"<workbook><sheets><sheet name="S" sheetId="1"/></sheets></workbook>"#;
        let bytes = zip_with(&[
            Entry::deflated("xl/workbook.xml", workbook.as_bytes()),
            Entry::deflated("xl/worksheets/sheet1.xml", sheet.as_bytes()),
        ]);
        let zip = ZipArchive::open(&bytes).unwrap();
        let preview = preview(&zip, &limits()).unwrap();
        assert_eq!(
            preview.sheets[0].rows,
            vec![vec!["a".to_string(), String::new(), "c".to_string()]]
        );
    }

    #[test]
    fn caps_rows_and_flags_truncation() {
        let rows: Vec<Vec<&str>> = (0..10).map(|_| vec!["x"]).collect();
        let bytes = xlsx(&[("S", rows)]);
        let zip = ZipArchive::open(&bytes).unwrap();
        let preview = preview(
            &zip,
            &Limits {
                max_rows: 3,
                ..limits()
            },
        )
        .unwrap();
        assert_eq!(preview.sheets[0].rows.len(), 3);
        assert_eq!(preview.sheets[0].rows_total, 10);
        assert!(preview.sheets[0].truncated);
        assert!(preview.truncated);
    }

    #[test]
    fn column_indices_follow_spreadsheet_letters() {
        assert_eq!(column_index("A1"), Some(0));
        assert_eq!(column_index("Z9"), Some(25));
        assert_eq!(column_index("AA1"), Some(26));
        assert_eq!(column_index("12"), None);
    }

    #[test]
    fn rejects_packages_without_a_workbook() {
        let bytes = zip_with(&[Entry::stored("xl/other.xml", b"<x/>")]);
        let zip = ZipArchive::open(&bytes).unwrap();
        assert!(preview(&zip, &limits()).is_err());
    }
}
