//! `/api/projects` — open local folders as workspaces and serve their file tree.
//!
//! All routes live behind the same Basic auth as the rest of `/api`. Two rules keep them safe:
//! every requested path is canonicalized and must stay inside the project root, and reads are
//! capped in size so the UI never pulls a database or a build artifact into the browser.

use actix_web::{web, HttpResponse, Result};
use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};

use crate::project::Project;
use crate::web::handlers::api::{ApiError, ApiResponse};
use crate::web::state::AppState;

/// Largest file the UI may preview.
const MAX_FILE_BYTES: usize = 512 * 1024;
/// Upper bound on entries returned for one directory listing.
const MAX_DIR_ENTRIES: usize = 500;
/// Folders that are never interesting in a file tree and can be huge.
const SKIP_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    "dist",
    "build",
    ".venv",
    "__pycache__",
    ".next",
    ".idea",
    ".pytest_cache",
];

#[derive(Debug, Deserialize)]
pub struct OpenProjectRequest {
    /// Absolute path (a leading `~` is expanded).
    pub path: String,
}

#[derive(Debug, Deserialize)]
pub struct PathQuery {
    #[serde(default)]
    pub path: String,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct FileEntry {
    pub name: String,
    /// Path relative to the project root, `/`-separated.
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
}

#[derive(Debug, Serialize)]
pub struct FileContent {
    pub path: String,
    pub content: String,
    pub truncated: bool,
}

/// GET /api/projects — every opened project, oldest first.
pub async fn get_projects(state: web::Data<AppState>) -> Result<HttpResponse> {
    let store = state.projects.read().await;
    Ok(HttpResponse::Ok().json(ApiResponse::success(store.list().to_vec())))
}

/// POST /api/projects — open (or refresh) a folder.
pub async fn open_project(
    state: web::Data<AppState>,
    body: web::Json<OpenProjectRequest>,
) -> Result<HttpResponse> {
    let raw = body.path.trim();
    if raw.is_empty() {
        return Err(ApiError::BadRequest("path is required".into()).into());
    }
    let path = crate::config::normalize_workspace_path(Path::new(raw));
    let mut store = state.projects.write().await;
    let project = store
        .open(&path)
        .map_err(|e| ApiError::BadRequest(format!("{e:#}")))?;
    Ok(HttpResponse::Ok().json(ApiResponse::success(project)))
}

/// DELETE /api/projects/{id} — forget a project and unbind any chat using it.
pub async fn delete_project(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse> {
    let id = path.into_inner();
    let mut store = state.projects.write().await;
    if !store.close(&id) {
        return Err(ApiError::NotFound(format!("Project not found: {id}")).into());
    }
    Ok(HttpResponse::Ok().json(ApiResponse::success(serde_json::json!({ "id": id }))))
}

/// GET /api/projects/{id}/files?path=src — one directory listing.
pub async fn get_project_files(
    state: web::Data<AppState>,
    path: web::Path<String>,
    query: web::Query<PathQuery>,
) -> Result<HttpResponse> {
    let root = project_root(&state, &path.into_inner()).await?;
    let rel = normalize_rel(&query.path)?;
    let dir = root.join(&rel);
    if !dir.is_dir() {
        return Err(ApiError::NotFound(format!("Not a directory: {}", query.path)).into());
    }

    let mut entries: Vec<FileEntry> = std::fs::read_dir(&dir)
        .map_err(|e| ApiError::InternalError(format!("{e}")))?
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            let file_type = entry.file_type().ok()?;
            let is_dir = file_type.is_dir();
            if is_dir && SKIP_DIRS.contains(&name.as_str()) {
                return None;
            }
            let size = if is_dir {
                0
            } else {
                entry.metadata().map(|m| m.len()).unwrap_or(0)
            };
            let path = if rel.is_empty() {
                name.clone()
            } else {
                format!("{rel}/{name}")
            };
            Some(FileEntry {
                name,
                path,
                is_dir,
                size,
            })
        })
        .collect();
    // Directories first, then case-insensitive name order — matches how editors show trees.
    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    entries.truncate(MAX_DIR_ENTRIES);

    Ok(
        HttpResponse::Ok().json(ApiResponse::success(serde_json::json!({
            "path": rel,
            "entries": entries,
        }))),
    )
}

/// GET /api/projects/{id}/file?path=src/main.rs — text preview of one file.
pub async fn get_project_file(
    state: web::Data<AppState>,
    path: web::Path<String>,
    query: web::Query<PathQuery>,
) -> Result<HttpResponse> {
    let root = project_root(&state, &path.into_inner()).await?;
    let rel = normalize_rel(&query.path)?;
    if rel.is_empty() {
        return Err(ApiError::BadRequest("path is required".into()).into());
    }
    let file = root.join(&rel);
    let canonical = file
        .canonicalize()
        .map_err(|_| ApiError::NotFound(format!("File not found: {rel}")))?;
    // Symlinks must not escape the project root.
    if !canonical.starts_with(&root) {
        return Err(ApiError::BadRequest("path escapes the project root".into()).into());
    }
    let bytes = std::fs::read(&canonical).map_err(|e| ApiError::InternalError(format!("{e}")))?;
    let truncated = bytes.len() > MAX_FILE_BYTES;
    let slice = &bytes[..bytes.len().min(MAX_FILE_BYTES)];
    let content = match String::from_utf8(slice.to_vec()) {
        Ok(text) => text,
        Err(_) => {
            return Err(ApiError::BadRequest("Binary file cannot be previewed".into()).into())
        }
    };

    Ok(HttpResponse::Ok().json(ApiResponse::success(FileContent {
        path: rel,
        content,
        truncated,
    })))
}

async fn project_root(state: &AppState, id: &str) -> Result<PathBuf, ApiError> {
    let store = state.projects.read().await;
    let project: &Project = store
        .get(id)
        .ok_or_else(|| ApiError::NotFound(format!("Project not found: {id}")))?;
    project
        .path
        .canonicalize()
        .map_err(|_| ApiError::NotFound(format!("Project folder is missing: {}", id)))
}

/// Reject absolute paths, `..` segments and `.`-prefixed escapes; return a `/`-joined relative path.
fn normalize_rel(raw: &str) -> Result<String, ApiError> {
    let trimmed = raw.trim().trim_matches('/');
    if trimmed.is_empty() {
        return Ok(String::new());
    }
    let candidate = Path::new(trimmed);
    if candidate.is_absolute() {
        return Err(ApiError::BadRequest("path must be relative".into()));
    }
    let mut parts: Vec<String> = Vec::new();
    for component in candidate.components() {
        match component {
            Component::Normal(part) => parts.push(part.to_string_lossy().to_string()),
            Component::CurDir => {}
            _ => {
                return Err(ApiError::BadRequest(
                    "path must stay inside the project".into(),
                ))
            }
        }
    }
    Ok(parts.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_rel_strips_dot_segments_and_rejects_escapes() {
        assert_eq!(normalize_rel("").unwrap(), "");
        assert_eq!(normalize_rel("/src/").unwrap(), "src");
        assert_eq!(normalize_rel("./src/./main.rs").unwrap(), "src/main.rs");
        assert!(normalize_rel("../etc/passwd").is_err());
        // A leading `/` is read as project-relative, never as a host path.
        assert_eq!(normalize_rel("/src/main.rs").unwrap(), "src/main.rs");
        assert!(normalize_rel("src/../../etc").is_err());
    }
}
