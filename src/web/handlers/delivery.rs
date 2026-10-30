//! `/api/projects/{id}/deliverables|preview|download` — the office artifacts an agent produced.
//!
//! Agents write `.docx`/`.xlsx`/`.pptx`/`.pdf` files into the workspace, and until now the dashboard
//! could only say "binary file, 42 KB". These three routes turn a generated document into something
//! the UI can show: a listing of the deliverables in the project, a structured text preview of one
//! of them, and the raw bytes for download (or for rendering PDFs and images in the browser).
//!
//! Path safety is inherited from [`projects`]: every path is normalized relative to the project root
//! and canonicalized, so a `..` segment or a symlink cannot reach outside the workspace.

use actix_web::{web, HttpResponse, Result};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use crate::office::{self, OfficeKind};
use crate::web::handlers::api::{ApiError, ApiResponse};
use crate::web::handlers::projects::{normalize_rel, project_root, PathQuery, SKIP_DIRS};
use crate::web::state::AppState;

/// Deliverables returned for one listing.
const MAX_DELIVERABLES: usize = 500;
/// Filesystem entries visited before the walk gives up, so a vendored tree cannot stall the request.
const MAX_VISITED: usize = 20_000;
/// Directory depth cap.
const MAX_DEPTH: usize = 12;
/// Downloads are served from memory, so they get the same ceiling as the preview.
const MAX_DOWNLOAD_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct Deliverable {
    pub name: String,
    /// Path relative to the project root, `/`-separated.
    pub path: String,
    /// `OfficeKind::slug()`.
    pub kind: &'static str,
    pub size: u64,
    /// Last modification, in milliseconds since the Unix epoch (0 when unavailable).
    pub modified_ms: u64,
}

#[derive(Debug, Serialize)]
pub struct DeliverableList {
    /// Directory the listing was scoped to (`""` for the project root).
    pub path: String,
    pub entries: Vec<Deliverable>,
    /// A cap was hit: the UI says "showing the first N".
    pub truncated: bool,
    /// Entries inspected, including the ones that were skipped.
    pub scanned: usize,
}

/// One deliverable plus its preview — the UI shows `path` in the header and `preview` in the body.
#[derive(Debug, Serialize)]
pub struct DeliverablePreview {
    pub path: String,
    pub name: String,
    pub preview: office::OfficePreview,
}

/// GET /api/projects/{id}/deliverables?path=output — office artifacts, newest first.
pub async fn list_deliverables(
    state: web::Data<AppState>,
    path: web::Path<String>,
    query: web::Query<PathQuery>,
) -> Result<HttpResponse> {
    let root = project_root(&state, &path.into_inner()).await?;
    let rel = normalize_rel(&query.path)?;
    let listed = web::block(move || collect(&root, &rel))
        .await
        .map_err(|e| ApiError::InternalError(format!("deliverable listing failed: {e}")))?;
    Ok(HttpResponse::Ok().json(ApiResponse::success(listed)))
}

/// GET /api/projects/{id}/preview?path=report.docx — structured text preview of one file.
pub async fn preview_deliverable(
    state: web::Data<AppState>,
    path: web::Path<String>,
    query: web::Query<PathQuery>,
) -> Result<HttpResponse> {
    let (root, rel) = resolve(&state, &path.into_inner(), &query.path).await?;
    let file = open_file(&root, &rel)?;
    let name = file_name(&rel);
    let size = file
        .metadata()
        .map_err(|e| ApiError::InternalError(format!("{e}")))?
        .len();
    let target = file.clone();
    let preview = web::block(move || office::preview(&target, size, &office::Limits::default()))
        .await
        .map_err(|e| ApiError::InternalError(format!("preview failed: {e}")))?
        // Everything `office::preview` rejects is a property of the file (wrong format, over the
        // limit, corrupt package), so a 400 tells the UI more than a 500 would.
        .map_err(|e| ApiError::BadRequest(format!("{e:#}")))?;
    Ok(
        HttpResponse::Ok().json(ApiResponse::success(DeliverablePreview {
            path: rel,
            name,
            preview,
        })),
    )
}

/// GET /api/projects/{id}/download?path=report.docx — the raw bytes, as an attachment.
pub async fn download_deliverable(
    state: web::Data<AppState>,
    path: web::Path<String>,
    query: web::Query<PathQuery>,
) -> Result<HttpResponse> {
    let (root, rel) = resolve(&state, &path.into_inner(), &query.path).await?;
    let file = open_file(&root, &rel)?;
    let size = file
        .metadata()
        .map_err(|e| ApiError::InternalError(format!("{e}")))?
        .len();
    if size > MAX_DOWNLOAD_BYTES {
        return Err(ApiError::BadRequest(format!(
            "file is {size} bytes (download limit {MAX_DOWNLOAD_BYTES})"
        ))
        .into());
    }
    let name = download_name(&rel);
    let content_type = OfficeKind::of(&file).content_type();
    let bytes = web::block(move || std::fs::read(&file))
        .await
        .map_err(|e| ApiError::InternalError(format!("download failed: {e}")))?
        .map_err(|e| ApiError::InternalError(format!("{e}")))?;

    Ok(HttpResponse::Ok()
        .content_type(content_type)
        .insert_header((
            "Content-Disposition",
            format!("attachment; filename=\"{name}\""),
        ))
        // The bytes are agent output that changes under the same path, and a sniffed content type
        // on an HTML deliverable would be a script injection vector.
        .insert_header(("Cache-Control", "no-store"))
        .insert_header(("X-Content-Type-Options", "nosniff"))
        .body(bytes))
}

/// Project root plus the normalized relative path of the requested file.
async fn resolve(state: &AppState, id: &str, raw: &str) -> Result<(PathBuf, String), ApiError> {
    let root = project_root(state, id).await?;
    let rel = normalize_rel(raw)?;
    if rel.is_empty() {
        return Err(ApiError::BadRequest("path is required".into()));
    }
    Ok((root, rel))
}

/// Canonicalized file inside `root`; rejects directories, missing files and symlink escapes.
fn open_file(root: &Path, rel: &str) -> Result<PathBuf, ApiError> {
    let canonical = root
        .join(rel)
        .canonicalize()
        .map_err(|_| ApiError::NotFound(format!("File not found: {rel}")))?;
    if !canonical.starts_with(root) {
        return Err(ApiError::BadRequest("path escapes the project root".into()));
    }
    if canonical.is_dir() {
        return Err(ApiError::BadRequest(format!("Not a file: {rel}")));
    }
    Ok(canonical)
}

fn file_name(rel: &str) -> String {
    rel.rsplit('/').next().unwrap_or(rel).to_string()
}

/// `report.docx` → `report.docx`; anything that could break the header becomes `_`.
fn download_name(rel: &str) -> String {
    let name = file_name(rel);
    let safe: String = name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || " ._-()[]+".contains(ch) {
                ch
            } else {
                '_'
            }
        })
        .collect();
    let safe = safe.trim().trim_start_matches('.').trim();
    if safe.is_empty() {
        "deliverable".to_string()
    } else {
        safe.to_string()
    }
}

/// Walk `start` (relative to `root`) and keep the office artifacts, newest first.
fn collect(root: &Path, start: &str) -> DeliverableList {
    let mut entries: Vec<Deliverable> = Vec::new();
    let mut truncated = false;
    let mut scanned = 0usize;
    let mut stack: Vec<(PathBuf, String, usize)> = vec![(root.join(start), start.to_string(), 0)];

    'walk: while let Some((dir, rel, depth)) = stack.pop() {
        let Ok(read) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in read.flatten() {
            scanned += 1;
            if scanned > MAX_VISITED {
                truncated = true;
                break 'walk;
            }
            // Symlinks are skipped: `DirEntry::metadata` does not follow them, and following would
            // reopen the escape problem that `open_file` guards against.
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_symlink() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            let path = if rel.is_empty() {
                name.clone()
            } else {
                format!("{rel}/{name}")
            };
            let Ok(meta) = entry.metadata() else {
                continue;
            };
            if meta.is_dir() {
                // Hidden and build folders are noise in a deliverables list.
                if name.starts_with('.') || SKIP_DIRS.contains(&name.as_str()) {
                    continue;
                }
                if depth < MAX_DEPTH {
                    stack.push((entry.path(), path, depth + 1));
                }
                continue;
            }
            let kind = OfficeKind::of(Path::new(&name));
            if !kind.is_deliverable() {
                continue;
            }
            entries.push(Deliverable {
                name,
                path,
                kind: kind.slug(),
                size: meta.len(),
                modified_ms: modified_ms(&meta),
            });
            if entries.len() >= MAX_DELIVERABLES {
                truncated = true;
                break 'walk;
            }
        }
    }

    // Newest first: the file an agent just wrote is the one the user is looking for.
    entries.sort_by(|a, b| {
        b.modified_ms
            .cmp(&a.modified_ms)
            .then_with(|| a.path.cmp(&b.path))
    });
    DeliverableList {
        path: start.to_string(),
        entries,
        truncated,
        scanned,
    }
}

fn modified_ms(meta: &std::fs::Metadata) -> u64 {
    meta.modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, rel: &str, bytes: &[u8]) {
        let path = dir.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, bytes).unwrap();
    }

    #[test]
    fn lists_deliverables_newest_first_and_skips_noise() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "notes.md", b"# notes");
        write(dir.path(), "main.rs", b"fn main() {}");
        write(dir.path(), "output/report.docx", b"PK\x03\x04");
        write(dir.path(), "output/chart.png", b"\x89PNG");
        write(dir.path(), "node_modules/pkg/README.md", b"# vendored");
        write(dir.path(), ".git/config", b"[core]");

        let listed = collect(dir.path(), "");
        let paths: Vec<&str> = listed
            .entries
            .iter()
            .map(|entry| entry.path.as_str())
            .collect();
        let mut sorted = paths.clone();
        sorted.sort_unstable();
        assert_eq!(
            sorted,
            vec!["notes.md", "output/chart.png", "output/report.docx"]
        );
        assert!(!listed.truncated);
        let docx = listed
            .entries
            .iter()
            .find(|entry| entry.path.ends_with("report.docx"))
            .unwrap();
        assert_eq!(docx.kind, "docx");
        assert_eq!(docx.name, "report.docx");
        assert_eq!(docx.size, 4);
    }

    #[test]
    fn scopes_the_listing_to_a_subdirectory() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "a/one.pdf", b"%PDF");
        write(dir.path(), "b/two.pdf", b"%PDF");
        let listed = collect(dir.path(), "b");
        assert_eq!(listed.path, "b");
        assert_eq!(listed.entries.len(), 1);
        assert_eq!(listed.entries[0].path, "b/two.pdf");
    }

    #[test]
    fn download_names_cannot_break_the_header() {
        assert_eq!(download_name("out/report.docx"), "report.docx");
        assert_eq!(download_name("evil\"\r\nX: 1.pdf"), "evil___X_ 1.pdf");
        assert_eq!(download_name(".hidden"), "hidden");
        assert_eq!(download_name(""), "deliverable");
    }
}
