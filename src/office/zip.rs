//! Minimal ZIP reader for OOXML packages: central directory, stored and deflated entries.
//!
//! A `.docx`/`.xlsx`/`.pptx` is a ZIP of XML parts. Reading a handful of known parts needs the
//! central directory and the two compression methods every producer uses (stored and deflate), so
//! this module stays small: no writing, no encryption, no ZIP64, no multi-disk archives.

use std::io::Read;

use anyhow::{bail, Context, Result};

use super::Limits;

const EOCD_SIGNATURE: u32 = 0x0605_4b50;
const CENTRAL_SIGNATURE: u32 = 0x0201_4b50;
const LOCAL_SIGNATURE: u32 = 0x0403_4b50;
/// The end-of-central-directory record sits in the last 64 KiB (its comment field is bounded).
const EOCD_SEARCH: usize = 65_536 + 22;
/// A package with more entries than this is not an office document.
const MAX_ENTRIES: usize = 4_096;
const METHOD_STORED: u16 = 0;
const METHOD_DEFLATE: u16 = 8;

#[derive(Debug, Clone)]
struct Entry {
    name: String,
    method: u16,
    compressed_size: usize,
    uncompressed_size: usize,
    local_offset: usize,
}

/// Read-only view over a ZIP archive held in memory.
#[derive(Debug)]
pub struct ZipArchive<'a> {
    bytes: &'a [u8],
    entries: Vec<Entry>,
}

impl<'a> ZipArchive<'a> {
    /// Parse the central directory. Entries are kept in file order.
    pub fn open(bytes: &'a [u8]) -> Result<Self> {
        let header = find_eocd(bytes).context("not a ZIP package (no end-of-central-directory)")?;
        let count = read_u16(bytes, header + 10)? as usize;
        let cd_offset = read_u32(bytes, header + 16)? as usize;
        if count == u16::MAX as usize || cd_offset == u32::MAX as usize {
            bail!("ZIP64 packages are not supported");
        }
        if count > MAX_ENTRIES {
            bail!("package has {count} entries (limit {MAX_ENTRIES})");
        }

        let mut entries = Vec::with_capacity(count);
        let mut cursor = cd_offset;
        for _ in 0..count {
            if read_u32(bytes, cursor)? != CENTRAL_SIGNATURE {
                bail!("broken ZIP central directory at offset {cursor}");
            }
            let method = read_u16(bytes, cursor + 10)?;
            let compressed_size = read_u32(bytes, cursor + 20)? as usize;
            let uncompressed_size = read_u32(bytes, cursor + 24)? as usize;
            let name_len = read_u16(bytes, cursor + 28)? as usize;
            let extra_len = read_u16(bytes, cursor + 30)? as usize;
            let comment_len = read_u16(bytes, cursor + 32)? as usize;
            let local_offset = read_u32(bytes, cursor + 42)? as usize;
            let name_range = cursor + 46..cursor + 46 + name_len;
            let raw_name = bytes
                .get(name_range)
                .context("ZIP entry name runs past the end of the package")?;
            let name = String::from_utf8_lossy(raw_name).into_owned();
            entries.push(Entry {
                name,
                method,
                compressed_size,
                uncompressed_size,
                local_offset,
            });
            cursor += 46 + name_len + extra_len + comment_len;
        }
        Ok(Self { bytes, entries })
    }

    /// Entry names, in archive order (directories included).
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.entries.iter().map(|entry| entry.name.as_str())
    }

    pub fn contains(&self, name: &str) -> bool {
        self.find(name).is_some()
    }

    /// Read one entry, refusing anything larger than `limits.max_entry_bytes`.
    pub fn read(&self, name: &str, limits: &Limits) -> Result<Vec<u8>> {
        let entry = self
            .find(name)
            .with_context(|| format!("'{name}' is not part of this package"))?;
        let declared = entry.uncompressed_size.max(entry.compressed_size);
        if declared > limits.max_entry_bytes {
            bail!(
                "'{}' is {declared} bytes (limit {})",
                entry.name,
                limits.max_entry_bytes
            );
        }
        let data = self.entry_data(entry)?;
        match entry.method {
            METHOD_STORED => Ok(data.to_vec()),
            METHOD_DEFLATE => {
                let mut out =
                    Vec::with_capacity(data.len().saturating_mul(3).min(limits.max_entry_bytes));
                let mut decoder = flate2::read::DeflateDecoder::new(data);
                // Read one byte past the limit so a lying header cannot blow up memory.
                decoder
                    .by_ref()
                    .take(limits.max_entry_bytes as u64 + 1)
                    .read_to_end(&mut out)
                    .with_context(|| format!("inflate '{}'", entry.name))?;
                if out.len() > limits.max_entry_bytes {
                    bail!(
                        "'{}' inflates past the {} byte limit",
                        entry.name,
                        limits.max_entry_bytes
                    );
                }
                Ok(out)
            }
            other => bail!("'{}' uses unsupported ZIP method {other}", entry.name),
        }
    }

    fn find(&self, name: &str) -> Option<&Entry> {
        self.entries
            .iter()
            .find(|entry| entry.name == name)
            .or_else(|| {
                self.entries
                    .iter()
                    .find(|entry| entry.name.eq_ignore_ascii_case(name))
            })
    }

    fn entry_data(&self, entry: &Entry) -> Result<&'a [u8]> {
        let header = entry.local_offset;
        if read_u32(self.bytes, header)? != LOCAL_SIGNATURE {
            bail!("broken local header for '{}'", entry.name);
        }
        let name_len = read_u16(self.bytes, header + 26)? as usize;
        let extra_len = read_u16(self.bytes, header + 28)? as usize;
        let start = header + 30 + name_len + extra_len;
        self.bytes
            .get(start..start + entry.compressed_size)
            .with_context(|| format!("'{}' runs past the end of the package", entry.name))
    }
}

fn find_eocd(bytes: &[u8]) -> Option<usize> {
    if bytes.len() < 22 {
        return None;
    }
    let earliest = bytes.len().saturating_sub(EOCD_SEARCH);
    (earliest..=bytes.len() - 22).rev().find(|offset| {
        read_u32(bytes, *offset)
            .map(|sig| sig == EOCD_SIGNATURE)
            .unwrap_or(false)
    })
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16> {
    let slice = bytes
        .get(offset..offset + 2)
        .context("unexpected end of ZIP package")?;
    Ok(u16::from_le_bytes([slice[0], slice[1]]))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32> {
    let slice = bytes
        .get(offset..offset + 4)
        .context("unexpected end of ZIP package")?;
    Ok(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::office::testing::{zip_with, Entry as TestEntry};

    #[test]
    fn reads_stored_and_deflated_entries() {
        let bytes = zip_with(&[
            TestEntry::stored("a.txt", b"stored"),
            TestEntry::deflated("dir/b.xml", br#"<root><t>hello</t></root>"#),
        ]);
        let zip = ZipArchive::open(&bytes).unwrap();
        assert_eq!(zip.names().collect::<Vec<_>>(), vec!["a.txt", "dir/b.xml"]);
        let limits = Limits::default();
        assert_eq!(zip.read("a.txt", &limits).unwrap(), b"stored");
        assert_eq!(
            zip.read("dir/b.xml", &limits).unwrap(),
            br#"<root><t>hello</t></root>"#
        );
        assert!(zip.contains("A.TXT"), "entry lookup is case-insensitive");
    }

    #[test]
    fn rejects_entries_over_the_limit() {
        let bytes = zip_with(&[TestEntry::deflated("big.xml", &vec![b'x'; 4096])]);
        let zip = ZipArchive::open(&bytes).unwrap();
        let limits = Limits {
            max_entry_bytes: 1024,
            ..Limits::default()
        };
        assert!(zip.read("big.xml", &limits).is_err());
    }

    #[test]
    fn refuses_archives_that_are_not_zip() {
        let err = ZipArchive::open(b"plain text, no zip at all").unwrap_err();
        assert!(format!("{err:#}").contains("ZIP"));
    }
}
