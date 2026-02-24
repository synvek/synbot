//! Shared helpers for channel file handling.
//!
//! Saves incoming files to workspace with unique names: if the file exists,
//! append _1, _2, ... before the extension until the path is free.

use std::path::{Path, PathBuf};

use anyhow::Result;
use tracing::info;

/// Save bytes to `workspace_dir` under a unique file name derived from `original_name`.
/// If `original_name` already exists, try `name_1`, `name_2`, ... (or `name_1.ext`, `name_2.ext` if has ext) until successful.
/// No extension is not changed: a name without suffix stays without suffix (no .bin added).
/// Returns the path (under workspace) that was written.
pub fn save_incoming_file(
    workspace_dir: &Path,
    original_name: &str,
    bytes: &[u8],
) -> Result<PathBuf> {
    let name = sanitize_filename(original_name);
    if name.is_empty() {
        anyhow::bail!("empty file name after sanitize");
    }
    let (stem, ext_opt) = split_stem_ext(&name);
    let mut n = 0u32;
    loop {
        let filename = match (&ext_opt, n) {
            (Some(ext), 0) => format!("{stem}.{ext}"),
            (Some(ext), k) => format!("{stem}_{k}.{ext}"),
            (None, 0) => stem.clone(),
            (None, k) => format!("{stem}_{k}"),
        };
        let path = workspace_dir.join(&filename);
        if !path.exists() {
            std::fs::write(&path, bytes)?;
            info!(path = %path.display(), size = bytes.len(), "saved incoming file");
            return Ok(path);
        }
        n += 1;
    }
}

/// Sanitize a file name: remove path components and characters unsafe for the filesystem.
fn sanitize_filename(name: &str) -> String {
    let s = name.trim();
    let base = Path::new(s).file_name().and_then(|n| n.to_str()).unwrap_or(s);
    base.chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' { c } else { '_' })
        .collect::<String>()
}

/// Split "name.ext" into (stem, Some(ext)). If no extension, returns (name, None).
/// Single trailing dot or empty stem after dot are treated as no extension.
fn split_stem_ext(name: &str) -> (String, Option<String>) {
    let name = name.trim();
    if let Some(dot) = name.rfind('.') {
        let stem = name[..dot].trim();
        let ext = name[dot + 1..].trim();
        if !stem.is_empty() && !ext.is_empty() {
            return (stem.to_string(), Some(ext.to_string()));
        }
    }
    (name.to_string(), None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_split_stem_ext() {
        assert_eq!(split_stem_ext("a.pdf"), ("a".to_string(), Some("pdf".to_string())));
        assert_eq!(split_stem_ext("doc.x.y"), ("doc.x".to_string(), Some("y".to_string())));
        assert_eq!(split_stem_ext("noext"), ("noext".to_string(), None));
        assert_eq!(split_stem_ext("Makefile"), ("Makefile".to_string(), None));
    }

    #[test]
    fn test_save_incoming_file_no_extension() {
        let tmp = TempDir::new().unwrap();
        let p1 = save_incoming_file(tmp.path(), "Makefile", b"all:").unwrap();
        assert_eq!(p1.file_name().unwrap(), "Makefile");
        let p2 = save_incoming_file(tmp.path(), "Makefile", b"other").unwrap();
        assert_eq!(p2.file_name().unwrap(), "Makefile_1");
    }

    #[test]
    fn test_save_incoming_file_unique_names() {
        let tmp = TempDir::new().unwrap();
        let p1 = save_incoming_file(tmp.path(), "f.txt", b"one").unwrap();
        assert_eq!(p1.file_name().unwrap(), "f.txt");
        let p2 = save_incoming_file(tmp.path(), "f.txt", b"two").unwrap();
        assert_eq!(p2.file_name().unwrap(), "f_1.txt");
        let p3 = save_incoming_file(tmp.path(), "f.txt", b"three").unwrap();
        assert_eq!(p3.file_name().unwrap(), "f_2.txt");
        assert_eq!(std::fs::read(&p1).unwrap(), b"one");
        assert_eq!(std::fs::read(&p2).unwrap(), b"two");
        assert_eq!(std::fs::read(&p3).unwrap(), b"three");
    }
}
