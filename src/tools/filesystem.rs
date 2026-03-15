//! File-system tools: read_file, write_file, edit_file, list_dir,
//! read_multiple_files, create_dir, move_file, search_files, search_text, get_file_info.
//!
//! When an agent runs, paths are restricted to that agent's workspace
//! via the tool execution context. Memory is accessed only via remember/list_memory tools.

use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use tracing::{info, warn};
use crate::tools::context;
use crate::tools::DynTool;

/// Normalize path for prefix comparison so that Windows verbatim prefix (\\?\)
/// and non-canonical paths compare correctly (e.g. write_file to a new path).
#[cfg(windows)]
fn path_for_prefix_check(p: &Path) -> PathBuf {
    let s = p.to_string_lossy();
    let s = s.trim_start_matches(r"\\?\");
    PathBuf::from(s)
}

#[cfg(not(windows))]
fn path_for_prefix_check(p: &Path) -> PathBuf {
    p.to_path_buf()
}

/// Resolve path and enforce scope: when restrict is true, path must be under
/// the current agent's allowed workspace (from context, or default workspace).
/// When a tool context is set (e.g. @@dev), relative paths are resolved against that agent's
/// workspace, not the tool's registered workspace (which may be main's).
fn resolve_path(path: &str, workspace: &Path, restrict: bool) -> anyhow::Result<PathBuf> {
    let effective_workspace: PathBuf = context::current_allowed_roots()
        .unwrap_or_else(|| workspace.to_path_buf());
    let p = if Path::new(path).is_absolute() {
        PathBuf::from(path)
    } else {
        effective_workspace.join(path)
    };
    // For new files (e.g. write_file) canonicalize() fails; use p so scope check still works
    let canonical = p.canonicalize().unwrap_or_else(|_| p.clone());
    let canonical_norm = path_for_prefix_check(&canonical);

    let allowed = if let Some(ctx_workspace) = context::current_allowed_roots() {
        let ws_canon = ctx_workspace.canonicalize().unwrap_or_else(|_| ctx_workspace.to_path_buf());
        let ws_norm = path_for_prefix_check(&ws_canon);
        if canonical_norm.starts_with(&ws_norm) {
            true
        } else {
            warn!(
                path = %path,
                resolved = %canonical.display(),
                allowed_workspace = %ws_canon.display(),
                "Path is outside current agent scope (workspace); access denied"
            );
            false
        }
    } else if restrict {
        let ws_canon = workspace.canonicalize().unwrap_or_else(|_| workspace.to_path_buf());
        let ws_norm = path_for_prefix_check(&ws_canon);
        canonical_norm.starts_with(&ws_norm)
    } else {
        true
    };
    if !allowed {
        anyhow::bail!(
            "Path {} is outside current agent scope (allowed: this agent's workspace only)",
            path
        );
    }
    Ok(p)
}

/// Extensions that are typically binary; read_file returns file info instead of UTF-8 content.
fn is_likely_binary_extension(ext: &str) -> bool {
    matches!(
        ext.to_lowercase().as_str(),
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "ico" | "svg"
            | "pdf" | "zip" | "rar" | "7z" | "tar" | "gz"
            | "mp3" | "mp4" | "wav" | "ogg" | "webm" | "avi" | "mov"
            | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx"
            | "woff" | "woff2" | "ttf" | "otf" | "eot"
    )
}

// ---- ReadFile ----

pub struct ReadFileTool {
    pub workspace: PathBuf,
    pub restrict: bool,
}

#[async_trait::async_trait]
impl DynTool for ReadFileTool {
    fn name(&self) -> &str { "read_file" }
    fn description(&self) -> &str { "Read the contents of a file. For binary files (e.g. png, jpg, pdf), returns file type and size instead of content." }
    fn parameters_schema(&self) -> Value {
        json!({"type":"object","properties":{"path":{"type":"string"}},"required":["path"]})
    }
    async fn call(&self, args: Value) -> anyhow::Result<String> {
        let path = resolve_path(args["path"].as_str().unwrap_or(""), &self.workspace, self.restrict)?;
        info!(path = %path.display(), "read_file");
        let ext = path
            .extension()
            .map(|e| e.to_string_lossy().into_owned())
            .unwrap_or_default();
        if is_likely_binary_extension(&ext) {
            let meta = tokio::fs::metadata(&path).await?;
            let size = meta.len();
            let mime_hint = match ext.to_lowercase().as_str() {
                "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "ico" => "image",
                "pdf" => "document (PDF)",
                "mp3" | "wav" | "ogg" => "audio",
                "mp4" | "webm" | "avi" | "mov" => "video",
                "zip" | "rar" | "7z" | "tar" | "gz" => "archive",
                _ => "binary",
            };
            return Ok(format!(
                "Binary file ({}), size: {} bytes. Content is not text; use get_file_info for metadata.",
                mime_hint, size
            ));
        }
        Ok(tokio::fs::read_to_string(&path).await?)
    }
}

// ---- WriteFile ----

pub struct WriteFileTool {
    pub workspace: PathBuf,
    pub restrict: bool,
}

#[async_trait::async_trait]
impl DynTool for WriteFileTool {
    fn name(&self) -> &str { "write_file" }
    fn description(&self) -> &str { "Write content to a file (creates dirs if needed)." }
    fn parameters_schema(&self) -> Value {
        json!({"type":"object","properties":{"path":{"type":"string"},"content":{"type":"string"}},"required":["path","content"]})
    }
    async fn call(&self, args: Value) -> anyhow::Result<String> {
        let path = resolve_path(args["path"].as_str().unwrap_or(""), &self.workspace, self.restrict)?;
        let content = args["content"].as_str().unwrap_or("");
        info!(path = %path.display(), len = content.len(), "write_file");
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(&path, content).await?;
        Ok(format!("Wrote {} bytes to {}", content.len(), path.display()))
    }
}

// ---- EditFile ----

pub struct EditFileTool {
    pub workspace: PathBuf,
    pub restrict: bool,
}

#[async_trait::async_trait]
impl DynTool for EditFileTool {
    fn name(&self) -> &str { "edit_file" }
    fn description(&self) -> &str {
        "Edit a file by replacing specific text. Supports single edit (old_text/new_text) or batch edits (edits array). Batch edits are applied sequentially and are atomic — if any edit fails, all changes are rolled back."
    }
    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "File path to edit" },
                "old_text": { "type": "string", "description": "Text to find (single edit mode)" },
                "new_text": { "type": "string", "description": "Replacement text (single edit mode)" },
                "edits": {
                    "type": "array",
                    "description": "Batch edits: array of {old_text, new_text} pairs applied sequentially",
                    "items": {
                        "type": "object",
                        "properties": {
                            "old_text": { "type": "string" },
                            "new_text": { "type": "string" }
                        },
                        "required": ["old_text", "new_text"]
                    }
                }
            },
            "required": ["path"]
        })
    }
    async fn call(&self, args: Value) -> anyhow::Result<String> {
        let path = resolve_path(args["path"].as_str().unwrap_or(""), &self.workspace, self.restrict)?;
        info!(path = %path.display(), "edit_file");
        let content = tokio::fs::read_to_string(&path).await?;

        // Build the list of edits: either from `edits` array or single old_text/new_text
        let edits: Vec<(String, String)> = if let Some(arr) = args["edits"].as_array() {
            arr.iter()
                .map(|e| {
                    let old = e["old_text"].as_str().unwrap_or("").to_string();
                    let new = e["new_text"].as_str().unwrap_or("").to_string();
                    (old, new)
                })
                .collect()
        } else {
            let old = args["old_text"].as_str().unwrap_or("").to_string();
            let new = args["new_text"].as_str().unwrap_or("").to_string();
            vec![(old, new)]
        };

        if edits.is_empty() {
            anyhow::bail!("No edits provided");
        }

        // Apply edits sequentially; bail on first failure (atomic: original file untouched)
        let mut result = content;
        for (i, (old, new)) in edits.iter().enumerate() {
            if !result.contains(old.as_str()) {
                anyhow::bail!(
                    "edit #{}: old_text not found in {} (after {} prior edit(s))",
                    i + 1,
                    path.display(),
                    i
                );
            }
            result = result.replacen(old.as_str(), new.as_str(), 1);
        }

        tokio::fs::write(&path, &result).await?;
        let count = edits.len();
        if count == 1 {
            Ok(format!("Edited {}", path.display()))
        } else {
            Ok(format!("Applied {} edits to {}", count, path.display()))
        }
    }
}

// ---- ListDir ----

pub struct ListDirTool {
    pub workspace: PathBuf,
    pub restrict: bool,
}

#[async_trait::async_trait]
impl DynTool for ListDirTool {
    fn name(&self) -> &str { "list_dir" }
    fn description(&self) -> &str {
        "List contents of a directory: returns both subdirectories and files, clearly labeled. Prefer this over exec for listing a folder so one tool call is enough."
    }
    fn parameters_schema(&self) -> Value {
        json!({"type":"object","properties":{"path":{"type":"string","description":"Directory path (default '.' for current workspace)"}},"required":["path"]})
    }
    async fn call(&self, args: Value) -> anyhow::Result<String> {
        let path = resolve_path(args["path"].as_str().unwrap_or("."), &self.workspace, self.restrict)?;
        info!(path = %path.display(), "list_dir");
        let mut entries = tokio::fs::read_dir(&path).await?;
        let mut dirs = Vec::new();
        let mut files = Vec::new();
        while let Some(entry) = entries.next_entry().await? {
            let name = entry.file_name().to_string_lossy().into_owned();
            let ft = entry.file_type().await?;
            if ft.is_dir() {
                dirs.push(name);
            } else {
                files.push(name);
            }
        }
        dirs.sort();
        files.sort();
        let mut out = Vec::new();
        if !dirs.is_empty() {
            out.push("Directories:".to_string());
            for d in &dirs {
                out.push(format!("  {} (dir)", d));
            }
        }
        if !files.is_empty() {
            out.push("Files:".to_string());
            for f in &files {
                out.push(format!("  {}", f));
            }
        }
        if out.is_empty() {
            out.push("(empty directory)".to_string());
        }
        Ok(out.join("\n"))
    }
}

// ---- ReadMultipleFiles ----

pub struct ReadMultipleFilesTool {
    pub workspace: PathBuf,
    pub restrict: bool,
}

#[async_trait::async_trait]
impl DynTool for ReadMultipleFilesTool {
    fn name(&self) -> &str { "read_multiple_files" }
    fn description(&self) -> &str {
        "Read the contents of multiple files at once. Pass an array of paths; returns each file's content with a header. Use when you need to read several files in one call."
    }
    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "paths": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "List of file paths to read (relative to workspace or absolute within scope)"
                }
            },
            "required": ["paths"]
        })
    }
    async fn call(&self, args: Value) -> anyhow::Result<String> {
        let paths = args["paths"].as_array().ok_or_else(|| anyhow::anyhow!("paths must be an array"))?;
        let mut out = Vec::new();
        for (i, p) in paths.iter().enumerate() {
            let path_str = p.as_str().unwrap_or("").trim();
            if path_str.is_empty() { continue; }
            let path = resolve_path(path_str, &self.workspace, self.restrict)?;
            info!(path = %path.display(), "read_multiple_files");
            let ext = path.extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_default();
            let content = if is_likely_binary_extension(&ext) {
                let meta = tokio::fs::metadata(&path).await?;
                format!("[Binary file, {} bytes; use get_file_info for metadata.]", meta.len())
            } else {
                tokio::fs::read_to_string(&path).await?
            };
            if i > 0 {
                out.push(String::new());
            }
            out.push(format!("=== {} ===", path.display()));
            out.push(content);
        }
        Ok(out.join("\n"))
    }
}

// ---- CreateDir ----

pub struct CreateDirTool {
    pub workspace: PathBuf,
    pub restrict: bool,
}

#[async_trait::async_trait]
impl DynTool for CreateDirTool {
    fn name(&self) -> &str { "create_dir" }
    fn description(&self) -> &str {
        "Create a directory (and any missing parent directories). Use for creating folders under the workspace."
    }
    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Directory path to create" }
            },
            "required": ["path"]
        })
    }
    async fn call(&self, args: Value) -> anyhow::Result<String> {
        let path = resolve_path(args["path"].as_str().unwrap_or(""), &self.workspace, self.restrict)?;
        info!(path = %path.display(), "create_dir");
        tokio::fs::create_dir_all(&path).await?;
        Ok(format!("Created directory {}", path.display()))
    }
}

// ---- MoveFile ----

pub struct MoveFileTool {
    pub workspace: PathBuf,
    pub restrict: bool,
}

#[async_trait::async_trait]
impl DynTool for MoveFileTool {
    fn name(&self) -> &str { "move_file" }
    fn description(&self) -> &str {
        "Move or rename a file or directory. Source and destination must be within the workspace."
    }
    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "source": { "type": "string", "description": "Source path" },
                "destination": { "type": "string", "description": "Destination path" }
            },
            "required": ["source", "destination"]
        })
    }
    async fn call(&self, args: Value) -> anyhow::Result<String> {
        let source = resolve_path(args["source"].as_str().unwrap_or(""), &self.workspace, self.restrict)?;
        let dest = resolve_path(args["destination"].as_str().unwrap_or(""), &self.workspace, self.restrict)?;
        info!(source = %source.display(), destination = %dest.display(), "move_file");
        if !tokio::fs::try_exists(&source).await.unwrap_or(false) {
            anyhow::bail!("Source does not exist: {}", source.display());
        }
        if let Some(parent) = dest.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        if tokio::fs::rename(&source, &dest).await.is_err() {
            // Cross-filesystem: copy then remove
            if tokio::fs::metadata(&source).await?.is_dir() {
                copy_dir_all(&source, &dest).await?;
                remove_dir_all(&source).await?;
            } else {
                tokio::fs::copy(&source, &dest).await?;
                tokio::fs::remove_file(&source).await?;
            }
        }
        Ok(format!("Moved {} to {}", source.display(), dest.display()))
    }
}

async fn copy_dir_all(src: &Path, dst: &Path) -> anyhow::Result<()> {
    tokio::fs::create_dir_all(dst).await?;
    let mut entries = tokio::fs::read_dir(src).await?;
    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        let name = entry.file_name();
        let dest_path = dst.join(&name);
        if entry.file_type().await?.is_dir() {
            let path = path.to_path_buf();
            let dest_path = dest_path.to_path_buf();
            Box::pin(copy_dir_all(&path, &dest_path)).await?;
        } else {
            tokio::fs::copy(&path, &dest_path).await?;
        }
    }
    Ok(())
}

async fn remove_dir_all(path: &Path) -> anyhow::Result<()> {
    let mut entries = tokio::fs::read_dir(path).await?;
    while let Some(entry) = entries.next_entry().await? {
        let p = entry.path();
        if entry.file_type().await?.is_dir() {
            let p = p.to_path_buf();
            Box::pin(remove_dir_all(&p)).await?;
        } else {
            tokio::fs::remove_file(&p).await?;
        }
    }
    tokio::fs::remove_dir(path).await?;
    Ok(())
}

// ---- SearchFiles ----

pub struct SearchFilesTool {
    pub workspace: PathBuf,
    pub restrict: bool,
}

const MAX_SEARCH_FILES: usize = 2000;

fn search_files_recursive(
    dir: &Path,
    base: &Path,
    pattern: &glob::Pattern,
    out: &mut Vec<String>,
) -> anyhow::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        if out.len() >= MAX_SEARCH_FILES {
            break;
        }
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            search_files_recursive(&path, base, pattern, out)?;
        } else if pattern.matches(&name) {
            let rel = path.strip_prefix(base).unwrap_or(&path);
            out.push(rel.to_string_lossy().into_owned());
        }
    }
    Ok(())
}

#[async_trait::async_trait]
impl DynTool for SearchFilesTool {
    fn name(&self) -> &str { "search_files" }
    fn description(&self) -> &str {
        "Search for files by name pattern (glob, e.g. '*.rs' or '*.md'). Returns paths relative to the given directory. Searches recursively."
    }
    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "directory": { "type": "string", "description": "Directory to search in (default '.')", "default": "." },
                "pattern": { "type": "string", "description": "Glob pattern for file name (e.g. '*.rs', '*.md')" }
            },
            "required": ["pattern"]
        })
    }
    async fn call(&self, args: Value) -> anyhow::Result<String> {
        let dir = resolve_path(
            args["directory"].as_str().unwrap_or("."),
            &self.workspace,
            self.restrict,
        )?;
        let pattern_str = args["pattern"].as_str().unwrap_or("*").trim();
        let pattern = glob::Pattern::new(pattern_str)
            .map_err(|e| anyhow::anyhow!("Invalid glob pattern '{}': {}", pattern_str, e))?;
        info!(directory = %dir.display(), pattern = %pattern_str, "search_files");
        let mut out = Vec::new();
        search_files_recursive(&dir, &dir, &pattern, &mut out)?;
        out.sort();
        Ok(if out.is_empty() {
            format!("No files matching '{}' in {}", pattern_str, dir.display())
        } else {
            out.join("\n")
        })
    }
}

// ---- SearchText ----

pub struct SearchTextTool {
    pub workspace: PathBuf,
    pub restrict: bool,
}

const MAX_SEARCH_TEXT_FILES: usize = 500;
const MAX_FILE_SIZE_FOR_SEARCH: u64 = 512 * 1024;

fn search_text_in_dir(
    dir: &Path,
    base: &Path,
    query: &str,
    file_glob: Option<&glob::Pattern>,
    out: &mut Vec<String>,
) -> anyhow::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        if out.len() >= MAX_SEARCH_TEXT_FILES {
            break;
        }
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            search_text_in_dir(&path, base, query, file_glob, out)?;
        } else if let Some(ref pat) = file_glob {
            if !pat.matches(&name) {
                continue;
            }
        }
        let meta = std::fs::metadata(&path).ok();
        if meta.as_ref().map(|m| m.len() > MAX_FILE_SIZE_FOR_SEARCH).unwrap_or(true) {
            continue;
        }
        if let Ok(content) = std::fs::read_to_string(&path) {
            if content.contains(query) {
                let rel = path.strip_prefix(base).unwrap_or(&path);
                out.push(rel.to_string_lossy().into_owned());
            }
        }
    }
    Ok(())
}

#[async_trait::async_trait]
impl DynTool for SearchTextTool {
    fn name(&self) -> &str { "search_text" }
    fn description(&self) -> &str {
        "Search for a text string inside files under a directory. Optionally restrict by file name glob (e.g. '*.md'). Returns list of file paths that contain the query."
    }
    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "directory": { "type": "string", "description": "Directory to search in (default '.')", "default": "." },
                "query": { "type": "string", "description": "Text string to search for" },
                "file_glob": { "type": "string", "description": "Optional glob to filter files (e.g. '*.md'). Omit to search all text files." }
            },
            "required": ["query"]
        })
    }
    async fn call(&self, args: Value) -> anyhow::Result<String> {
        let dir = resolve_path(
            args["directory"].as_str().unwrap_or("."),
            &self.workspace,
            self.restrict,
        )?;
        let query = args["query"].as_str().unwrap_or("").trim();
        if query.is_empty() {
            anyhow::bail!("query must be non-empty");
        }
        let file_glob = args["file_glob"]
            .as_str()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| glob::Pattern::new(s))
            .transpose()
            .map_err(|e| anyhow::anyhow!("Invalid file_glob: {}", e))?;
        info!(directory = %dir.display(), query_len = query.len(), "search_text");
        let mut out = Vec::new();
        search_text_in_dir(&dir, &dir, query, file_glob.as_ref(), &mut out)?;
        out.sort();
        Ok(if out.is_empty() {
            format!("No files containing {:?} in {}", query, dir.display())
        } else {
            out.join("\n")
        })
    }
}

// ---- GetFileInfo ----

pub struct GetFileInfoTool {
    pub workspace: PathBuf,
    pub restrict: bool,
}

#[async_trait::async_trait]
impl DynTool for GetFileInfoTool {
    fn name(&self) -> &str { "get_file_info" }
    fn description(&self) -> &str {
        "Get metadata for a file or directory: size, type (file/dir), last modified time. Path must be within workspace."
    }
    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Path to file or directory" }
            },
            "required": ["path"]
        })
    }
    async fn call(&self, args: Value) -> anyhow::Result<String> {
        let path = resolve_path(args["path"].as_str().unwrap_or(""), &self.workspace, self.restrict)?;
        info!(path = %path.display(), "get_file_info");
        let meta = tokio::fs::metadata(&path).await?;
        let kind = if meta.is_dir() { "directory" } else { "file" };
        let modified = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .and_then(|d| {
                chrono::DateTime::from_timestamp(d.as_secs() as i64, d.subsec_nanos())
            })
            .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
            .unwrap_or_else(|| "unknown".to_string());
        let size = meta.len();
        Ok(format!(
            "path: {}\ntype: {}\nsize: {} bytes\nmodified: {}",
            path.display(),
            kind,
            size,
            modified
        ))
    }
}
